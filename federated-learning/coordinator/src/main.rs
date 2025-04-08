//! Umazen Federated Learning Core - Decentralized Collaborative Training

#![deny(
    unsafe_code,
    missing_docs,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]
use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        system_instruction,
    },
};
use anchor_spl::{
    token::{self, Mint, Token, TokenAccount, Transfer},
};
use std::convert::TryInto;

declare_id!("FEDe5JfDcn4za21FLpZ5qXfGxkh4Deg7EKprcjZo9Q6B");

// Program Constants
const MIN_STAKE: u64 = 1_000_000; // 1 UMZ token
const MAX_PARTICIPANTS: usize = 100;
const MODEL_DIMENSIONS: usize = 1024;
const CONTRIBUTION_PROOF_SIZE: usize = 64;

#[program]
mod federated_learning {
    use super::*;

    // Initialize a new FL task
    pub fn initialize_task(
        ctx: Context<InitializeTask>,
        task_params: TaskParams,
    ) -> Result<()> {
        let task = &mut ctx.accounts.task;
        task.authority = *ctx.accounts.authority.key;
        task.model_hash = task_params.model_hash;
        task.reward_pool = task_params.reward_pool;
        task.min_updates = task_params.min_updates;
        task.current_round = 0;
        task.status = TaskStatus::Active;
        task.updated_at = Clock::get()?.unix_timestamp;
        
        Ok(())
    }

    // Register participant for a task
    pub fn register_participant(
        ctx: Context<RegisterParticipant>,
        stake_amount: u64,
    ) -> Result<()> {
        require!(stake_amount >= MIN_STAKE, FlError::InsufficientStake);
        
        let participant = &mut ctx.accounts.participant;
        participant.authority = *ctx.accounts.authority.key;
        participant.task = ctx.accounts.task.key();
        participant.stake_amount = stake_amount;
        participant.status = ParticipantStatus::Registered;
        
        // Transfer stake to escrow
        let cpi_accounts = Transfer {
            from: ctx.accounts.participant_token_account.to_account_info(),
            to: ctx.accounts.task_escrow.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );
        
        token::transfer(cpi_ctx, stake_amount)?;
        
        Ok(())
    }

    // Submit model updates with ZK proof
    pub fn submit_update(
        ctx: Context<SubmitUpdate>,
        model_update: [f32; MODEL_DIMENSIONS],
        contribution_proof: [u8; CONTRIBUTION_PROOF_SIZE],
    ) -> Result<()> {
        let task = &mut ctx.accounts.task;
        let participant = &mut ctx.accounts.participant;
        
        require!(task.status == TaskStatus::Active, FlError::TaskNotActive);
        require!(
            participant.status == ParticipantStatus::Registered,
            FlError::InvalidParticipant
        );
        
        // Verify ZK proof of valid computation
        verify_contribution_proof(
            &task.model_hash,
            &model_update,
            &contribution_proof,
        )?;
        
        // Store update
        let update = &mut ctx.accounts.model_update;
        update.round = task.current_round;
        update.participant = participant.key();
        update.weights = model_update;
        update.proof = contribution_proof;
        update.verified = true;
        
        // Track participation
        task.contributions += 1;
        participant.contributions += 1;
        
        // Check if ready for aggregation
        if task.contributions >= task.min_updates {
            task.status = TaskStatus::Aggregating;
        }
        
        Ok(())
    }

    // Aggregate model updates
    pub fn aggregate_model(
        ctx: Context<AggregateModel>,
    ) -> Result<()> {
        let task = &mut ctx.accounts.task;
        let model = &mut ctx.accounts.global_model;
        
        require!(task.status == TaskStatus::Aggregating, FlError::InvalidTaskState);
        
        // Load all verified updates
        let updates = ModelUpdate::load_verified_updates(task.current_round)?;
        
        // Calculate federated average
        let aggregated_weights = compute_federated_average(updates);
        
        // Update global model
        model.current_round = task.current_round + 1;
        model.weights = aggregated_weights;
        model.updated_at = Clock::get()?.unix_timestamp;
        
        // Prepare next round
        task.current_round += 1;
        task.contributions = 0;
        task.status = TaskStatus::Active;
        
        Ok(())
    }

    // Distribute rewards based on contributions
    pub fn distribute_rewards(
        ctx: Context<DistributeRewards>,
    ) -> Result<()> {
        let task = &mut ctx.accounts.task;
        require!(task.status == TaskStatus::Completed, FlError::InvalidTaskState);
        
        // Calculate reward shares
        let participants = Participant::load_active(task.key())?;
        let total_contributions: u64 = participants.iter()
            .map(|p| p.contributions)
            .sum();
        
        for participant in participants {
            let share = (participant.contributions as f64) / (total_contributions as f64);
            let reward = (task.reward_pool as f64 * share) as u64;
            
            // Transfer reward
            let seeds = &[b"task_escrow", &task.key().to_bytes()];
            let (_, bump) = Pubkey::find_program_address(seeds, ctx.program_id);
            let signer_seeds = &[&seeds[0], &seeds[1], &[bump]];
            
            let cpi_accounts = Transfer {
                from: ctx.accounts.task_escrow.to_account_info(),
                to: ctx.accounts.participant_token_account.to_account_info(),
                authority: ctx.accounts.task_escrow.to_account_info(),
            };
            
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            );
            
            token::transfer(cpi_ctx, reward)?;
        }
        
        task.status = TaskStatus::Completed;
        Ok(())
    }
}

// Data Structures
#[account]
#[derive(Default)]
pub struct Task {
    pub authority: Pubkey,
    pub model_hash: String,
    pub reward_pool: u64,
    pub min_updates: u64,
    pub current_round: u32,
    pub contributions: u64,
    pub status: TaskStatus,
    pub updated_at: i64,
}

#[account]
#[derive(Default)]
pub struct Participant {
    pub task: Pubkey,
    pub authority: Pubkey,
    pub stake_amount: u64,
    pub contributions: u64,
    pub status: ParticipantStatus,
}

#[account]
pub struct ModelUpdate {
    pub round: u32,
    pub participant: Pubkey,
    pub weights: [f32; MODEL_DIMENSIONS],
    pub proof: [u8; CONTRIBUTION_PROOF_SIZE],
    pub verified: bool,
}

#[account]
pub struct GlobalModel {
    pub current_round: u32,
    pub weights: [f32; MODEL_DIMENSIONS],
    pub updated_at: i64,
}

// Context Structures
#[derive(Accounts)]
pub struct InitializeTask<'info> {
    #[account(init, payer = authority, space = 8 + 256)]
    pub task: Account<'info, Task>,
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        seeds = [b"task_escrow", task.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = task,
    )]
    pub task_escrow: Account<'info, TokenAccount>,
    pub token_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterParticipant<'info> {
    #[account(mut)]
    pub task: Account<'info, Task>,
    #[account(init, payer = authority, space = 8 + 128)]
    pub participant: Account<'info, Participant>,
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        constraint = participant_token_account.owner == *authority.key,
        constraint = participant_token_account.mint == token_mint.key(),
    )]
    pub participant_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub task_escrow: Account<'info, TokenAccount>,
    pub token_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitUpdate<'info> {
    #[account(mut)]
    pub task: Account<'info, Task>,
    #[account(mut)]
    pub participant: Account<'info, Participant>,
    #[account(init, payer = authority, space = 8 + ModelUpdate::LEN)]
    pub model_update: Account<'info, ModelUpdate>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// Helper Implementations
impl ModelUpdate {
    const LEN: usize = 4 + 32 + (MODEL_DIMENSIONS * 4) + CONTRIBUTION_PROOF_SIZE + 1;
}

// Error Handling
#[error_code]
pub enum FlError {
    #[msg("Insufficient stake amount")]
    InsufficientStake,
    #[msg("Task not in active state")]
    TaskNotActive,
    #[msg("Invalid participant status")]
    InvalidParticipant,
    #[msg("Invalid contribution proof")]
    InvalidProof,
    #[msg("Task not ready for aggregation")]
    InvalidAggregationState,
    #[msg("Invalid reward distribution state")]
    InvalidRewardState,
}

// State Enums
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Active,
    Aggregating,
    Completed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum ParticipantStatus {
    Registered,
    Active,
    Slashed,
}

// Private Functions
fn verify_contribution_proof(
    model_hash: &str,
    weights: &[f32; MODEL_DIMENSIONS],
    proof: &[u8; CONTRIBUTION_PROOF_SIZE],
) -> Result<()> {
    // Implementation would integrate with ZK verifier
    // For production use actual proof verification
    Ok(())
}

fn compute_federated_average(updates: Vec<ModelUpdate>) -> [f32; MODEL_DIMENSIONS] {
    let mut aggregated = [0.0; MODEL_DIMENSIONS];
    let num_updates = updates.len() as f32;
    
    for update in updates {
        for (i, &w) in update.weights.iter().enumerate() {
            aggregated[i] += w / num_updates;
        }
    }
    
    aggregated
}
