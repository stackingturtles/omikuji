//! Test data factories for creating common test objects

use crate::database::models::{FeedLog, NewFeedLog};
use crate::gas::GasEstimate;
use crate::gas_price::models::GasTokenPrice;
use crate::metrics::gas_metrics::TransactionDetails;
use alloy::primitives::{TxHash, U256};
use chrono::{DateTime, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

/// Factory for creating GasTokenPrice test objects
pub struct GasTokenPriceFactory;

impl GasTokenPriceFactory {
    /// Create a standard Ethereum price
    pub fn create_eth_price(price_usd: f64) -> GasTokenPrice {
        GasTokenPrice {
            token_id: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            price_usd,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            source: "test".to_string(),
        }
    }

    /// Create a price for any token
    pub fn create_price(token_id: &str, symbol: &str, price_usd: f64) -> GasTokenPrice {
        GasTokenPrice {
            token_id: token_id.to_string(),
            symbol: symbol.to_string(),
            price_usd,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            source: "test".to_string(),
        }
    }

    /// Create an expired price (older than 5 minutes)
    pub fn create_expired_price(token_id: &str, symbol: &str, price_usd: f64) -> GasTokenPrice {
        GasTokenPrice {
            token_id: token_id.to_string(),
            symbol: symbol.to_string(),
            price_usd,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(600), // 10 minutes ago
            source: "test".to_string(),
        }
    }

    /// Create a standard BASE price
    pub fn create_base_price(price_usd: f64) -> GasTokenPrice {
        Self::create_price("ethereum", "ETH", price_usd) // BASE uses ETH for gas
    }
}

/// Factory for creating FeedLog test objects
pub struct FeedLogFactory;

impl FeedLogFactory {
    /// Create a successful feed log entry
    pub fn create_success_log(feed_name: &str, network: &str, value: f64) -> NewFeedLog {
        NewFeedLog {
            feed_name: feed_name.to_string(),
            network_name: network.to_string(),
            feed_value: value,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: None,
            network_error: false,
        }
    }

    /// Create an error feed log entry with HTTP status code
    pub fn create_error_log(feed_name: &str, network: &str, status_code: i32) -> NewFeedLog {
        NewFeedLog {
            feed_name: feed_name.to_string(),
            network_name: network.to_string(),
            feed_value: 0.0,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: Some(status_code),
            network_error: false,
        }
    }

    /// Create a network error feed log entry
    pub fn create_network_error_log(feed_name: &str, network: &str) -> NewFeedLog {
        NewFeedLog {
            feed_name: feed_name.to_string(),
            network_name: network.to_string(),
            feed_value: 0.0,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: None,
            network_error: true,
        }
    }

    /// Create a complete FeedLog (as returned from database)
    pub fn create_feed_log(
        id: i32,
        feed_name: &str,
        network: &str,
        value: f64,
        created_at: DateTime<Utc>,
    ) -> FeedLog {
        FeedLog {
            id,
            feed_name: feed_name.to_string(),
            network_name: network.to_string(),
            feed_value: value,
            feed_timestamp: created_at.timestamp(),
            updated_at: created_at,
            error_status_code: None,
            network_error: false,
            created_at,
        }
    }

    /// Create a batch of test feed logs
    pub fn create_batch(
        feed_name: &str,
        network: &str,
        count: usize,
        base_value: f64,
    ) -> Vec<NewFeedLog> {
        (0..count)
            .map(|i| Self::create_success_log(feed_name, network, base_value + i as f64))
            .collect()
    }
}

/// Factory for creating GasEstimate test objects
pub struct GasEstimateFactory;

impl GasEstimateFactory {
    /// Create a legacy gas estimate
    pub fn create_legacy(gas_limit: u64, gas_price: u64) -> GasEstimate {
        GasEstimate {
            gas_limit: U256::from(gas_limit),
            gas_price: Some(U256::from(gas_price)),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        }
    }

    /// Create an EIP-1559 gas estimate
    pub fn create_eip1559(
        gas_limit: u64,
        max_fee_per_gas: u64,
        max_priority_fee_per_gas: u64,
    ) -> GasEstimate {
        GasEstimate {
            gas_limit: U256::from(gas_limit),
            gas_price: None,
            max_fee_per_gas: Some(U256::from(max_fee_per_gas)),
            max_priority_fee_per_gas: Some(U256::from(max_priority_fee_per_gas)),
        }
    }

    /// Create a standard Ethereum gas estimate
    pub fn create_eth_standard() -> GasEstimate {
        Self::create_eip1559(
            200_000,        // gas limit
            50_000_000_000, // max fee per gas (50 gwei)
            2_000_000_000,  // max priority fee per gas (2 gwei)
        )
    }

    /// Create a high gas price estimate for congested network
    pub fn create_high_gas() -> GasEstimate {
        Self::create_eip1559(
            200_000,         // gas limit
            200_000_000_000, // max fee per gas (200 gwei)
            20_000_000_000,  // max priority fee per gas (20 gwei)
        )
    }

    /// Create a low gas price estimate for quiet network
    pub fn create_low_gas() -> GasEstimate {
        Self::create_eip1559(
            200_000,        // gas limit
            10_000_000_000, // max fee per gas (10 gwei)
            1_000_000_000,  // max priority fee per gas (1 gwei)
        )
    }
}

/// Factory for creating TransactionDetails test objects
pub struct TransactionDetailsFactory;

impl TransactionDetailsFactory {
    /// Create successful transaction details
    pub fn create_success(
        tx_hash: &str,
        feed_name: &str,
        network: &str,
        gas_used: u64,
        gas_price_gwei: f64,
    ) -> TransactionDetails {
        let gas_limit = gas_used + 50_000; // Add some buffer
        let efficiency = (gas_used as f64 / gas_limit as f64) * 100.0;
        let total_cost_wei = (gas_used as f64 * gas_price_gwei * 1e9) as u128;

        TransactionDetails {
            tx_hash: tx_hash.to_string(),
            feed_name: feed_name.to_string(),
            network: network.to_string(),
            gas_limit,
            gas_used,
            gas_price_gwei,
            total_cost_wei,
            efficiency_percent: efficiency,
            tx_type: "eip1559".to_string(),
            status: "success".to_string(),
            block_number: 18_000_000,
            error_message: None,
        }
    }

    /// Create failed transaction details
    pub fn create_failed(
        tx_hash: &str,
        feed_name: &str,
        network: &str,
        error_message: &str,
    ) -> TransactionDetails {
        TransactionDetails {
            tx_hash: tx_hash.to_string(),
            feed_name: feed_name.to_string(),
            network: network.to_string(),
            gas_limit: 200_000,
            gas_used: 0, // No gas used for failed transactions
            gas_price_gwei: 30.0,
            total_cost_wei: 0,
            efficiency_percent: 0.0,
            tx_type: "eip1559".to_string(),
            status: "failed".to_string(),
            block_number: 18_000_000,
            error_message: Some(error_message.to_string()),
        }
    }

    /// Create highly efficient transaction details
    pub fn create_efficient(tx_hash: &str, feed_name: &str, network: &str) -> TransactionDetails {
        Self::create_success(tx_hash, feed_name, network, 190_000, 25.0) // 95% efficiency
    }

    /// Create inefficient transaction details
    pub fn create_inefficient(tx_hash: &str, feed_name: &str, network: &str) -> TransactionDetails {
        Self::create_success(tx_hash, feed_name, network, 100_000, 25.0) // 50% efficiency
    }
}

/// Create a test database pool
#[cfg(test)]
pub async fn create_test_db_pool() -> Pool<Postgres> {
    use std::env;

    // Use TEST_DATABASE_URL if available, otherwise use DATABASE_URL
    let database_url = env::var("TEST_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .expect("Neither TEST_DATABASE_URL nor DATABASE_URL is set");

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create test database pool")
}

/// Factory for creating test transaction hashes
pub struct TxHashFactory;

impl TxHashFactory {
    /// Create a test transaction hash from a number
    pub fn create(number: u8) -> TxHash {
        TxHash::from([number; 32])
    }

    /// Create a test transaction hash from a string (truncated/padded to 32 bytes)
    pub fn from_string(s: &str) -> TxHash {
        let mut bytes = [0u8; 32];
        let s_bytes = s.as_bytes();
        let len = std::cmp::min(s_bytes.len(), 32);
        bytes[..len].copy_from_slice(&s_bytes[..len]);
        TxHash::from(bytes)
    }

    /// Create a sequence of test transaction hashes
    pub fn create_sequence(count: u8) -> Vec<TxHash> {
        (1..=count).map(Self::create).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_token_price_factory() {
        let eth_price = GasTokenPriceFactory::create_eth_price(2500.0);
        assert_eq!(eth_price.symbol, "ETH");
        assert_eq!(eth_price.price_usd, 2500.0);
        assert_eq!(eth_price.source, "test");

        let custom_price = GasTokenPriceFactory::create_price("bitcoin", "BTC", 45000.0);
        assert_eq!(custom_price.token_id, "bitcoin");
        assert_eq!(custom_price.symbol, "BTC");
        assert_eq!(custom_price.price_usd, 45000.0);
    }

    #[test]
    fn test_feed_log_factory() {
        let success_log = FeedLogFactory::create_success_log("eth_usd", "ethereum", 2500.0);
        assert_eq!(success_log.feed_name, "eth_usd");
        assert_eq!(success_log.network_name, "ethereum");
        assert_eq!(success_log.feed_value, 2500.0);
        assert!(success_log.error_status_code.is_none());
        assert!(!success_log.network_error);

        let error_log = FeedLogFactory::create_error_log("btc_usd", "ethereum", 500);
        assert_eq!(error_log.error_status_code, Some(500));
        assert!(!error_log.network_error);

        let network_error_log = FeedLogFactory::create_network_error_log("eth_usd", "base");
        assert!(network_error_log.error_status_code.is_none());
        assert!(network_error_log.network_error);
    }

    #[test]
    fn test_gas_estimate_factory() {
        let legacy = GasEstimateFactory::create_legacy(100_000, 30_000_000_000);
        assert_eq!(legacy.gas_limit, U256::from(100_000));
        assert_eq!(legacy.gas_price, Some(U256::from(30_000_000_000u64)));
        assert!(legacy.max_fee_per_gas.is_none());

        let eip1559 = GasEstimateFactory::create_eip1559(200_000, 50_000_000_000, 2_000_000_000);
        assert_eq!(eip1559.gas_limit, U256::from(200_000));
        assert!(eip1559.gas_price.is_none());
        assert_eq!(eip1559.max_fee_per_gas, Some(U256::from(50_000_000_000u64)));
        assert_eq!(
            eip1559.max_priority_fee_per_gas,
            Some(U256::from(2_000_000_000u64))
        );
    }

    #[test]
    fn test_transaction_details_factory() {
        let success = TransactionDetailsFactory::create_success(
            "0xabc123", "eth_usd", "ethereum", 150_000, 30.0,
        );
        assert_eq!(success.status, "success");
        assert_eq!(success.gas_used, 150_000);
        assert!(success.error_message.is_none());

        let failed =
            TransactionDetailsFactory::create_failed("0xfailed", "btc_usd", "ethereum", "Reverted");
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.gas_used, 0);
        assert_eq!(failed.error_message, Some("Reverted".to_string()));
    }

    #[test]
    fn test_tx_hash_factory() {
        let hash1 = TxHashFactory::create(1);
        let hash2 = TxHashFactory::create(2);
        assert_ne!(hash1, hash2);

        let hash_from_string = TxHashFactory::from_string("test");
        assert_eq!(&hash_from_string.as_slice()[0..4], b"test");

        let sequence = TxHashFactory::create_sequence(3);
        assert_eq!(sequence.len(), 3);
        assert_ne!(sequence[0], sequence[1]);
        assert_ne!(sequence[1], sequence[2]);
    }

    #[test]
    fn test_feed_log_batch() {
        let batch = FeedLogFactory::create_batch("eth_usd", "ethereum", 5, 2000.0);
        assert_eq!(batch.len(), 5);
        assert_eq!(batch[0].feed_value, 2000.0);
        assert_eq!(batch[4].feed_value, 2004.0);

        for log in &batch {
            assert_eq!(log.feed_name, "eth_usd");
            assert_eq!(log.network_name, "ethereum");
        }
    }
}
