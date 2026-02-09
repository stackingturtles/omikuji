//! Integration tests for verify secrets key validation
//!
//! These tests verify that `check_secrets` validates keys are parseable,
//! not just that they exist.
//!
//! NOTE: The env secret provider prefix used by check_secrets is "OMIKUJI_PRIVATE_KEY"
//! (without trailing underscore). For a network named "foo", the env var is
//! "OMIKUJI_PRIVATE_KEYFOO". For "testvalid" -> "OMIKUJI_PRIVATE_KEYTESTVALID".

use omikuji::plugins::register_community_plugins;
use omikuji_core::{
    config::models::{KeyStorageConfig, Network, NetworkNode},
    plugin_registry::global_registry,
    verify::{secrets::check_secrets, types::CheckStatus},
};
use std::time::Duration;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// First default Anvil account private key.
const VALID_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

fn ensure_plugins_registered() {
    let registry = global_registry();
    // Ignore error if already registered by another test
    let _ = register_community_plugins(registry);
}

fn test_network(name: &str) -> Network {
    Network {
        name: name.to_string(),
        nodes: vec![NetworkNode {
            name: "test".to_string(),
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
    }
}

/// Build the env var name the same way the EnvSecretProvider does with
/// prefix "OMIKUJI_PRIVATE_KEY" (the value check_secrets passes).
fn env_var_for(network: &str) -> String {
    format!(
        "OMIKUJI_PRIVATE_KEY{}",
        network.to_uppercase().replace('-', "_")
    )
}

#[tokio::test]
async fn test_check_secrets_valid_key_shows_wallet_address() {
    ensure_plugins_registered();

    let network_name = "secvalid";
    std::env::set_var(env_var_for(network_name), VALID_KEY);

    let config = KeyStorageConfig {
        storage_type: "env".to_string(),
        ..KeyStorageConfig::default()
    };
    let networks = vec![test_network(network_name)];

    let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

    let key_result = results
        .iter()
        .find(|r| r.name == format!("Key: {network_name}"))
        .expect("should have a result for the network");

    assert_eq!(
        key_result.status,
        CheckStatus::Pass,
        "message: {}",
        key_result.message
    );
    assert!(
        key_result.message.contains("Key valid"),
        "should say key is valid: {}",
        key_result.message
    );
    assert!(
        key_result.message.contains("wallet:"),
        "should include wallet address: {}",
        key_result.message
    );

    std::env::remove_var(env_var_for(network_name));
}

#[tokio::test]
async fn test_check_secrets_invalid_key_reports_failure() {
    ensure_plugins_registered();

    let network_name = "secinvalid";
    std::env::set_var(env_var_for(network_name), "not-a-real-key");

    let config = KeyStorageConfig {
        storage_type: "env".to_string(),
        ..KeyStorageConfig::default()
    };
    let networks = vec![test_network(network_name)];

    let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

    let key_result = results
        .iter()
        .find(|r| r.name == format!("Key: {network_name}"))
        .expect("should have a result for the network");

    assert_eq!(
        key_result.status,
        CheckStatus::Fail,
        "invalid key should fail: {}",
        key_result.message
    );
    assert!(
        key_result.message.contains("Key found but invalid"),
        "should say key is invalid: {}",
        key_result.message
    );
    assert!(
        key_result.hint.as_ref().unwrap().contains("64-char hex"),
        "should include format hint: {:?}",
        key_result.hint
    );

    std::env::remove_var(env_var_for(network_name));
}

#[tokio::test]
async fn test_check_secrets_key_with_trailing_newline_passes() {
    ensure_plugins_registered();

    let network_name = "sectrimmed";
    // Simulate AWS Secrets Manager adding a trailing newline
    std::env::set_var(env_var_for(network_name), format!("{VALID_KEY}\n"));

    let config = KeyStorageConfig {
        storage_type: "env".to_string(),
        ..KeyStorageConfig::default()
    };
    let networks = vec![test_network(network_name)];

    let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

    let key_result = results
        .iter()
        .find(|r| r.name == format!("Key: {network_name}"))
        .expect("should have a result for the network");

    assert_eq!(
        key_result.status,
        CheckStatus::Pass,
        "key with trailing newline should pass after trimming: {}",
        key_result.message
    );

    std::env::remove_var(env_var_for(network_name));
}

#[tokio::test]
async fn test_check_secrets_missing_key_warns() {
    ensure_plugins_registered();

    // Don't set any env var for this network
    let network_name = "secmissing";
    // Ensure env var does not exist
    std::env::remove_var(env_var_for(network_name));

    let config = KeyStorageConfig {
        storage_type: "env".to_string(),
        ..KeyStorageConfig::default()
    };
    let networks = vec![test_network(network_name)];

    let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

    let key_result = results
        .iter()
        .find(|r| r.name == format!("Key: {network_name}"))
        .expect("should have a result for the network");

    assert_eq!(
        key_result.status,
        CheckStatus::Warn,
        "missing key should warn: {}",
        key_result.message
    );
    assert!(
        key_result.message.contains("No key found"),
        "should say no key found: {}",
        key_result.message
    );
}
