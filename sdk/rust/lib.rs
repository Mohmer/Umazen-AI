//! Umazen Core Library - Decentralized AI Infrastructure Program

#![deny(
    missing_docs,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]
#![cfg_attr(not(test), forbid(clippy::unwrap_used))]

use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};
use vipers::prelude::*;

mod constants;
mod errors;
mod instructions;
mod state;
mod utils;

pub use constants::*;
pub use errors::*;
pub use state::*;
pub use utils::*;

declare_id!("UMZENXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

/// Program entrypoint handlers
#[program]
pub mod umazen {
    use super::*;

    /// Initialize main protocol state
    pub fn initialize_protocol(ctx: Context<InitializeProtocol>) -> Result<()> {
        instructions::initialize::handler(ctx)
    }

    /// Mint AI Model NFT with metadata
    pub fn mint_model(
        ctx: Context<MintModel>,
        metadata_uri: String,
        royalty_basis_points: u16,
        compute_requirements: ComputeRequirements,
    ) -> Result<()> {
        instructions::mint::handler(ctx, metadata_uri, royalty_basis_points, compute_requirements)
    }

    /// Start federated training session
    pub fn start_training(
        ctx: Context<StartTraining>,
        model_hash: [u8; 32],
        params: TrainingParams,
    ) -> Result<()> {
        instructions::training::handler(ctx, model_hash, params)
    }

    /// Submit inference request
    pub fn request_inference(
        ctx: Context<RequestInference>,
        input_data: Vec<u8>,
        payment: u64,
    ) -> Result<()> {
        instructions::inference::handler(ctx, input_data, payment)
    }

    /// Stake model for rewards
    pub fn stake_model(
        ctx: Context<StakeModel>,
        duration: i64,
        amount: u64,
    ) -> Result<()> {
        instructions::staking::handler(ctx, duration, amount)
    }

    /// Update model metadata (DAO-only)
    pub fn update_metadata(
        ctx: Context<UpdateMetadata>,
        new_uri: String,
    ) -> Result<()> {
        instructions::metadata::handler(ctx, new_uri)
    }
}

/// Cross-Program Invocation (CPI) helpers
pub mod cpi {
    use super::*;
    
    /// Helper for token CPI operations
    pub fn token_transfer<'info>(
        token_program: &Program<'info, Token>,
        source: &Account<'info, TokenAccount>,
        destination: &Account<'info, TokenAccount>,
        authority: &AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        let cpi_accounts = token::Transfer {
            from: source.to_account_info(),
            to: destination.to_account_info(),
            authority: authority.to_account_info(),
        };
        token::transfer(
            CpiContext::new(token_program.to_account_info(), cpi_accounts),
            amount,
        )
    }
}

/// Program security validations
pub mod guards {
    use super::*;

    /// Validate training session parameters
    pub fn validate_training_params(params: &TrainingParams) -> Result<()> {
        require!(params.max_iterations > 0, ProtocolError::InvalidParameter);
        require!(params.batch_size <= MAX_BATCH_SIZE, ProtocolError::BatchLimitExceeded);
        Ok(())
    }

    /// Verify model ownership
    pub fn verify_model_owner(
        model: &Account<ModelNFT>,
        owner: &Signer,
    ) -> Result<()> {
        require!(model.owner == owner.key(), ProtocolError::NotAuthorized);
        Ok(())
    }
}

/// Event logging
#[event]
pub enum ProtocolEvent {
    /// Model minted @timestamp
    ModelMinted {
        model: Pubkey,
        mint_time: i64,
        metadata_uri: String,
    },
    
    /// Training session completed
    TrainingCompleted {
        model: Pubkey,
        iterations: u32,
        accuracy: f32,
        reward_distributed: u64,
    },
}

/// Account context structures
#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(init, payer = authority, space = 8 + ProtocolState::LEN)]
    pub protocol_state: Account<'info, ProtocolState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(Default)]
pub struct ProtocolState {
    pub version: u8,
    pub model_count: u64,
    pub total_staked: u64,
    pub dao_treasury: Pubkey,
}

/// Testing utilities
#[cfg(test)]
mod test {
    use super::*;
    use solana_program_test::*;
    use solana_sdk::{signature::Keypair, signer::Signer};

    #[tokio::test]
    async fn test_initialize_protocol() {
        let mut context = ProgramTest::new(
            "umazen",
            id(),
            processor!(entry),
        ).start_with_context().await;

        let protocol_state = Keypair::new();
        let ix = Instruction {
            program_id: id(),
            accounts: vec![
                AccountMeta::new(protocol_state.pubkey(), false),
                AccountMeta::new(context.payer.pubkey(), true),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: UmazenInstruction::InitializeProtocol.try_to_vec().unwrap(),
        };

        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&context.payer.pubkey()),
            &[&context.payer, &protocol_state],
            context.last_blockhash,
        );

        context.banks_client.process_transaction(tx).await.unwrap();
    }
}
