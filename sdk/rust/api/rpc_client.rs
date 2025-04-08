//! Umazen RPC Client - Robust Blockchain Communication Layer

use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use solana_client::{
    client_error::ClientError,
    nonblocking::rpc_client::RpcClient as AsyncRpcClient,
    rpc_client::RpcClient,
    rpc_config::{
        RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcSendTransactionConfig,
    },
    rpc_filter::{Memcmp, RpcFilterType},
    rpc_response::RpcResult,
};
use solana_sdk::{
    account::Account,
    clock::Slot,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use tokio::{
    sync::{Mutex, Semaphore},
    time::sleep,
};
use tracing::{debug, error, info_span, instrument, warn};

/// Configuration for RPC connection management
#[derive(Debug, Clone)]
pub struct RpcClientConfig {
    pub max_retries: u8,
    pub retry_delay_ms: u64,
    pub timeout_secs: u64,
    pub max_connections: usize,
    pub commitment: CommitmentLevel,
}

impl Default for RpcClientConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            retry_delay_ms: 500,
            timeout_secs: 30,
            max_connections: 100,
            commitment: CommitmentLevel::Confirmed,
        }
    }
}

/// Core RPC client with connection pooling and retry mechanisms
pub struct UmazenRpcClient {
    clients: Arc<Vec<AsyncRpcClient>>,
    config: RpcClientConfig,
    connection_semaphore: Arc<Semaphore>,
    current_index: Mutex<usize>,
}

impl UmazenRpcClient {
    /// Initialize RPC client with multiple endpoints for failover
    pub fn new(
        endpoints: Vec<String>,
        config: RpcClientConfig,
    ) -> Result<Self, ClientError> {
        let clients = endpoints
            .into_iter()
            .map(|url| {
                AsyncRpcClient::new_with_commitment(
                    url,
                    CommitmentConfig {
                        commitment: config.commitment,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            clients: Arc::new(clients),
            config,
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            current_index: Mutex::new(0),
        })
    }

    /// Get next client using round-robin load balancing
    async fn get_client(&self) -> &AsyncRpcClient {
        let mut index = self.current_index.lock().await;
        let client = &self.clients[*index];
        *index = (*index + 1) % self.clients.len();
        client
    }

    /// Execute RPC operation with retry logic and connection limiting
    #[instrument(skip(self, op))]
    async fn execute_rpc<F, T, Fut>(&self, op: F) -> Result<T, ClientError>
    where
        F: Fn(&AsyncRpcClient) -> Fut,
        Fut: std::future::Future<Output = Result<T, ClientError>>,
    {
        let _permit = self
            .connection_semaphore
            .acquire()
            .await
            .expect("Semaphore error");
        let start_time = Instant::now();
        let mut attempts = 0;

        loop {
            let client = self.get_client().await;
            let result = op(client).await;

            match result {
                Ok(res) => return Ok(res),
                Err(e) => {
                    attempts += 1;
                    if attempts > self.config.max_retries {
                        return Err(e);
                    }

                    warn!(
                        "RPC attempt {}/{} failed: {:?}",
                        attempts, self.config.max_retries, e
                    );
                    sleep(Duration::from_millis(
                        self.config.retry_delay_ms * u64::from(attempts),
                    ))
                    .await;
                }
            }

            if start_time.elapsed().as_secs() > self.config.timeout_secs {
                return Err(ClientError::Custom("Timeout exceeded".into()));
            }
        }
    }

    /// Submit transaction with enhanced error handling
    #[instrument(skip(self, tx))]
    pub async fn send_transaction(
        &self,
        tx: &Transaction,
    ) -> RpcResult<Signature> {
        self.execute_rpc(|client| {
            client.send_transaction_with_config(
                tx,
                RpcSendTransactionConfig {
                    skip_preflight: false,
                    preflight_commitment: Some(self.config.commitment),
                    encoding: None,
                    max_retries: None,
                    min_context_slot: None,
                },
            )
        })
        .await
    }

    /// Get account info with automatic retries
    #[instrument(skip(self))]
    pub async fn get_account_info(
        &self,
        pubkey: &Pubkey,
    ) -> RpcResult<Option<Account>> {
        self.execute_rpc(|client| {
            client.get_account_with_commitment(
                pubkey,
                CommitmentConfig {
                    commitment: self.config.commitment,
                },
            )
        })
        .await
    }

    /// Query program accounts with filters
    #[instrument(skip(self))]
    pub async fn get_program_accounts(
        &self,
        program_id: &Pubkey,
        filters: Vec<RpcFilterType>,
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        self.execute_rpc(|client| {
            client.get_program_accounts_with_config(
                program_id,
                RpcProgramAccountsConfig {
                    filters: Some(filters),
                    account_config: RpcAccountInfoConfig {
                        encoding: None,
                        data_slice: None,
                        commitment: Some(self.config.commitment),
                        min_context_slot: None,
                    },
                },
            )
        })
        .await
    }

    /// Get current slot number
    #[instrument(skip(self))]
    pub async fn get_slot(&self) -> RpcResult<Slot> {
        self.execute_rpc(|client| {
            client.get_slot_with_commitment(CommitmentConfig {
                commitment: self.config.commitment,
            })
        })
        .await
    }
}

/// Extended RPC methods for AI-specific operations
impl UmazenRpcClient {
    /// Find model NFTs by metadata hash
    #[instrument(skip(self))]
    pub async fn find_models_by_hash(
        &self,
        model_hash: [u8; 32],
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        let filter = RpcFilterType::Memcmp(Memcmp::new_base58_encoded(64, &model_hash));
        self.get_program_accounts(&umazen::ID, vec![filter]).await
    }

    /// Subscribe to training task events
    #[instrument(skip(self))]
    pub async fn subscribe_training_events(
        &self,
        program_id: Pubkey,
    ) -> RpcResult<()> {
        // Implementation would use websocket client
        // Actual implementation requires async streams
        unimplemented!()
    }

    /// Batch account fetching
    #[instrument(skip(self, pubkeys))]
    pub async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> RpcResult<Vec<Option<Account>>> {
        self.execute_rpc(|client| {
            client.get_multiple_accounts_with_commitment(
                pubkeys,
                CommitmentConfig {
                    commitment: self.config.commitment,
                },
            )
        })
        .await
    }
}

/// Connection management utilities
impl UmazenRpcClient {
    /// Health check for all endpoints
    pub async fn check_endpoints(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        for client in self.clients.iter() {
            let url = client.url().to_string();
            let healthy = self.execute_rpc(|c| c.get_health()).await.is_ok();
            results.push((url, healthy));
        }
        results
    }

    /// Rotate primary endpoint
    pub async fn rotate_endpoint(&self) {
        let mut index = self.current_index.lock().await;
        *index = (*index + 1) % self.clients.len();
    }
}
