//! Umazen Metrics System - Prometheus Integration for Production Monitoring

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use {
    prometheus::{
        self, 
        histogram_opts, opts, register_,
        Encoder, TextEncoder,
        core::{AtomicF64, AtomicI64, GenericCounter, GenericGauge},
        exponential_buckets,
        Histogram, HistogramVec,
        IntCounter, IntCounterVec,
        IntGauge, IntGaugeVec,
    },
    std::{
        net::SocketAddr,
        sync::Arc,
        time::{Duration, Instant},
    },
    tokio::{
        task::JoinHandle,
        time,
    },
    warp::{
        Filter,
        Reply,
    },
};

/// Main metrics container
#[derive(Clone, Debug)]
pub struct Metrics {
    pub blockchain: BlockchainMetrics,
    pub ai: AiMetrics,
    pub rpc: RpcMetrics,
    pub system: SystemMetrics,
}

/// Blockchain-specific metrics
#[derive(Clone, Debug)]
pub struct BlockchainMetrics {
    pub transactions_processed: IntCounterVec,
    pub slot_height: IntGauge,
    pub confirmation_time: HistogramVec,
    pub stake_amount: GenericGauge<AtomicF64>,
}

/// AI-specific metrics
#[derive(Clone, Debug)]
pub struct AiMetrics {
    pub training_requests: IntCounter,
    pub inference_latency: Histogram,
    pub model_versions: IntGaugeVec,
    pub gpu_utilization: GenericGauge<AtomicF64>,
}

/// RPC server metrics
#[derive(Clone, Debug)]
pub struct RpcMetrics {
    pub requests_total: IntCounterVec,
    pub request_duration: HistogramVec,
    pub active_connections: IntGauge,
}

/// System resource metrics
#[derive(Clone, Debug)]
pub struct SystemMetrics {
    pub memory_usage: GenericGauge<AtomicF64>,
    pub cpu_usage: GenericGauge<AtomicF64>,
    pub disk_io: HistogramVec,
}

/// Metrics handler configuration
#[derive(Clone, Debug)]
pub struct MetricsConfig {
    pub bind_address: SocketAddr,
    pub push_interval: Option<Duration>,
    pub push_gateway: Option<String>,
}

impl Metrics {
    /// Create new metrics registry with default buckets
    pub fn new() -> Result<Self, prometheus::Error> {
        let blockchain = BlockchainMetrics {
            transactions_processed: IntCounterVec::new(
                opts!(
                    "blockchain_transactions_total",
                    "Total blockchain transactions processed"
                ),
                &["tx_type", "status"]
            )?,
            slot_height: IntGauge::new(
                "blockchain_slot_height",
                "Current slot height"
            )?,
            confirmation_time: HistogramVec::new(
                histogram_opts!(
                    "blockchain_confirmation_seconds",
                    "Transaction confirmation times",
                    exponential_buckets(0.1, 2.0, 10)?
                ),
                &["priority"]
            )?,
            stake_amount: GenericGauge::new(
                "blockchain_stake_amount",
                "Current stake amount in SOL"
            )?,
        };

        let ai = AiMetrics {
            training_requests: IntCounter::new(
                "ai_training_requests_total",
                "Total training requests received"
            )?,
            inference_latency: Histogram::with_opts(
                histogram_opts!(
                    "ai_inference_latency_seconds",
                    "AI model inference latency",
                    exponential_buckets(0.05, 2.0, 10)?
                )
            )?,
            model_versions: IntGaugeVec::new(
                opts!(
                    "ai_model_versions",
                    "Deployed model versions"
                ),
                &["model_type"]
            )?,
            gpu_utilization: GenericGauge::new(
                "ai_gpu_utilization_ratio",
                "GPU utilization percentage"
            )?,
        };

        let rpc = RpcMetrics {
            requests_total: IntCounterVec::new(
                opts!(
                    "rpc_requests_total",
                    "Total RPC requests handled"
                ),
                &["method", "status_code"]
            )?,
            request_duration: HistogramVec::new(
                histogram_opts!(
                    "rpc_request_duration_seconds",
                    "RPC request handling duration",
                    vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0]
                ),
                &["method"]
            )?,
            active_connections: IntGauge::new(
                "rpc_active_connections",
                "Current active HTTP connections"
            )?,
        };

        let system = SystemMetrics {
            memory_usage: GenericGauge::new(
                "system_memory_usage_bytes",
                "Memory usage in bytes"
            )?,
            cpu_usage: GenericGauge::new(
                "system_cpu_usage_ratio",
                "CPU utilization percentage"
            )?,
            disk_io: HistogramVec::new(
                histogram_opts!(
                    "system_disk_io_seconds",
                    "Disk I/O operation duration",
                    exponential_buckets(0.01, 2.0, 10)?
                ),
                &["operation"]
            )?,
        };

        // Register all metrics
        register_(
            blockchain.transactions_processed.clone()
        )?;
        register_(blockchain.slot_height.clone())?;
        register_(
            blockchain.confirmation_time.clone()
        )?;
        register_(blockchain.stake_amount.clone())?;

        register_(ai.training_requests.clone())?;
        register_(ai.inference_latency.clone())?;
        register_(ai.model_versions.clone())?;
        register_(ai.gpu_utilization.clone())?;

        register_(rpc.requests_total.clone())?;
        register_(rpc.request_duration.clone())?;
        register_(rpc.active_connections.clone())?;

        register_(system.memory_usage.clone())?;
        register_(system.cpu_usage.clone())?;
        register_(system.disk_io.clone())?;

        Ok(Self {
            blockchain,
            ai,
            rpc,
            system,
        })
    }

    /// Start metrics HTTP server
    pub fn start_server(
        &self,
        config: MetricsConfig
    ) -> JoinHandle<()> {
        let metrics_route = warp::path!("metrics")
            .map(move || {
                let encoder = TextEncoder::new();
                let mut buffer = vec![];
                encoder
                    .encode(&prometheus::gather(), &mut buffer)
                    .unwrap();
                warp::reply::with_header(
                    buffer,
                    "Content-Type",
                    encoder.format_type(),
                )
            });

        let (addr, server) = warp::serve(metrics_route)
            .bind_ephemeral(config.bind_address);

        tokio::spawn(async move {
            server.await;
        })
    }

    /// Start push gateway client
    pub fn start_push_gateway(
        &self,
        config: MetricsConfig
    ) -> Option<JoinHandle<()>> {
        let Some(gateway) = config.push_gateway else {
            return None;
        };
        let interval = config.push_interval
            .unwrap_or(Duration::from_secs(30));

        let handle = tokio::spawn(async move {
            let client = prometheus::push::Pusher::new(
                gateway,
                "umazen",
            );

            loop {
                time::sleep(interval).await;
                client.push().await.unwrap();
            }
        });

        Some(handle)
    }
}

/// Warp filter for request metrics
pub fn with_metrics(
    metrics: Arc<Metrics>
) -> impl Filter<Extract = (Arc<Metrics>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || metrics.clone())
}

/// Metrics middleware for HTTP handlers
pub async fn metrics_middleware<F, Fut>(
    method: String,
    metrics: Arc<Metrics>,
    f: F,
) -> impl Reply
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = impl Reply>,
{
    let start = Instant::now();
    let response = f().await;
    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16();

    metrics.rpc.requests_total
        .with_label_values(&[&method, &status.to_string()])
        .inc();
    
    metrics.rpc.request_duration
        .with_label_values(&[&method])
        .observe(duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::proto::MetricFamily;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_metrics_server() {
        let metrics = Metrics::new().unwrap();
        let config = MetricsConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            push_interval: None,
            push_gateway: None,
        };

        let handle = metrics.start_server(config);
        
        // Verify metrics endpoint
        let client = reqwest::Client::new();
        let response = client.get(format!(
            "http://{}/metrics", 
            handle.addr()
        ))
        .send()
        .await
        .unwrap();

        assert_eq!(response.status(), 200);
        assert!(
            response.text().await.unwrap()
            .contains("blockchain_transactions_total")
        );
    }

    #[test]
    fn test_metrics_registration() {
        let metrics = Metrics::new().unwrap();
        
        // Verify all metrics registered
        let families: Vec<MetricFamily> = prometheus::gather();
        let names: Vec<_> = families.iter()
            .map(|mf| mf.get_name())
            .collect();
        
        assert!(names.contains(&"blockchain_transactions_total"));
        assert!(names.contains(&"ai_training_requests_total"));
        assert!(names.contains(&"rpc_requests_total"));
        assert!(names.contains(&"system_memory_usage_bytes"));
    }
}
