//! Integration test for plugin loading and switching

use omikuji::plugins::{register_community_plugins, EnvSecretProvider};
use omikuji_core::{
    plugin_registry::global_registry,
    traits::secrets::{SecretProvider, SecretProviderConfig},
};
use std::env;

#[tokio::test]
async fn test_plugin_registration_and_switching() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt::try_init();

    let registry = global_registry();

    // Register community plugins
    register_community_plugins(registry).expect("Failed to register community plugins");

    // Test 1: Set up environment variable provider
    let env_config = SecretProviderConfig::Env {
        prefix: Some("TEST_PLUGIN_".to_string()),
    };

    registry
        .set_active_secret_provider(env_config)
        .await
        .expect("Failed to set env provider");

    // Verify we can get the provider
    let provider = registry
        .get_secret_provider()
        .expect("No secret provider active");

    assert_eq!(provider.provider_name(), "env");

    // Test 2: Use the provider
    env::set_var("TEST_PLUGIN_TESTNET", "test_key_123");

    let key = provider
        .get_private_key("testnet")
        .await
        .expect("Failed to get key");

    assert_eq!(key, "test_key_123");

    // Test 3: Switch to keyring provider (if available)
    let keyring_config = SecretProviderConfig::Keyring {
        service: "omikuji_test".to_string(),
    };

    // This might fail if keyring is not available (e.g., in CI)
    if registry
        .set_active_secret_provider(keyring_config)
        .await
        .is_ok()
    {
        let provider = registry
            .get_secret_provider()
            .expect("No secret provider active");

        assert_eq!(provider.provider_name(), "keyring");
    }

    // Clean up
    env::remove_var("TEST_PLUGIN_TESTNET");
}

#[tokio::test]
async fn test_direct_provider_usage() {
    // Test using provider directly without registry
    let provider = EnvSecretProvider::new(Some("DIRECT_TEST_".to_string()));

    env::set_var("DIRECT_TEST_MAINNET", "direct_key");

    let key = provider
        .get_private_key("mainnet")
        .await
        .expect("Failed to get key");

    assert_eq!(key, "direct_key");

    // Test listing networks
    env::set_var("DIRECT_TEST_NETWORK1", "key1");
    env::set_var("DIRECT_TEST_NETWORK2", "key2");

    let networks = provider.list_networks().await.expect("Failed to list");
    assert!(networks.len() >= 3); // mainnet, network1, network2

    // Clean up
    env::remove_var("DIRECT_TEST_MAINNET");
    env::remove_var("DIRECT_TEST_NETWORK1");
    env::remove_var("DIRECT_TEST_NETWORK2");
}

#[test]
fn test_provider_types() {
    use omikuji_core::traits::secrets::SecretProviderType;

    // Verify provider types are correctly defined
    assert_eq!(format!("{:?}", SecretProviderType::Env), "Env");
    assert_eq!(format!("{:?}", SecretProviderType::Keyring), "Keyring");
}
