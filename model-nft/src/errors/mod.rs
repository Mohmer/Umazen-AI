//! Umazen Error Hierarchy - Centralized error management system
//!
//! Error Codes:
//! 0x0000-0x0FFF: Core system errors
//! 0x1000-0x1FFF: Cryptographic verification failures  
//! 0x2000-0x2FFF: Economic rule violations
//! 0x3000-0x3FFF: State transition invalidations
//! 0x4000-0x4FFF: Oracle/External service failures
//! 0x5000-0x5FFF: Governance/DAO policy violations

#[macro_use]
extern crate thiserror;

use anchor_lang::prelude::*;
use num_derive::FromPrimitive;
use solana_program::program_error::PrintProgramError;
use std::fmt::{Debug, Display, Formatter};

/// Main error type implementing Anchor compatibility
#[derive(FromPrimitive, Clone, PartialEq, Eq, AnchorSerialize, AnchorDeserialize)]
pub enum UmazenError {
    // Core System Errors (0x0000-0x00FF)
    #[num_traps(0x0001)]
    ArithmeticOverflow = 0x0001,
    InvalidAccountOwner = 0x0002,
    AccountNotInitialized = 0x0003,
    InvalidProgramData = 0x0004,
    
    // Cryptographic Failures (0x1000-0x10FF)
    InvalidZKProof = 0x1001,
    SignatureVerificationFailed = 0x1002,
    InvalidMerkleRoot = 0x1003,
    HashMismatch = 0x1004,
    
    // Economic Rules (0x2000-0x20FF)  
    InsufficientStake = 0x2001,
    RewardPoolDepleted = 0x2002,
    CollateralizationFailure = 0x2003,
    InvalidRoyaltyConfiguration = 0x2004,
    
    // State Transition Errors (0x3000-0x30FF)
    InvalidModelState = 0x3001,
    TrainingPhaseConflict = 0x3002,
    InvalidDatasetVersion = 0x3003,
    StaleModelWeights = 0x3004,
    
    // External Services (0x4000-0x40FF)
    OracleTimeout = 0x4001,
    InvalidIPFSHash = 0x4002,
    GPUVerificationFailed = 0x4003,
    
    // Governance Errors (0x5000-0x50FF)
    DAOVeto = 0x5001,
    ProposalExpired = 0x5002,
    QuorumNotMet = 0x5003,
}

impl From<UmazenError> for ProgramError {
    fn from(e: UmazenError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl Display for UmazenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UmazenError::ArithmeticOverflow => 
                write!(f, "Arithmetic overflow detected in critical path"),
            UmazenError::InvalidZKProof => 
                write!(f, "Zero-knowledge proof verification failed"),
            // ... other variants match accordingly
            _ => write!(f, "Uncategorized error occurred"),
        }
    }
}

impl Debug for UmazenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl PrintProgramError for UmazenError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + Decode<AnchorDecode> + PrintProgramError + Clone,
    {
        solana_program::msg!(&self.to_string());
    }
}

/// Validation-specific error subsystem
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid model architecture version")]
    ModelVersionMismatch,
    #[error("Training parameters out of bounds")]
    HyperparameterViolation,
    #[error("Dataset quality check failed")]
    DatasetIntegrityFailure,
}

impl From<ValidationError> for UmazenError {
    fn from(e: ValidationError) -> Self {
        match e {
            ValidationError::ModelVersionMismatch => UmazenError::InvalidModelState,
            ValidationError::HyperparameterViolation => UmazenError::InvalidModelState,
            ValidationError::DatasetIntegrityFailure => UmazenError::HashMismatch,
        }
    }
}

/// Economic policy error subsystem
#[derive(Error, Debug)]
pub enum EconomicError {
    #[error("Insufficient staked collateral")]
    CollateralShortage,
    #[error("Reward distribution schedule violation")]
    RewardScheduleConflict,
    #[error("Royalty payment threshold not met")]
    RoyaltyThresholdError,
}

impl From<EconomicError> for UmazenError {
    fn from(e: EconomicError) -> Self {
        match e {
            EconomicError::CollateralShortage => UmazenError::InsufficientStake,
            EconomicError::RewardScheduleConflict => UmazenError::InvalidRoyaltyConfiguration,
            EconomicError::RoyaltyThresholdError => UmazenError::InvalidRoyaltyConfiguration,
        }
    }
}

/// Cryptographic error subsystem
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("ZK-SNARK proving system failure")]
    ZkProofGenerationError,
    #[error("Bulletproofs range check failed")]
    RangeProofFailure,
    #[error("Invalid elliptic curve point")]
    CurvePointDecodingError,
}

impl From<CryptoError> for UmazenError {
    fn from(e: CryptoError) -> Self {
        match e {
            CryptoError::ZkProofGenerationError => UmazenError::InvalidZKProof,
            CryptoError::RangeProofFailure => UmazenError::InvalidZKProof,
            CryptoError::CurvePointDecodingError => UmazenError::HashMismatch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::error_msg;

    #[test]
    fn test_error_conversion() {
        let validation_err = ValidationError::ModelVersionMismatch;
        let umazen_err: UmazenError = validation_err.into();
        assert_eq!(umazen_err as u32, 0x3001);
    }

    #[test]
    fn test_error_printing() {
        let err = UmazenError::ArithmeticOverflow;
        let output = format!("{}", err);
        assert!(output.contains("Arithmetic overflow"));
    }

    #[test]
    fn test_anchor_integration() {
        let err = UmazenError::InvalidZKProof;
        let program_err: ProgramError = err.into();
        assert_eq!(program_err, ProgramError::Custom(0x1001));
    }
}
