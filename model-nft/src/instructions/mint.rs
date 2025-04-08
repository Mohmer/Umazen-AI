//! Model Minting Program - Core logic for creating AI model NFTs

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata},
    token::{spl_token::instruction::initialize_mint, Mint, Token, TokenAccount},
};
use mpl_token_metadata::{
    instruction::create_master_edition_v3,
    state::{Collection, DataV2, Creator, Uses},
};
use sha3::{Digest, Keccak256};

declare_id!("Mintmazn1111111111111111111111111111111111111");

// --------------------------
// Core Data Structures
// --------------------------

#[account]
#[derive(Default)]
pub struct MintConfig {
    pub version: u8,
    pub authority: Pubkey,
    pub model_hash: [u8; 32],  // Keccak-256 hash
    pub model_uri: String,     // Arweave/IPFS URI
    pub mint_count: u64,
    pub bump: u8,
    pub creators: Vec<Creator>,
    pub collection: Option<Collection>,
    pub uses: Option<Uses>,
}

#[account]
#[derive(Default)]
pub struct ModelMetadata {
    pub framework: ModelFramework,
    pub task_type: TaskType,
    pub accuracy: f32,
    pub precision: f32,
    pub recall: f32,
    pub f1_score: f32,
    pub training_data_hash: [u8; 32],
    pub inference_time: u64,  // ms
    pub hardware_reqs: HardwareRequirements,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub enum ModelFramework {
    #[default]
    TensorFlow,
    PyTorch,
    ONNX,
    Custom(String),
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub enum TaskType {
    #[default]
    Classification,
    Generation,
    Regression,
    Clustering,
    Reinforcement,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct HardwareRequirements {
    pub min_vram: u64,    // MB
    pub min_ram: u64,     // MB
    pub cuda_support: bool,
    pub rocm_support: bool,
}

// --------------------------
// Instruction Contexts
// --------------------------

#[derive(Accounts)]
#[instruction(metadata_uri: String)]
pub struct InitializeMintConfig<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + MintConfig::LEN,
        seeds = [b"mint_config", authority.key().as_ref()],
        bump
    )]
    pub config: Account<'info, MintConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(metadata: ModelMetadata)]
pub struct MintModelNft<'info> {
    #[account(mut, has_one = authority)]
    pub config: Account<'info, MintConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    
    // Mint accounts
    #[account(
        init,
        payer = authority,
        mint::decimals = 0,
        mint::authority = config,
        mint::freeze_authority = config,
    )]
    pub mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        associated_token::mint = mint,
        associated_token::authority = authority,
    )]
    pub token_account: Account<'info, TokenAccount>,
    
    // Metadata accounts
    #[account(
        mut,
        address = Metadata::find_pda(&mint.key()).0
    )]
    pub metadata_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = MasterEdition::find_pda(&mint.key()).0
    )]
    pub master_edition: UncheckedAccount<'info>,
    
    // Programs
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
}

// --------------------------
// Program Logic
// --------------------------

#[program]
pub mod model_mint {
    use super::*;

    /// Initialize mint configuration
    pub fn initialize_config(
        ctx: Context<InitializeMintConfig>,
        model_hash: [u8; 32],
        metadata_uri: String,
        creators: Vec<Creator>,
        collection: Option<Collection>,
        uses: Option<Uses>,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        // Validate creators
        let mut total_share = 0;
        for creator in &creators {
            total_share += creator.share;
            require!(creator.verified == false, MintError::CreatorAlreadyVerified);
        }
        require!(total_share == 100, MintError::InvalidCreatorShare);
        
        // Initialize config
        config.version = 1;
        config.authority = *ctx.accounts.authority.key;
        config.model_hash = model_hash;
        config.model_uri = metadata_uri;
        config.creators = creators;
        config.collection = collection;
        config.uses = uses;
        config.bump = *ctx.bumps.get("config").ok_or(MintError::BumpNotFound)?;
        
        Ok(())
    }

    /// Mint AI Model NFT with full metadata
    pub fn mint_model_nft(
        ctx: Context<MintModelNft>,
        metadata: ModelMetadata,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let mint = &ctx.accounts.mint;
        
        // Verify model hash
        let mut hasher = Keccak256::new();
        hasher.update(&metadata.try_to_vec()?);
        let computed_hash = hasher.finalize().into();
        require!(config.model_hash == computed_hash, MintError::HashMismatch);
        
        // Create metadata
        let data_v2 = DataV2 {
            name: format!("AI Model #{}", config.mint_count),
            symbol: "AI".to_string(),
            uri: config.model_uri.clone(),
            seller_fee_basis_points: 1000, // 10% royalty
            creators: Some(config.creators.clone()),
            collection: config.collection.clone(),
            uses: config.uses.clone(),
        };
        
        // CPI: Create Metadata
        let create_metadata_ix = CreateMetadataAccountsV3 {
            metadata: ctx.accounts.metadata_account.to_account_info(),
            mint: mint.to_account_info(),
            mint_authority: config.to_account_info(),
            update_authority: config.to_account_info(),
            payer: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            data: data_v2,
            is_mutable: false,
            collection_details: None,
        };
        
        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.metadata_program.to_account_info(),
                create_metadata_ix
            ),
            1,  // Collection details version
        )?;
        
        // CPI: Create Master Edition
        let create_edition_ix = create_master_edition_v3(
            ctx.accounts.metadata_program.key(),
            ctx.accounts.master_edition.key(),
            mint.key(),
            ctx.accounts.authority.key(),
            ctx.accounts.metadata_account.key(),
            ctx.accounts.payer.key(),
            None,  // Max supply
        );
        
        invoke(
            &create_edition_ix,
            &[
                ctx.accounts.master_edition.to_account_info(),
                mint.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.metadata_account.to_account_info(),
                ctx.accounts.payer.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
        
        // Update state
        config.mint_count = config.mint_count.checked_add(1)
            .ok_or(MintError::ArithmeticOverflow)?;
            
        // Store technical metadata
        let metadata_account = &mut ctx.accounts.metadata_account;
        let mut data = metadata_account.try_borrow_mut_data()?;
        let model_metadata = metadata.try_to_vec()?;
        data[..model_metadata.len()].copy_from_slice(&model_metadata);
        
        Ok(())
    }
}

// --------------------------
// Validation Logic
// --------------------------

impl ModelMetadata {
    /// Validate model performance metrics
    pub fn validate_performance(&self) -> Result<()> {
        require!(self.accuracy >= 0.0 && self.accuracy <= 1.0, 
            MintError::InvalidAccuracy);
        require!(self.precision >= 0.0 && self.precision <= 1.0, 
            MintError::InvalidPrecision);
        require!(self.recall >= 0.0 && self.recall <= 1.0, 
            MintError::InvalidRecall);
        require!(self.f1_score >= 0.0 && self.f1_score <= 1.0, 
            MintError::InvalidF1);
        Ok(())
    }
}

// --------------------------
// Error Handling
// --------------------------

#[error_code]
pub enum MintError {
    #[msg("Invalid creator shares (total must be 100)")]
    InvalidCreatorShare,
    #[msg("Creator already verified")]
    CreatorAlreadyVerified,
    #[msg("Hash mismatch between config and metadata")]
    HashMismatch,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Bump seed not found")]
    BumpNotFound,
    #[msg("Invalid accuracy value")]
    InvalidAccuracy,
    #[msg("Invalid precision value")]
    InvalidPrecision,
    #[msg("Invalid recall value")]
    InvalidRecall,
    #[msg("Invalid F1 score")]
    InvalidF1,
    #[msg("Invalid training data hash")]
    InvalidTrainingHash,
    #[msg("Unauthorized mint operation")]
    Unauthorized,
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
    fn test_initialize_config() {
        let mut ctx = test_context!(InitializeMintConfig);
        let creators = vec![
            Creator { address: Pubkey::new_unique(), verified: false, share: 100 }
        ];
        
        let result = model_mint::initialize_config(
            &mut ctx,
            [0; 32],
            "uri://test".to_string(),
            creators,
            None,
            None,
        );

        assert!(result.is_ok());
        let config = &ctx.accounts.config;
        assert_eq!(config.creators[0].share, 100);
    }

    #[test]
    fn test_mint_with_valid_hash() {
        let mut ctx = test_context!(MintModelNft);
        let metadata = ModelMetadata::default();
        
        // Pre-initialize config
        model_mint::initialize_config(
            ctx.accounts.config.clone(),
            [0; 32],
            "uri://test".to_string(),
            vec![],
            None,
            None,
        ).unwrap();

        let result = model_mint::mint_model_nft(
            &mut ctx,
            metadata,
        );

        assert!(result.is_ok());
    }
}
