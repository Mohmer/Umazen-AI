//! Model NFT Transfer Logic with Royalty Enforcement

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, sysvar::rent::Rent},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{Metadata, TokenMetadata},
    token::{
        spl_token::{self, instruction::transfer_checked},
        Token, TokenAccount,
    },
};
use crate::{royalty::RoyaltyConfig, MintConfig};

declare_id!("Trnsmazn111111111111111111111111111111111111");

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct TransferModel<'info> {
    // Source
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = from,
    )]
    pub from_ata: Account<'info, TokenAccount>,

    // Destination
    /// CHECK: Verified by CPI
    pub to: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = from,
        associated_token::mint = mint,
        associated_token::authority = to,
    )]
    pub to_ata: Account<'info, TokenAccount>,

    // Mint Details
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(
        has_one = mint,
        seeds = [b"mint_config", config.authority.key().as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, MintConfig>,
    #[account(
        mut,
        seeds = [b"royalty", mint.key().as_ref()],
        bump,
        constraint = royalty.recipients.iter().all(|r| r.share <= 100)
    )]
    pub royalty: Account<'info, RoyaltyConfig>,

    // Programs
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[program]
pub mod model_transfer {
    use super::*;

    /// Transfer AI Model NFT with automatic royalty payments
    pub fn transfer_model(ctx: Context<TransferModel>, amount: u64) -> Result<()> {
        require!(amount == 1, TransferError::InvalidAmount);
        require!(!ctx.accounts.mint.is_frozen, TransferError::FrozenModel);
        
        let mint = &ctx.accounts.mint;
        let royalty = &mut ctx.accounts.royalty;
        let total_royalty = royalty.recipients.iter().map(|r| r.share).sum::<u8>() as u64;

        // Deduct royalties
        let royalty_amount = amount
            .checked_mul(total_royalty.into())
            .and_then(|v| v.checked_div(100))
            .ok_or(TransferError::RoyaltyOverflow)?;
        
        let transfer_amount = amount.checked_sub(royalty_amount)
            .ok_or(TransferError::RoyaltyOverflow)?;

        // Main transfer
        let transfer_ix = transfer_checked(
            ctx.accounts.token_program.key(),
            ctx.accounts.from_ata.key(),
            mint.key(),
            ctx.accounts.to_ata.key(),
            ctx.accounts.from.key(),
            &[],
            transfer_amount,
            mint.decimals,
        )?;

        invoke(
            &transfer_ix,
            &[
                ctx.accounts.from_ata.to_account_info(),
                ctx.accounts.to_ata.to_account_info(),
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
            ],
        )?;

        // Distribute royalties
        if royalty_amount > 0 {
            for recipient in &royalty.recipients {
                let share = recipient.share as u64;
                let amount = royalty_amount
                    .checked_mul(share)
                    .and_then(|v| v.checked_div(total_royalty.into()))
                    .ok_or(TransferError::RoyaltyOverflow)?;

                let royalty_ix = transfer_checked(
                    ctx.accounts.token_program.key(),
                    ctx.accounts.from_ata.key(),
                    mint.key(),
                    recipient.wallet.key(),
                    ctx.accounts.from.key(),
                    &[],
                    amount,
                    mint.decimals,
                )?;

                invoke(
                    &royalty_ix,
                    &[
                        ctx.accounts.from_ata.to_account_info(),
                        recipient.wallet.to_account_info(),
                        ctx.accounts.from.to_account_info(),
                        ctx.accounts.mint.to_account_info(),
                    ],
                )?;
            }
        }

        // Update metadata
        if ctx.accounts.to_ata.amount == 1 {
            let update_ix = TokenMetadata::update_authority(
                ctx.accounts.metadata_program.key(),
                ctx.accounts.metadata.key(),
                ctx.accounts.to.key(),
                Some(ctx.accounts.to.key()),
            )?;

            invoke(
                &update_ix,
                &[
                    ctx.accounts.metadata.to_account_info(),
                    ctx.accounts.to.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        Ok(())
    }
}

#[error_code]
pub enum TransferError {
    #[msg("Only single NFT transfers allowed")]
    InvalidAmount,
    #[msg("Royalty calculation overflow")]
    RoyaltyOverflow,
    #[msg("Model NFT is frozen")]
    FrozenModel,
    #[msg("Invalid metadata update authority")]
    MetadataAuthorityMismatch,
    #[msg("Royalty recipients not configured")]
    MissingRoyaltyConfig,
    #[msg("Unauthorized transfer attempt")]
    Unauthorized,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{solana_program::system_instruction, test::*};
    use spl_token::state::Mint;

    #[test]
    fn test_successful_transfer_with_royalty() {
        let mut test_env = TestEnvironment::new();
        let from = test_env.create_user(1_000_000_000);
        let to = test_env.create_user(0);
        let royalty_wallet = test_env.create_user(0);

        // Initialize royalty config
        test_env.initialize_royalty(vec![
            RecipientConfig {
                wallet: royalty_wallet.key(),
                share: 10,
            }
        ]);

        // Test transfer
        let result = test_env.transfer(from, to, 1);
        assert!(result.is_ok());
        
        // Verify balances
        let from_balance = test_env.get_token_balance(from);
        let to_balance = test_env.get_token_balance(to);
        let royalty_balance = test_env.get_token_balance(royalty_wallet);
        
        assert_eq!(from_balance, 0);
        assert_eq!(to_balance, 1);
        assert_eq!(royalty_balance, 1);
    }
}
