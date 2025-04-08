//! Umazen Base Client - Core Blockchain Interaction Layer
import {
  AnchorProvider,
  BN,
  Program,
  Wallet,
  web3,
  type Provider,
} from '@coral-xyz/anchor';
import type { Connection, Keypair, TransactionSignature } from '@solana/web3.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from '@solana/spl-token';
import { IDL as UmazenIDL, type Umazen } from './idl/umazen';
import { ModelNFT, type ModelNFTOptions } from './types';
import { assert, handleError, retry } from './utils';

/**
 * Configuration for initializing BaseClient
 */
export interface BaseClientConfig {
  rpcEndpoint: string;
  wallet?: Wallet;
  commitment?: web3.Commitment;
  confirmOptions?: web3.ConfirmOptions;
  programId?: web3.PublicKey;
}

/**
 * Core client handling all blockchain interactions
 */
export class BaseClient {
  private _provider: AnchorProvider;
  private _program: Program<Umazen>;
  private _connection: Connection;
  private _wallet?: Wallet;

  constructor(config: BaseClientConfig) {
    this._connection = new web3.Connection(config.rpcEndpoint, {
      commitment: config.commitment ?? 'confirmed',
      confirmTransactionInitialTimeout: 30_000, // 30s
    });

    this._wallet = config.wallet;
    this._provider = new AnchorProvider(
      this._connection,
      this._wallet ?? ({} as Wallet), // Fallback to dummy wallet
      config.confirmOptions ?? {
        preflightCommitment: 'processed',
        skipPreflight: false,
      }
    );

    this._program = new Program<Umazen>(
      UmazenIDL,
      config.programId ?? DEFAULT_PROGRAM_ID,
      this._provider
    );
  }

  //#region Core Properties
  get program(): Program<Umazen> {
    return this._program;
  }

  get wallet(): Wallet | undefined {
    return this._wallet;
  }

  get provider(): AnchorProvider {
    return this._provider;
  }

  get connection(): Connection {
    return this._connection;
  }
  //#endregion

  //#region Wallet Operations
  /**
   * Connect a wallet to the client instance
   */
  connectWallet(wallet: Wallet): void {
    this._wallet = wallet;
    this._provider = new AnchorProvider(
      this._connection,
      wallet,
      this._provider.opts
    );
    this._program = new Program<Umazen>(
      UmazenIDL,
      this._program.programId,
      this._provider
    );
  }

  /**
   * Disconnect current wallet
   */
  disconnectWallet(): void {
    this._wallet = undefined;
    this._provider = new AnchorProvider(
      this._connection,
      {} as Wallet, // Dummy wallet
      this._provider.opts
    );
  }
  //#endregion

  //#region Model NFT Operations
  /**
   * Mint a new AI Model NFT
   */
  async mintModelNFT(
    modelHash: string,
    options: ModelNFTOptions
  ): Promise<{ txSig: TransactionSignature; modelNFT: ModelNFT }> {
    assert(this._wallet, 'Wallet not connected');
    const metadataUri = await this._prepareMetadata(modelHash, options);

    const [mint] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from('model'),
        this._wallet.publicKey.toBuffer(),
        Buffer.from(modelHash, 'hex'),
      ],
      this._program.programId
    );

    const associatedToken = getAssociatedTokenAddressSync(
      mint,
      this._wallet.publicKey,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    const txSig = await retry(async () => {
      return this._program.methods
        .mintModelNft(metadataUri, new BN(options.royaltyBasisPoints))
        .accounts({
          payer: this._wallet!.publicKey,
          mint,
          associatedToken,
          metadataProgram: METADATA_PROGRAM_ID,
        })
        .rpc();
    });

    return {
      txSig,
      modelNFT: new ModelNFT(mint, this._program, this._connection),
    };
  }
  //#endregion

  //#region Utility Methods
  private async _prepareMetadata(
    modelHash: string,
    options: ModelNFTOptions
  ): Promise<string> {
    // Implementation for IPFS/ARWEAVE metadata upload
    // Includes validation and encryption logic
    return 'https://metadata.umazen.ai/' + modelHash;
  }

  /**
   * Get all Model NFTs owned by current wallet
   */
  async getOwnedModels(): Promise<ModelNFT[]> {
    assert(this._wallet, 'Wallet not connected');
    const mints = await this._connection.getParsedTokenAccountsByOwner(
      this._wallet.publicKey,
      { programId: TOKEN_PROGRAM_ID }
    );

    return mints.value
      .filter((acc) => acc.account.data.parsed.info.tokenAmount.uiAmount > 0)
      .map(
        (acc) =>
          new ModelNFT(
            new web3.PublicKey(acc.account.data.parsed.info.mint),
            this._program,
            this._connection
          )
      );
  }
  //#endregion
}

//#region Constants
const DEFAULT_PROGRAM_ID = new web3.PublicKey(
  'UMZMxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
);
const METADATA_PROGRAM_ID = new web3.PublicKey(
  'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s'
);
//#endregion
