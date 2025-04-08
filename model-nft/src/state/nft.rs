//! Model NFT Program - SPL-compatible NFT implementation for AI models

use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata},
    token::{spl_token::state::Account as TokenAccount, Mint, Token, TokenAccount},
};
use mpl_token_metadata::{
    instructions::CreateMetadataAccountV3InstructionArgs,
    types::{DataV2, Creator, TokenStandard},
};
use solana_program::{entrypoint::ProgramResult, program_memory::sol_memcpy};

declare_id!("NFTmazn1111111111111111111111111111111111111");

// --------------------------
// Core Data Structures
// --------------------------

#[account]
pub struct ModelNFT {
    pub version: u8,
    pub model_hash: [u8; 32],
    pub metadata_uri: String,
    pub mint: Pubkey,
    pub authority: Pubkey,
    pub royalty_basis_points: u16,
    pub update_authority: Pubkey,
    pub current_deployment: Option<Pubkey>,
    pub last_updated: i64,
    pub nonce: u8,
    pub model_type: ModelType,
    pub training_rounds: u32,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum ModelType {
    Generic,
    ImageGeneration,
    LanguageModel,
    RecommendationSystem,
    Custom(u8),
}

#[derive(Accounts)]
#[instruction(metadata_uri: String, royalty_basis_points: u16)]
pub struct InitializeModelNFT<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + ModelNFT::LEN,
        seeds = [b"model_nft", mint.key().as_ref()],
        bump
    )]
    pub model_nft: Account<'info, ModelNFT>,
    #[account(
        init,
        payer = payer,
        mint::decimals = 0,
        mint::authority = model_nft,
        mint::freeze_authority = model_nft,
    )]
    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = authority,
    )]
    pub token_account: Account<'info, TokenAccount>,
    pub authority: SystemAccount<'info>,
    pub update_authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
    /// CHECK: Metadata account validated in CPI
    #[account(mut)]
    pub metadata_account: UncheckedAccount<'info>,
    pub token_metadata_program: Program<'info, Metadata>,
}

// --------------------------
// Instruction Handlers
// --------------------------

#[program]
pub mod model_nft {
    use super::*;

    /// Initialize a new AI Model NFT
    pub fn initialize(
        ctx: Context<InitializeModelNFT>,
        metadata_uri: String,
        model_hash: [u8; 32],
        royalty_basis_points: u16,
        model_type: ModelType,
    ) -> Result<()> {
        require!(
            royalty_basis_points <= 10000,
            NftError::InvalidRoyaltyPercentage
        );
        require!(metadata_uri.len() <= 256, NftError::UriTooLong);
        require!(
            ctx.accounts.update_authority.is_signer,
            NftError::UpdateAuthorityRequired
        );

        let model_nft = &mut ctx.accounts.model_nft;
        model_nft.version = 1;
        model_nft.model_hash = model_hash;
        model_nft.metadata_uri = metadata_uri.clone();
        model_nft.mint = ctx.accounts.mint.key();
        model_nft.authority = ctx.accounts.authority.key();
        model_nft.royalty_basis_points = royalty_basis_points;
        model_nft.update_authority = ctx.accounts.update_authority.key();
        model_nft.last_updated = Clock::get()?.unix_timestamp;
        model_nft.nonce = *ctx.bumps.get("model_nft").ok_or(NftError::BumpNotFound)?;
        model_nft.model_type = model_type;
        model_nft.training_rounds = 0;

        // Create metadata
        let metadata_args = CreateMetadataAccountV3InstructionArgs {
            data: DataV2 {
                name: "Umazen AI Model".to_string(),
                symbol: "UMAI".to_string(),
                uri: metadata_uri,
                seller_fee_basis_points: royalty_basis_points,
                creators: Some(vec![Creator {
                    address: ctx.accounts.update_authority.key(),
                    verified: true,
                    share: 100,
                }]),
                collection: None,
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        };

        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata_account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    update_authority: ctx.accounts.update_authority.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
            ),
            metadata_args,
        )?;

        Ok(())
    }

    /// Update model metadata
    pub fn update_metadata(
        ctx: Context<UpdateMetadata>,
        new_uri: String,
        new_royalty_bp: u16,
    ) -> Result<()> {
        require!(
            new_royalty_bp <= 10000,
            NftError::InvalidRoyaltyPercentage
        );
        require!(new_uri.len() <= 256, NftError::UriTooLong);

        let model_nft = &mut ctx.accounts.model_nft;
        model_nft.metadata_uri = new_uri;
        model_nft.royalty_basis_points = new_royalty_bp;
        model_nft.last_updated = Clock::get()?.unix_timestamp;

        // Update metadata account via CPI
        let metadata = Metadata::from_account_info(&ctx.accounts.metadata_account)?;
        metadata.update_v2(
            ctx.accounts.token_metadata_program.to_account_info(),
            ctx.accounts.update_authority.to_account_info(),
            None,
            Some(new_royalty_bp),
            None,
            None,
        )?;

        Ok(())
    }

    /// Transfer model ownership
    pub fn transfer_authority(ctx: Context<TransferAuthority>, new_authority: Pubkey) -> Result<()> {
        let model_nft = &mut ctx.accounts.model_nft;
        model_nft.authority = new_authority;
        model_nft.last_updated = Clock::get()?.unix_timestamp;
        Ok(())
    }

    /// Record new training round
    pub fn record_training_round(ctx: Context<RecordTraining>) -> Result<()> {
        let model_nft = &mut ctx.accounts.model_nft;
        model_nft.training_rounds = model_nft.training_rounds.checked_add(1)
            .ok_or(NftError::ArithmeticOverflow)?;
        model_nft.last_updated = Clock::get()?.unix_timestamp;
        Ok(())
    }
}

// --------------------------
// Context Structures
// --------------------------

#[derive(Accounts)]
pub struct UpdateMetadata<'info> {
    #[account(mut, has_one = update_authority)]
    pub model_nft: Account<'info, ModelNFT>,
    #[account(mut)]
    pub metadata_account: Account<'info, Metadata>,
    #[account(mut)]
    pub update_authority: Signer<'info>,
    pub token_metadata_program: Program<'info, Metadata>,
}

#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    #[account(mut, has_one = authority)]
    pub model_nft: Account<'info, ModelNFT>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct RecordTraining<'info> {
    #[account(mut)]
    pub model_nft: Account<'info, ModelNFT>,
    pub training_round: AccountInfo<'info>,
}

// --------------------------
// Error Handling
// --------------------------

#[error_code]
pub enum NftError {
    #[msg("Invalid royalty percentage (0-10000)")]
    InvalidRoyaltyPercentage,
    #[msg("Metadata URI exceeds maximum length")]
    UriTooLong,
    #[msg("Update authority signature required")]
    UpdateAuthorityRequired,
    #[msg("Authority mismatch")]
    AuthorityMismatch,
    #[msg("Bump seed not found")]
    BumpNotFound,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Invalid model state")]
    InvalidModelState,
}

// --------------------------
// Utility Methods
// --------------------------

impl ModelNFT {
    pub const LEN: usize = 1 + 32 + 256 + 32 + 32 + 2 + 32 + 33 + 8 + 1 + 4;

    /// Validate model ownership
    pub fn verify_owner(&self, signer: &Pubkey) -> Result<()> {
        if &self.authority != signer {
            return Err(NftError::AuthorityMismatch.into());
        }
        Ok(())
    }

    /// Calculate royalty amount
    pub fn calculate_royalty(&self, amount: u64) -> Option<u64> {
        amount.checked_mul(self.royalty_basis_points as u64)
            .and_then(|v| v.checked_div(10000))
    }
}

// --------------------------
// Unit Tests
// --------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::test::*;
    use solana_program::{program_error::ProgramError, system_program};

    #[test]
    fn test_initialize_nft() {
        let mut ctx = test_context!(InitializeModelNFT);
        let metadata_uri = "https://example.com/model.json".to_string();
        let model_hash = [0u8; 32];
        let result = model_nft::initialize(
            &mut ctx,
            metadata_uri.clone(),
            model_hash,
            500,
            ModelType::LanguageModel,
        );

        assert!(result.is_ok());
        let nft = &ctx.accounts.model_nft;
        assert_eq!(nft.metadata_uri, metadata_uri);
        assert_eq!(nft.royalty_basis_points, 500);
    }

    #[test]
    fn test_invalid_royalty() {
        let mut ctx = test_context!(InitializeModelNFT);
        let result = model_nft::initialize(
            &mut ctx,
            "uri".to_string(),
            [0u8; 32],
            10001,
            ModelType::Generic,
        );
        assert_eq!(
            result.unwrap_err(),
            NftError::InvalidRoyaltyPercentage.into()
        );
    }

    #[test]
    fn test_metadata_update() {
        let mut ctx = test_context!(UpdateMetadata);
        let new_uri = "https://new.metadata.uri".to_string();
        let result = model_nft::update_metadata(&mut ctx, new_uri.clone(), 750);

        assert!(result.is_ok());
        let nft = &ctx.accounts.model_nft;
        assert_eq!(nft.metadata_uri, new_uri);
        assert_eq!(nft.royalty_basis_points, 750);
    }
}
