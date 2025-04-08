//! Umazen NFT Client - AI Model Ownership Management Layer
import {
  type AccountMeta,
  type Program,
  type Provider,
  type web3,
  BN,
} from '@coral-xyz/anchor';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  getAssociatedTokenAddressSync,
} from '@solana/spl-token';
import { BaseClient } from './BaseClient';
import { type ModelMetadata, type RoyaltyConfig } from './types';
import { assert, handleError, retry } from './utils';

/**
 * Configuration for NFT client initialization
 */
export interface NFTClientConfig {
  metadataProgramId?: web3.PublicKey;
  defaultCommitment?: web3.Commitment;
}

/**
 * Advanced NFT management client with royalty enforcement
 */
export class NFTClient extends BaseClient {
  private readonly _metadataProgramId: web3.PublicKey;

  constructor(baseClient: BaseClient, config?: NFTClientConfig) {
    super({
      rpcEndpoint: baseClient.connection.rpcEndpoint,
      wallet: baseClient.wallet,
      commitment: config?.defaultCommitment,
      programId: baseClient.program.programId,
    });
    this._metadataProgramId =
      config?.metadataProgramId ?? DEFAULT_METADATA_PROGRAM_ID;
  }

  //#region Core Operations
  /**
   * Transfer AI Model NFT with royalty enforcement
   */
  async transferModelNFT(
    mint: web3.PublicKey,
    recipient: web3.PublicKey,
    options?: {
      createAssociatedTokenAccount?: boolean;
      royaltyBasisPoints?: number;
    }
  ): Promise<web3.TransactionSignature> {
    assert(this.wallet, 'Wallet not connected');

    const sourceAccount = getAssociatedTokenAddressSync(
      mint,
      this.wallet.publicKey,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    const destAccount = getAssociatedTokenAddressSync(
      mint,
      recipient,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    const metadata = await this.getMetadata(mint);
    const royaltyBasisPoints =
      options?.royaltyBasisPoints ?? metadata.royaltyBasisPoints;

    const additionalKeys: AccountMeta[] = [];
    if (options?.createAssociatedTokenAccount) {
      additionalKeys.push({
        pubkey: destAccount,
        isWritable: true,
        isSigner: false,
      });
      additionalKeys.push({
        pubkey: recipient,
        isWritable: false,
        isSigner: true,
      });
    }

    return retry(async () => {
      return this.program.methods
        .transferModelNft(new BN(royaltyBasisPoints))
        .accounts({
          source: sourceAccount,
          destination: destAccount,
          mint,
          owner: this.wallet!.publicKey,
          metadata: metadata.metadataAccount,
          metadataProgram: this._metadataProgramId,
        })
        .remainingAccounts(additionalKeys)
        .rpc();
    });
  }

  /**
   * Update model metadata (requires update authority)
   */
  async updateModelMetadata(
    mint: web3.PublicKey,
    newMetadata: Partial<ModelMetadata>
  ): Promise<web3.TransactionSignature> {
    const metadata = await this.getMetadata(mint);
    assert(
      metadata.updateAuthority.equals(this.wallet!.publicKey),
      'Caller is not update authority'
    );

    const metadataAccount = await this.program.account.modelMetadata.fetch(
      metadata.metadataAccount
    );

    return retry(async () => {
      return this.program.methods
        .updateModelMetadata({
          modelHash: newMetadata.modelHash ?? metadataAccount.modelHash,
          royaltyBasisPoints:
            newMetadata.royaltyBasisPoints ?? metadataAccount.royaltyBasisPoints,
          licenseType: newMetadata.licenseType ?? metadataAccount.licenseType,
        })
        .accounts({
          metadata: metadata.metadataAccount,
          updateAuthority: this.wallet!.publicKey,
        })
        .rpc();
    });
  }
  //#endregion

  //#region Query Operations
  /**
   * Get full metadata for a model NFT
   */
  async getMetadata(mint: web3.PublicKey): Promise<ModelMetadata> {
    const [metadataAccount] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('metadata'),
        this._metadataProgramId.toBuffer(),
        mint.toBuffer(),
      ],
      this.program.programId
    );

    const metadata = await this.program.account.modelMetadata.fetch(
      metadataAccount
    );
    return {
      mint,
      metadataAccount,
      modelHash: metadata.modelHash,
      createdAt: metadata.createdAt.toNumber(),
      royaltyBasisPoints: metadata.royaltyBasisPoints,
      updateAuthority: metadata.updateAuthority,
      licenseType: metadata.licenseType,
    };
  }

  /**
   * Check if wallet is authorized to use the model
   */
  async checkAuthorization(
    mint: web3.PublicKey,
    user: web3.PublicKey
  ): Promise<boolean> {
    const tokenAccount = getAssociatedTokenAddressSync(
      mint,
      user,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    try {
      const accountInfo = await this.connection.getAccountInfo(tokenAccount);
      return accountInfo?.data.readUInt8(64) === 1; // Check token balance > 0
    } catch {
      return false;
    }
  }
  //#endregion

  //#region Royalty Management
  /**
   * Configure royalty distribution parameters
   */
  async configureRoyalty(
    mint: web3.PublicKey,
    config: RoyaltyConfig
  ): Promise<web3.TransactionSignature> {
    const metadata = await this.getMetadata(mint);
    assert(
      metadata.updateAuthority.equals(this.wallet!.publicKey),
      'Caller is not royalty authority'
    );

    return retry(async () => {
      return this.program.methods
        .setRoyaltyParameters(
          new BN(config.basisPoints),
          config.recipients.map(r => ({
            address: r.address,
            share: r.share,
          }))
        )
        .accounts({
          metadata: metadata.metadataAccount,
          authority: this.wallet!.publicKey,
        })
        .rpc();
    });
  }
  //#endregion
}

//#region Type Definitions
export interface ModelMetadata {
  mint: web3.PublicKey;
  metadataAccount: web3.PublicKey;
  modelHash: string;
  createdAt: number;
  royaltyBasisPoints: number;
  updateAuthority: web3.PublicKey;
  licenseType: 'personal' | 'commercial' | 'enterprise';
}

export interface RoyaltyConfig {
  basisPoints: number;
  recipients: {
    address: web3.PublicKey;
    share: number;
  }[];
}
//#endregion

//#region Constants
const DEFAULT_METADATA_PROGRAM_ID = new web3.PublicKey(
  'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s'
);
//#endregion
