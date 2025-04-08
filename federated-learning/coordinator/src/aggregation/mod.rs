//! Federated Learning Module - Core Implementation for Decentralized Training

#![deny(
    unsafe_code,
    missing_docs,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

// External Dependencies
use anchor_lang::prelude::*;
use solana_program::program_error::ProgramError;

// Sub-module Declarations
pub mod client;         // Client-facing operations
pub mod state;          // Program state structures
pub mod instructions;   // CPI instruction builders
pub mod errors;         // Custom error handling
pub mod processor;      // Core business logic
pub mod utils;          // Cryptographic utilities
pub mod zk;             // Zero-knowledge proof integration

// Public Interface Exports
pub use client::FederatedLearningClient;
pub use state::{
    Task,
    Participant,
    ModelUpdate,
    GlobalModel,
    TaskStatus,
    ParticipantStatus
};
pub use instructions::{
    create_task_instruction,
    submit_update_instruction,
    aggregate_model_instruction
};
pub use errors::FederatedLearningError;
pub use processor::{
    process_initialize_task,
    process_submit_update,
    process_aggregate_model
};

/// Program ID constant (Replace with actual program ID)
pub const ID: Pubkey = Pubkey::new_from_array([0u8; 32]);

/// Program Entrypoint Configuration
#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint {
    use super::*;
    
    /// Program entrypoint handler
    #[allow(clippy::too_many_arguments)]
    pub fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> Result<(), ProgramError> {
        processor::Processor::process(program_id, accounts, instruction_data)
    }
}

/// Anchor Program Declaration
#[derive(Clone)]
pub struct FederatedLearningProgram;

impl anchor_lang::Id for FederatedLearningProgram {
    fn id() -> Pubkey {
        ID
    }
}

/// Program-Specific Macros
#[macro_export]
macro_rules! validate_task_state {
    ($task:expr, $expected_status:pat) => {
        if !matches!($task.status, $expected_status) {
            return Err(FederatedLearningError::InvalidTaskState.into());
        }
    };
}

/// Core Constants
pub mod constants {
    /// Minimum stake requirement in lamports
    pub const MIN_STAKE: u64 = 1_000_000;
    
    /// Model dimensions for fixed-size arrays
    pub const MODEL_DIMENSIONS: usize = 1024;
    
    /// Maximum participants per task
    pub const MAX_PARTICIPANTS: usize = 100;
    
    /// Minimum updates required for aggregation
    pub const MIN_UPDATES: u64 = 10;
}

/// Program-Specific Prelude
pub mod prelude {
    pub use super::{
        ID,
        FederatedLearningProgram,
        FederatedLearningError,
        Task,
        Participant,
        ModelUpdate,
        GlobalModel
    };
    
    pub use anchor_lang::{
        prelude::*,
        solana_program::{
            program_pack::Pack,
            sysvar
        }
    };
}

/// Test Module (Feature-Gated)
#[cfg(feature = "test")]
pub mod test {
    use super::*;
    use solana_program_test::*;
    
    /// Test Fixture Builder
    pub struct TestFixture {
        pub program_id: Pubkey,
        pub context: ProgramTestContext,
    }
    
    impl TestFixture {
        /// Initialize new test environment
        pub async fn new() -> Self {
            let mut program_test = ProgramTest::new(
                "federated_learning",
                ID,
                None
            );
            
            let context = program_test.start_with_context().await;
            
            Self {
                program_id: ID,
                context,
            }
        }
    }
    
    /// Test Utilities
    pub mod test_utils {
        use super::*;
        
        /// Generate mock model weights
        pub fn mock_weights() -> [f32; constants::MODEL_DIMENSIONS] {
            [0.5; constants::MODEL_DIMENSIONS]
        }
    }
}

/// Program Version Information
pub mod version {
    /// Major version number
    pub const MAJOR: u8 = 1;
    
    /// Minor version number
    pub const MINOR: u8 = 0;
    
    /// Patch version number
    pub const PATCH: u8 = 0;
    
    /// Version string
    pub const VERSION: &str = concat!(
        stringify!($MAJOR), ".",
        stringify!($MINOR), ".",
        stringify!($PATCH)
    );
}

// Cross-Module Validation
pub(crate) fn validate_authority<'info>(
    authority: &AccountInfo<'info>,
    expected_authority: &Pubkey,
) -> Result<(), ProgramError> {
    if authority.key != expected_authority {
        return Err(FederatedLearningError::Unauthorized.into());
    }
    if !authority.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    Ok(())
}
