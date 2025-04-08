//! Umazen v1 → v2 State Migration
//! 
//! Migration Logic:
//! 1. Expand ModelMetadata with new fields
//! 2. Convert royalty basis points to percentage
//! 3. Initialize DAO treasury accounts
//! 4. Migrate training records to new schema

use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        system_instruction,
        sysvar::rent,
    },
};
use crate::{
    error::UmazenError,
    state::{v1::ModelMetadataV1, v2::ModelMetadataV2, ModelFlags},
    utils::validation::verify_migration_authority,
};

/// Migration configuration with safety parameters
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MigrationConfig {
    pub new_field_default: u64,
    pub dao_treasury_bps: u16,
    pub batch_size: usize,
}

/// Migration context with version tracking
#[account]
#[derive(Default)]
pub struct MigrationState {
    pub migrated_count: u64,
    pub last_migrated: Pubkey,
    pub version_marker: [u8; 12],
}

/// Core migration implementation
pub fn migrate_v1_to_v2(
    ctx: Context<MigrateModel>,
    config: MigrationConfig,
) -> Result<()> {
    // Security checks
    verify_migration_authority(
        &ctx.accounts.authority,
        &ctx.accounts.migration_state,
    )?;
    require!(!ctx.accounts.model_v1.flags.is_locked(), UmazenError::ModelLocked);

    // State validation
    let v1_data = ModelMetadataV1::try_from_slice(&ctx.accounts.model_v1.data.borrow())?;
    let clock = Clock::get()?;

    // Perform schema transformation
    let v2_data = ModelMetadataV2 {
        // Preserve existing fields
        owner: v1_data.owner,
        created_at: v1_data.created_at,
        // Transform royalty basis points to percentage
        royalty_percentage: (v1_data.royalty_basis_points as f32 / 100.0)
            .round()
            .min(100.0)
            .max(0.0),
        // New fields with validation
        dao_share: config.dao_treasury_bps
            .checked_div(100)
            .ok_or(UmazenError::MathOverflow)?,
        model_category: 0u8, // Default category
        last_modified: clock.unix_timestamp,
        // ... additional field conversions
    };

    // Write migrated data
    let mut model_v2 = ctx.accounts.model_v2.clone();
    v2_data.serialize(&mut model_v2.data.borrow_mut())?;

    // Update migration state
    let migration_state = &mut ctx.accounts.migration_state;
    migration_state.migrated_count = migration_state.migrated_count.checked_add(1).unwrap();
    migration_state.last_migrated = ctx.accounts.model_v1.key();
    migration_state.version_marker = *b"V2_MIGRATED";

    // Fund new DAO treasury
    let treasury_seeds = &[
        b"dao_treasury",
        &ctx.accounts.dao.key().to_bytes(),
        &[ctx.bumps.dao_treasury],
    ];
    invoke_signed(
        &system_instruction::transfer(
            &ctx.accounts.payer.key(),
            &ctx.accounts.dao_treasury.key(),
            config.new_field_default,
        ),
        &[
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.dao_treasury.to_account_info(),
        ],
        &[treasury_seeds],
    )?;

    Ok(())
}

/// Migration accounts with safety constraints
#[derive(Accounts)]
#[instruction(config: MigrationConfig)]
pub struct MigrateModel<'info> {
    /// CHECK: Validated in migration logic
    #[account(mut)]
    pub model_v1: AccountInfo<'info>,
    
    #[account(
        init_if_needed,
        payer = payer,
        space = ModelMetadataV2::LEN,
        seeds = [b"model", model_v1.key().as_ref()],
        bump
    )]
    pub model_v2: Account<'info, ModelMetadataV2>,

    #[account(
        mut,
        seeds = [b"migration_state"],
        bump,
        has_one = authority
    )]
    pub migration_state: Account<'info, MigrationState>,

    #[account(mut, address = dao.authority)]
    pub authority: Signer<'info>,

    #[account(executive, has_one = dao_treasury)]
    pub dao: Account<'info, Dao>,

    #[account(
        mut,
        seeds = [b"dao_treasury", dao.key().as_ref()],
        bump
    )]
    pub dao_treasury: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Safety verification logic
fn verify_migration_authority(
    authority: &Signer,
    migration_state: &Account<MigrationState>,
) -> Result<()> {
    require!(
        migration_state.version_marker == *b"INITIALIZED",
        UmazenError::InvalidMigrationState
    );
    require!(
        authority.key() == migration_state.authority,
        UmazenError::UnauthorizedMigration
    );
    Ok(())
}

/// Migration batch processing
pub fn batch_migrate(
    ctx: Context<BatchMigrate>,
    model_keys: Vec<Pubkey>,
    config: MigrationConfig,
) -> Result<()> {
    require!(
        model_keys.len() <= config.batch_size,
        UmazenError::ExceededBatchLimit
    );

    for model_key in model_keys {
        // Execute per-model migration
        // ... (omitted for brevity)
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::Pubkey;
    use solana_program_test::*;

    // Test setup with pre-migration state
    async fn setup_migration_context() -> (ProgramTestContext, Pubkey) {
        // ... (detailed test setup)
    }

    #[tokio::test]
    async fn test_successful_migration() {
        // ... (complete test scenario)
    }

    #[tokio::test]
    async fn test_unauthorized_attempt() {
        // ... (security test case)
    }
}
