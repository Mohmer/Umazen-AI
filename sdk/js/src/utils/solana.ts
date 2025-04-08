//! Solana Utilities - Core Blockchain Interactions

import {
  Connection, 
  PublicKey,
  Keypair,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  SendTransactionError,
  type Signer,
} from '@solana/web3.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  AccountLayout,
  MintLayout,
  createAssociatedTokenAccountInstruction,
  getAssociatedTokenAddressSync,
} from '@solana/spl-token';
import { WalletAdapter } from '@solana/wallet-adapter-base';
import { AnchorProvider, Program, type Provider } from '@coral-xyz/anchor';
import { sha256 } from 'js-sha256';
import { Buffer } from 'buffer';

// Environment Config
const NETWORK = process.env.NEXT_PUBLIC_SOLANA_NETWORK!;
const RPC_ENDPOINT = process.env.NEXT_PUBLIC_RPC_ENDPOINT!;
const PREFLIGHT_COMMITMENT = 'confirmed' as const;

// Core Utilities
export class SolanaUtils {
  static async getProvider(wallet: WalletAdapter): Promise<Provider> {
    if (!wallet.publicKey || !wallet.signTransaction) {
      throw new Error('Wallet not connected');
    }

    const connection = new Connection(RPC_ENDPOINT, PREFLIGHT_COMMITMENT);
    return new AnchorProvider(
      connection,
      {
        publicKey: wallet.publicKey,
        signTransaction: wallet.signTransaction.bind(wallet),
        signAllTransactions: wallet.signAllTransactions?.bind(wallet),
      },
      { commitment: PREFLIGHT_COMMITMENT }
    );
  }

  static async getBalance(pubkey: PublicKey): Promise<number> {
    const connection = new Connection(RPC_ENDPOINT);
    const balance = await connection.getBalance(pubkey);
    return balance / 1e9;
  }

  static async sendTransaction(
    provider: Provider,
    instructions: TransactionInstruction[],
    signers: Signer[] = []
  ): Promise<string> {
    const tx = new Transaction().add(...instructions);
    tx.recentBlockhash = (await provider.connection.getLatestBlockhash()).blockhash;
    tx.feePayer = provider.publicKey;

    if (signers.length > 0) {
      tx.sign(...signers);
    }

    try {
      const sig = await provider.connection.sendRawTransaction(
        tx.serialize(),
        { skipPreflight: false }
      );
      
      await provider.connection.confirmTransaction(
        sig,
        PREFLIGHT_COMMITMENT
      );
      return sig;
    } catch (err) {
      throw this.parseTransactionError(err);
    }
  }

  static parseTransactionError(err: unknown): Error {
    if (err instanceof SendTransactionError) {
      const msg = err.logs?.join('\n') || err.message;
      return new Error(`Transaction failed: ${msg}`);
    }
    return err instanceof Error ? err : new Error('Unknown transaction error');
  }

  static async getTokenAccountsByOwner(
    owner: PublicKey,
    mint?: PublicKey
  ): Promise<{ pubkey: PublicKey; amount: number }[]> {
    const connection = new Connection(RPC_ENDPOINT);
    const accounts = await connection.getTokenAccountsByOwner(owner, {
      programId: TOKEN_PROGRAM_ID,
      ...(mint ? { mint } : {}),
    });

    return accounts.value.map(acc => ({
      pubkey: acc.pubkey,
      amount: AccountLayout.decode(acc.account.data).amount.toNumber(),
    }));
  }

  static findAssociatedTokenAddress(
    mint: PublicKey,
    owner: PublicKey
  ): PublicKey {
    return getAssociatedTokenAddressSync(
      mint,
      owner,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );
  }

  static createATokenAccountInstruction(
    mint: PublicKey,
    owner: PublicKey,
    payer: PublicKey
  ): TransactionInstruction {
    const ata = this.findAssociatedTokenAddress(mint, owner);
    return createAssociatedTokenAccountInstruction(
      payer,
      ata,
      owner,
      mint,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );
  }

  static async simulateTransaction(
    provider: Provider,
    instructions: TransactionInstruction[]
  ): Promise<number> {
    const tx = new Transaction().add(...instructions);
    tx.recentBlockhash = (await provider.connection.getLatestBlockhash()).blockhash;
    tx.feePayer = provider.publicKey;

    const simulation = await provider.connection.simulateTransaction(tx);
    if (simulation.value.err) {
      throw new Error(
        `Simulation failed: ${JSON.stringify(simulation.value.err)}`
      );
    }
    return simulation.value.feeCalculator?.lamportsPerSignature || 5000;
  }

  static async airdrop(
    provider: Provider,
    amount: number
  ): Promise<string> {
    if (!NETWORK.includes('devnet')) {
      throw new Error('Airdrop only available on devnet');
    }

    const sig = await provider.connection.requestAirdrop(
      provider.publicKey,
      amount * 1e9
    );
    
    await provider.connection.confirmTransaction(sig);
    return sig;
  }

  static async getAccountInfo<T>(
    provider: Provider,
    pubkey: PublicKey,
    decodeFn: (data: Buffer) => T
  ): Promise<T | null> {
    const accountInfo = await provider.connection.getAccountInfo(pubkey);
    if (!accountInfo?.data) return null;
    return decodeFn(accountInfo.data);
  }

  static generatePda(
    seeds: (Buffer | Uint8Array)[],
    programId: PublicKey
  ): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(seeds, programId);
  }

  static async getProgramInstance<T>(
    provider: Provider,
    idl: T,
    programId: PublicKey
  ): Promise<Program<T>> {
    return new Program(
      idl as any,
      programId,
      provider
    );
  }

  static async signMessage(
    wallet: WalletAdapter,
    message: string
  ): Promise<Uint8Array> {
    if (!wallet.signMessage) {
      throw new Error('Wallet does not support message signing');
    }

    const data = new TextEncoder().encode(message);
    return wallet.signMessage(data);
  }

  static hashData(data: string): Buffer {
    return Buffer.from(sha256.digest(data));
  }
}

// Type Augmentations
declare module '@solana/web3.js' {
  interface PublicKey {
    toBytes(): Uint8Array;
  }
}

// Utility Functions
export const createMintAccountInstructions = (
  payer: PublicKey,
  mint: PublicKey,
  decimals: number
): TransactionInstruction[] => {
  const lamports = 1461600; // Minimum balance for mint account
  return [
    SystemProgram.createAccount({
      fromPubkey: payer,
      newAccountPubkey: mint,
      space: MintLayout.span,
      lamports,
      programId: TOKEN_PROGRAM_ID,
    }),
    new TransactionInstruction({
      keys: [
        { pubkey: mint, isSigner: false, isWritable: true },
        { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      ],
      programId: TOKEN_PROGRAM_ID,
      data: Buffer.from(
        Uint8Array.of(0, ...new BN(decimals).toArray('le', 1))
      ),
    }),
  ];
};
