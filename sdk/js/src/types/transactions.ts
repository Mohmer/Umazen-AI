//! Umazen Transaction Builder - Core Blockchain Operations
import {
  Connection,
  Keypair,
  PublicKey,
  TransactionInstruction,
  Transaction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  sendAndConfirmTransaction,
} from '@solana/web3.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  createInitializeMintInstruction,
  getAssociatedTokenAddress,
} from '@solana/spl-token';
import {
  Program,
  AnchorProvider,
  BN,
} from '@coral-xyz/anchor';
import { sha256 } from 'js-sha256';
import * as borsh from 'borsh';
import { ModelNFT, TrainingTaskParams, RoyaltyRecipient } from './types/models';
import { IDL } from './idl/umazen';

// Program ID and constants
const PROGRAM_ID = new PublicKey('UMZMxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');
const METADATA_PROGRAM_ID = new PublicKey('metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s');

export class UmazenTransactions {
  private program: Program;
  
  constructor(provider: AnchorProvider) {
    this.program = new Program(IDL, PROGRAM_ID, provider);
  }

  // ===== Core Transaction Builders =====

  /**
   * Create federated learning task transaction
   */
  async createTrainingTask(
    params: TrainingTaskParams,
    creator: PublicKey,
  ): Promise<Transaction> {
    const taskPda = await this.findTaskPda(params.taskId);
    const [modelMetadataPda] = await this.findMetadataPda(taskPda);

    const ix = await this.program.methods
      .initializeTrainingTask({
        taskId: params.taskId,
        modelTemplateCid: params.modelTemplateCid,
        targetAccuracy: new BN(params.targetAccuracy),
        rewardPerEpoch: new BN(params.rewardPerEpoch),
        proofSystem: params.proofSystem,
      })
      .accounts({
        taskAccount: taskPda,
        modelMetadata: modelMetadataPda,
        creator,
        systemProgram: SystemProgram.programId,
      })
      .instruction();

    return new Transaction().add(ix);
  }

  /**
   * Mint AI Model NFT transaction
   */
  async mintModelNft(
    model: ModelNFT,
    payer: PublicKey,
  ): Promise<Transaction> {
    const [mint] = await this.findMintPda(model.metadataUri);
    const metadataPda = await this.findMetadataAccount(mint);
    const tokenAccount = await getAssociatedTokenAddress(mint, payer);

    const metadata = await this.prepareMetadata(model);
    const metadataIx = await this.createMetadataInstruction(
      mint,
      payer,
      metadata,
    );

    const mintIx = await this.program.methods
      .mintModelNft({
        metadataUri: model.metadataUri,
        modelHash: model.modelHash,
        royalties: model.royalties,
        permissions: model.permissions,
      })
      .accounts({
        mint,
        metadata: metadataPda,
        tokenAccount,
        payer,
        tokenProgram: TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .instruction();

    return new Transaction().add(metadataIx, mintIx);
  }

  /**
   * Distribute royalties transaction
   */
  async distributeRoyalties(
    mint: PublicKey,
    amount: BN,
    authority: Keypair,
  ): Promise<Transaction> {
    const [vault] = await this.findRoyaltyVaultPda(mint);
    const metadata = await this.findMetadataAccount(mint);
    const ix = await this.program.methods
      .distributeRoyalties(amount)
      .accounts({
        vault,
        mint,
        metadata,
        authority: authority.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([authority])
      .instruction();

    return new Transaction().add(ix);
  }

  // ===== PDA Derivation Methods =====

  private async findTaskPda(taskId: string): Promise<[PublicKey, number]> {
    return PublicKey.findProgramAddressSync(
      [
        Buffer.from('training_task'),
        Buffer.from(taskId),
      ],
      PROGRAM_ID,
    );
  }

  private async findMintPda(metadataUri: string): Promise<[PublicKey, number]> {
    const hash = sha256(metadataUri);
    return PublicKey.findProgramAddressSync(
      [
        Buffer.from('model_nft'),
        Buffer.from(hash, 'hex'),
      ],
      PROGRAM_ID,
    );
  }

  private async findMetadataAccount(mint: PublicKey): Promise<PublicKey> {
    return PublicKey.findProgramAddressSync(
      [
        Buffer.from('metadata'),
        METADATA_PROGRAM_ID.toBuffer(),
        mint.toBuffer(),
      ],
      METADATA_PROGRAM_ID,
    )[0];
  }

  private async findRoyaltyVaultPda(mint: PublicKey): Promise<[PublicKey, number]> {
    return PublicKey.findProgramAddressSync(
      [
        Buffer.from('royalty_vault'),
        mint.toBuffer(),
      ],
      PROGRAM_ID,
    );
  }

  // ===== Metadata Utilities =====

  private async prepareMetadata(model: ModelNFT): Promise<Buffer> {
    const metadata = {
      name: `AI Model - ${model.metadataUri.slice(0, 8)}`,
      symbol: 'UMZM',
      uri: model.metadataUri,
      sellerFeeBasisPoints: model.royalties.reduce((sum, r) => sum + r.basisPoints, 0),
      creators: model.royalties.map(r => ({
        address: r.address,
        verified: false,
        share: r.basisPoints,
      })),
    };
    return Buffer.from(JSON.stringify(metadata));
  }

  private async createMetadataInstruction(
    mint: PublicKey,
    payer: PublicKey,
    metadata: Buffer,
  ): Promise<TransactionInstruction> {
    const metadataAccount = await this.findMetadataAccount(mint);
    const keys = [
      { pubkey: metadataAccount, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: false },
      { pubkey: METADATA_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ];
    return new TransactionInstruction({
      keys,
      programId: METADATA_PROGRAM_ID,
      data: metadata,
    });
  }

  // ===== Transaction Execution =====

  async signAndSend(
    tx: Transaction,
    signers: Keypair[],
  ): Promise<string> {
    const { connection, wallet } = this.program.provider as AnchorProvider;
    tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tx.feePayer = wallet.publicKey;
    return await sendAndConfirmTransaction(connection, tx, signers);
  }
}

// Example Usage:
/*
const provider = new AnchorProvider(connection, wallet, {});
const client = new UmazenTransactions(provider);

// Create training task
const taskTx = await client.createTrainingTask(
  {
    taskId: 'unique_task_id',
    modelTemplateCid: 'Qm...',
    targetAccuracy: 95.5,
    rewardPerEpoch: 1000000,
    proofSystem: 'groth16',
  },
  wallet.publicKey,
);
await client.signAndSend(taskTx, [wallet.payer]);

// Mint model NFT
const model: ModelNFT = { ... };
const mintTx = await client.mintModelNft(model, wallet.publicKey);
await client.signAndSend(mintTx, [wallet.payer]);

// Distribute royalties
const mintKey = new PublicKey('...');
const distributeTx = await client.distributeRoyalties(
  mintKey,
  new BN(1000000),
  authorityKeypair,
);
await client.signAndSend(distributeTx, [authorityKeypair]);
*/
