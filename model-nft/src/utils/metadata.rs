//! Model Metadata Management - On-chain/off-chain hybrid storage system

use anchor_lang::{
    prelude::*,
    solana_program::{
        program_memory::sol_memcmp,
        pubkey::Pubkey,
        sysvar::instructions::{get_instruction_relative, InstructionSysvarAccount},
    },
};
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{MetadataAccount, MasterEditionAccount, Metadata},
    token::Token,
};
use arrayref::{array_ref, array_refs};
use ipfs_api::IpfsClient;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Core metadata structure stored on-chain
#[account]
#[derive(Default, Debug)]
pub struct ModelMetadata {
    /// Model owner's base58 encoded public key
    pub owner: Pubkey,
    /// Timestamp of metadata creation
    pub created_at: i64,
    /// Last update timestamp  
    pub updated_at: i64,
    /// IPFS CIDv1 for off-chain metadata JSON
    pub metadata_uri: String,
    /// SHA-256 hash of model weights
    pub model_hash: [u8; 32],
    /// Model architecture identifier (e.g. "resnet50")
    pub architecture: String,
    /// Training hyperparameters
    pub hyperparameters: Hyperparameters,
    /// Performance metrics
    pub metrics: PerformanceMetrics,
    /// Royalty basis points (0-10000)
    pub royalty_basis_points: u16,
    /// Current model version
    pub version: u32,
    /// DAO governance flags
    pub governance: GovernanceFlags,
}

/// Training hyperparameters sub-structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct Hyperparameters {
    pub learning_rate: f32,
    pub batch_size: u32,
    pub epochs: u32,
    pub optimizer: String,  // "adam", "sgd", etc.
    pub loss_function: String,
}

/// Model performance metrics  
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct PerformanceMetrics {
    pub accuracy: f32,
    pub precision: f32,
    pub recall: f32,
    pub inference_time_ms: f32,
    pub f1_score: f32,
}

/// Governance flags for DAO control
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct GovernanceFlags {
    pub updatable: bool,
    pub transferable: bool,
    pub requires_dao_approval: bool,
}

/// Off-chain metadata JSON schema stored on IPFS
#[derive(Serialize, Deserialize, Debug)]
pub struct OffchainMetadata {
    pub name: String,
    pub description: String,
    pub image: String,
    pub attributes: Vec<Trait>,
    pub external_url: String,
    pub model_spec: ModelSpecification,
}

/// Detailed model specification
#[derive(Serialize, Deserialize, Debug)]
pub struct ModelSpecification {
    pub framework: String,
    pub input_shape: Vec<u32>,
    pub output_shape: Vec<u32>,
    pub ops_requirements: OpsRequirements,
    pub quantization: String,
}

/// Hardware requirements
#[derive(Serialize, Deserialize, Debug)]
pub struct OpsRequirements {
    pub min_vram_gb: u8,
    pub compute_capability: f32,
    pub supported_accelerators: Vec<String>,
}

/// Model trait attributes
#[derive(Serialize, Deserialize, Debug)]
pub struct Trait {
    pub trait_type: String,
    pub value: String,
}

/// Initialize model metadata
#[derive(Accounts)]
#[instruction(metadata_uri: String, model_hash: [u8;32])]
pub struct InitializeMetadata<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + ModelMetadata::LEN,
        seeds = [b"metadata", model_nft.key().as_ref()],
        bump
    )]
    pub metadata_account: Account<'info, ModelMetadata>,
    
    /// Associated NFT representing model ownership
    #[account(mut)]
    pub model_nft: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub owner: Signer<'info>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

/// Update existing metadata
#[derive(Accounts)]
#[instruction(new_uri: String)]
pub struct UpdateMetadata<'info> {
    #[account(
        mut,
        has_one = owner @ MetadataError::InvalidAuthority,
        constraint = metadata_account.governance.updatable || dao_approval.is_some() 
            @ MetadataError::UpdateForbidden
    )]
    pub metadata_account: Account<'info, ModelMetadata>,
    
    #[account(mut)]
    pub owner: Signer<'info>,
    
    /// Optional DAO approval account
    #[account(
        constraint = dao_approval.is_some() == metadata_account.governance.requires_dao_approval
    )]
    pub dao_approval: Option<Account<'info, DaoApproval>>,
}

impl<'info> InitializeMetadata<'info> {
    pub fn initialize(
        &mut self,
        metadata_uri: String,
        model_hash: [u8; 32],
        architecture: String,
        hyperparameters: Hyperparameters,
        royalty_basis_points: u16,
    ) -> Result<()> {
        let clock = Clock::get()?;
        
        self.metadata_account.owner = self.owner.key();
        self.metadata_account.created_at = clock.unix_timestamp;
        self.metadata_account.updated_at = clock.unix_timestamp;
        self.metadata_account.metadata_uri = metadata_uri;
        self.metadata_account.model_hash = model_hash;
        self.metadata_account.architecture = architecture;
        self.metadata_account.hyperparameters = hyperparameters;
        self.metadata_account.royalty_basis_points = royalty_basis_points;
        self.metadata_account.version = 1;
        
        // Initialize governance flags
        self.metadata_account.governance = GovernanceFlags {
            updatable: true,
            transferable: true,
            requires_dao_approval: false,
        };
        
        Ok(())
    }
}

impl<'info> UpdateMetadata<'info> {
    pub fn update_metadata(
        &mut self,
        new_uri: String,
        new_hash: [u8; 32],
        new_version: u32,
    ) -> Result<()> {
        let clock = Clock::get()?;
        
        // Verify hash changes require DAO approval
        if self.metadata_account.model_hash != new_hash 
            && self.metadata_account.governance.requires_dao_approval
        {
            require!(self.dao_approval.is_some(), MetadataError::DaoApprovalRequired);
        }
        
        self.metadata_account.metadata_uri = new_uri;
        self.metadata_account.model_hash = new_hash;
        self.metadata_account.version = new_version;
        self.metadata_account.updated_at = clock.unix_timestamp;
        
        Ok(())
    }
}

/// IPFS integration utilities
pub struct IpfsManager;

impl IpfsManager {
    /// Upload metadata JSON to IPFS
    pub async fn upload_metadata(metadata: OffchainMetadata) -> Result<String> {
        let client = IpfsClient::default();
        let json = serde_json::to_vec(&metadata)?;
        let cid = client.add(json).await?;
        Ok(cid)
    }

    /// Fetch metadata from IPFS
    pub async fn fetch_metadata(cid: &str) -> Result<OffchainMetadata> {
        let client = IpfsClient::default();
        let data = client.cat(cid).await?;
        let metadata: OffchainMetadata = serde_json::from_slice(&data)?;
        Ok(metadata)
    }
}

/// Custom error codes
#[error_code]
pub enum MetadataError {
    #[msg("Metadata account already initialized")]
    AlreadyInitialized,
    #[msg("Invalid update authority")]
    InvalidAuthority,
    #[msg("Metadata URI exceeds maximum length")]
    UriTooLong,
    #[msg("DAO approval required for this operation")]
    DaoApprovalRequired,
    #[msg("Metadata updates are locked")]
    UpdateForbidden,
    #[msg("Invalid IPFS CID format")]
    InvalidCid,
    #[msg("Model hash verification failed")]
    HashMismatch,
}

// Validation utilities
impl ModelMetadata {
    /// Validate metadata format
    pub fn validate(&self) -> Result<()> {
        // Check URI length
        require!(self.metadata_uri.len() <= 200, MetadataError::UriTooLong);
        
        // Validate CID format
        require!(
            cid::Cid::from_str(&self.metadata_uri).is_ok(),
            MetadataError::InvalidCid
        );
        
        // Verify royalty basis points
        require!(
            self.royalty_basis_points <= 10000,
            MetadataError::InvalidRoyalty
        );
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::test::*;
    use solana_program::clock::Epoch;

    #[test]
    fn test_metadata_initialization() {
        let mut program = Test::new();
        let owner = program.create_account(100);
        let nft_account = program.create_token_account();
        
        let metadata_uri = "QmXYZ".to_string();
        let model_hash = [0u8; 32];
        
        let result = program
            .run(
                InitializeMetadata::new(
                    &program,
                    owner.pubkey(),
                    nft_account.pubkey(),
                ),
                |ctx| {
                    ctx.accounts.initialize(
                        metadata_uri.clone(),
                        model_hash,
                        "resnet50".to_string(),
                        Hyperparameters::default(),
                        500, // 5% royalty
                    )
                },
            );
        
        assert!(result.is_ok());
        let metadata = program.get_account::<ModelMetadata>(metadata_account);
        assert_eq!(metadata.metadata_uri, metadata_uri);
    }
}
