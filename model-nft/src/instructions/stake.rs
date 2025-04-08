//! Model Staking Program - Stake AI models to earn rewards

use anchor_lang::{
    prelude::*,
    solana_program::{clock::UnixTimestamp, program::invoke, sysvar::clock::Clock},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, spl_token::instruction::transfer_checked, Mint, Token, TokenAccount},
};
use crate::{governance::StakeConfig, MintConfig};

declare_id!("Stakmazn111111111111111111111111111111111111");

#[account]
#[derive(Default)]
pub struct StakeAccount {
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub reward_debt: u64,
    pub start_time: UnixTimestamp,
    pub last_claim_time: UnixTimestamp,
    pub lock_duration: u64,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(amount: u64, lock_duration: u64)]
pub struct StakeModel<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    
    #[account(
        init_if_needed,
        payer = owner,
        space = 8 + StakeAccount::LEN,
        seeds = [
            b"stake",
            owner.key().as_ref(),
            mint.key().as_ref(),
            &lock_duration.to_le_bytes()
        ],
        bump
    )]
    pub stake_account: Account<'info, StakeAccount>,
    
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = owner,
    )]
    pub owner_ata: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = stake_pool,
    )]
    pub stake_pool_ata: Account<'info, TokenAccount>,
    
    #[account(
        seeds = [b"stake_pool", config.key().as_ref()],
        bump = config.stake_pool_bump,
    )]
    pub stake_pool: SystemAccount<'info>,
    
    #[account(
        seeds = [b"stake_config", config.authority.key().as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,
    
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[program]
pub mod model_staking {
    use super::*;

    /// Stake AI model tokens to earn rewards
    pub fn stake(ctx: Context<StakeModel>, amount: u64, lock_duration: u64) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let config = &ctx.accounts.config;
        
        require!(amount >= config.min_stake_amount, StakeError::InsufficientAmount);
        require!(
            lock_duration >= config.min_lock_duration && 
            lock_duration <= config.max_lock_duration,
            StakeError::InvalidLockDuration
        );

        // Transfer tokens to stake pool
        let transfer_ix = transfer_checked(
            ctx.accounts.token_program.key(),
            ctx.accounts.owner_ata.key(),
            ctx.accounts.mint.key(),
            ctx.accounts.stake_pool_ata.key(),
            ctx.accounts.owner.key(),
            &[],
            amount,
            ctx.accounts.mint.decimals,
        )?;

        invoke(
            &transfer_ix,
            &[
                ctx.accounts.owner_ata.to_account_info(),
                ctx.accounts.stake_pool_ata.to_account_info(),
                ctx.accounts.owner.to_account_info(),
                ctx.accounts.mint.to_account_info(),
            ],
        )?;

        // Initialize stake account
        let clock = Clock::get()?;
        stake_account.owner = ctx.accounts.owner.key();
        stake_account.mint = ctx.accounts.mint.key();
        stake_account.amount = amount;
        stake_account.start_time = clock.unix_timestamp;
        stake_account.last_claim_time = clock.unix_timestamp;
        stake_account.lock_duration = lock_duration;
        stake_account.reward_debt = amount
            .checked_mul(config.acc_reward_per_share)
            .ok_or(StakeError::CalculationOverflow)?;

        Ok(())
    }

    /// Claim staking rewards
    pub fn claim_rewards(ctx: Context<StakeModel>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let config = &ctx.accounts.config;
        let clock = Clock::get()?;
        
        require!(
            clock.unix_timestamp >= stake_account.start_time + stake_account.lock_duration as i64,
            StakeError::LockNotExpired
        );

        let elapsed_time = clock.unix_timestamp
            .checked_sub(stake_account.last_claim_time)
            .ok_or(StakeError::InvalidTimeCalculation)? as u64;
        
        let reward = stake_account.amount
            .checked_mul(config.reward_rate_per_second)
            .and_then(|v| v.checked_mul(elapsed_time))
            .ok_or(StakeError::CalculationOverflow)?;

        // Transfer rewards
        let transfer_ix = transfer_checked(
            ctx.accounts.token_program.key(),
            ctx.accounts.stake_pool_ata.key(),
            ctx.accounts.mint.key(),
            ctx.accounts.owner_ata.key(),
            ctx.accounts.stake_pool.key(),
            &[],
            reward,
            ctx.accounts.mint.decimals,
        )?;

        invoke(
            &transfer_ix,
            &[
                ctx.accounts.stake_pool_ata.to_account_info(),
                ctx.accounts.owner_ata.to_account_info(),
                ctx.accounts.stake_pool.to_account_info(),
                ctx.accounts.mint.to_account_info(),
            ],
        )?;

        stake_account.last_claim_time = clock.unix_timestamp;
        stake_account.reward_debt = stake_account.amount
            .checked_mul(config.acc_reward_per_share)
            .ok_or(StakeError::CalculationOverflow)?;

        Ok(())
    }

    /// Unstake model tokens after lock period
    pub fn unstake(ctx: Context<StakeModel>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let clock = Clock::get()?;
        
        require!(
            clock.unix_timestamp >= stake_account.start_time + stake_account.lock_duration as i64,
            StakeError::LockNotExpired
        );

        // Transfer back staked amount
        let transfer_ix = transfer_checked(
            ctx.accounts.token_program.key(),
            ctx.accounts.stake_pool_ata.key(),
            ctx.accounts.mint.key(),
            ctx.accounts.owner_ata.key(),
            ctx.accounts.stake_pool.key(),
            &[],
            stake_account.amount,
            ctx.accounts.mint.decimals,
        )?;

        invoke(
            &transfer_ix,
            &[
                ctx.accounts.stake_pool_ata.to_account_info(),
                ctx.accounts.owner_ata.to_account_info(),
                ctx.accounts.stake_pool.to_account_info(),
                ctx.accounts.mint.to_account_info(),
            ],
        )?;

        // Close stake account
        let stake_account_info = ctx.accounts.stake_account.to_account_info();
        **stake_account_info.lamports.borrow_mut() = 0;
        **ctx.accounts.owner.lamports.borrow_mut() += stake_account_info.lamports();

        Ok(())
    }
}

#[error_code]
pub enum StakeError {
    #[msg("Insufficient stake amount")]
    InsufficientAmount,
    #[msg("Invalid lock duration")]
    InvalidLockDuration,
    #[msg("Lock period not expired")]
    LockNotExpired,
    #[msg("Calculation overflow")]
    CalculationOverflow,
    #[msg("Invalid time calculation")]
    InvalidTimeCalculation,
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Stake pool underflow")]
    StakePoolUnderflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{solana_program::system_instruction, test::*};
    use spl_token::state::Mint;

    #[test]
    fn test_full_staking_cycle() {
        let mut test_env = TestEnvironment::new();
        let user = test_env.create_user(1_000_000_000);
        let config = test_env.initialize_config();

        // Stake
        let stake_amount = 100;
        let lock_duration = 86400; // 1 day
        let stake_result = test_env.stake(user, stake_amount, lock_duration);
        assert!(stake_result.is_ok());
        
        // Fast-forward time
        test_env.set_clock(lock_duration + 1);
        
        // Claim rewards
        let claim_result = test_env.claim_rewards(user);
        assert!(claim_result.is_ok());
        
        // Unstake
        let unstake_result = test_env.unstake(user);
        assert!(unstake_result.is_ok());
        
        // Verify balances
        let user_balance = test_env.get_token_balance(user);
        assert!(user_balance > stake_amount); // Should have rewards
    }
}
