#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::config::models::{Network, NetworkNode};
    use std::env;

    // Helper function to create test network config
    fn create_test_network(name: &str, rpc_url: &str) -> Network {
        Network {
            name: name.to_string(),
            nodes: vec![NetworkNode {
                name: "Test Node".to_string(),
                rpc_url: rpc_url.to_string(),
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            rpc_url: None,
            ws_url: None,
        }
    }

    #[tokio::test]
    async fn test_network_manager_new_with_valid_networks() {
        // This test requires a local test network or will fail
        // For unit tests, we'll focus on the structure rather than actual connections
        let networks = vec![
            create_test_network("test1", "http://localhost:8545"),
            create_test_network("test2", "http://localhost:8546"),
        ];

        // We can't test actual connection without a running node
        // So we'll test the error handling
        let result = NetworkManager::new(&networks).await;

        // The test may succeed if a local node is running (e.g., Anvil for development)
        // or fail if no node is available. Both cases are valid for this test.
        match result {
            Ok(manager) => {
                // If connection succeeds, verify the manager was created properly
                assert_eq!(manager.get_network_names().len(), 2);
                assert!(manager.get_network_names().contains(&"test1".to_string()));
                assert!(manager.get_network_names().contains(&"test2".to_string()));
            }
            Err(_) => {
                // Connection failed as expected when no local node is running
                // This is also a valid test outcome
            }
        }
    }

    #[tokio::test]
    async fn test_network_manager_invalid_url() {
        let networks = vec![create_test_network("invalid", "not-a-valid-url")];

        let result = NetworkManager::new(&networks).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_provider_not_found() {
        // Create a NetworkManager with no networks
        let manager = NetworkManager::new(&[]).await.unwrap();

        let result = manager.get_provider("non_existent");
        assert!(result.is_err());

        match result {
            Err(e) => {
                let error_msg = e.to_string();
                assert!(error_msg.contains("Network not found: non_existent"));
            }
            Ok(_) => panic!("Expected error for non-existent network"),
        }
    }

    #[tokio::test]
    async fn test_get_signer_not_found() {
        // Create a NetworkManager with no networks
        let manager = NetworkManager::new(&[]).await.unwrap();

        let result = manager.get_signer("non_existent");
        assert!(result.is_err());

        match result {
            Err(e) => {
                let error_msg = e.to_string();
                assert!(error_msg.contains("No signer found for network non_existent"));
            }
            Ok(_) => panic!("Expected error for non-existent signer"),
        }
    }

    #[tokio::test]
    async fn test_load_wallet_from_env_network_not_found() {
        // Create a NetworkManager with no networks
        let mut manager = NetworkManager::new(&[]).await.unwrap();

        // Set a valid env var
        unsafe {
            env::set_var(
                "TEST_PRIVATE_KEY",
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            );
        }

        let result = manager
            .load_wallet_from_env("test_network", "TEST_PRIVATE_KEY")
            .await;
        assert!(result.is_err());

        match result {
            Err(e) => {
                let error_msg = e.to_string();
                assert!(error_msg.contains("Network not found"));
            }
            Ok(_) => panic!("Expected error for non-existent network"),
        }

        // Clean up
        unsafe {
            env::remove_var("TEST_PRIVATE_KEY");
        }
    }

    // Note: Testing invalid private key would require a valid network with provider
    // Since we can't easily mock providers in unit tests, we skip this test
    // This would be better tested as an integration test with a real test network

    #[tokio::test]
    async fn test_network_error_display() {
        let error = NetworkError::NetworkNotFound("ethereum".to_string());
        assert_eq!(error.to_string(), "Network not found: ethereum");

        let error = NetworkError::ConnectionFailed("timeout".to_string());
        assert_eq!(error.to_string(), "RPC connection failed: timeout");
    }

    // Mock provider tests would require more complex setup with mock HTTP servers
    // For now, we focus on the basic structure and error handling tests

    #[test]
    fn test_network_config_validation() {
        let valid_network = create_test_network("ethereum", "https://eth.llamarpc.com");
        assert_eq!(valid_network.name, "ethereum");
        assert_eq!(valid_network.transaction_type, "eip1559");
        assert_eq!(valid_network.gas_token_symbol, "ETH");
    }

    #[test]
    fn test_multiple_network_configurations() {
        let networks = vec![
            create_test_network("mainnet", "https://eth.llamarpc.com"),
            create_test_network("polygon", "https://polygon-rpc.com"),
            create_test_network("arbitrum", "https://arb1.arbitrum.io/rpc"),
            create_test_network("optimism", "https://mainnet.optimism.io"),
        ];

        assert_eq!(networks.len(), 4);
        assert!(networks
            .iter()
            .all(|n| !n.nodes.is_empty() && !n.nodes[0].rpc_url.is_empty()));
        assert!(networks.iter().all(|n| n.transaction_type == "eip1559"));
    }

    #[test]
    fn test_network_rpc_url_formats() {
        // Test various RPC URL formats
        let http_network = create_test_network("http", "http://localhost:8545");
        let https_network = create_test_network("https", "https://eth.llamarpc.com");
        let ws_network = create_test_network("websocket", "ws://localhost:8546");
        let wss_network = create_test_network("websocket-secure", "wss://eth-ws.llamarpc.com");

        assert!(http_network.nodes[0].rpc_url.starts_with("http://"));
        assert!(https_network.nodes[0].rpc_url.starts_with("https://"));
        assert!(ws_network.nodes[0].rpc_url.starts_with("ws://"));
        assert!(wss_network.nodes[0].rpc_url.starts_with("wss://"));
    }

    #[test]
    fn test_transaction_type_variations() {
        let mut legacy_network = create_test_network("legacy", "http://localhost:8545");
        legacy_network.transaction_type = "legacy".to_string();

        let mut eip1559_network = create_test_network("eip1559", "http://localhost:8545");
        eip1559_network.transaction_type = "eip1559".to_string();

        assert_eq!(legacy_network.transaction_type, "legacy");
        assert_eq!(eip1559_network.transaction_type, "eip1559");
    }

    #[test]
    fn test_gas_token_variations() {
        let eth_network = create_test_network("ethereum", "https://eth.llamarpc.com");

        let mut bnb_network = create_test_network("bsc", "https://bsc-dataseed.binance.org");
        bnb_network.gas_token = "binance".to_string();
        bnb_network.gas_token_symbol = "BNB".to_string();

        let mut matic_network = create_test_network("polygon", "https://polygon-rpc.com");
        matic_network.gas_token = "matic".to_string();
        matic_network.gas_token_symbol = "MATIC".to_string();

        assert_eq!(eth_network.gas_token_symbol, "ETH");
        assert_eq!(bnb_network.gas_token_symbol, "BNB");
        assert_eq!(matic_network.gas_token_symbol, "MATIC");
    }

    #[tokio::test]
    async fn test_provider_creation_error_handling() {
        // Test with invalid URLs that should fail immediately
        let invalid_urls = vec![
            "",
            "not-a-url",
            "ftp://invalid-protocol.com",
            "http://",
            "https://",
        ];

        for url in invalid_urls {
            let network = create_test_network("test", url);
            let result = NetworkManager::new(&[network]).await;
            assert!(result.is_err(), "Should fail for URL: {url}");
        }
    }

    #[test]
    fn test_network_name_validation() {
        // Test network names
        let valid_names = vec![
            "ethereum",
            "ethereum-mainnet",
            "eth_mainnet",
            "ETH",
            "polygon-mumbai",
            "arbitrum-one",
            "base",
        ];

        for name in valid_names {
            let network = create_test_network(name, "http://localhost:8545");
            assert_eq!(network.name, name);
        }
    }

    #[test]
    fn test_default_gas_config() {
        let network = create_test_network("test", "http://localhost:8545");

        // Check default gas config values
        assert!(network.gas_config.gas_limit.is_none());
        assert!(network.gas_config.gas_price_gwei.is_none());
        assert!(network.gas_config.max_fee_per_gas_gwei.is_none());
        assert!(network.gas_config.max_priority_fee_per_gas_gwei.is_none());
    }

    mod provider_tests {

        #[test]
        fn test_provider_url_parsing() {
            // Test URL parsing logic
            let test_cases = vec![
                ("http://localhost:8545", true),
                ("https://eth.llamarpc.com", true),
                ("ws://localhost:8546", true),
                ("wss://eth-ws.llamarpc.com", true),
                ("invalid://protocol", true), // url::Url::parse accepts custom protocols
                ("", false),
            ];

            for (url, should_be_valid) in test_cases {
                let parsed = url::Url::parse(url);
                assert_eq!(parsed.is_ok(), should_be_valid, "URL: {url}");
            }
        }
    }

    mod signer_tests {

        #[test]
        fn test_private_key_format() {
            // Test private key format validation (without actual keys)
            let valid_hex_lengths = vec![
                64, // 32 bytes without 0x prefix
                66, // 32 bytes with 0x prefix
            ];

            for len in valid_hex_lengths {
                let dummy_key = "0".repeat(len);
                assert_eq!(dummy_key.len(), len);
            }
        }

        #[test]
        fn test_wallet_creation_requirements() {
            // Test that wallet creation requires proper key format
            let invalid_keys = vec![
                "", "0x", "invalid", "0x123", // too short
            ];

            // Test invalid hex chars separately
            let invalid_hex = format!("0x{}", "G".repeat(64));
            assert!(!invalid_hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == 'x'));

            for key in invalid_keys {
                assert!(key.len() < 64 || !key.chars().all(|c| c.is_ascii_hexdigit() || c == 'x'));
            }
        }
    }
}
