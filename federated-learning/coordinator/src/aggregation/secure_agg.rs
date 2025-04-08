//! Secure Aggregation Protocol - Privacy-Preserving Parameter Aggregation

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use anchor_lang::prelude::*;
use solana_program::{
    program_error::ProgramError,
    sysvar::clock::Clock,
};
use std::collections::HashMap;

/// Secure Aggregation Session Configuration
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SecureAggConfig {
    /// Minimum participants required for decryption
    pub min_participants: u8,
    /// Maximum time duration for aggregation (seconds)
    pub timeout: i64,
    /// Privacy budget (ε) for differential privacy
    pub privacy_budget: f64,
    /// Threshold for cryptographic secret sharing
    pub threshold: u8,
    /// Allowed public keys for participation
    pub allowed_participants: Vec<Pubkey>,
}

/// Participant's Secret Share Structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SecretShare {
    /// Recipient public key
    pub receiver: Pubkey,
    /// Encrypted share
    pub encrypted_data: Vec<u8>,
    /// Nonce for encryption
    pub nonce: [u8; 12],
}

/// Participant State for Secure Aggregation
#[account]
#[derive(Debug)]
pub struct ParticipantState {
    /// Participant public key
    pub authority: Pubkey,
    /// Aggregation session ID
    pub session: Pubkey,
    /// Encrypted model parameters
    pub encrypted_params: Vec<u8>,
    /// Distributed secret shares
    pub secret_shares: Vec<SecretShare>,
    /// Zero-knowledge proof of valid encryption
    pub zk_proof: Vec<u8>,
    /// Timestamp of submission
    pub timestamp: i64,
    /// Status flags
    pub status: u8,
}

/// Secure Aggregation Session State
#[account]
#[derive(Debug)]
pub struct AggregationSession {
    /// Session initiator
    pub creator: Pubkey,
    /// Current protocol phase
    pub phase: AggregationPhase,
    /// Participant states
    pub participants: Vec<Pubkey>,
    /// Final aggregated parameters
    pub aggregated_result: Option<Vec<u8>>,
    /// Session configuration
    pub config: SecureAggConfig,
    /// Session timestamps
    pub timestamps: SessionTimestamps,
    /// Session nonce
    pub nonce: [u8; 32],
}

/// Protocol Phase Tracking
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum AggregationPhase {
    Initialization,
    ParameterSubmission,
    ShareDistribution,
    Verification,
    Aggregation,
    Completed,
    Aborted,
}

/// Session Time Constraints
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SessionTimestamps {
    pub start: i64,
    pub submission_end: i64,
    pub verification_end: i64,
    pub aggregation_end: i64,
}

impl AggregationSession {
    /// Initialize new aggregation session
    pub fn new(
        creator: Pubkey,
        config: SecureAggConfig,
        clock: &Clock,
    ) -> Result<Self> {
        let now = clock.unix_timestamp;
        Ok(Self {
            creator,
            phase: AggregationPhase::Initialization,
            participants: vec![],
            aggregated_result: None,
            config,
            timestamps: SessionTimestamps {
                start: now,
                submission_end: now + 3600, // 1 hour default
                verification_end: now + 7200,
                aggregation_end: now + 10800,
            },
            nonce: rand::random(),
        })
    }

    /// Add participant to session
    pub fn add_participant(
        &mut self,
        participant: &Account<ParticipantState>,
        clock: &Clock,
    ) -> Result<()> {
        // Validate session phase
        if self.phase != AggregationPhase::Initialization {
            return Err(ErrorCode::InvalidSessionPhase.into());
        }

        // Check allow list
        if !self.config.allowed_participants.is_empty() 
            && !self.config.allowed_participants.contains(&participant.authority) 
        {
            return Err(ErrorCode::UnauthorizedParticipant.into());
        }

        // Check time constraints
        if clock.unix_timestamp > self.timestamps.submission_end {
            return Err(ErrorCode::SessionExpired.into());
        }

        self.participants.push(participant.authority);
        Ok(())
    }

    /// Process parameter submission
    pub fn submit_parameters(
        &mut self,
        participant: &mut Account<ParticipantState>,
        encrypted_params: Vec<u8>,
        shares: Vec<SecretShare>,
        proof: Vec<u8>,
        clock: &Clock,
    ) -> Result<()> {
        // Validate phase transition
        if self.phase != AggregationPhase::ParameterSubmission {
            return Err(ErrorCode::InvalidSubmissionPhase.into());
        }

        // Verify proof of proper encryption
        if !Self::verify_encryption_proof(&encrypted_params, &proof) {
            return Err(ErrorCode::InvalidProof.into());
        }

        // Verify secret shares threshold
        if shares.len() < self.config.threshold as usize {
            return Err(ErrorCode::InsufficientShares.into());
        }

        // Update participant state
        participant.encrypted_params = encrypted_params;
        participant.secret_shares = shares;
        participant.zk_proof = proof;
        participant.timestamp = clock.unix_timestamp;
        participant.status |= 0x01; // Mark as submitted

        Ok(())
    }

    /// Perform secure aggregation
    pub fn secure_aggregate(&mut self, participants: &[Account<ParticipantState>]) -> Result<()> {
        // Phase validation
        if self.phase != AggregationPhase::Aggregation {
            return Err(ErrorCode::InvalidAggregationPhase.into());
        }

        // Collect valid submissions
        let valid_params = participants
            .iter()
            .filter(|p| p.status & 0x01 != 0)
            .map(|p| &p.encrypted_params)
            .collect::<Vec<_>>();

        // Check minimum participants
        if valid_params.len() < self.config.min_participants as usize {
            return Err(ErrorCode::InsufficientParticipants.into());
        }

        // Combine encrypted parameters (simplified example)
        let combined = self.combine_parameters(valid_params)?;

        // Apply differential privacy
        let noisy_result = self.apply_dp_noise(combined);

        self.aggregated_result = Some(noisy_result);
        self.phase = AggregationPhase::Completed;

        Ok(())
    }

    /// Cryptographic parameter combination
    fn combine_parameters(&self, params: Vec<&Vec<u8>>) -> Result<Vec<u8>> {
        // Placeholder for actual cryptographic combination
        // In real implementation would perform homomorphic addition
        Ok(params
            .iter()
            .flat_map(|v| v.iter())
            .map(|b| b.wrapping_add(rand::random::<u8>()))
            .collect())
    }

    /// Differential privacy noise injection
    fn apply_dp_noise(&self, data: Vec<u8>) -> Vec<u8> {
        let scale = (self.config.privacy_budget * 100.0) as f64;
        data.into_iter()
            .map(|b| {
                let noise: f64 = rand::random::<f64>() * scale;
                b.wrapping_add(noise as u8)
            })
            .collect()
    }

    /// Proof verification placeholder
    fn verify_encryption_proof(encrypted: &[u8], proof: &[u8]) -> bool {
        // Simplified verification logic
        !encrypted.is_empty() && !proof.is_empty()
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid session phase for this operation")]
    InvalidSessionPhase,
    #[msg("Unauthorized participant")]
    UnauthorizedParticipant,
    #[msg("Session time constraints violated")]
    SessionExpired,
    #[msg("Invalid encryption proof")]
    InvalidProof,
    #[msg("Insufficient secret shares")]
    InsufficientShares,
    #[msg("Invalid parameter submission phase")]
    InvalidSubmissionPhase,
    #[msg("Invalid aggregation phase")]
    InvalidAggregationPhase,
    #[msg("Minimum participant count not met")]
    InsufficientParticipants,
    #[msg("Parameter combination failed")]
    CombinationFailure,
    #[msg("Noise injection error")]
    NoiseInjectionError,
}

// Unit tests module
#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::clock::Epoch;

    #[test]
    fn test_session_initialization() {
        let creator = Pubkey::new_unique();
        let config = SecureAggConfig {
            min_participants: 3,
            timeout: 3600,
            privacy_budget: 0.5,
            threshold: 2,
            allowed_participants: vec![],
        };
        let clock = Clock {
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 0,
        };

        let session = AggregationSession::new(creator, config, &clock).unwrap();
        assert_eq!(session.phase, AggregationPhase::Initialization);
        assert_eq!(session.participants.len(), 0);
    }

    #[test]
    fn test_participant_addition() {
        let mut session = create_test_session();
        let participant = create_test_participant();

        session.add_participant(&participant, &test_clock()).unwrap();
        assert_eq!(session.participants.len(), 1);
    }

    // Helper functions
    fn create_test_session() -> AggregationSession {
        AggregationSession::new(
            Pubkey::new_unique(),
            SecureAggConfig {
                min_participants: 1,
                timeout: 3600,
                privacy_budget: 1.0,
                threshold: 1,
                allowed_participants: vec![],
            },
            &test_clock(),
        ).unwrap()
    }

    fn create_test_participant() -> Account<ParticipantState> {
        Account::try_from(participant_state! {
            authority: Pubkey::new_unique(),
            session: Pubkey::new_unique(),
            encrypted_params: vec![],
            secret_shares: vec![],
            zk_proof: vec![],
            timestamp: 0,
            status: 0,
        }).unwrap()
    }

    fn test_clock() -> Clock {
        Clock {
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 0,
        }
    }
}
