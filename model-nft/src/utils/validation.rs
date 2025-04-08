//! Umazen Validation Module - Centralized security and data integrity checks

use anchor_lang::{
    prelude::*,
    solana_program::{
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        msg
    },
};
use std::convert::TryInto;
use sha3::{Digest, Keccak256};
use bytemuck::{Pod, Zeroable};

/// Central validation hub for core program operations
pub struct UmazenValidator;

impl UmazenValidator {
    /// Validate model metadata structure
    pub fn validate_metadata(metadata: &ModelMetadata) -> ProgramResult {
        // Check metadata URI length
        if metadata.metadata_uri.len() > 200 {
            msg!("Metadata URI exceeds 200 characters");
            return Err(ValidationError::InvalidMetadataLength.into());
        }

        // Validate royalty basis points
        if metadata.royalty_basis_points > 10000 {
            msg!("Royalty exceeds 100%");
            return Err(ValidationError::InvalidRoyaltyValue.into());
        }

        // Verify IPFS CID format
        Self::validate_cid(&metadata.metadata_uri)?;

        // Check model architecture format
        Self::validate_architecture(&metadata.architecture)?;

        Ok(())
    }

    /// Validate cryptographic model hash
    pub fn validate_model_hash(
        claimed_hash: &[u8; 32],
        model_data: &[u8]
    ) -> ProgramResult {
        let mut hasher = Keccak256::new();
        hasher.update(model_data);
        let computed_hash = hasher.finalize();
        
        if claimed_hash != computed_hash.as_slice() {
            msg!("Model hash mismatch");
            return Err(ValidationError::HashMismatch.into());
        }

        Ok(())
    }

    /// Validate computational requirements
    pub fn validate_compute_requirements(
        requirements: &ComputeRequirements,
        node_specs: &NodeSpecs
    ) -> ProgramResult {
        // Check VRAM requirements
        if node_specs.available_vram < requirements.min_vram {
            msg!("Insufficient VRAM: {} < {}", 
                node_specs.available_vram, requirements.min_vram);
            return Err(ValidationError::InsufficientResources.into());
        }

        // Validate CUDA compute capability
        let node_cc = f32::from_str(&node_specs.compute_capability)
            .map_err(|_| ValidationError::InvalidComputeCapability)?;
        
        if node_cc < requirements.min_compute_capability {
            msg!("Compute capability too low: {} < {}", 
                node_cc, requirements.min_compute_capability);
            return Err(ValidationError::InsufficientCompute.into());
        }

        Ok(())
    }

    /// Validate governance permissions
    pub fn validate_governance_permissions(
        action: GovernanceAction,
        authority: &Pubkey,
        governance_flags: &GovernanceFlags
    ) -> ProgramResult {
        match action {
            GovernanceAction::UpdateMetadata => {
                if !governance_flags.updatable {
                    msg!("Metadata updates are disabled");
                    return Err(ValidationError::UpdateForbidden.into());
                }
            }
            GovernanceAction::TransferOwnership => {
                if !governance_flags.transferable {
                    msg!("Transfers are disabled");
                    return Err(ValidationError::TransferForbidden.into());
                }
            }
        }

        // DAO approval checks
        if governance_flags.requires_dao_approval {
            let dao = Pubkey::find_program_address(
                &[b"dao"], 
                &crate::ID
            ).0;
            
            if authority != &dao {
                msg!("DAO approval required");
                return Err(ValidationError::DaoApprovalRequired.into());
            }
        }

        Ok(())
    }

    /// Validate IPFS CID format
    fn validate_cid(cid: &str) -> ProgramResult {
        // Check multibase prefix
        if !cid.starts_with('Q') {
            msg!("Invalid CID multibase prefix");
            return Err(ValidationError::InvalidCidFormat.into());
        }

        // Decode base58btc
        let decoded = bs58::decode(&cid[1..])
            .into_vec()
            .map_err(|_| ValidationError::InvalidCidFormat)?;

        // Verify length for SHA-256 hash (34 bytes: 0x12 0x20 + 32 bytes)
        if decoded.len() != 34 || decoded[0] != 0x12 || decoded[1] != 0x20 {
            msg!("Invalid CID digest format");
            return Err(ValidationError::InvalidCidFormat.into());
        }

        Ok(())
    }

    /// Validate model architecture format
    fn validate_architecture(arch: &str) -> ProgramResult {
        // Allow alphanumeric and common symbols
        for c in arch.chars() {
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
                msg!("Invalid architecture character: {}", c);
                return Err(ValidationError::InvalidArchitecture.into());
            }
        }

        // Length check
        if arch.len() > 50 {
            msg!("Architecture name too long");
            return Err(ValidationError::InvalidArchitecture.into());
        }

        Ok(())
    }
}

/// Hardware requirements structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ComputeRequirements {
    pub min_vram: u64,        // In megabytes
    pub min_compute_capability: f32,
    pub required_instructions: Vec<String>,
}

/// Node specifications structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct NodeSpecs {
    pub available_vram: u64,
    pub compute_capability: String,
    pub supported_instructions: Vec<String>,
}

/// Governance action types
pub enum GovernanceAction {
    UpdateMetadata,
    TransferOwnership,
    ModifyRoyalties,
}

/// Custom validation errors
#[error_code]
pub enum ValidationError {
    #[msg("Invalid metadata structure")]
    InvalidMetadataLength,
    #[msg("Royalty value out of bounds")]
    InvalidRoyaltyValue,
    #[msg("Cryptographic hash mismatch")]
    HashMismatch,
    #[msg("Insufficient compute resources")]
    InsufficientResources,
    #[msg("Invalid compute capability format")]
    InvalidComputeCapability,
    #[msg("Compute capability below minimum")]
    InsufficientCompute,
    #[msg("DAO approval required")]
    DaoApprovalRequired,
    #[msg("Invalid IPFS CID format")]
    InvalidCidFormat,
    #[msg("Invalid model architecture")]
    InvalidArchitecture,
    #[msg("Metadata updates forbidden")]
    UpdateForbidden,
    #[msg("Transfers forbidden")]
    TransferForbidden,
}

// Safety: Implement Pod for GPU-accelerated validation
unsafe impl Pod for ComputeRequirements {}
unsafe impl Zeroable for ComputeRequirements {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_validation() {
        let mut metadata = ModelMetadata::default();
        metadata.metadata_uri = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG".to_string();
        metadata.royalty_basis_points = 500;

        let result = UmazenValidator::validate_metadata(&metadata);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hash_validation() {
        let data = b"test_data";
        let mut hasher = Keccak256::new();
        hasher.update(data);
        let valid_hash = hasher.finalize();

        let result = UmazenValidator::validate_model_hash(
            valid_hash.as_slice().try_into().unwrap(),
            data
        );
        assert!(result.is_ok());
    }
}
