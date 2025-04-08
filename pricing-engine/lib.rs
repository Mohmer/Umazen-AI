//! Umazen Pricing Engine - Decentralized AI Resource Pricing Mechanism

#![deny(
    unsafe_code,
    missing_docs,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};
use fixed::types::I80F48;
use std::collections::HashMap;

declare_id!("PRI5v5eHGRxUQZc5sJzsLHj5V7uQ3VZKt9J7WY4W7Zq");

/// Pricing engine state account
#[account]
#[derive(Default)]
pub struct PricingEngine {
    pub authority: Pubkey,
    pub fee_receiver: Pubkey,
    pub config: PricingConfig,
    pub market_conditions: MarketConditions,
    pub historical_data: HistoricalPriceData,
    pub active_models: u64,
    pub last_update_ts: i64,
    pub bump: u8,
}

/// Dynamic pricing configuration
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct PricingConfig {
    pub base_fee: u64,                     // Base fee in USDC
    pub compute_unit_price: I80F48,        // Price per compute unit
    pub storage_price_per_slot: I80F48,    // Storage price per slot
    pub dynamic_fee_multiplier: I80F48,    // Market-based multiplier
    pub stability_factor: I80F48,          // Anti-volatility factor
    pub min_fee: u64,                      // Minimum fee floor
    pub max_fee: u64,                      // Maximum fee ceiling
    pub decay_factor: I80F48,              // Price decay over time
    pub incentive_params: IncentiveParams, // Training incentives
}

/// Market condition parameters
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct MarketConditions {
    pub network_congestion: I80F48,        // 0-1.0 scale
    pub resource_utilization: I80F48,      // 0-1.0 scale
    pub token_price: I80F48,              // USDC price in USD
    pub stake_concentration: I80F48,      // 0-1.0 scale
    pub current_epoch: u64,               // Solana epoch
}

/// Historical pricing data for algorithmic adjustments
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct HistoricalPriceData {
    pub moving_average_24h: I80F48,
    pub volatility_index: I80F48,
    pub last_peak_price: I80F48,
    pub last_trough_price: I80F48,
    pub correlation_matrix: [I80F48; 5], // Market factor correlations
}

/// Training incentive parameters
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct IncentiveParams {
    pub accuracy_bonus: I80F48,         // Reward for model accuracy
    pub data_quality_multiplier: I80F48,
    pub early_adopter_discount: I80F48,
    pub staking_discount: I80F48,
    pub reputation_multiplier: I80F48,
}

/// Pricing engine instructions
#[program]
pub mod pricing_engine {
    use super::*;

    /// Initialize pricing engine state
    pub fn initialize(ctx: Context<Initialize>, 
                     config: PricingConfig,
                     bump: u8) -> Result<()> {
        let engine = &mut ctx.accounts.pricing_engine;
        engine.authority = *ctx.accounts.authority.key;
        engine.fee_receiver = *ctx.accounts.fee_receiver.key;
        engine.config = config;
        engine.bump = bump;
        engine.last_update_ts = Clock::get()?.unix_timestamp;
        Ok(())
    }

    /// Update dynamic pricing parameters
    pub fn update_pricing(ctx: Context<UpdatePricing>,
                         new_config: PricingConfig) -> Result<()> {
        let engine = &mut ctx.accounts.pricing_engine;
        engine.config = new_config;
        engine.last_update_ts = Clock::get()?.unix_timestamp;
        Ok(())
    }

    /// Calculate price for AI resource usage
    pub fn calculate_price(ctx: Context<CalculatePrice>,
                          params: ResourceParams) -> Result<PriceQuote> {
        let engine = &ctx.accounts.pricing_engine;
        let clock = Clock::get()?;
        
        // Base fee calculation
        let mut price = I80F48::from_num(engine.config.base_fee);
        
        // Compute costs
        price += engine.config.compute_unit_price 
               * I80F48::from(params.compute_units);
        
        // Storage costs
        price += engine.config.storage_price_per_slot 
               * I80F48::from(params.storage_slots);
        
        // Market dynamics
        price *= engine.config.dynamic_fee_multiplier 
               * (I80F48::ONE + engine.market_conditions.network_congestion);
        
        // Stability adjustment
        price *= I80F48::ONE 
               + (engine.config.stability_factor 
                  * engine.historical_data.volatility_index);
        
        // Time-based decay
        let time_decay = I80F48::ONE 
                        - (engine.config.decay_factor 
                           * I80F48::from(clock.unix_timestamp 
                                        - engine.last_update_ts));
        price *= time_decay.max(I80F48::from_num(0.8));
        
        // Apply incentives
        price *= I80F48::ONE 
               - (params.incentives.accuracy_bonus 
                  * engine.config.incentive_params.accuracy_bonus)
               - (params.incentives.staking_discount 
                  * engine.config.incentive_params.staking_discount);
        
        // Enforce min/max bounds
        let final_price = price
            .max(I80F48::from_num(engine.config.min_fee))
            .min(I80F48::from_num(engine.config.max_fee))
            .ceil()
            .to_num::<u64>();
        
        Ok(PriceQuote {
            total: final_price,
            breakdown: PriceBreakdown {
                base_fee: engine.config.base_fee,
                compute_cost: (engine.config.compute_unit_price 
                              * I80F48::from(params.compute_units)).to_num(),
                storage_cost: (engine.config.storage_price_per_slot 
                             * I80F48::from(params.storage_slots)).to_num(),
                market_fee: (price - I80F48::from_num(engine.config.base_fee)).to_num(),
                incentives: (-price 
                            * (params.incentives.accuracy_bonus 
                               * engine.config.incentive_params.accuracy_bonus)).to_num(),
            },
            valid_until: clock.unix_timestamp + 300, // 5 minute validity
        })
    }

    /// Execute payment for resource usage
    pub fn execute_payment(ctx: Context<ExecutePayment>,
                          quote: PriceQuote) -> Result<()> {
        let engine = &ctx.accounts.pricing_engine;
        let clock = Clock::get()?;
        
        // Validate quote expiration
        require!(clock.unix_timestamp < quote.valid_until,
                ErrorCode::ExpiredQuote);
        
        // Transfer funds
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.payer_token.to_account_info(),
                    to: ctx.accounts.fee_receiver_token.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            ),
            quote.total,
        )?;
        
        emit!(PaymentExecuted {
            payer: *ctx.accounts.payer.key,
            amount: quote.total,
            timestamp: clock.unix_timestamp,
        });
        
        Ok(())
    }
}

/// Resource parameters for price calculation
#[derive(AnchorSerialize, AnchorDeserialize, Default)]
pub struct ResourceParams {
    pub compute_units: u64,
    pub storage_slots: u64,
    pub incentives: ResourceIncentives,
}

/// Price incentive qualifications
#[derive(AnchorSerialize, AnchorDeserialize, Default)]
pub struct ResourceIncentives {
    pub accuracy_bonus: I80F48,
    pub data_quality: I80F48,
    pub early_adopter: bool,
    pub staked_tokens: u64,
    pub reputation_score: I80F48,
}

/// Detailed price quote
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct PriceQuote {
    pub total: u64,
    pub breakdown: PriceBreakdown,
    pub valid_until: i64,
}

/// Price component breakdown
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct PriceBreakdown {
    pub base_fee: u64,
    pub compute_cost: u64,
    pub storage_cost: u64,
    pub market_fee: i64,
    pub incentives: i64,
}

/// Accounts for initialization
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + PricingEngine::LEN)]
    pub pricing_engine: Account<'info, PricingEngine>,
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Validated in constraint
    #[account(constraint = fee_receiver.data_is_empty())]
    pub fee_receiver: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

/// Accounts for price updates
#[derive(Accounts)]
pub struct UpdatePricing<'info> {
    #[account(mut, has_one = authority)]
    pub pricing_engine: Account<'info, PricingEngine>,
    pub authority: Signer<'info>,
}

/// Accounts for price calculation
#[derive(Accounts)]
pub struct CalculatePrice<'info> {
    pub pricing_engine: Account<'info, PricingEngine>,
}

/// Accounts for payment execution
#[derive(Accounts)]
pub struct ExecutePayment<'info> {
    #[account(mut)]
    pub pricing_engine: Account<'info, PricingEngine>,
    #[account(mut)]
    pub payer_token: Account<'info, TokenAccount>,
    #[account(mut)]
    pub fee_receiver_token: Account<'info, TokenAccount>,
    pub payer: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

/// Events
#[event]
pub struct PaymentExecuted {
    pub payer: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Price quote has expired")]
    ExpiredQuote,
    #[msg("Invalid pricing parameters")]
    InvalidParameters,
    #[msg("Insufficient funds for payment")]
    InsufficientFunds,
    #[msg("Arithmetic overflow in price calculation")]
    ArithmeticOverflow,
}

/// Constant space requirements
impl PricingEngine {
    const LEN: usize = 32 + 32 + 512 + 128 + 256 + 8 + 8 + 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::test::*;
    use fixed::macros::*;

    #[test]
    fn test_price_calculation() {
        let mut engine = PricingEngine::default();
        engine.config.base_fee = 100_000; // 0.1 USDC
        engine.config.compute_unit_price = fixed!(0.0001: I80F48); 
        engine.config.storage_price_per_slot = fixed!(0.001: I80F48);
        engine.config.dynamic_fee_multiplier = fixed!(1.2: I80F48);
        engine.market_conditions.network_congestion = fixed!(0.3: I80F48);
        
        let params = ResourceParams {
            compute_units: 1_000_000,
            storage_slots: 500,
            incentives: ResourceIncentives {
                accuracy_bonus: fixed!(0.1: I80F48),
                ..Default::default()
            },
        };
        
        let quote = calculate_price(engine, params).unwrap();
        assert_eq!(quote.total, 159_600); // Verify complex calculation
    }
}
