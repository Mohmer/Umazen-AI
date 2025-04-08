//! Umazen Training Client - Federated Learning Orchestration Layer
import {
  type AccountMeta,
  type BN,
  type Program,
  type Provider,
  type web3,
} from '@coral-xyz/anchor';
import { BaseClient } from './BaseClient';
import {
  type ComputeNodeConfig,
  type FederatedLearningTask,
  type GradientUpdate,
  type TrainingParams,
  type ZKProof,
} from './types';
import { assert, handleError, retry, validateIpfsHash } from './utils';
import { sha256 } from '@noble/hashes/sha256';
import { blake3 } from '@noble/hashes/blake3';

/**
 * Configuration for training client initialization
 */
export interface TrainingClientConfig {
  maxRetries?: number;
  taskPollInterval?: number;
  defaultComputeRequirements?: ComputeNodeConfig;
}

/**
 * Advanced federated learning coordinator with ZK validation
 */
export class TrainingClient extends BaseClient {
  private readonly _maxRetries: number;
  private readonly _taskPollInterval: number;
  private readonly _defaultComputeRequirements: ComputeNodeConfig;

  constructor(baseClient: BaseClient, config?: TrainingClientConfig) {
    super({
      rpcEndpoint: baseClient.connection.rpcEndpoint,
      wallet: baseClient.wallet,
      commitment: baseClient.commitment,
      programId: baseClient.program.programId,
    });
    this._maxRetries = config?.maxRetries ?? 3;
    this._taskPollInterval = config?.taskPollInterval ?? 15_000; // 15 seconds
    this._defaultComputeRequirements = config?.defaultComputeRequirements ?? {
      minVram: 16,
      minRam: 64,
      supportedHardware: ['cuda11+', 'rocm5+'],
    };
  }

  //#region Core Operations
  /**
   * Initialize a new federated learning task
   */
  async createTrainingTask(
    params: TrainingParams
  ): Promise<web3.TransactionSignature> {
    assert(this.wallet, 'Wallet not connected');
    validateIpfsHash(params.modelTemplateCid);

    const [taskAccount] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('task'),
        this.wallet.publicKey.toBuffer(),
        Buffer.from(params.modelTemplateCid),
      ],
      this.program.programId
    );

    return retry(
      async () => {
        return this.program.methods
          .initializeTrainingTask({
            modelTemplateCid: params.modelTemplateCid,
            maxNodes: params.maxNodes,
            minBatchSize: params.minBatchSize,
            targetAccuracy: params.targetAccuracy,
            rewardPerEpoch: new BN(params.rewardPerEpoch),
            stakeRequired: new BN(params.stakeRequired),
            proofSystem: params.proofSystem ?? 'plonk',
          })
          .accounts({
            taskAccount,
            authority: this.wallet!.publicKey,
            systemProgram: web3.SystemProgram.programId,
          })
          .rpc();
      },
      this._maxRetries,
      this._taskPollInterval
    );
  }

  /**
   * Register compute node for participation
   */
  async registerComputeNode(
    taskId: web3.PublicKey,
    config?: Partial<ComputeNodeConfig>
  ): Promise<web3.TransactionSignature> {
    assert(this.wallet, 'Wallet not connected');

    const [nodeAccount] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('node'),
        taskId.toBuffer(),
        this.wallet.publicKey.toBuffer(),
      ],
      this.program.programId
    );

    const mergedConfig: ComputeNodeConfig = {
      ...this._defaultComputeRequirements,
      ...config,
    };

    return retry(
      async () => {
        return this.program.methods
          .registerNode({
            vram: mergedConfig.minVram,
            ram: mergedConfig.minRam,
            supportedHardware: mergedConfig.supportedHardware,
          })
          .accounts({
            nodeAccount,
            taskAccount: taskId,
            nodeOwner: this.wallet!.publicKey,
            systemProgram: web3.SystemProgram.programId,
          })
          .rpc();
      },
      this._maxRetries,
      this._taskPollInterval
    );
  }

  /**
   * Submit gradient update with ZK proof
   */
  async submitGradientUpdate(
    taskId: web3.PublicKey,
    update: GradientUpdate,
    proof: ZKProof
  ): Promise<web3.TransactionSignature> {
    assert(this.wallet, 'Wallet not connected');

    const [nodeAccount] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('node'),
        taskId.toBuffer(),
        this.wallet.publicKey.toBuffer(),
      ],
      this.program.programId
    );

    const gradientHash = Buffer.from(
      blake3(update.encryptedGradients)
    ).toString('hex');

    return retry(
      async () => {
        return this.program.methods
          .submitGradient({
            epoch: new BN(update.epoch),
            gradientHash,
            proof: {
              a: proof.a,
              b: proof.b,
              c: proof.c,
              publicInputs: proof.publicInputs,
            },
          })
          .accounts({
            taskAccount: taskId,
            nodeAccount,
            nodeOwner: this.wallet!.publicKey,
          })
          .remainingAccounts(
            update.dataProviders.map(p => ({
              pubkey: p,
              isWritable: false,
              isSigner: false,
            }))
          )
          .rpc();
      },
      this._maxRetries,
      this._taskPollInterval
    );
  }
  //#endregion

  //#region Query Operations
  /**
   * Get current state of a training task
   */
  async getTrainingTask(
    taskId: web3.PublicKey
  ): Promise<FederatedLearningTask> {
    const task = await this.program.account.federatedLearningTask.fetch(taskId);
    return {
      ...task,
      modelTemplateCid: task.modelTemplateCid,
      currentEpoch: task.currentEpoch.toNumber(),
      rewardPerEpoch: task.rewardPerEpoch.toNumber(),
      stakeRequired: task.stakeRequired.toNumber(),
    };
  }

  /**
   * Check node participation status
   */
  async getNodeStatus(
    taskId: web3.PublicKey,
    node: web3.PublicKey
  ): Promise<'active' | 'pending' | 'slashed'> {
    const [nodeAccount] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from('node'), taskId.toBuffer(), node.toBuffer()],
      this.program.programId
    );

    try {
      const state = await this.program.account.computeNode.fetch(nodeAccount);
      return state.status.active ? 'active' : 'pending';
    } catch {
      return 'slashed';
    }
  }
  //#endregion

  //#region Reward Management
  /**
   * Claim training rewards for completed epochs
   */
  async claimTrainingRewards(
    taskId: web3.PublicKey
  ): Promise<web3.TransactionSignature> {
    assert(this.wallet, 'Wallet not connected');

    const [nodeAccount] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('node'),
        taskId.toBuffer(),
        this.wallet.publicKey.toBuffer(),
      ],
      this.program.programId
    );

    return retry(
      async () => {
        return this.program.methods
          .claimRewards()
          .accounts({
            taskAccount: taskId,
            nodeAccount,
            nodeOwner: this.wallet!.publicKey,
          })
          .rpc();
      },
      this._maxRetries,
      this._taskPollInterval
    );
  }
  //#endregion

  //#region Verification
  /**
   * Validate ZK proof for gradient update
   */
  private validateGradientProof(
    proof: ZKProof,
    publicInputs: string[]
  ): boolean {
    // Implementation would vary based on proof system
    // Placeholder for actual verification logic
    return sha256(JSON.stringify(proof)) === publicInputs[0];
  }
  //#endregion
}

//#region Type Definitions
export interface TrainingParams {
  modelTemplateCid: string;
  maxNodes: number;
  minBatchSize: number;
  targetAccuracy: number;
  rewardPerEpoch: number;
  stakeRequired: number;
  proofSystem?: 'plonk' | 'groth16' | 'marlin';
}

export interface GradientUpdate {
  epoch: number;
  encryptedGradients: Uint8Array;
  dataProviders: web3.PublicKey[];
}

export interface ZKProof {
  a: [string, string];
  b: [[string, string], [string, string]];
  c: [string, string];
  publicInputs: string[];
}
//#endregion
