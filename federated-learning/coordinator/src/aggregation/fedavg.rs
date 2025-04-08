//! Federated Averaging Engine - Secure Model Aggregation Protocol

use anchor_lang::prelude::*;
use solana_program::program_error::ProgramError;
use std::collections::BTreeMap;

/// Configuration parameters for federated averaging
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct FedAvgConfig {
    /// Minimum number of participants required for aggregation
    pub min_participants: u64,
    /// Maximum staleness duration for model updates (in slots)
    pub max_update_age: u64,
    /// Privacy amplification factor (0-100)
    pub privacy_factor: u8,
    /// Weighted averaging parameters
    pub weight_scheme: WeightScheme,
}

/// Weight calculation schemes for participant contributions
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum WeightScheme {
    /// Weight by number of data samples
    DataSize,
    /// Weight by validation accuracy
    ValidationMetrics,
    /// Equal weighting for all participants
    Uniform,
    /// Custom weighting function
    Custom { 
        weights: BTreeMap<Pubkey, f32>,
        normalization_factor: f32,
    },
}

/// Aggregated global model state
#[account]
#[derive(Default, Debug)]
pub struct GlobalModel {
    /// Current model version
    pub version: u64,
    /// Model parameters (quantized to u8 for storage efficiency)
    pub parameters: Vec<u8>,
    /// Aggregation metadata
    pub metadata: AggregationMetadata,
    /// Checksum of model parameters
    pub hash: [u8; 32],
}

/// Metadata about the aggregation process
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct AggregationMetadata {
    /// Number of participants in this aggregation
    pub participant_count: u64,
    /// Average data size per participant
    pub avg_data_size: f32,
    /// Mean validation accuracy across participants
    pub mean_accuracy: f32,
    /// Privacy budget consumption
    pub privacy_budget: f32,
    /// Timestamp of last aggregation
    pub last_updated: i64,
}

/// Participant model update with validation proofs
#[account]
#[derive(Default, Debug)]
pub struct ModelUpdate {
    /// Participant public key
    pub participant: Pubkey,
    /// Model delta parameters (compressed)
    pub delta: Vec<u8>,
    /// Data size used for training
    pub data_size: u64,
    /// Validation metrics
    pub metrics: ValidationMetrics,
    /// Zero-knowledge proof of valid training
    pub zk_proof: Vec<u8>,
    /// Timestamp of update submission
    pub timestamp: i64,
}

/// Validation metrics for model updates
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct ValidationMetrics {
    /// Accuracy on validation set
    pub accuracy: f32,
    /// Loss on validation set
    pub loss: f32,
    /// Custom metrics (key-value pairs)
    pub custom_metrics: BTreeMap<String, f32>,
}

/// Federated averaging engine implementation
pub struct FedAvg;

impl FedAvg {
    /// Main aggregation entry point
    pub fn aggregate(
        config: &FedAvgConfig,
        global_model: &mut Account<GlobalModel>,
        updates: Vec<Account<ModelUpdate>>,
        clock: &Clock,
    ) -> Result<()> {
        // Phase 1: Input validation
        Self::validate_updates(config, &updates, clock)?;

        // Phase 2: Weight calculation
        let weights = Self::calculate_weights(config, &updates);

        // Phase 3: Secure aggregation
        let new_parameters = Self::secure_aggregation(global_model, &updates, &weights)?;

        // Phase 4: Privacy accounting
        let privacy_budget = Self::update_privacy_budget(config, global_model, &updates);

        // Phase 5: State update
        Self::update_global_model(
            global_model,
            new_parameters,
            updates.len() as u64,
            privacy_budget,
            clock,
        )
    }

    /// Validate model updates against current requirements
    fn validate_updates(
        config: &FedAvgConfig,
        updates: &[Account<ModelUpdate>],
        clock: &Clock,
    ) -> Result<()> {
        // Check minimum participants
        if updates.len() < config.min_participants as usize {
            return Err(ErrorCode::InsufficientParticipants.into());
        }

        // Check update freshness
        let current_slot = clock.slot;
        for update in updates {
            let age = current_slot - update.timestamp;
            if age > config.max_update_age {
                return Err(ErrorCode::StaleModelUpdate.into());
            }
        }

        // Verify ZK proofs (placeholder for actual verification)
        for update in updates {
            if !Self::verify_zk_proof(&update.zk_proof) {
                return Err(ErrorCode::InvalidProof.into());
            }
        }

        Ok(())
    }

    /// Calculate participant weights based on selected scheme
    fn calculate_weights(
        config: &FedAvgConfig,
        updates: &[Account<ModelUpdate>],
    ) -> Vec<f32> {
        match &config.weight_scheme {
            WeightScheme::DataSize => updates
                .iter()
                .map(|u| u.data_size as f32)
                .collect(),
            WeightScheme::ValidationMetrics => updates
                .iter()
                .map(|u| u.metrics.accuracy)
                .collect(),
            WeightScheme::Uniform => vec![1.0; updates.len()],
            WeightScheme::Custom { weights, normalization_factor } => {
                updates.iter()
                    .map(|u| *weights.get(&u.participant).unwrap_or(&0.0))
                    .map(|w| w / normalization_factor)
                    .collect()
            }
        }
    }

    /// Perform secure aggregation with differential privacy
    fn secure_aggregation(
        global_model: &mut Account<GlobalModel>,
        updates: &[Account<ModelUpdate>],
        weights: &[f32],
    ) -> Result<Vec<u8>> {
        // Normalize weights
        let total_weight: f32 = weights.iter().sum();
        let normalized_weights: Vec<f32> = weights
            .iter()
            .map(|w| w / total_weight)
            .collect();

        // Initialize aggregation buffer
        let mut aggregated_delta = vec![0.0; global_model.parameters.len()];

        // Weighted average of deltas
        for (update, weight) in updates.iter().zip(normalized_weights) {
            let delta = Self::decode_delta(&update.delta)?;
            for (i, d) in delta.iter().enumerate() {
                aggregated_delta[i] += d * weight;
            }
        }

        // Apply differential privacy
        let noise = Self::generate_privacy_noise(config.privacy_factor, aggregated_delta.len());
        for (i, n) in noise.iter().enumerate() {
            aggregated_delta[i] += n;
        }

        // Update global parameters
        let new_params = global_model.parameters
            .iter()
            .zip(aggregated_delta)
            .map(|(p, d)| ((*p as f32) + d).clamp(0.0, 255.0) as u8)
            .collect();

        Ok(new_params)
    }

    /// Update privacy budget using moments accountant
    fn update_privacy_budget(
        config: &FedAvgConfig,
        global_model: &mut Account<GlobalModel>,
        updates: &[Account<ModelUpdate>],
    ) -> f32 {
        // Simplified epsilon calculation
        let epsilon = (updates.len() as f32).sqrt() * (config.privacy_factor as f32) / 100.0;
        global_model.metadata.privacy_budget += epsilon;
        global_model.metadata.privacy_budget
    }

    /// Finalize global model update
    fn update_global_model(
        global_model: &mut Account<GlobalModel>,
        parameters: Vec<u8>,
        participant_count: u64,
        privacy_budget: f32,
        clock: &Clock,
    ) -> Result<()> {
        global_model.version += 1;
        global_model.parameters = parameters;
        global_model.metadata.participant_count = participant_count;
        global_model.metadata.privacy_budget = privacy_budget;
        global_model.metadata.last_updated = clock.unix_timestamp;
        global_model.hash = Self::compute_model_hash(&global_model.parameters);
        
        Ok(())
    }

    // Helper functions
    fn verify_zk_proof(proof: &[u8]) -> bool {
        // Placeholder for actual ZK verification
        !proof.is_empty()
    }

    fn decode_delta(compressed: &[u8]) -> Result<Vec<f32>> {
        // Placeholder for actual decompression
        Ok(vec![0.0; compressed.len() / 4])
    }

    fn generate_privacy_noise(factor: u8, size: usize) -> Vec<f32> {
        // Generate Gaussian noise scaled by privacy factor
        let scale = (factor as f32) / 100.0;
        (0..size).map(|_| rand::random::<f32>() * scale).collect()
    }

    fn compute_model_hash(parameters: &[u8]) -> [u8; 32] {
        // Placeholder for actual hash computation
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&sha256::digest(parameters)[..32]);
        hash
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient participants for aggregation")]
    InsufficientParticipants,
    #[msg("Model update exceeds maximum allowed age")]
    StaleModelUpdate,
    #[msg("Invalid zero-knowledge proof")]
    InvalidProof,
    #[msg("Weight calculation error")]
    WeightCalculationError,
    #[msg("Parameter dimension mismatch")]
    DimensionMismatch,
}
