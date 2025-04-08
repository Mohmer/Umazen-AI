//! Umazen SDK Main Entry Point - Unified API for Decentralized AI

// Core Modules
export { BaseClient } from './clients/BaseClient';
export { NFTClient } from './clients/NFTClient';
export { TrainingClient } from './clients/TrainingClient';
export { InferenceClient } from './clients/InferenceClient';

// Utilities
export * as solana from './utils/solana';
export * as crypto from './utils/crypto';
export * as transactions from './utils/transactions';
export * as ipfs from './utils/ipfs';

// Types
export type {
  ModelMetadata,
  TrainingParams,
  InferenceRequest,
  RoyaltyRecipient,
  ComputeRequirements,
  Keypair,
} from './types/models';

// Configuration Management
import { PublicKey } from '@solana/web3.js';
import { BaseClient } from './clients/BaseClient';

/**
 * Global SDK Configuration
 */
type UmazenConfig = {
  network: 'devnet' | 'testnet' | 'mainnet';
  ipfsApiKey?: string;
  programId: PublicKey;
};

const DEFAULT_CONFIG: UmazenConfig = {
  network: 'devnet',
  programId: new PublicKey('UMZ...'), // Replace with actual program ID
};

let globalConfig = { ...DEFAULT_CONFIG };

/**
 * Initialize the SDK with custom configuration
 */
export function initialize(config: Partial<UmazenConfig> = {}): void {
  globalConfig = {
    ...DEFAULT_CONFIG,
    ...config,
  };

  // Initialize submodules
  BaseClient.initialize(globalConfig);
  if (config.ipfsApiKey) {
    ipfs.setApiKey(config.ipfsApiKey);
  }
}

/**
 * Environment Detection
 */
export const isBrowser = typeof window !== 'undefined';
export const isNode = typeof process !== 'undefined' && process.versions?.node;

// Auto-initialize in browser environments
if (isBrowser) {
  initialize({
    network: (window as any).UMZEN_NETWORK || 'devnet',
    ipfsApiKey: (window as any).UMZEN_IPFS_KEY,
  });
}

// Node.js environment detection
if (isNode) {
  import('dotenv').then((dotenv) => {
    dotenv.config();
    initialize({
      network: process.env.UMZEN_NETWORK as any,
      ipfsApiKey: process.env.UMZEN_IPFS_KEY,
      programId: new PublicKey(process.env.UMZEN_PROGRAM_ID || DEFAULT_CONFIG.programId),
    });
  });
}

// Error Handling Setup
import { AnchorError, AnchorProvider } from '@coral-xyz/anchor';
import { TransactionError } from './types/models';

/**
 * Global Error Decoder
 */
export function decodeError(err: unknown): TransactionError | null {
  if (err instanceof AnchorError) {
    return {
      code: err.error.errorCode.code,
      message: err.error.errorMessage,
      logs: err.logs,
    };
  }
  return null;
}

// Export Version Info
export const VERSION = {
  major: 1,
  minor: 0,
  patch: 0,
  toString: () => `v1.0.0-${__BUILD_HASH__}`,
};

// Wallet Adapter Integration
declare global {
  interface Window {
    phantom?: { solana?: any };
  }
}

/**
 * Auto-detect Phantom Wallet
 */
export async function autoConnect(): Promise<AnchorProvider | null> {
  if (isBrowser && window.phantom?.solana?.isPhantom) {
    const provider = solana.createDefaultProvider();
    await provider.connect();
    return provider;
  }
  return null;
}

// Export Core SDK Instance
export const umazenSDK = {
  nft: new NFTClient(),
  training: new TrainingClient(),
  inference: new InferenceClient(),
  config: globalConfig,
  utils: {
    solana,
    crypto,
    transactions,
    ipfs,
  },
  version: VERSION,
};

// Type Augmentation for Global Access
declare global {
  interface Window {
    umazen: typeof umazenSDK;
  }
}

if (isBrowser) {
  window.umazen = umazenSDK;
}

export default umazenSDK;
