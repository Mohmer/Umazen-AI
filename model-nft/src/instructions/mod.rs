//! Umazen Core Module Hierarchy
//! 
//! Architecture:
//! - Root module organizes cross-cutting concerns
//! - Submodules follow SOLID principles
//! - Feature gates for conditional compilation

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    missing_debug_implementations,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]
#![warn(clippy::all, clippy::pedantic)]

#[macro_use]
extern crate alloc;

// Core Platform APIs
pub mod api {
    //! Cross-platform abstraction layer

    pub mod blockchain {
        //! Solana blockchain integration
        pub mod rpc;
        pub mod transactions;
        pub mod accounts;
    }

    pub mod compute {
        //! Distributed compute orchestration
        pub mod zk;
        pub mod fl;
        pub mod inference;
    }
}

// Domain-Specific Components
pub mod domain {
    //! Business logic implementation

    pub mod nft {
        //! Model ownership management
        pub mod metadata;
        pub mod royalties;
        pub mod licensing;
    }

    pub mod marketplace {
        //! Inference marketplace operations
        pub mod orderbook;
        pub mod matching;
        pub mod settlement;
    }

    pub mod training {
        //! Federated learning workflows
        pub mod coordinator;
        pub mod aggregation;
        pub mod validation;
    }
}

// Infrastructure Services
pub mod infrastructure {
    //! Technical subsystem implementations

    pub mod crypto {
        //! Cryptographic primitives
        pub mod zkp;
        pub mod signatures;
        pub mod hashing;
    }

    pub mod storage {
        //! Distributed persistence layer
        pub mod ipfs;
        pub mod arweave;
        pub mod solana;
    }

    pub mod networking {
        //! P2P communication layer
        pub mod libp2p;
        pub mod grpc;
    }
}

// Shared Utilities
pub mod utils {
    //! Cross-cutting utilities

    pub mod serialization {
        //! Data transformation utilities
        pub mod borsh;
        pub mod json;
    }

    pub mod math {
        //! Numerical operations
        pub mod fixed_point;
        pub mod statistics;
    }

    pub mod time {
        //! Temporal operations
        pub mod scheduler;
        pub mod clocks;
    }
}

// Platform Configuration
pub mod config {
    //! Runtime configuration management
    pub mod chain;
    pub mod network;
    pub mod compute;
}

// Error Hierarchy
pub mod error {
    //! Unified error taxonomy
    pub mod core;
    pub mod blockchain;
    pub mod compute;
}

// Testing Infrastructure
#[cfg(any(test, feature = "testing"))]
pub mod test {
    //! Testing utilities and mocks
    pub mod fixtures;
    pub mod simulators;
    pub mod assertions;
}

// Public API Surface
pub use api::blockchain::*;
pub use domain::{nft::metadata, marketplace::orderbook};
pub use infrastructure::crypto::zkp;

/// Prelude for common imports
pub mod prelude {
    //! Curated namespace imports

    pub use super::{
        api::blockchain::{rpc, transactions},
        domain::nft::metadata::ModelMetadata,
        infrastructure::crypto::{zkp::ZkCircuit, signatures::Keypair},
        utils::serialization::borsh::BorshSerialize,
    };

    #[cfg(feature = "solana")]
    pub use solana_program::{
        entrypoint, program_error::ProgramError, 
        program_pack::Pack, pubkey::Pubkey,
    };
}

// Cross-module type reexports
pub mod types {
    //! Common type definitions
    
    pub use domain::nft::metadata::ModelMetadata;
    pub use domain::marketplace::orderbook::Order;
    pub use infrastructure::crypto::zkp::Proof;
}

// Platform Initialization
pub fn initialize_platform() -> Result<(), error::core::PlatformError> {
    //! Bootstrap platform components
    use cfg_if::cfg_if;

    cfg_if! {
        if #[cfg(feature = "solana")] {
            solana_program::process_instruction(
                crate::api::blockchain::ENTRYPOINT, 
                &[], 
                &[]
            )?;
        } else {
            unimplemented!()
        }
    }

    Ok(())
}

#[cfg(test)]
mod integration_tests {
    //! Cross-module integration tests

    use super::*;
    use test::simulators::*;

    #[test]
    fn full_workflow_simulation() {
        let mut test_env = BlockchainSimulator::new();
        test_env.initialize();

        let metadata = ModelMetadata::default();
        let proof = ZkCircuit::generate_proof(&metadata).unwrap();
        
        assert!(proof.verify());
    }
}
