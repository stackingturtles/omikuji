use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use std::time::Duration;
use tracing::{debug, error, warn};

lazy_static! {
    /// Contract read operations counter
    static ref CONTRACT_READ_COUNT: CounterVec = register_counter_vec!(
        "omikuji_contract_reads_total",
        "Total number of contract read operations",
        &["feed_name", "network", "method", "status"]
    ).expect("Failed to create contract_read_count metric");

    /// Contract write operations counter
    static ref CONTRACT_WRITE_COUNT: CounterVec = register_counter_vec!(
        "omikuji_contract_writes_total",
        "Total number of contract write operations",
        &["feed_name", "network", "status"]
    ).expect("Failed to create contract_write_count metric");

    /// Contract operation latency
    static ref CONTRACT_OPERATION_LATENCY_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_contract_operation_latency_seconds",
        "Contract operation latency in seconds",
        &["feed_name", "network", "operation_type"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]
    ).expect("Failed to create contract_operation_latency metric");

    /// Transaction queue size
    static ref TRANSACTION_QUEUE_SIZE: GaugeVec = register_gauge_vec!(
        "omikuji_transaction_queue_size",
        "Number of transactions in queue",
        &["feed_name", "network", "state"]
    ).expect("Failed to create transaction_queue_size metric");

    /// Nonce gap occurrences
    static ref NONCE_GAP_COUNT: CounterVec = register_counter_vec!(
        "omikuji_nonce_gaps_total",
        "Total number of nonce gap occurrences",
        &["network", "severity"]
    ).expect("Failed to create nonce_gap_count metric");

    /// Transaction revert counter
    static ref TRANSACTION_REVERT_COUNT: CounterVec = register_counter_vec!(
        "omikuji_transaction_reverts_total",
        "Total number of transaction reverts",
        &["feed_name", "network", "reason"]
    ).expect("Failed to create transaction_revert_count metric");

    /// Contract permission errors
    static ref CONTRACT_PERMISSION_ERROR_COUNT: CounterVec = register_counter_vec!(
        "omikuji_contract_permission_errors_total",
        "Total number of contract permission errors",
        &["feed_name", "network", "method"]
    ).expect("Failed to create contract_permission_error metric");

    /// Transaction confirmation time
    static ref TRANSACTION_CONFIRMATION_TIME_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_transaction_confirmation_time_seconds",
        "Time from submission to confirmation",
        &["feed_name", "network"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0]
    ).expect("Failed to create transaction_confirmation_time metric");

    /// Transaction retry attempts
    static ref TRANSACTION_RETRY_COUNT: CounterVec = register_counter_vec!(
        "omikuji_transaction_retries_total",
        "Total number of transaction retry attempts",
        &["feed_name", "network", "retry_reason"]
    ).expect("Failed to create transaction_retry_count metric");

    /// Contract state sync gauge
    static ref CONTRACT_STATE_SYNC: GaugeVec = register_gauge_vec!(
        "omikuji_contract_state_sync",
        "Contract state synchronization status (1 = synced, 0 = out of sync)",
        &["feed_name", "network"]
    ).expect("Failed to create contract_state_sync metric");

    /// Transaction mempool time
    static ref TRANSACTION_MEMPOOL_TIME_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_transaction_mempool_time_seconds",
        "Time spent in mempool before inclusion",
        &["feed_name", "network"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0]
    ).expect("Failed to create transaction_mempool_time metric");
}

/// Contract metrics collector
pub struct ContractMetrics;

impl ContractMetrics {
    /// Record a contract read operation
    pub fn record_contract_read(
        feed_name: &str,
        network: &str,
        method: &str,
        success: bool,
        latency: Duration,
        error: Option<&str>,
    ) {
        let status = if success { "success" } else { "error" };

        CONTRACT_READ_COUNT
            .with_label_values(&[feed_name, network, method, status])
            .inc();

        CONTRACT_OPERATION_LATENCY_SECONDS
            .with_label_values(&[feed_name, network, "read"])
            .observe(latency.as_secs_f64());

        if !success {
            let err = error.unwrap_or("unknown");

            // Check for permission errors
            if err.contains("permission") || err.contains("unauthorized") || err.contains("access")
            {
                CONTRACT_PERMISSION_ERROR_COUNT
                    .with_label_values(&[feed_name, network, method])
                    .inc();

                error!(
                    "Permission error reading contract {}/{} method {}: {}",
                    feed_name, network, method, err
                );
            } else {
                warn!(
                    "Contract read failed {}/{} method {}: {}",
                    feed_name, network, method, err
                );
            }
        } else {
            debug!(
                "Contract read {}/{} method {} completed in {:.3}s",
                feed_name,
                network,
                method,
                latency.as_secs_f64()
            );
        }
    }

    /// Record a contract write operation
    pub fn record_contract_write(
        feed_name: &str,
        network: &str,
        success: bool,
        latency: Duration,
        tx_hash: Option<&str>,
    ) {
        let status = if success { "success" } else { "error" };

        CONTRACT_WRITE_COUNT
            .with_label_values(&[feed_name, network, status])
            .inc();

        CONTRACT_OPERATION_LATENCY_SECONDS
            .with_label_values(&[feed_name, network, "write"])
            .observe(latency.as_secs_f64());

        if success {
            debug!(
                "Contract write {}/{} succeeded in {:.3}s, tx: {}",
                feed_name,
                network,
                latency.as_secs_f64(),
                tx_hash.unwrap_or("unknown")
            );
        }
    }

    /// Update transaction queue metrics
    pub fn update_transaction_queue(
        feed_name: &str,
        network: &str,
        pending: usize,
        processing: usize,
        failed: usize,
    ) {
        TRANSACTION_QUEUE_SIZE
            .with_label_values(&[feed_name, network, "pending"])
            .set(pending as f64);

        TRANSACTION_QUEUE_SIZE
            .with_label_values(&[feed_name, network, "processing"])
            .set(processing as f64);

        TRANSACTION_QUEUE_SIZE
            .with_label_values(&[feed_name, network, "failed"])
            .set(failed as f64);

        let total = pending + processing + failed;
        if total > 10 {
            warn!(
                "Large transaction queue for {}/{}: {} total (pending: {}, processing: {}, failed: {})",
                feed_name, network, total, pending, processing, failed
            );
        }
    }

    /// Record a nonce gap
    pub fn record_nonce_gap(network: &str, expected: u64, actual: u64) {
        let gap = actual.abs_diff(expected);

        let severity = match gap {
            1 => "minor",
            2..=5 => "moderate",
            _ => "severe",
        };

        NONCE_GAP_COUNT
            .with_label_values(&[network, severity])
            .inc();

        error!(
            "Nonce gap detected on {}: expected {}, got {} (gap: {})",
            network, expected, actual, gap
        );
    }

    /// Record a transaction revert
    pub fn record_transaction_revert(feed_name: &str, network: &str, reason: &str) {
        let reason_category = if reason.contains("gas") {
            "out_of_gas"
        } else if reason.contains("nonce") {
            "nonce_error"
        } else if reason.contains("permission") || reason.contains("unauthorized") {
            "permission"
        } else if reason.contains("value") || reason.contains("invalid") {
            "invalid_value"
        } else {
            "other"
        };

        TRANSACTION_REVERT_COUNT
            .with_label_values(&[feed_name, network, reason_category])
            .inc();

        error!(
            "Transaction reverted for {}/{}: {} (category: {})",
            feed_name, network, reason, reason_category
        );
    }

    /// Record transaction confirmation time
    pub fn record_confirmation_time(
        feed_name: &str,
        network: &str,
        submission_time: u64,
        confirmation_time: u64,
    ) {
        if confirmation_time > submission_time {
            let duration_seconds = (confirmation_time - submission_time) as f64;

            TRANSACTION_CONFIRMATION_TIME_SECONDS
                .with_label_values(&[feed_name, network])
                .observe(duration_seconds);

            debug!(
                "Transaction confirmed for {}/{} after {:.0}s",
                feed_name, network, duration_seconds
            );
        }
    }

    /// Record a transaction retry
    pub fn record_transaction_retry(
        feed_name: &str,
        network: &str,
        retry_reason: &str,
        attempt_number: u32,
    ) {
        TRANSACTION_RETRY_COUNT
            .with_label_values(&[feed_name, network, retry_reason])
            .inc();

        warn!(
            "Retrying transaction for {}/{} (attempt {}): {}",
            feed_name, network, attempt_number, retry_reason
        );
    }

    /// Update contract state sync status
    pub fn update_contract_sync_status(
        feed_name: &str,
        network: &str,
        is_synced: bool,
        feed_value: Option<f64>,
        contract_value: Option<f64>,
    ) {
        CONTRACT_STATE_SYNC
            .with_label_values(&[feed_name, network])
            .set(if is_synced { 1.0 } else { 0.0 });

        if !is_synced {
            warn!(
                "Contract out of sync for {}/{}: feed={:?}, contract={:?}",
                feed_name, network, feed_value, contract_value
            );
        }
    }

    /// Record mempool time
    pub fn record_mempool_time(feed_name: &str, network: &str, mempool_seconds: f64) {
        TRANSACTION_MEMPOOL_TIME_SECONDS
            .with_label_values(&[feed_name, network])
            .observe(mempool_seconds);

        if mempool_seconds > 300.0 {
            warn!(
                "Transaction stuck in mempool for {}/{}: {:.0}s",
                feed_name, network, mempool_seconds
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_read_recording() {
        // Test successful read
        ContractMetrics::record_contract_read(
            "eth_usd",
            "ethereum",
            "latestAnswer",
            true,
            Duration::from_millis(100),
            None,
        );

        // Test failed read with error
        ContractMetrics::record_contract_read(
            "btc_usd",
            "bitcoin",
            "getAnswer",
            false,
            Duration::from_millis(500),
            Some("connection timeout"),
        );

        // Test various methods
        let methods = vec!["latestAnswer", "latestTimestamp", "latestRound", "decimals"];
        for method in methods {
            ContractMetrics::record_contract_read(
                "test_feed",
                "testnet",
                method,
                true,
                Duration::from_millis(50),
                None,
            );
        }
    }

    #[test]
    fn test_contract_write_recording() {
        // Test successful write
        ContractMetrics::record_contract_write(
            "eth_usd",
            "ethereum",
            true,
            Duration::from_secs(5),
            None,
        );

        // Test failed write
        ContractMetrics::record_contract_write(
            "btc_usd",
            "bitcoin",
            false,
            Duration::from_secs(30),
            Some("insufficient gas"),
        );
    }

    #[test]
    fn test_operation_latency_buckets() {
        // Test various latencies that should fall into different buckets
        let latencies = vec![
            Duration::from_millis(5),   // 0.005s - under 0.01
            Duration::from_millis(30),  // 0.03s - between 0.01 and 0.05
            Duration::from_millis(200), // 0.2s - between 0.1 and 0.25
            Duration::from_secs(1),     // 1s - between 0.5 and 1.0
            Duration::from_secs(15),    // 15s - between 10.0 and 30.0
        ];

        for (i, latency) in latencies.iter().enumerate() {
            ContractMetrics::record_contract_read(
                &format!("test_feed_{i}"),
                "testnet",
                "testMethod",
                true,
                *latency,
                None,
            );
        }
    }

    #[test]
    fn test_transaction_queue_updates() {
        // Test queue size updates
        ContractMetrics::update_transaction_queue("eth_usd", "ethereum", 5, 2, 1);
        ContractMetrics::update_transaction_queue("btc_usd", "bitcoin", 0, 0, 0);
        ContractMetrics::update_transaction_queue("link_usd", "ethereum", 10, 8, 0);
    }

    #[test]
    fn test_nonce_gap_severity() {
        // Test minor gap
        ContractMetrics::record_nonce_gap("ethereum", 100, 101);

        // Test moderate gap
        ContractMetrics::record_nonce_gap("polygon", 200, 203);

        // Test severe gap
        ContractMetrics::record_nonce_gap("arbitrum", 300, 320);

        // Test backward gap
        ContractMetrics::record_nonce_gap("optimism", 400, 395);
    }

    #[test]
    fn test_transaction_revert_categorization() {
        // Test gas-related revert
        ContractMetrics::record_transaction_revert("eth_usd", "ethereum", "out of gas");

        // Test nonce-related revert
        ContractMetrics::record_transaction_revert("btc_usd", "bitcoin", "nonce too low");

        // Test permission-related revert
        ContractMetrics::record_transaction_revert("link_usd", "ethereum", "unauthorized caller");

        // Test value-related revert
        ContractMetrics::record_transaction_revert("uni_usd", "ethereum", "invalid value");

        // Test generic revert
        ContractMetrics::record_transaction_revert("aave_usd", "polygon", "unknown error");
    }

    #[test]
    fn test_confirmation_time_buckets() {
        // Test various confirmation times
        let times = [0.5, 3.0, 15.0, 45.0, 90.0, 240.0, 450.0];

        for (i, &time) in times.iter().enumerate() {
            let submission_time = 1700000000u64;
            let confirmation_time = submission_time + (time as u64);
            ContractMetrics::record_confirmation_time(
                &format!("feed_{i}"),
                "testnet",
                submission_time,
                confirmation_time,
            );
        }
    }

    #[test]
    fn test_transaction_retry_reasons() {
        let retry_reasons = [
            "insufficient_gas",
            "nonce_conflict",
            "network_congestion",
            "timeout",
            "unknown",
        ];

        for (i, reason) in retry_reasons.iter().enumerate() {
            ContractMetrics::record_transaction_retry("test_feed", "testnet", reason, i as u32 + 1);
        }
    }

    #[test]
    fn test_contract_state_sync() {
        // Test synced state
        ContractMetrics::update_contract_sync_status(
            "eth_usd",
            "ethereum",
            true,
            Some(2500.0),
            Some(2499.0),
        );

        // Test out of sync state
        ContractMetrics::update_contract_sync_status(
            "btc_usd",
            "bitcoin",
            false,
            Some(45000.0),
            Some(44000.0),
        );

        // Test state changes
        ContractMetrics::update_contract_sync_status("link_usd", "ethereum", true, None, None);
        ContractMetrics::update_contract_sync_status(
            "link_usd",
            "ethereum",
            false,
            Some(25.0),
            Some(24.5),
        );
        ContractMetrics::update_contract_sync_status(
            "link_usd",
            "ethereum",
            true,
            Some(25.0),
            Some(25.0),
        );
    }

    #[test]
    fn test_mempool_time_recording() {
        // Test various mempool times
        let times = [2.0, 8.0, 25.0, 75.0, 180.0, 420.0];

        for (i, &time) in times.iter().enumerate() {
            ContractMetrics::record_mempool_time(&format!("feed_{i}"), "testnet", time);
        }
    }

    #[test]
    fn test_error_message_handling() {
        // Test various error messages
        let errors = [
            Some("connection refused"),
            Some("timeout after 30s"),
            Some("invalid response format"),
            None,
            Some(""),
        ];

        for (i, error) in errors.iter().enumerate() {
            ContractMetrics::record_contract_read(
                &format!("error_feed_{i}"),
                "testnet",
                "testMethod",
                false,
                Duration::from_millis(100),
                error.as_deref(),
            );
        }
    }

    #[test]
    fn test_extreme_durations() {
        // Test very short duration
        ContractMetrics::record_contract_read(
            "fast_feed",
            "testnet",
            "quickMethod",
            true,
            Duration::from_nanos(1),
            None,
        );

        // Test very long duration
        ContractMetrics::record_contract_write(
            "slow_feed",
            "testnet",
            false,
            Duration::from_secs(3600),
            Some("extreme timeout"),
        );
    }
}
