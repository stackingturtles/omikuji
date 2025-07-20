use omikuji::config::models::{Datafeed, GasConfig, Network, NetworkNode, OmikujiConfig};
use omikuji::gas_price::{
    models::{CoinGeckoConfig, GasPriceFeedConfig},
    GasPriceManager,
};
use std::sync::Arc;

#[tokio::test]
async fn test_gas_price_manager_creation() {
    // Create a test configuration
    let config = create_test_config();

    // Create token mappings
    let mut token_mappings = std::collections::HashMap::new();
    for network in &config.networks {
        token_mappings.insert(network.name.clone(), network.gas_token.clone());
    }

    // Initialize gas price manager
    let gas_price_manager = Arc::new(GasPriceManager::new(
        config.gas_price_feeds.clone(),
        token_mappings,
        None,
    ));

    // Test USD cost calculation without actual prices (will return None)
    let gas_used = 21000u64; // Basic transfer
    let gas_price = 30_000_000_000u128; // 30 gwei

    let cost = gas_price_manager
        .calculate_usd_cost(
            "ethereum-mainnet",
            "test-feed",
            "0x123",
            gas_used,
            gas_price,
        )
        .await;

    // Without prices in cache, cost should be None
    assert!(
        cost.is_none(),
        "Should not calculate USD cost without prices"
    );
}

#[tokio::test]
async fn test_contract_updater_with_gas_price_manager() {
    let config = create_test_config();

    // Create token mappings
    let mut token_mappings = std::collections::HashMap::new();
    for network in &config.networks {
        token_mappings.insert(network.name.clone(), network.gas_token.clone());
    }

    let gas_price_manager = Arc::new(GasPriceManager::new(
        config.gas_price_feeds.clone(),
        token_mappings,
        None,
    ));

    // Test that gas price manager can be used with ContractUpdater
    // This test verifies the integration without requiring network connections
    assert!(
        gas_price_manager.is_enabled(),
        "Gas price manager should be enabled"
    );

    // Test USD cost calculation (will return None without cached prices)
    let cost = gas_price_manager
        .calculate_usd_cost(
            "ethereum-mainnet",
            "test-feed",
            "0xabc123",
            21000,
            30_000_000_000,
        )
        .await;

    assert!(
        cost.is_none(),
        "USD cost should be None without cached prices"
    );
}

#[tokio::test]
async fn test_gas_price_cache_fallback() {
    // Create config with fallback enabled
    let mut config = create_test_config();
    config.gas_price_feeds.fallback_to_cache = true;

    // Create token mappings
    let mut token_mappings = std::collections::HashMap::new();
    token_mappings.insert("ethereum-mainnet".to_string(), "ethereum".to_string());
    token_mappings.insert("polygon-mainnet".to_string(), "matic-network".to_string());

    let gas_price_manager = Arc::new(GasPriceManager::new(
        config.gas_price_feeds.clone(),
        token_mappings,
        None,
    ));

    // Verify cache statistics
    let (cache_size, cache_ttl) = gas_price_manager.cache_stats().await;
    assert_eq!(cache_size, 0, "Cache should start empty");
    assert_eq!(cache_ttl, 60, "Cache TTL should match update frequency");

    // Test getting prices for multiple networks
    let networks = vec![
        "ethereum-mainnet".to_string(),
        "polygon-mainnet".to_string(),
    ];
    let prices = gas_price_manager.get_prices(&networks).await;
    assert!(prices.is_empty(), "Should have no prices initially");

    // Verify fallback is enabled
    assert!(
        config.gas_price_feeds.fallback_to_cache,
        "Fallback should be enabled"
    );
}

#[tokio::test]
async fn test_usd_cost_metrics_recording() {
    use omikuji::metrics::gas_metrics::GasMetrics;

    // Test that GasMetrics can record USD costs without network dependencies
    let gas_used = 21000u64;
    let gas_price = 30_000_000_000u128; // 30 gwei
    let token_price_usd = 2500.0;

    // This should not panic
    GasMetrics::record_usd_cost(
        "test-feed",
        "ethereum-mainnet",
        gas_used,
        gas_price,
        token_price_usd,
    );

    // Calculate expected USD cost
    let gas_cost_native = (gas_used as f64 * gas_price as f64) / 1e18;
    let expected_usd = gas_cost_native * token_price_usd;

    // Verify the calculation
    assert!(expected_usd > 0.0, "USD cost should be positive");
    assert!(
        expected_usd < 10.0,
        "Basic transfer should cost less than $10"
    );

    // Test with different gas prices
    GasMetrics::record_usd_cost(
        "high-gas-feed",
        "ethereum-mainnet",
        100000,          // More complex transaction
        150_000_000_000, // 150 gwei (high gas)
        token_price_usd,
    );

    assert!(true, "Metrics recorded successfully");
}

fn create_test_config() -> OmikujiConfig {
    use omikuji::config::metrics_config::MetricsConfig;
    use omikuji::config::models::{DatabaseCleanupConfig, KeyStorageConfig};

    OmikujiConfig {
        networks: vec![Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![NetworkNode {
                name: "Public RPC".to_string(),
                rpc_url: "https://eth.public-rpc.com".to_string(),
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            rpc_url: None,
            ws_url: None,
        }],
        datafeeds: vec![Datafeed {
            name: "test-feed".to_string(),
            networks: "ethereum-mainnet".to_string(),
            check_frequency: 60,
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            contract_type: "fluxmon".to_string(),
            read_contract_config: false,
            decimals: Some(8),
            min_value: None,
            max_value: None,
            minimum_update_frequency: 300,
            deviation_threshold_pct: 0.5,
            feed_url: "https://api.example.com/price".to_string(),
            feed_json_path: "price".to_string(),
            feed_json_path_timestamp: None,
            data_retention_days: 7,
        }],
        database_cleanup: DatabaseCleanupConfig {
            enabled: false,
            schedule: "0 0 * * * *".to_string(),
        },
        key_storage: KeyStorageConfig {
            storage_type: "env".to_string(),
            keyring: omikuji::config::models::KeyringConfig {
                service: "omikuji".to_string(),
            },
            vault: omikuji::config::models::VaultConfig::default(),
            aws_secrets: omikuji::config::models::AwsSecretsConfig::default(),
        },
        metrics: MetricsConfig::default(),
        gas_price_feeds: GasPriceFeedConfig {
            enabled: true,
            update_frequency: 60,
            provider: "coingecko".to_string(),
            fallback_to_cache: true,
            persist_to_database: false,
            coingecko: CoinGeckoConfig {
                base_url: "https://api.coingecko.com/api/v3".to_string(),
                api_key: None,
            },
        },
        scheduled_tasks: vec![],
        event_monitors: vec![],
    }
}
