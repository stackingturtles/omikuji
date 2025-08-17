use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use std::time::Duration;
use tracing::{debug, warn};

lazy_static! {
    /// RPC request counter
    static ref RPC_REQUEST_COUNT: CounterVec = register_counter_vec!(
        "omikuji_rpc_requests_total",
        "Total number of RPC requests",
        &["network", "method", "status"]
    ).expect("Failed to create rpc_request_count metric");

    /// RPC request latency histogram
    static ref RPC_REQUEST_LATENCY_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_rpc_request_latency_seconds",
        "RPC request latency in seconds",
        &["network", "method"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    ).expect("Failed to create rpc_request_latency metric");

    /// Chain head block number
    static ref CHAIN_HEAD_BLOCK: GaugeVec = register_gauge_vec!(
        "omikuji_chain_head_block",
        "Current chain head block number",
        &["network"]
    ).expect("Failed to create chain_head_block metric");

    /// Chain reorganization counter
    static ref CHAIN_REORG_COUNT: CounterVec = register_counter_vec!(
        "omikuji_chain_reorgs_total",
        "Total number of chain reorganizations detected",
        &["network", "depth"]
    ).expect("Failed to create chain_reorg_count metric");

    /// Network sync status (1 = synced, 0 = not synced)
    static ref NETWORK_SYNC_STATUS: GaugeVec = register_gauge_vec!(
        "omikuji_network_sync_status",
        "Network sync status (1 = synced, 0 = not synced)",
        &["network"]
    ).expect("Failed to create network_sync_status metric");

    /// RPC endpoint health (1 = healthy, 0 = unhealthy)
    static ref RPC_ENDPOINT_HEALTH: GaugeVec = register_gauge_vec!(
        "omikuji_rpc_endpoint_health",
        "RPC endpoint health status",
        &["network", "endpoint"]
    ).expect("Failed to create rpc_endpoint_health metric");

    /// Block time gauge
    static ref BLOCK_TIME_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_block_time_seconds",
        "Average block time in seconds",
        &["network"]
    ).expect("Failed to create block_time_seconds metric");

    /// Pending transaction count
    static ref PENDING_TRANSACTION_COUNT: GaugeVec = register_gauge_vec!(
        "omikuji_pending_transactions",
        "Number of pending transactions",
        &["network", "feed_name"]
    ).expect("Failed to create pending_transaction_count metric");

    /// Network gas price
    static ref NETWORK_GAS_PRICE_GWEI: GaugeVec = register_gauge_vec!(
        "omikuji_network_gas_price_gwei",
        "Current network gas price in gwei",
        &["network", "percentile"]
    ).expect("Failed to create network_gas_price metric");

    /// Connection pool metrics
    static ref CONNECTION_POOL_SIZE: GaugeVec = register_gauge_vec!(
        "omikuji_rpc_connection_pool_size",
        "RPC connection pool size",
        &["network", "state"]
    ).expect("Failed to create connection_pool_size metric");

    /// RPC error counter by type
    static ref RPC_ERROR_COUNT: CounterVec = register_counter_vec!(
        "omikuji_rpc_errors_total",
        "Total number of RPC errors by type",
        &["network", "error_type", "method"]
    ).expect("Failed to create rpc_error_count metric");

    /// Network base fee for EIP-1559
    static ref NETWORK_BASE_FEE_GWEI: GaugeVec = register_gauge_vec!(
        "omikuji_network_base_fee_gwei",
        "Current network base fee in gwei (EIP-1559)",
        &["network"]
    ).expect("Failed to create network_base_fee metric");

    /// Network priority fee percentiles
    static ref NETWORK_PRIORITY_FEE_PERCENTILES_GWEI: GaugeVec = register_gauge_vec!(
        "omikuji_network_priority_fee_percentiles_gwei",
        "Network priority fee percentiles in gwei",
        &["network", "percentile"]
    ).expect("Failed to create network_priority_fee_percentiles metric");

    /// Network congestion level (0-100)
    static ref NETWORK_CONGESTION_LEVEL: GaugeVec = register_gauge_vec!(
        "omikuji_network_congestion_level",
        "Network congestion level (0=low, 100=high)",
        &["network"]
    ).expect("Failed to create network_congestion_level metric");

    /// Last block timestamp
    static ref LAST_BLOCK_TIMESTAMP: GaugeVec = register_gauge_vec!(
        "omikuji_last_block_timestamp",
        "Timestamp of the last processed block",
        &["network"]
    ).expect("Failed to create last_block_timestamp metric");
}

/// Network metrics collector
pub struct NetworkMetrics;

impl NetworkMetrics {
    /// Record an RPC request
    pub fn record_rpc_request(
        network: &str,
        method: &str,
        success: bool,
        latency: Duration,
        error_type: Option<&str>,
    ) {
        let status = if success { "success" } else { "error" };

        RPC_REQUEST_COUNT
            .with_label_values(&[network, method, status])
            .inc();

        RPC_REQUEST_LATENCY_SECONDS
            .with_label_values(&[network, method])
            .observe(latency.as_secs_f64());

        if !success {
            let error = error_type.unwrap_or("unknown");
            RPC_ERROR_COUNT
                .with_label_values(&[network, error, method])
                .inc();

            warn!(
                "RPC request failed on {}: {} - {} (latency: {:.3}s)",
                network,
                method,
                error,
                latency.as_secs_f64()
            );
        } else {
            debug!(
                "RPC request on {}: {} completed in {:.3}s",
                network,
                method,
                latency.as_secs_f64()
            );
        }
    }

    /// Update chain head block number
    pub fn update_chain_head(network: &str, block_number: u64) {
        let gauge = CHAIN_HEAD_BLOCK.with_label_values(&[network]);
        let previous = gauge.get() as u64;

        gauge.set(block_number as f64);

        // Check for reorg
        if previous > 0 && block_number < previous {
            let depth = previous - block_number;
            let depth_category = match depth {
                1 => "shallow",
                2..=5 => "medium",
                _ => "deep",
            };

            CHAIN_REORG_COUNT
                .with_label_values(&[network, depth_category])
                .inc();

            warn!(
                "Chain reorganization detected on {}: {} -> {} (depth: {})",
                network, previous, block_number, depth
            );
        }
    }

    /// Update network sync status
    pub fn update_sync_status(network: &str, is_synced: bool) {
        NETWORK_SYNC_STATUS
            .with_label_values(&[network])
            .set(if is_synced { 1.0 } else { 0.0 });

        if !is_synced {
            warn!("Network {} is not synced", network);
        }
    }

    /// Update RPC endpoint health
    pub fn update_endpoint_health(network: &str, endpoint: &str, is_healthy: bool) {
        RPC_ENDPOINT_HEALTH
            .with_label_values(&[network, endpoint])
            .set(if is_healthy { 1.0 } else { 0.0 });

        if !is_healthy {
            warn!(
                "RPC endpoint {} for network {} is unhealthy",
                endpoint, network
            );
        }
    }

    /// Update block time
    pub fn update_block_time(network: &str, block_time_seconds: f64) {
        BLOCK_TIME_SECONDS
            .with_label_values(&[network])
            .set(block_time_seconds);
    }

    /// Update pending transaction count
    pub fn update_pending_transactions(network: &str, feed_name: &str, count: usize) {
        PENDING_TRANSACTION_COUNT
            .with_label_values(&[network, feed_name])
            .set(count as f64);

        if count > 5 {
            warn!(
                "High pending transaction count for {}/{}: {}",
                network, feed_name, count
            );
        }
    }

    /// Update network gas price
    pub fn update_gas_price(network: &str, percentile: &str, price_gwei: f64) {
        NETWORK_GAS_PRICE_GWEI
            .with_label_values(&[network, percentile])
            .set(price_gwei);
    }

    /// Update connection pool metrics
    pub fn update_connection_pool(network: &str, active: usize, idle: usize, total: usize) {
        CONNECTION_POOL_SIZE
            .with_label_values(&[network, "active"])
            .set(active as f64);

        CONNECTION_POOL_SIZE
            .with_label_values(&[network, "idle"])
            .set(idle as f64);

        CONNECTION_POOL_SIZE
            .with_label_values(&[network, "total"])
            .set(total as f64);

        let utilization = if total > 0 {
            (active as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        if utilization > 80.0 {
            warn!(
                "High connection pool utilization for {}: {:.1}% ({}/{})",
                network, utilization, active, total
            );
        }
    }

    /// Update network base fee
    pub fn update_base_fee(network: &str, base_fee_gwei: f64) {
        NETWORK_BASE_FEE_GWEI
            .with_label_values(&[network])
            .set(base_fee_gwei);
    }

    /// Update priority fee percentiles
    pub fn update_priority_fee_percentile(network: &str, percentile: &str, fee_gwei: f64) {
        NETWORK_PRIORITY_FEE_PERCENTILES_GWEI
            .with_label_values(&[network, percentile])
            .set(fee_gwei);
    }

    /// Update network congestion level
    pub fn update_congestion_level(network: &str, level: f64) {
        let clamped_level = level.clamp(0.0, 100.0);
        NETWORK_CONGESTION_LEVEL
            .with_label_values(&[network])
            .set(clamped_level);

        if clamped_level > 80.0 {
            warn!(
                "High network congestion on {}: {:.1}%",
                network, clamped_level
            );
        }
    }

    /// Update last block timestamp
    pub fn update_last_block_timestamp(network: &str, timestamp: u64) {
        LAST_BLOCK_TIMESTAMP
            .with_label_values(&[network])
            .set(timestamp as f64);
    }

    /// Get error type from error string
    pub fn classify_rpc_error(error: &str) -> &'static str {
        if error.contains("timeout") {
            "timeout"
        } else if error.contains("rate") || error.contains("429") {
            "rate_limit"
        } else if error.contains("connection") || error.contains("transport") {
            "connection"
        } else if error.contains("nonce") {
            "nonce"
        } else if error.contains("insufficient funds") {
            "insufficient_funds"
        } else if error.contains("revert") {
            "revert"
        } else if error.contains("gas") {
            "gas"
        } else {
            "other"
        }
    }
}
