//! Examples demonstrating how to use the test utilities

#[cfg(test)]
mod examples {
    use crate::config::models::{Network, NetworkNode};
    use crate::test_utils::{assertions::*, builders::*, edge_cases::*, factories::*, mocks::*};

    /// Example: Before and after using test utilities
    #[test]
    fn test_network_creation_comparison() {
        // OLD WAY (without utilities) - lots of boilerplate
        let old_network = Network {
            name: "test".to_string(),
            nodes: vec![NetworkNode {
                name: "Test Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        // NEW WAY (with utilities) - clean and expressive
        let new_network = NetworkBuilder::new("test")
            .with_rpc_url("http://localhost:8545")
            .with_transaction_type("eip1559")
            .with_gas_token("ethereum", "ETH")
            .build();

        assert_eq!(old_network.name, new_network.name);
        assert_eq!(old_network.rpc_url, new_network.rpc_url);
        assert_eq!(old_network.transaction_type, new_network.transaction_type);
    }

    /// Example: Using prebuilt configurations
    #[test]
    fn test_prebuilt_configurations() {
        // Use common prebuilt networks
        let eth_network = NetworkBuilder::ethereum_mainnet();
        let base_network = NetworkBuilder::base_mainnet();

        assert_eq!(eth_network.name, "ethereum-mainnet");
        assert_eq!(base_network.name, "base-mainnet");

        // Use prebuilt configurations
        let minimal_config = ConfigBuilder::minimal();
        let multi_network_config = ConfigBuilder::multi_network();

        assert_eq!(minimal_config.networks.len(), 1);
        assert_eq!(multi_network_config.networks.len(), 2);
    }

    /// Example: Using data factories
    #[test]
    fn test_data_factories() {
        // Create test data with factories
        let eth_price = GasTokenPriceFactory::create_eth_price(2500.0);
        let success_log = FeedLogFactory::create_success_log("eth_usd", "ethereum", 2500.0);
        let gas_estimate = GasEstimateFactory::create_eth_standard();

        assert_eq!(eth_price.symbol, "ETH");
        assert_eq!(success_log.feed_name, "eth_usd");
        assert!(gas_estimate.max_fee_per_gas.is_some());

        // Create batch data
        let logs_batch = FeedLogFactory::create_batch("btc_usd", "ethereum", 5, 45000.0);
        assert_eq!(logs_batch.len(), 5);
    }

    /// Example: Using custom assertions
    #[test]
    fn test_custom_assertions() {
        // Test address validation
        address_assertions::assert_valid_address("0x1234567890123456789012345678901234567890");

        // Test gas price conversions
        gas_assertions::assert_gwei_to_wei_conversion(50.0, 50_000_000_000);
        gas_assertions::assert_gas_efficiency(150_000, 200_000, 75.0);

        // Test value scaling
        value_assertions::assert_value_scaling(123.456, 8, 12345600000);

        // Test configuration validation
        config_assertions::assert_valid_network_name("ethereum-mainnet");
        config_assertions::assert_valid_feed_name("eth_usd");
    }

    /// Example: Using edge case testing
    #[test]
    fn test_edge_case_utilities() {
        let mut tested_count = 0;

        // Test with floating point edge cases
        float_edge_cases::test_with_edge_cases(|value, description| {
            // Your test logic here - this function will be called
            // with each edge case value
            assert!(value.is_finite() || value.is_nan() || value.is_infinite());
            tested_count += 1;
        });

        assert!(tested_count > 10); // Ensure many edge cases were tested

        // Test with price-specific values
        float_edge_cases::test_with_price_values(|price, _description| {
            assert!(price >= 0.0); // Prices should be non-negative
        });

        // Test with address edge cases
        address_edge_cases::test_address_edge_cases(|address, _description| {
            assert!(address.starts_with("0x"));
            assert_eq!(address.len(), 42);
        });
    }

    /// Example: Using mock objects
    #[tokio::test]
    async fn test_mock_usage() {
        // Create a mock key storage
        let mock_storage = MockKeyStorage::new();

        // Test normal operations
        mock_storage.store("test_key", "test_value").await.unwrap();
        let value = mock_storage.get("test_key").await.unwrap();
        assert_eq!(value, "test_value");

        // Test with failure configuration
        let failing_storage =
            MockKeyStorage::with_failures(FailureConfig::fail_on_operation("get"));

        failing_storage.store("key", "value").await.unwrap(); // Should work
        let result = failing_storage.get("key").await; // Should fail
        assert!(result.is_err());

        // Test mock price cache
        let price_cache = MockPriceCache::new();
        price_cache.set_price("ethereum", 2500.0, 1234567890);
        let price = price_cache.get_price("ethereum").unwrap().unwrap();
        assert_eq!(price.0, 2500.0);
    }

    /// Example: Comprehensive test using multiple utilities
    #[test]
    fn test_comprehensive_example() {
        // Create test configuration using builders
        let config = ConfigBuilder::new()
            .add_network(
                NetworkBuilder::new("test-network")
                    .with_gas_config(
                        GasConfigBuilder::new()
                            .with_gas_limit(200_000)
                            .with_multiplier(1.5)
                            .build(),
                    )
                    .build(),
            )
            .build();

        assert_eq!(config.networks.len(), 1);
        let network = &config.networks[0];

        // Validate using custom assertions
        config_assertions::assert_valid_network_name(&network.name);
        gas_assertions::assert_reasonable_gas_limit(network.gas_config.gas_limit.unwrap());

        // Create test data using factories
        let transaction_details = TransactionDetailsFactory::create_success(
            "0xabc123",
            "eth_usd",
            &network.name,
            150_000,
            30.0,
        );

        // Validate the transaction details
        transaction_assertions::assert_transaction_success(&transaction_details.status);
        gas_assertions::assert_gas_efficiency(
            transaction_details.gas_used,
            transaction_details.gas_limit,
            transaction_details.efficiency_percent,
        );
    }
}

/// Example demonstrating test utility patterns for different scenarios
#[cfg(test)]
mod patterns {
    use crate::test_utils::{assertions::*, builders::*, edge_cases::*, mocks::*};

    /// Pattern: Testing configuration validation
    #[test]
    fn test_configuration_validation_pattern() {
        // Test valid configurations
        let valid_names = vec!["ethereum", "base", "polygon", "test-network", "local_dev"];
        for name in valid_names {
            config_assertions::assert_valid_network_name(name);
        }

        // Test with edge cases
        string_edge_cases::test_network_name_edge_cases(|name, _description| {
            // Each network name will be tested automatically
            if !name.is_empty() && name.len() <= 50 {
                config_assertions::assert_valid_network_name(name);
            }
        });
    }

    /// Pattern: Testing error scenarios comprehensively
    #[test]
    fn test_error_handling_pattern() {
        let http_errors = error_scenarios::http_error_codes();
        for (status_code, description) in http_errors {
            // Test that your error handling code properly handles each status code
            assert!(status_code >= 400);
            assert!(!description.is_empty());
        }

        let blockchain_errors = error_scenarios::blockchain_error_scenarios();
        for (error_message, error_type) in blockchain_errors {
            // Test that your error parsing code handles each type
            assert!(!error_message.is_empty());
            assert!(!error_type.is_empty());
        }
    }

    /// Pattern: Testing with realistic data variations
    #[test]
    fn test_realistic_data_pattern() {
        // Test with realistic price variations
        float_edge_cases::test_with_price_values(|price, description| {
            let scaled = (price * 1e8) as i128; // Scale for 8 decimals

            // Verify scaling doesn't overflow for realistic prices
            if price > 0.0 && price < 1e10 {
                assert!(scaled > 0);
                assert!(scaled < i128::MAX);
            }
        });

        // Test with realistic gas values
        integer_edge_cases::test_gas_values(|gas_limit, _description| {
            gas_assertions::assert_reasonable_gas_limit(gas_limit);
        });
    }

    /// Pattern: Integration testing with builders
    #[tokio::test]
    async fn test_integration_pattern() {
        // Build a complete test environment
        let config = ConfigBuilder::new()
            .add_networks(vec![
                NetworkBuilder::ethereum_mainnet(),
                NetworkBuilder::base_mainnet(),
            ])
            .build();

        let mock_storage = MockKeyStorage::new();
        let mock_cache = MockPriceCache::new();

        // Set up test data
        mock_cache.set_price("ethereum", 2500.0, 1234567890);
        mock_storage
            .store("eth-key", "test-private-key")
            .await
            .unwrap();

        // Verify the complete setup
        assert_eq!(config.networks.len(), 2);
        assert!(mock_cache.has_token("ethereum"));
        assert!(mock_storage.key_exists("eth-key"));

        // Test interactions between components
        for network in &config.networks {
            config_assertions::assert_valid_network_name(&network.name);
            config_assertions::assert_valid_url(&network.nodes[0].rpc_url);
        }
    }
}
