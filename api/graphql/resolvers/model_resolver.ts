// src/graphql/resolvers/model_resolver.ts

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
import { SolanaService } from '../../services/solana';
import { ModelService } from '../../services/model';
import { AuthService } from '../../services/auth';
import { Logger } from '../../utils/logger';
import {
  ModelMetadata,
  ModelUploadResponse,
  TrainingTask,
  TrainingParams,
  ModelVersion,
  ModelPermission,
  ModelSearchParams,
  PaginatedModels
} from '../schemas/model';
import { Context } from '../../types/context';
import { 
  ModelUploadInput,
  TrainingStartInput,
  ModelUpdateInput,
  PermissionUpdateInput
} from '../inputs/model';
import { 
  validateModelMetadata,
  validateTrainingParams,
  validatePermissionUpdate 
} from '../validators/model';
import { 
  ModelNotFoundError,
  TrainingConfigError,
  StorageQuotaExceededError,
  InsufficientFundsError
} from '../../errors/models';
import { 
  ModelStorage,
  TrainingOrchestrator,
  ModelCompression
} from '../../core/model';
import { 
  ModelPrivacyLevel,
  TrainingTaskStatus,
  ModelFramework,
  CompressionAlgorithm
} from '../../types/enums';
import { 
  FileUpload,
  GraphQLUpload
} from 'graphql-upload';
import { 
  createSignature,
  verifyWalletSignature
} from '../../crypto/signatures';

@Resolver(() => ModelMetadata)
export class ModelResolver implements ResolverInterface<ModelMetadata> {
  private readonly logger = new Logger('ModelResolver');
  private readonly solana = new SolanaService();
  private readonly modelService = new ModelService();
  private readonly auth = new AuthService();

  // ----- Query Resolvers -----
  
  @Query(() => ModelMetadata)
  async getModelMetadata(
    @Arg("modelId") modelId: string,
    @Ctx() { user }: Context
  ): Promise<ModelMetadata> {
    try {
      const model = await this.modelService.getModelMetadata(modelId);
      
      if (!this.auth.checkModelAccess(user, model)) {
        throw new ForbiddenError('Unauthorized model access');
      }

      return this.modelService.sanitizeMetadata(model);
    } catch (error) {
      this.logger.error(`Failed to fetch model ${modelId}`, error);
      
      if (error instanceof ModelNotFoundError) {
        throw new ApolloError(error.message, 'MODEL_NOT_FOUND');
      }
      
      throw new ApolloError('Failed to retrieve model metadata', 'MODEL_FETCH_ERROR');
    }
  }

  @Query(() => PaginatedModels)
  async searchModels(
    @Arg("params") params: ModelSearchParams,
    @Ctx() { user }: Context
  ): Promise<PaginatedModels> {
    try {
      const result = await this.modelService.searchModels(params, user);
      return {
        models: result.models.map(m => this.modelService.sanitizeMetadata(m)),
        totalCount: result.totalCount,
        pageInfo: result.pageInfo
      };
    } catch (error) {
      this.logger.error('Model search failed', error);
      throw new ApolloError('Failed to search models', 'MODEL_SEARCH_ERROR');
    }
  }

  // ----- Mutation Resolvers -----

  @Authorized()
  @Mutation(() => ModelUploadResponse)
  async uploadModel(
    @Arg("input") input: ModelUploadInput,
    @Arg("file", () => GraphQLUpload) file: FileUpload,
    @Ctx() { user, ip }: Context
  ): Promise<ModelUploadResponse> {
    try {
      // Validate input
      await validateModelMetadata(input.metadata);
      
      // Verify wallet ownership
      const isValidSig = verifyWalletSignature(
        input.walletAddress,
        input.signature,
        input.nonce
      );
      
      if (!isValidSig) {
        throw new ForbiddenError('Invalid wallet signature');
      }

      // Check storage quota
      const quota = await this.modelService.checkStorageQuota(user.id);
      if (file.size > quota.remaining) {
        throw new StorageQuotaExceededError();
      }

      // Process model upload
      const uploadResult = await ModelStorage.uploadModel(file, {
        encryptionKey: input.encryptionKey,
        compress: input.compress || CompressionAlgorithm.ZSTD,
        framework: input.metadata.framework
      });

      // Create Solana transaction
      const tx = await this.solana.createModelUploadTx({
        modelHash: uploadResult.hash,
        owner: input.walletAddress,
        storageFee: quota.rate,
        metadata: input.metadata
      });

      // Store metadata
      const model = await this.modelService.createModelMetadata({
        ...input.metadata,
        ownerId: user.id,
        ipfsCID: uploadResult.cid,
        solanaTx: tx.transactionSignature,
        storageCost: uploadResult.storageCost,
        privacyLevel: input.privacy || ModelPrivacyLevel.PRIVATE
      });

      return {
        success: true,
        modelId: model.id,
        transaction: tx,
        storageCost: uploadResult.storageCost
      };
    } catch (error) {
      this.logger.error(`Model upload failed from ${ip}`, error);
      
      if (error instanceof StorageQuotaExceededError) {
        throw new ApolloError(error.message, 'STORAGE_QUOTA_EXCEEDED');
      }
      
      throw new ApolloError('Model upload failed', 'MODEL_UPLOAD_ERROR', {
        originalError: error.message
      });
    }
  }

  @Authorized()
  @Mutation(() => TrainingTask)
  async startTrainingTask(
    @Arg("input") input: TrainingStartInput,
    @Ctx() { user }: Context
  ): Promise<TrainingTask> {
    try {
      // Validate training parameters
      await validateTrainingParams(input.params);

      // Verify model ownership
      const model = await this.modelService.getModelMetadata(input.modelId);
      if (model.ownerId !== user.id) {
        throw new ForbiddenError('Not model owner');
      }

      // Check training deposit
      const balance = await this.solana.getTokenBalance(
        user.walletAddress,
        process.env.TRAINING_TOKEN_MINT
      );
      
      if (balance < input.params.deposit) {
        throw new InsufficientFundsError();
      }

      // Initialize training task
      const task = await TrainingOrchestrator.initializeTask({
        modelId: input.modelId,
        params: input.params,
        wallet: user.walletAddress,
        deposit: input.params.deposit
      });

      // Create on-chain commitment
      const tx = await this.solana.createTrainingCommitmentTx({
        taskId: task.id,
        modelHash: model.ipfsCID,
        deposit: input.params.deposit,
        wallet: user.walletAddress
      });

      return {
        ...task,
        transaction: tx
      };
    } catch (error) {
      this.logger.error(`Training start failed for model ${input.modelId}`, error);
      
      if (error instanceof InsufficientFundsError) {
        throw new ApolloError(error.message, 'INSUFFICIENT_FUNDS');
      }
      
      throw new ApolloError('Failed to start training', 'TRAINING_START_ERROR');
    }
  }

  // ----- Field Resolvers -----

  @FieldResolver(() => [ModelVersion])
  async versions(@Root() model: ModelMetadata): Promise<ModelVersion[]> {
    try {
      return this.modelService.getModelVersions(model.id);
    } catch (error) {
      this.logger.error(`Failed to fetch versions for model ${model.id}`, error);
      return [];
    }
  }

  @FieldResolver(() => [TrainingTask])
  async trainingHistory(
    @Root() model: ModelMetadata,
    @Arg("status", { nullable: true }) status?: TrainingTaskStatus
  ): Promise<TrainingTask[]> {
    try {
      return this.modelService.getTrainingHistory(model.id, status);
    } catch (error) {
      this.logger.error(`Failed to fetch training history for ${model.id}`, error);
      return [];
    }
  }

  @FieldResolver(() => ModelPermission)
  async permissions(@Root() model: ModelMetadata): Promise<ModelPermission> {
    try {
      return this.modelService.getModelPermissions(model.id);
    } catch (error) {
      this.logger.error(`Failed to fetch permissions for ${model.id}`, error);
      return { read: [], write: [], train: [] };
    }
  }

  // ----- Admin Operations -----

  @Authorized("ADMIN")
  @Mutation(() => Boolean)
  async updateModelLicense(
    @Arg("modelId") modelId: string,
    @Arg("license") license: string
  ): Promise<boolean> {
    try {
      await this.modelService.updateLicense(modelId, license);
      return true;
    } catch (error) {
      this.logger.error(`License update failed for ${modelId}`, error);
      throw new ApolloError('Failed to update license', 'LICENSE_UPDATE_ERROR');
    }
  }

  @Authorized("ADMIN")
  @Mutation(() => ModelPermission)
  async updateModelPermissions(
    @Arg("input") input: PermissionUpdateInput
  ): Promise<ModelPermission> {
    try {
      await validatePermissionUpdate(input);
      return this.modelService.updatePermissions(input);
    } catch (error) {
      this.logger.error(`Permission update failed for ${input.modelId}`, error);
      throw new ApolloError('Failed to update permissions', 'PERMISSION_UPDATE_ERROR');
    }
  }

  // ----- Utility Methods -----

  private async handleModelCompression(
    file: FileUpload,
    algorithm: CompressionAlgorithm
  ): Promise<Buffer> {
    try {
      const { createReadStream } = await file;
      const stream = createReadStream();
      return ModelCompression.compress(stream, algorithm);
    } catch (error) {
      this.logger.error('Model compression failed', error);
      throw new ApolloError('Model compression failed', 'COMPRESSION_ERROR');
    }
  }
}
