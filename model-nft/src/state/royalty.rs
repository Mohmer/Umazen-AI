//! Royalty Distribution Module - Automated royalty payments for AI model usage

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{spl_token::instruction::transfer, Mint, Token, TokenAccount},
};
use arrayref::array_ref;
use std::convert::TryInto;

declare_id!("Roymazn1111111111111111111111111111111111111");

// --------------------------
// Core Data Structures
// --------------------------

#[account]
#[derive(Default)]
pub struct RoyaltyVault {
    pub version: u8,
    pub model_nft: Pubkey,
    pub total_earned: u64,
    pub distribution_schedule: DistributionSchedule,
    pub recipients: Vec<RoyaltyRecipient>,
    pub bump: u8,
    pub last_distributed: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct DistributionSchedule {
    pub creator_percent: u8,
    pub training_nodes_percent: u8,
    pub data_providers_percent: u8,
    pub dao_treasury_percent: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RoyaltyRecipient {
    pub recipient_type: RecipientType,
    pub address: Pubkey,
    pub share: u8,
    pub associated_token: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq)]
pub enum RecipientType {
    Creator,
    TrainingNode,
    DataProvider,
    DAO,
}

// --------------------------
// Instruction Contexts
// --------------------------

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct DistributeRoyalties<'info> {
    #[account(mut, has_one = model_nft)]
    pub royalty_vault: Account<'info, RoyaltyVault>,
    pub model_nft: Account<'info, ModelNFT>, // From NFT module

    // System
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    // Fee destination
    #[account(mut)]
    pub fee_destination: Account<'info, TokenAccount>,
    pub fee_mint: Account<'info, Mint>,
}

#[derive(Accounts)]
pub struct InitializeRoyaltyVault<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + RoyaltyVault::LEN,
        seeds = [b"royalty_vault", model_nft.key().as_ref()],
        bump
    )]
    pub royalty_vault: Account<'info, RoyaltyVault>,
    pub model_nft: Account<'info, ModelNFT>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// --------------------------
// Program Logic
// --------------------------

#[program]
pub mod royalty {
    use super::*;

    /// Initialize royalty vault for a model
    pub fn initialize_vault(
        ctx: Context<InitializeRoyaltyVault>,
        schedule: DistributionSchedule,
        recipients: Vec<RoyaltyRecipient>,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.royalty_vault;
        
        // Validate percentages sum to 100
        let total = schedule.creator_percent
            + schedule.training_nodes_percent
            + schedule.data_providers_percent
            + schedule.dao_treasury_percent;
        
        require!(total == 100, RoyaltyError::InvalidDistribution);

        // Initialize vault
        vault.version = 1;
        vault.model_nft = ctx.accounts.model_nft.key();
        vault.distribution_schedule = schedule;
        vault.recipients = recipients;
        vault.bump = *ctx.bumps.get("royalty_vault").ok_or(RoyaltyError::BumpNotFound)?;
        vault.last_distributed = -1;

        Ok(())
    }

    /// Distribute royalties according to configured rules
    pub fn distribute(ctx: Context<DistributeRoyalties>, amount: u64) -> Result<()> {
        let vault = &mut ctx.accounts.royalty_vault;
        let model_nft = &ctx.accounts.model_nft;

        // Calculate royalty amount
        let royalty_amount = model_nft.calculate_royalty(amount)
            .ok_or(RoyaltyError::ArithmeticError)?;

        // Transfer funds to vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.payer.to_account_info(),
            to: ctx.accounts.fee_destination.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );
        
        anchor_spl::token::transfer(cpi_ctx, royalty_amount)?;

        // Update vault state
        vault.total_earned = vault.total_earned.checked_add(royalty_amount)
            .ok_or(RoyaltyError::ArithmeticError)?;
        vault.last_distributed = Clock::get()?.unix_timestamp;

        // Distribute to recipients
        for recipient in &vault.recipients {
            let share_amount = royalty_amount
                .checked_mul(recipient.share as u64)
                .and_then(|v| v.checked_div(100))
                .ok_or(RoyaltyError::ArithmeticError)?;

            if share_amount > 0 {
                // Create transfer instruction
                let transfer_ix = transfer(
                    &ctx.accounts.token_program.key(),
                    &ctx.accounts.fee_destination.key(),
                    &recipient.associated_token,
                    &ctx.accounts.payer.key(),
                    &[],
                    share_amount,
                )?;

                // Invoke transfer
                invoke(
                    &transfer_ix,
                    &[
                        ctx.accounts.fee_destination.to_account_info(),
                        ctx.accounts.token_program.to_account_info(),
                        ctx.accounts.payer.to_account_info(),
                    ],
                )?;
            }
        }

        Ok(())
    }
}

// --------------------------
// Utility Methods
// --------------------------

impl RoyaltyVault {
    pub const LEN: usize = 1 + 32 + 8 + DistributionSchedule::LEN + 4 + (RoyaltyRecipient::LEN * 10) + 1 + 8;

    /// Validate royalty distribution parameters
    pub fn validate_recipients(&self) -> Result<()> {
        let mut total_share: u8 = 0;
        for recipient in &self.recipients {
            total_share = total_share.checked_add(recipient.share)
                .ok_or(RoyaltyError::ArithmeticError)?;
        }
        require!(total_share == 100, RoyaltyError::InvalidRecipientShare);
        Ok(())
    }
}

impl DistributionSchedule {
    pub const LEN: usize = 4;
}

impl RoyaltyRecipient {
    pub const LEN: usize = 1 + 32 + 1 + 32;
}

// --------------------------
// Error Handling
// --------------------------

#[error_code]
pub enum RoyaltyError {
    #[msg("Invalid distribution schedule (must sum to 100)")]
    InvalidDistribution,
    #[msg("Recipient shares must sum to 100")]
    InvalidRecipientShare,
    #[msg("Arithmetic operation overflow")]
    ArithmeticError,
    #[msg("Bump seed not found")]
    BumpNotFound,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    #[msg("Unauthorized royalty access")]
    Unauthorized,
    #[msg("Invalid payment amount")]
    InvalidAmount,
}

// --------------------------
// Unit Tests
// --------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::test::*;
    use solana_program::{clock::Epoch, system_program};

    #[test]
    fn test_initialize_vault() {
        let mut ctx = test_context!(InitializeRoyaltyVault);
        let schedule = DistributionSchedule {
            creator_percent: 40,
            training_nodes_percent: 30,
            data_providers_percent: 20,
            dao_treasury_percent: 10,
        };
        
        let result = royalty::initialize_vault(
            &mut ctx,
            schedule,
            vec![]
        );

        assert!(result.is_ok());
        let vault = &ctx.accounts.royalty_vault;
        assert_eq!(vault.distribution_schedule.creator_percent, 40);
    }

    #[test]
    fn test_invalid_distribution() {
        let mut ctx = test_context!(InitializeRoyaltyVault);
        let invalid_schedule = DistributionSchedule {
            creator_percent: 50,
            training_nodes_percent: 50,
            data_providers_percent: 50,
            dao_treasury_percent: 50,
        };
        
        let result = royalty::initialize_vault(
            &mut ctx,
            invalid_schedule,
            vec![]
        );

        assert_eq!(result.unwrap_err(), RoyaltyError::InvalidDistribution.into());
    }
}
