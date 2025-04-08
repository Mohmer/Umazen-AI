//! Umazen v1 → v2 State Migration
//!
//! Migration Scope:
//! 1. ModelMetadata struct expansion
//! 2. DAO authority transition to multisig
//! 3. TrainingState format optimization
//! 4. Royalty standard upgrade

use anchor_lang::{
    prelude::*,
    solana_program::{
        program_memory::sol_memcpy,
        sysvar::rent,
    },
};
use umazen_program::{
    self,
    accounts::{
        v1::ModelMetadataV1,
        v2::{ModelMetadataV2, ModelTrainingStateV2},
        UpgradeAuthority,
    },
    constants,
    error::MigrationError,
    state::{
        v2::{ComputeRequirementsV2, ModelLicense},
        MigrationFlags,
    },
    utils::assert_valid_metadata,
};

/// Performs atomic migration of a model from v1 to v2
#[derive(Accounts)]
pub struct MigrateModelV1ToV2<'info> {
    /// CHECK: Legacy metadata account validated in handler
    #[account(
        mut,
        constraint = metadata_v1.owner == umazen_program::ID
    )]
    pub metadata_v1: AccountInfo<'info>,
    
    #[account(
        init,
        payer = payer,
        space = ModelMetadataV2::LEN,
        seeds = [
            b"metadata",
            metadata_v1.model_nft.as_ref()
        ],
        bump,
        owner = umazen_program::ID
    )]
    pub metadata_v2: Account<'info, ModelMetadataV2>,
    
    #[account(
        mut,
        has_one = legacy_dao,
        seeds = [b"upgrade_authority"],
        bump = upgrade_authority.bump,
    )]
    pub upgrade_authority: Account<'info, UpgradeAuthority>,
    
    /// Legacy DAO authority (v1)
    pub legacy_dao: Signer<'info>,
    
    /// New multisig authority (v2)
    /// CHECK: Validated against upgrade_authority
    pub new_multisig: AccountInfo<'info>,
    
    #[account(mut)]
    pub payer: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

pub fn migrate_model_v1_to_v2(ctx: Context<MigrateModelV1ToV2>) -> Result<()> {
    // Phase 1: Security checks
    require!(
        ctx.accounts.upgrade_authority.migration_flags
            .contains(MigrationFlags::MODEL_METADATA_UPGRADE),
        MigrationError::MigrationNotAllowed
    );
    
    // Verify new multisig is registered
    require!(
        ctx.accounts.upgrade_authority.new_authority
            == ctx.accounts.new_multisig.key(),
        MigrationError::InvalidUpgradeAuthority
    );

    // Phase 2: Deserialize legacy data
    let metadata_v1 = ModelMetadataV1::try_deserialize(
        &mut &ctx.accounts.metadata_v1.data.borrow()[..]
    )?;
    
    // Validate legacy data integrity
    assert_valid_metadata(&metadata_v1)?;

    // Phase 3: Migrate to v2 format
    let metadata_v2 = ModelMetadataV2 {
        model_nft: metadata_v1.model_nft,
        metadata_uri: metadata_v1.metadata_uri,
        royalty_basis_points: metadata_v1.royalty_basis_points,
        architecture: metadata_v1.architecture,
        model_hash: metadata_v1.model_hash,
        compute_requirements: ComputeRequirementsV2 {
            min_vram: metadata_v1.compute_requirements.min_vram,
            min_compute_capability: metadata_v1
                .compute_requirements.min_compute_capability,
            required_instructions: metadata_v1
                .compute_requirements.required_instructions,
            // New field with default
            max_power_consumption: constants::DEFAULT_MAX_POWER,
        },
        // New v2 fields
        license: ModelLicense {
            license_type: constants::DEFAULT_LICENSE,
            commercial_use: false,
        },
        migrated_at: Clock::get()?.unix_timestamp,
    };

    // Phase 4: Write new state
    ctx.accounts.metadata_v2.set_inner(metadata_v2);

    // Phase 5: Cleanup legacy account
    let dest_start = 0;
    let src_start = 0;
    let data_len = ctx.accounts.metadata_v1.data_len();
    
    // Securely erase v1 data
    sol_memcpy(
        &mut ctx.accounts.metadata_v1.try_borrow_mut_data()?[dest_start..],
        &[0u8; ModelMetadataV1::LEN],
        data_len,
    );
    
    // Phase 6: Transfer authority
    ctx.accounts.upgrade_authority.new_authority = 
        ctx.accounts.new_multisig.key();
    ctx.accounts.upgrade_authority.migration_flags.insert(
        MigrationFlags::MODEL_METADATA_UPGRADE_COMPLETE
    );

    Ok(())
}

/// Security validations for legacy metadata
fn assert_valid_metadata(metadata: &ModelMetadataV1) -> Result<()> {
    require!(
        metadata.royalty_basis_points <= 10_000,
        MigrationError::InvalidRoyalty
    );
    
    require!(
        !metadata.architecture.is_empty() 
            && metadata.architecture.len() <= 50,
        MigrationError::InvalidArchitecture
    );
    
    Ok(())
}
