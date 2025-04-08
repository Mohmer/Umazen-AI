//! Umazen API Layer - Core Service Abstraction for Blockchain & AI Integration

use std::sync::Arc;
use tokio::sync::RwLock;
use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::Keypair,
        signer::Signer,
    },
    Client, Program,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};

mod error;
mod middleware;
mod models;
mod routes;
mod utils;

pub use error::ApiError;
pub use models::*;
pub use routes::{configure_routes, ApiState};
pub use utils::*;

/// Core API service implementation
#[derive(Clone)]
pub struct ApiService {
    program: Arc<Program<Client<Keypair>>>,
    state: Arc<RwLock<ApiState>>,
}

impl ApiService {
    /// Initialize API service with Solana connection
    pub async fn new(
        rpc_url: String,
        ws_url: String,
        keypair: Keypair,
    ) -> Result<Self> {
        let client = Client::new_with_options(
            CommitmentConfig::confirmed(),
            rpc_url.clone(),
            Some(ws_url),
            keypair,
        )?;

        let program = client.program(umazen::ID);
        
        Ok(Self {
            program: Arc::new(program),
            state: Arc::new(RwLock::new(ApiState::default())),
        })
    }

    /// Submit training task to network
    #[instrument(skip(self, params))]
    pub async fn submit_training_task(
        &self,
        params: TrainingTaskParams,
    ) -> Result<TrainingTaskReceipt, ApiError> {
        validate_training_params(&params)
            .map_err(ApiError::ValidationError)?;

        let model_hash = compute_model_hash(&params.model_data)
            .await
            .context("Model hash computation failed")?;

        let tx_builder = TrainingTransactionBuilder::new(
            self.program.clone(),
            params.clone(),
            model_hash,
        );

        let signed_tx = tx_builder.build().await?;
        
        let sig = self.program.rpc()
            .send_transaction(signed_tx)
            .await
            .map_err(|e| {
                error!("Training submission failed: {:?}", e);
                ApiError::TransactionFailed(e)
            })?;

        Ok(TrainingTaskReceipt {
            task_id: uuid::Uuid::new_v4(),
            model_hash,
            transaction_signature: sig.to_string(),
            estimated_completion: Utc::now() + Duration::hours(2),
        })
    }

    /// Query active training tasks
    pub async fn get_active_tasks(
        &self,
        filter: Option<TaskFilter>,
    ) -> Result<Vec<TrainingTask>> {
        let state = self.state.read().await;
        state.task_manager.get_active_tasks(filter).await
    }

    /// Submit inference request
    #[instrument(skip(self, request))]
    pub async fn submit_inference_request(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResult, ApiError> {
        validate_inference_input(&request.input_data)?;

        let model_account = self.program.account::<ModelNFT>(request.model_pubkey)
            .await
            .map_err(|e| {
                error!("Model account fetch failed: {:?}", e);
                ApiError::ModelNotFound
            })?;

        verify_model_ownership(&model_account, &request.user_pubkey)?;

        let result = process_inference(
            &model_account.metadata.model_uri,
            &request.input_data,
        ).await?;

        Ok(InferenceResult {
            request_id: request.request_id,
            output_data: result,
            processing_time: Utc::now(),
        })
    }
}

/// Core API trait for cross-service integration
#[async_trait]
pub trait UmazenApi: Send + Sync {
    async fn get_model_metadata(
        &self, 
        model_id: Uuid
    ) -> Result<ModelMetadata, ApiError>;
    
    async fn submit_training_task(
        &self,
        params: TrainingTaskParams
    ) -> Result<TrainingTaskReceipt, ApiError>;
    
    async fn get_inference_result(
        &self,
        request_id: Uuid
    ) -> Result<InferenceResult, ApiError>;
}

#[async_trait]
impl UmazenApi for ApiService {
    async fn get_model_metadata(
        &self, 
        model_id: Uuid
    ) -> Result<ModelMetadata, ApiError> {
        // Implementation
    }

    async fn submit_training_task(
        &self,
        params: TrainingTaskParams
    ) -> Result<TrainingTaskReceipt, ApiError> {
        self.submit_training_task(params).await
    }

    async fn get_inference_result(
        &self,
        request_id: Uuid
    ) -> Result<InferenceResult, ApiError> {
        // Implementation
    }
}

/// API configuration and startup
pub async fn run_api_server(
    service: Arc<dyn UmazenApi>,
    port: u16,
) -> Result<()> {
    let app = configure_routes(service);
    
    info!("Starting API server on port {}", port);
    
    axum::Server::bind(&format!("0.0.0.0:{}", port).parse()?)
        .serve(app.into_make_service())
        .await
        .context("API server failed to start")
}
