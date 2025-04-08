// src/graphql/resolvers/pricing_resolver.ts

import {
  Arg,
  Authorized,
  Ctx,
  FieldResolver,
  Mutation,
  Query,
  Resolver,
  ResolverInterface,
  Root
} from 'type-graphql';
import { ForbiddenError, ApolloError } from 'apollo-server-errors';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { SolanaPricingService } from '../../services/pricing';
import { AuthService } from '../../services/auth';
import { Logger } from '../../utils/logger';
import {
  PricingPlan,
  CostBreakdown,
  PaymentHistory,
  DiscountConfig,
  DynamicPriceConfig,
  PricingStrategy,
  PaymentResult
} from '../schemas/pricing';
import { Context } from '../../types/context';
import { 
  PaymentRequestInput,
  DiscountApplicationInput,
  PricingConfigUpdateInput,
  DynamicPricingInput
} from '../inputs/pricing';
import { 
  validatePaymentRequest,
  validateDiscountConfig,
  validatePricingUpdate
} from '../validators/pricing';
import { 
  InsufficientBalanceError,
  InvalidPricingConfigError,
  ExpiredDiscountError,
  PaymentVerificationError
} from '../../errors/pricing';
import { 
  PRICING_LAMPORTS_BUFFER,
  DEFAULT_PRICING_STRATEGY,
  DISCOUNT_TYPE,
  PAYMENT_STATUS
} from '../../constants/pricing';
import { 
  getTokenExchangeRate,
  fetchNetworkConditions
} from '../../oracles/price';
import { 
  createPaymentSignature,
  verifyPaymentAuthorization
} from '../../crypto/payments';

@Resolver(() => PricingPlan)
export class PricingResolver implements ResolverInterface<PricingPlan> {
  private readonly logger = new Logger('PricingResolver');
  private readonly solana = new SolanaPricingService();
  private readonly auth = new AuthService();
  private readonly connection = new Connection(process.env.SOLANA_RPC_ENDPOINT!);

  // ----- Core Pricing Operations -----

  @Authorized()
  @Mutation(() => PaymentResult)
  async processPayment(
    @Arg("input") input: PaymentRequestInput,
    @Ctx() { user, ip }: Context
  ): Promise<PaymentResult> {
    try {
      // 1. Validate payment request
      await validatePaymentRequest(input);

      // 2. Verify wallet ownership
      const isValidSig = verifyPaymentAuthorization(
        input.walletAddress,
        input.signature,
        input.paymentNonce
      );
      if (!isValidSig) {
        throw new ForbiddenError('Invalid payment signature');
      }

      // 3. Calculate dynamic pricing
      const priceConfig = await this.calculateDynamicPrice({
        serviceType: input.serviceType,
        modelId: input.modelId,
        resourceUnits: input.resourceUnits,
        urgency: input.urgency
      });

      // 4. Check account balance
      const [balance, feeEstimate] = await Promise.all([
        this.solana.getTokenBalance(user.walletAddress, input.tokenMint),
        this.estimateNetworkFees()
      ]);

      const totalCost = priceConfig.total + feeEstimate;
      if (balance < (totalCost + PRICING_LAMPORTS_BUFFER)) {
        throw new InsufficientBalanceError();
      }

      // 5. Apply discounts
      const discount = await this.applyDiscounts(
        input.discountCode,
        user.walletAddress,
        input.serviceType
      );
      const finalCost = this.applyDiscountToTotal(totalCost, discount);

      // 6. Create blockchain transaction
      const paymentTx = await this.solana.createPaymentTransaction({
        fromAddress: user.walletAddress,
        toAddress: process.env.TREASURY_ACCOUNT!,
        amount: finalCost,
        tokenMint: input.tokenMint,
        referenceId: input.paymentNonce
      });

      // 7. Store payment record
      const paymentRecord = await this.solana.recordPaymentAttempt({
        userId: user.id,
        amount: finalCost,
        txSignature: paymentTx.signature,
        serviceType: input.serviceType,
        status: PAYMENT_STATUS.PENDING
      });

      return {
        success: true,
        transaction: paymentTx,
        costBreakdown: priceConfig,
        discountApplied: discount
      };
    } catch (error) {
      this.logger.error(`Payment failed from ${ip}`, error);
      
      if (error instanceof InsufficientBalanceError) {
        throw new ApolloError(error.message, 'INSUFFICIENT_BALANCE');
      }
      if (error instanceof ExpiredDiscountError) {
        throw new ApolloError(error.message, 'DISCOUNT_EXPIRED');
      }
      
      throw new ApolloError('Payment processing failed', 'PAYMENT_ERROR', {
        originalError: error.message
      });
    }
  }

  // ----- Pricing Configuration -----

  @Authorized("ADMIN")
  @Mutation(() => PricingStrategy)
  async updatePricingConfig(
    @Arg("input") input: PricingConfigUpdateInput
  ): Promise<PricingStrategy> {
    try {
      await validatePricingUpdate(input);
      return this.solana.updatePricingStrategy(input);
    } catch (error) {
      this.logger.error('Pricing config update failed', error);
      throw new ApolloError('Failed to update pricing', 'PRICING_UPDATE_ERROR');
    }
  }

  @Authorized("ADMIN")
  @Mutation(() => DiscountConfig)
  async createDiscount(
    @Arg("input") input: DiscountApplicationInput
  ): Promise<DiscountConfig> {
    try {
      await validateDiscountConfig(input);
      return this.solana.createDiscountConfig(input);
    } catch (error) {
      this.logger.error('Discount creation failed', error);
      throw new ApolloError('Discount setup failed', 'DISCOUNT_ERROR');
    }
  }

  // ----- Query Operations -----

  @Query(() => CostBreakdown)
  async estimateCost(
    @Arg("input") input: DynamicPricingInput
  ): Promise<CostBreakdown> {
    try {
      return this.calculateDynamicPrice({
        serviceType: input.serviceType,
        modelId: input.modelId,
        resourceUnits: input.resourceUnits,
        urgency: input.urgency
      });
    } catch (error) {
      this.logger.error('Cost estimation failed', error);
      throw new ApolloError('Failed to estimate costs', 'PRICING_ESTIMATE_ERROR');
    }
  }

  @FieldResolver(() => [PaymentHistory])
  async paymentHistory(@Root() plan: PricingPlan): Promise<PaymentHistory[]> {
    try {
      return this.solana.getPaymentHistory(plan.id);
    } catch (error) {
      this.logger.error('Failed to fetch payment history', error);
      return [];
    }
  }

  // ----- Pricing Engine Internals -----

  private async calculateDynamicPrice(params: {
    serviceType: string;
    modelId?: string;
    resourceUnits: number;
    urgency: number;
  }): Promise<CostBreakdown> {
    try {
      const [basePrice, networkConditions, tokenPrice] = await Promise.all([
        this.solana.getBasePrice(params.serviceType),
        fetchNetworkConditions(),
        getTokenExchangeRate()
      ]);

      // Dynamic pricing algorithm
      let price = basePrice * params.resourceUnits;
      price *= networkConditions.congestionMultiplier; 
      price *= (1 + (params.urgency * 0.15));
      price /= tokenPrice;

      // Model-specific adjustments
      if (params.modelId) {
        const modelPremium = await this.solana.getModelPremium(params.modelId);
        price *= (1 + modelPremium);
      }

      return {
        base: basePrice,
        resourceUnits: params.resourceUnits,
        networkFee: networkConditions.baseFee,
        urgencyFee: price * (params.urgency * 0.15),
        tokenConversionRate: tokenPrice,
        total: price
      };
    } catch (error) {
      this.logger.error('Dynamic pricing calculation failed', error);
      throw new InvalidPricingConfigError();
    }
  }

  private async applyDiscounts(
    code: string | undefined,
    wallet: string,
    serviceType: string
  ): Promise<DiscountConfig | null> {
    if (!code) return null;

    const discount = await this.solana.verifyDiscountCode(code, wallet);
    
    if (discount.expiresAt < Date.now()/1000) {
      throw new ExpiredDiscountError();
    }

    if (!discount.applicableServices.includes(serviceType)) {
      throw new ForbiddenError('Discount not valid for this service');
    }

    return discount;
  }

  private applyDiscountToTotal(total: number, discount: DiscountConfig | null): number {
    if (!discount) return total;

    switch(discount.type) {
      case DISCOUNT_TYPE.PERCENTAGE:
        return total * (1 - (discount.value/100));
      case DISCOUNT_TYPE.FIXED:
        return Math.max(0, total - discount.value);
      default:
        return total;
    }
  }

  private async estimateNetworkFees(): Promise<number> {
    const fees = await this.connection.getFeeForMessage(
      await this.connection.getLatestBlockhash()
    );
    return fees.value || 5000; // Fallback to 5000 lamports
  }
}
