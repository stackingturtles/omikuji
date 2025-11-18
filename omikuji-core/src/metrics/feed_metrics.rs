use lazy_static::lazy_static;
use prometheus::{register_gauge_vec, GaugeVec};
use tracing::{debug, warn};

lazy_static! {
    /// Wallet balance in wei for each network
    static ref WALLET_BALANCE_WEI: GaugeVec = register_gauge_vec!(
        "omikuji_wallet_balance_wei",
        "Wallet balance in wei",
        &["network", "address"]
    ).expect("Failed to create wallet_balance_wei metric");

    /// Latest feed value from external source
    static ref FEED_VALUE: GaugeVec = register_gauge_vec!(
        "omikuji_feed_value",
        "Latest value from external feed source",
        &["feed", "network"]
    ).expect("Failed to create feed_value metric");

    /// Timestamp of last feed update from external source
    static ref FEED_LAST_UPDATE_TIMESTAMP: GaugeVec = register_gauge_vec!(
        "omikuji_feed_last_update_timestamp",
        "Unix timestamp of last feed update",
        &["feed", "network"]
    ).expect("Failed to create feed_last_update_timestamp metric");

    /// Timestamp of last contract update
    static ref CONTRACT_LAST_UPDATE_TIMESTAMP: GaugeVec = register_gauge_vec!(
        "omikuji_contract_last_update_timestamp",
        "Unix timestamp of last contract update",
        &["feed", "network"]
    ).expect("Failed to create contract_last_update_timestamp metric");

    /// Latest on-chain contract value
    static ref CONTRACT_VALUE: GaugeVec = register_gauge_vec!(
        "omikuji_contract_value",
        "Latest value from on-chain contract",
        &["feed", "network"]
    ).expect("Failed to create contract_value metric");

    /// Latest round number
    static ref CONTRACT_ROUND: GaugeVec = register_gauge_vec!(
        "omikuji_contract_round",
        "Latest round number from contract",
        &["feed", "network"]
    ).expect("Failed to create contract_round metric");

    /// Absolute deviation percentage between contract and feed values
    static ref FEED_DEVIATION_PERCENT: GaugeVec = register_gauge_vec!(
        "omikuji_feed_deviation_percent",
        "Absolute deviation percentage between contract and feed values",
        &["feed", "network"]
    ).expect("Failed to create feed_deviation_percent metric");

    /// Data staleness indicator (seconds since last update)
    static ref DATA_STALENESS_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_data_staleness_seconds",
        "Seconds since last successful data update",
        &["feed", "network", "data_type"]
    ).expect("Failed to create data_staleness_seconds metric");

    /// Time since this node last submitted a value to the feed contract
    pub static ref FEED_LAST_SUBMISSION_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_feed_last_submission_seconds",
        "Seconds since this node last submitted a value to the feed contract",
        &["feed_name", "network"]
    ).expect("Failed to create feed_last_submission_seconds metric");
}

/// Feed metrics collector
pub struct FeedMetrics;

impl FeedMetrics {
    /// Update wallet balance metric
    pub fn set_wallet_balance(network: &str, address: &str, balance_wei: u128) {
        WALLET_BALANCE_WEI
            .with_label_values(&[network, address])
            .set(balance_wei as f64);

        debug!(
            "Updated wallet balance for {} on {}: {} wei",
            address, network, balance_wei
        );
    }

    /// Update feed value from external source
    pub fn set_feed_value(feed_name: &str, network: &str, value: f64, timestamp: u64) {
        FEED_VALUE
            .with_label_values(&[feed_name, network])
            .set(value);

        FEED_LAST_UPDATE_TIMESTAMP
            .with_label_values(&[feed_name, network])
            .set(timestamp as f64);

        // Reset staleness counter
        DATA_STALENESS_SECONDS
            .with_label_values(&[feed_name, network, "feed"])
            .set(0.0);

        debug!(
            "Updated feed value for {} on {}: {} at timestamp {}",
            feed_name, network, value, timestamp
        );
    }

    /// Update contract value and round
    pub fn set_contract_value(
        feed_name: &str,
        network: &str,
        value: f64,
        round: u64,
        timestamp: u64,
    ) {
        CONTRACT_VALUE
            .with_label_values(&[feed_name, network])
            .set(value);

        CONTRACT_ROUND
            .with_label_values(&[feed_name, network])
            .set(round as f64);

        CONTRACT_LAST_UPDATE_TIMESTAMP
            .with_label_values(&[feed_name, network])
            .set(timestamp as f64);

        // Reset staleness counter
        DATA_STALENESS_SECONDS
            .with_label_values(&[feed_name, network, "contract"])
            .set(0.0);

        debug!(
            "Updated contract value for {} on {}: {} (round {}) at timestamp {}",
            feed_name, network, value, round, timestamp
        );
    }

    /// Calculate and update deviation percentage
    pub fn update_deviation(feed_name: &str, network: &str, feed_value: f64, contract_value: f64) {
        if contract_value == 0.0 {
            warn!(
                "Cannot calculate deviation for {} on {}: contract value is zero",
                feed_name, network
            );
            return;
        }

        let deviation_percent = ((feed_value - contract_value).abs() / contract_value) * 100.0;

        FEED_DEVIATION_PERCENT
            .with_label_values(&[feed_name, network])
            .set(deviation_percent);

        debug!(
            "Updated deviation for {} on {}: {:.2}% (feed: {}, contract: {})",
            feed_name, network, deviation_percent, feed_value, contract_value
        );
    }

    /// Record a successful contract update
    pub fn record_contract_update(feed_name: &str, network: &str) {
        let timestamp = chrono::Utc::now().timestamp() as f64;
        CONTRACT_LAST_UPDATE_TIMESTAMP
            .with_label_values(&[feed_name, network])
            .set(timestamp);

        debug!(
            "Recorded contract update for {} on {} at timestamp {}",
            feed_name, network, timestamp
        );
    }

    /// Update the time since last submission metric
    pub fn set_last_submission_seconds(feed_name: &str, network: &str, seconds: Option<f64>) {
        let value = seconds.unwrap_or(f64::NAN);
        FEED_LAST_SUBMISSION_SECONDS
            .with_label_values(&[feed_name, network])
            .set(value);

        if seconds.is_some() {
            debug!(
                "Updated last submission for {} on {}: {:.0} seconds ago",
                feed_name, network, value
            );
        } else {
            debug!(
                "Feed {} on {} has no submission history",
                feed_name, network
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_balance_metric() {
        // Test setting wallet balance
        FeedMetrics::set_wallet_balance(
            "ethereum",
            "0x1234567890123456789012345678901234567890",
            1000000000000000000,
        );
        FeedMetrics::set_wallet_balance(
            "base",
            "0xabcdef0123456789012345678901234567890123",
            500000000000000000,
        );

        // Test with zero balance
        FeedMetrics::set_wallet_balance("polygon", "0x0000000000000000000000000000000000000000", 0);

        // Test with large balance
        FeedMetrics::set_wallet_balance(
            "arbitrum",
            "0xffffffffffffffffffffffffffffffffffffffff",
            u128::MAX,
        );
    }

    #[test]
    fn test_feed_value_metric() {
        // Test setting feed values
        FeedMetrics::set_feed_value("eth_usd", "ethereum", 2500.50, 1700000000);
        FeedMetrics::set_feed_value("btc_usd", "bitcoin", 45000.75, 1700000100);

        // Test with zero value
        FeedMetrics::set_feed_value("test_feed", "testnet", 0.0, 1700000200);

        // Test with negative value (for special cases)
        FeedMetrics::set_feed_value("temperature", "weather", -10.5, 1700000300);
    }

    #[test]
    fn test_contract_value_metric() {
        // Test setting contract values
        FeedMetrics::set_contract_value("eth_usd", "ethereum", 2499.99, 1700000000, 12345);
        FeedMetrics::set_contract_value("btc_usd", "bitcoin", 44999.00, 1700000100, 67890);

        // Test with zero round
        FeedMetrics::set_contract_value("new_feed", "testnet", 100.0, 1700000200, 0);

        // Test with large round number
        FeedMetrics::set_contract_value("old_feed", "mainnet", 1.0, 1700000300, u64::MAX);
    }

    #[test]
    fn test_deviation_calculation() {
        // Test exact match (0% deviation)
        FeedMetrics::update_deviation("eth_usd", "ethereum", 2500.0, 2500.0);

        // Test positive deviation
        FeedMetrics::update_deviation("btc_usd", "bitcoin", 45100.0, 45000.0);

        // Test negative deviation
        FeedMetrics::update_deviation("bnb_usd", "bsc", 299.5, 300.0);

        // Test large deviation
        FeedMetrics::update_deviation("volatile_feed", "testnet", 150.0, 100.0);

        // Test small deviation
        FeedMetrics::update_deviation("stable_feed", "mainnet", 1.0001, 1.0000);
    }

    #[test]
    fn test_deviation_edge_cases() {
        // Test with very small contract value (avoid division by zero)
        FeedMetrics::update_deviation("micro_feed", "testnet", 0.00001, 0.000001);

        // Test with large values
        FeedMetrics::update_deviation("large_feed", "mainnet", 1_000_000.0, 999_999.0);
    }

    #[test]
    fn test_contract_update_recording() {
        // Test recording updates for different feeds
        FeedMetrics::record_contract_update("eth_usd", "ethereum");
        FeedMetrics::record_contract_update("btc_usd", "bitcoin");
        FeedMetrics::record_contract_update("link_usd", "ethereum");

        // Test multiple updates for same feed
        FeedMetrics::record_contract_update("eth_usd", "ethereum");
        std::thread::sleep(std::time::Duration::from_millis(10));
        FeedMetrics::record_contract_update("eth_usd", "ethereum");
    }

    #[test]
    fn test_data_staleness_metric() {
        // When feed value is set, staleness should reset to 0
        FeedMetrics::set_feed_value("eth_usd", "ethereum", 2500.0, 1700000000);

        // Test that staleness metric was set via the feed value update
        // In real usage, staleness would be calculated periodically
    }

    #[test]
    fn test_metric_labels() {
        // Test various label combinations
        let networks = vec!["ethereum", "polygon", "arbitrum", "optimism", "base"];
        let feeds = vec!["eth_usd", "btc_usd", "link_usd", "uni_usd", "aave_usd"];

        for network in &networks {
            for feed in &feeds {
                FeedMetrics::set_feed_value(feed, network, 100.0, 1700000000);
                FeedMetrics::set_contract_value(feed, network, 99.5, 1700000000, 1);
                FeedMetrics::update_deviation(feed, network, 100.0, 99.5);
            }
        }
    }

    #[test]
    fn test_timestamp_values() {
        // Test with various timestamp values
        let timestamps = vec![
            0u64,            // Epoch start
            1700000000,      // Recent timestamp
            u32::MAX as u64, // Max u32 as u64
            1,               // Minimum positive
        ];

        for (i, &timestamp) in timestamps.iter().enumerate() {
            FeedMetrics::set_feed_value(&format!("test_feed_{i}"), "testnet", 100.0, timestamp);
            FeedMetrics::set_contract_value(
                &format!("test_feed_{i}"),
                "testnet",
                100.0,
                timestamp,
                i as u64,
            );
        }
    }

    #[test]
    fn test_extreme_values() {
        // Test with extreme float values
        FeedMetrics::set_feed_value("max_feed", "testnet", f64::MAX, 1700000000);
        FeedMetrics::set_feed_value("min_feed", "testnet", f64::MIN, 1700000000);
        FeedMetrics::set_feed_value("inf_feed", "testnet", f64::INFINITY, 1700000000);
        FeedMetrics::set_feed_value("neg_inf_feed", "testnet", f64::NEG_INFINITY, 1700000000);

        // Test extreme balance values
        FeedMetrics::set_wallet_balance("rich", "0xrich", u128::MAX);
        FeedMetrics::set_wallet_balance("poor", "0xpoor", 0);
    }

    #[test]
    fn test_last_submission_seconds_metric() {
        // Test with valid submission time
        FeedMetrics::set_last_submission_seconds("eth_usd", "ethereum", Some(300.0));
        FeedMetrics::set_last_submission_seconds("btc_usd", "bitcoin", Some(3600.0));

        // Test with no submission history (None)
        FeedMetrics::set_last_submission_seconds("new_feed", "testnet", None);

        // Test with zero seconds (just submitted)
        FeedMetrics::set_last_submission_seconds("active_feed", "mainnet", Some(0.0));

        // Test with large time gap
        FeedMetrics::set_last_submission_seconds("stale_feed", "testnet", Some(86400.0));
    }
}
