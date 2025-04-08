//! Umazen Core Modules - Decentralized AI Infrastructure

#![forbid(unsafe_code)]
#![warn(missing_docs, unreachable_pub, unused_crate_dependencies)]

/// Blockchain interaction layer
pub mod blockchain {
    pub mod programs {
        //! Solana program entrypoints
        pub mod model_nft;
        pub mod training;
        pub mod marketplace;
    }

    pub mod client {
        //! Client SDK for blockchain operations
        pub mod rpc;
        pub mod transactions;
        pub mod accounts;
    }

    pub mod tests {
        //! Blockchain integration tests
        pub mod program_tests;
        pub mod stress_tests;
    }
}

/// AI computation layer
pub mod ai {
    pub mod federated_learning {
        //! Federated learning coordination
        pub mod aggregation;
        pub mod gradients;
        pub mod coordinator;
    }

    pub mod zk_proofs {
        //! Zero-knowledge ML components
        pub mod circuits;
        pub mod prover;
        pub mod verifier;
    }

    pub mod models {
        //! Model architectures & serialization
        pub mod tensor;
        pub mod onnx;
        pub mod pytorch;
    }
}

/// Marketplace components
pub mod marketplace {
    pub mod listings {
        //! Model deployment management
        pub mod pricing;
        pub mod compute_requirements;
        pub mod sla;
    }

    pub mod orders {
        //! Inference request handling
        pub mod validation;
        pub mod execution;
        pub mod billing;
    }

    pub mod reputation {
        //! Node reputation system
        pub mod scoring;
        pub mod disputes;
    }
}

/// Cryptographic primitives
pub mod crypto {
    pub mod signatures {
        //! Digital signature schemes
        pub mod ed25519;
        pub mod secp256k1;
    }

    pub mod encryption {
        //! Data encryption modules
        pub mod aes;
        pub mod fhe;
    }

    pub mod hashing {
        //! Hash functions
        pub mod keccak;
        pub mod poseidon;
    }
}

/// Utility modules
pub mod utils {
    pub mod error {
        //! Error handling utilities
        pub mod macros;
        pub mod codes;
    }

    pub mod serialization {
        //! Data serialization
        pub mod borsh;
        pub mod postcard;
    }

    pub mod config {
        //! Runtime configuration
        pub mod network;
        pub mod logging;
        pub mod performance;
    }
}

/// CLI interface
pub mod cli {
    pub mod commands {
        //! CLI command handlers
        pub mod deploy;
        pub mod train;
        pub mod infer;
    }

    pub mod interface {
        //! User interaction layer
        pub mod prompts;
        pub mod output;
    }
}

/// Benchmarking suite
pub mod benches {
    pub mod blockchain {
        //! Blockchain performance tests
        pub mod tps;
        pub mod finality;
    }

    pub mod ai {
        //! Model performance metrics
        pub mod inference_speed;
        pub mod training_throughput;
    }
}

// Re-export core types
pub use blockchain::programs::model_nft::ModelNFT;
pub use ai::federated_learning::coordinator::TrainingSession;
pub use marketplace::listings::ModelListing;

/// Prelude module for common imports
pub mod prelude {
    pub use super::blockchain::client::*;
    pub use super::ai::federated_learning::*;
    pub use super::marketplace::listings::*;
    pub use super::crypto::signatures::*;
    pub use solana_program::{
        account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError, pubkey::Pubkey,
    };
}

#[cfg(test)]
mod integration_tests {
    //! Cross-module integration tests
    pub mod full_workflow;
    pub mod failure_modes;
}

#[cfg(feature = "wasm")]
mod wasm_bindings {
    //! WASM compatibility layer
    pub mod js_interop;
    pub mod memory;
}
