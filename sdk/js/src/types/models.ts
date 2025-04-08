//! Umazen Type Definitions - Centralized schema for blockchain and AI entities

import { BN } from '@coral-xyz/anchor';
import { PublicKey } from '@solana/web3.js';

/**
 * Core NFT Model Schema
 */
export interface ModelNFT {
  /** NFT mint address */
  mint: PublicKey;
  /** Metadata URI (IPFS) */
  metadataUri: string;
  /** Current owner of the model */
  owner: PublicKey;
  /** Royalty recipients with basis points */
  royalties: RoyaltyRecipient[];
  /** Model architecture hash (SHA256) */
  modelHash: string;
  /** Access control flags */
  permissions: {
    /** Allow commercial use */
    commercialUse: boolean;
    /** Allow fine-tuning */
    fineTuneable: boolean;
  };
  /** Training provenance data */
  provenance: {
    /** Original trainer wallet */
    trainer: PublicKey;
    /** Training dataset CID */
    datasetCid: string;
    /** Final accuracy metric */
    accuracy: number;
  };
  /** Version history */
  versions: ModelVersion[];
}

/**
 * Model Metadata Schema (IPFS)
 */
export interface ModelMetadata {
  /** UUID v4 */
  id: string;
  /** Model architecture type */
  modelType: 'image-gen' | 'text-classification' | 'llm' | 'recommendation';
  /** Framework identifier */
  framework: 'pytorch' | 'tensorflow' | 'jax';
  /** Base model identifier */
  baseModel?: string;
  /** License SPDX identifier */
  license: string;
  /** Required hardware specs */
  hardwareRequirements: {
    /** Minimum VRAM in GB */
    vram: number;
    /** Minimum RAM in GB */
    ram: number;
    /** Supported accelerators */
    supportedHardware: string[];
  };
  /** IPFS CID of model binaries */
  modelCid: string;
  /** Input/output schema */
  schema: {
    input: Record<string, 'image' | 'text' | 'tensor'>;
    output: Record<string, 'image' | 'text' | 'tensor'>;
  };
  /** Creation timestamp */
  createdAt: UnixTimestamp;
  /** Last update timestamp */
  updatedAt: UnixTimestamp;
}

/**
 * Federated Learning Task Schema
 */
export interface TrainingTask {
  /** Task UUID */
  id: string;
  /** Model template CID */
  modelTemplateCid: string;
  /** Current training epoch */
  epoch: number;
  /** Participating nodes */
  nodes: ComputeNode[];
  /** Reward parameters */
  reward: {
    /** Reward per epoch (lamports) */
    perEpoch: BN;
    /** Total distributed rewards */
    distributed: BN;
  };
  /** Accuracy target */
  targetAccuracy: number;
  /** Task status */
  status: 'active' | 'completed' | 'failed';
  /** ZK Proof system */
  proofSystem: 'groth16' | 'plonk' | 'marlin';
}

/**
 * Compute Node Configuration
 */
export interface ComputeNode {
  /** Node owner wallet */
  owner: PublicKey;
  /** Hardware specs */
  hardware: {
    vram: number;
    ram: number;
    supportedAccelerators: string[];
  };
  /** Performance metrics */
  performance: {
    /** Average iterations per second */
    ips: number;
    /** Last successful proof */
    lastProofTime: UnixTimestamp;
  };
  /** Staked amount */
  stake: BN;
  /** Node status */
  status: 'active' | 'pending' | 'slashed';
}

/**
 * ZK Proof Schema
 */
export interface Proof {
  /** Proof components */
  a: [string, string];
  b: [[string, string], [string, string]];
  c: [string, string];
  /** Public inputs */
  publicInputs: string[];
  /** Proof system identifier */
  system: 'groth16' | 'plonk';
}

/**
 * Royalty Distribution Schema
 */
export interface RoyaltyRecipient {
  /** Recipient wallet */
  address: PublicKey;
  /** Basis points (0-10000) */
  basisPoints: number;
}

/**
 * Model Version History
 */
export interface ModelVersion {
  /** Version semantic tag */
  version: string;
  /** IPFS CID of this version */
  cid: string;
  /** Training parameters */
  params: {
    epochs: number;
    datasetSize: number;
    batchSize: number;
  };
  /** Checksum of model weights */
  checksum: string;
  /** Timestamp of version creation */
  timestamp: UnixTimestamp;
}

/**
 * Inference Request Schema
 */
export interface InferenceRequest {
  /** Model NFT mint address */
  modelId: PublicKey;
  /** Encrypted input data */
  input: EncryptedData;
  /** Client proof of payment */
  paymentProof: string;
  /** Request metadata */
  metadata?: {
    /** Application context */
    appId?: string;
    /** Request purpose */
    useCase?: 'research' | 'commercial';
  };
}

/** Unix timestamp in seconds */
export type UnixTimestamp = number;

/** UUID v4 string */
export type UUID = string;

/** Encrypted data container */
export type EncryptedData = {
  /** Initialization vector */
  iv: string;
  /** Encrypted payload */
  data: string;
  /** Authentication tag */
  tag: string;
  /** Encryption algorithm */
  algorithm: 'aes-256-gcm' | 'xchacha20-poly1305';
};

/** Hash digest types */
export type SHA256Hash = string;
export type Blake3Hash = string;
