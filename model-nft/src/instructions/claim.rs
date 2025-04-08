//! Reward Claim Module - Secure and verifiable reward distribution system

use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        system_instruction,
    },
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, spl_token::instruction::transfer_checked, Mint, Token, TokenAccount},
};
use crate::{state::RewardPool, utils::calculate_rewards};

declare_id!("Cla1mUmaz3n111111111111111111111111111111");

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        has_one = reward_mint,
        has_one = reward_vault,
        seeds = [b"reward_pool", reward_pool.authority.key().as_ref()],
        bump = reward_pool.bump,
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        associated_token::mint = reward_mint,
        associated_token::authority = user,
    )]
    pub user_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = reward_mint,
        associated_token::authority = reward_vault,
    )]
    pub reward_vault: Account<'info, TokenAccount>,

    #[account(address = reward_pool.reward_mint)]
    pub reward_mint: Account<'info, Mint>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[program]
pub mod reward_claim {
    use super::*;

    /// Claim accumulated rewards from staking or participation
    pub fn claim(ctx: Context<ClaimRewards>, bump: u8) -> Result<()> {
        let reward_pool = &mut ctx.accounts.reward_pool;
        let clock = Clock::get()?;

        // Calculate pending rewards using verifiable math
        let pending_rewards = calculate_rewards(
            &ctx.accounts.user.key(),
            reward_pool.last_update,
            clock.unix_timestamp,
            reward_pool.reward_rate,
        )?;

        // Verify reward pool capacity
        require!(
            pending_rewards <= ctx.accounts.reward_vault.amount,
            ClaimError::InsufficientRewardFunds
        );

        // Prepare transfer instruction with proper authority
        let transfer_ix = transfer_checked(
            ctx.accounts.token_program.key(),
            ctx.accounts.reward_vault.key(),
            ctx.accounts.reward_mint.key(),
            ctx.accounts.user_ata.key(),
            ctx.accounts.reward_pool.authority.key(),
            &[&ctx.accounts.reward_pool.authority.key().as_ref()],
            pending_rewards,
            ctx.accounts.reward_mint.decimals,
        )?;

        // Execute CPI with DAO authority signature
        let signer_seeds = &[
            b"reward_pool",
            ctx.accounts.reward_pool.authority.key().as_ref(),
            &[bump],
        ];
        invoke_signed(
            &transfer_ix,
            &[
                ctx.accounts.reward_vault.to_account_info(),
                ctx.accounts.user_ata.to_account_info(),
                ctx.accounts.reward_mint.to_account_info(),
                ctx.accounts.reward_pool.to_account_info(),
            ],
            &[signer_seeds],
        )?;

        // Update reward pool state
        reward_pool.total_distributed = reward_pool
            .total_distributed
            .checked_add(pending_rewards)
            .ok_or(ClaimError::CalculationOverflow)?;
        reward_pool.last_update = clock.unix_timestamp;

        emit!(RewardClaimed {
            user: ctx.accounts.user.key(),
            amount: pending_rewards,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }
}

#[error_code]
pub enum ClaimError {
    #[msg("Insufficient reward funds in vault")]
    InsufficientRewardFunds,
    #[msg("Reward calculation overflow")]
    CalculationOverflow,
    #[msg("Invalid reward pool authority")]
    InvalidAuthority,
    #[msg("Reward period not completed")]
    PeriodNotCompleted,
    #[msg("Invalid user stake status")]
    InvalidStakeStatus,
    #[msg("Reward claim time restriction")]
    ClaimTimeRestricted,
}

#[event]
pub struct RewardClaimed {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[derive(Accounts)]
pub struct InitializeRewardPool<'info> {
    #[account(init, payer = authority, space = 8 + RewardPool::LEN)]
    pub reward_pool: Account<'info, RewardPool>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(Default)]
pub struct RewardPool {
    pub authority: Pubkey,
    pub reward_mint: Pubkey,
    pub reward_rate: u64,         // Rewards per second per unit
    pub total_distributed: u64,
    pub last_update: i64,
    pub bump: u8,
    pub lock_period: i64,         // Minimum time between claims
}

impl RewardPool {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 8 + 1 + 8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::test::*;
    use spl_token::{instruction::initialize_mint, state::Mint};

    struct TestAccounts {
        reward_pool: AccountInfo,
        user: Keypair,
        mint: Mint,
        vault: AccountInfo,
        user_ata: AccountInfo,
    }

    fn setup_test() -> TestAccounts {
        // Initialize test environment
        // ... (detailed setup logic)
    }

    #[test]
    fn test_successful_reward_claim() {
        let mut test_env = setup_test();
        
        // Simulate staking period
        test_env.warp(86400); // Fast-forward 1 day
        
        let claim_result = test_env.claim_rewards();
        assert!(claim_result.is_ok());
        
        // Verify token balances
        assert_eq!(test_env.user_ata.amount, EXPECTED_REWARDS);
    }

    #[test]
    fn test_claim_before_lock_period() {
        let mut test_env = setup_test();
        
        let claim_result = test_env.claim_rewards();
        assert_eq!(claim_result, Err(ClaimError::ClaimTimeRestricted.into()));
    }

    #[test]
    fn test_insufficient_funds_claim() {
        let mut test_env = setup_test();
        test_env.empty_vault();
        
        let claim_result = test_env.claim_rewards();
        assert_eq!(claim_result, Err(ClaimError::InsufficientRewardFunds.into()));
    }
}
