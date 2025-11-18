//! Environment variable secret provider for community edition

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use omikuji_core::traits::secrets::{SecretInfo, SecretProvider, SecretProviderType};
use std::collections::HashMap;
use std::env;
use tracing::{debug, info, warn};

/// Environment variable implementation of SecretProvider
pub struct EnvSecretProvider {
    prefix: String,
}

impl EnvSecretProvider {
    /// Create a new environment variable provider
    pub fn new(prefix: Option<String>) -> Self {
        Self {
            prefix: prefix.unwrap_or_else(|| "OMIKUJI_PRIVATE_KEY_".to_string()),
        }
    }

    /// Get the environment variable name for a network
    fn get_env_var_name(&self, network: &str) -> String {
        // Convert network name to uppercase and replace hyphens with underscores
        let network_upper = network.to_uppercase().replace('-', "_");
        format!("{}{}", self.prefix, network_upper)
    }

    /// Parse the network name from an environment variable name
    fn parse_network_name(&self, env_var: &str) -> Option<String> {
        env_var
            .strip_prefix(&self.prefix)
            .map(|n| n.to_lowercase().replace('_', "-"))
    }
}

#[async_trait]
impl SecretProvider for EnvSecretProvider {
    async fn get_private_key(&self, network: &str) -> Result<String> {
        let env_var_name = self.get_env_var_name(network);
        debug!(
            "Looking for private key in environment variable: {}",
            env_var_name
        );

        env::var(&env_var_name).map_err(|e| {
            warn!("Failed to get private key for network '{}': {}", network, e);
            anyhow!(
                "Private key not found for network '{}' in environment variable '{}'",
                network,
                env_var_name
            )
        })
    }

    async fn store_private_key(&self, network: &str, private_key: &str) -> Result<()> {
        let env_var_name = self.get_env_var_name(network);

        // We can't actually modify environment variables at runtime in a persistent way
        // This would need to be done outside the application
        warn!(
            "Cannot store private key in environment variable '{}' at runtime. \
            Please set it manually: export {}='{}'",
            env_var_name, env_var_name, private_key
        );

        Err(anyhow!(
            "Environment variables cannot be persistently modified at runtime. \
            Please set {} manually",
            env_var_name
        ))
    }

    async fn remove_private_key(&self, network: &str) -> Result<()> {
        let env_var_name = self.get_env_var_name(network);

        // We can't actually remove environment variables at runtime
        warn!(
            "Cannot remove environment variable '{}' at runtime. \
            Please unset it manually: unset {}",
            env_var_name, env_var_name
        );

        Err(anyhow!(
            "Environment variables cannot be removed at runtime. \
            Please unset {} manually",
            env_var_name
        ))
    }

    async fn list_networks(&self) -> Result<Vec<String>> {
        let mut networks = Vec::new();

        for (key, _) in env::vars() {
            if key.starts_with(&self.prefix) {
                if let Some(network) = self.parse_network_name(&key) {
                    networks.push(network);
                }
            }
        }

        info!(
            "Found {} networks with environment variable keys",
            networks.len()
        );
        Ok(networks)
    }

    async fn get_secret_info(&self, network: &str) -> Result<Option<SecretInfo>> {
        let env_var_name = self.get_env_var_name(network);

        if env::var(&env_var_name).is_ok() {
            let mut metadata = HashMap::new();
            metadata.insert("env_var".to_string(), env_var_name);
            metadata.insert("source".to_string(), "environment".to_string());

            Ok(Some(SecretInfo {
                network: network.to_string(),
                last_updated: None, // Environment variables don't track update time
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    async fn health_check(&self) -> Result<()> {
        // Environment variables are always available
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "env"
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::Env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_get_private_key() {
        let provider = EnvSecretProvider::new(Some("TEST_KEY_".to_string()));

        // Set a test environment variable
        env::set_var("TEST_KEY_MAINNET", "test_private_key_123");

        let key = provider.get_private_key("mainnet").await.unwrap();
        assert_eq!(key, "test_private_key_123");

        // Clean up
        env::remove_var("TEST_KEY_MAINNET");
    }

    #[tokio::test]
    async fn test_network_name_conversion() {
        let provider = EnvSecretProvider::new(Some("TEST_KEY_".to_string()));

        // Test hyphenated network names
        env::set_var("TEST_KEY_ETHEREUM_MAINNET", "key1");
        env::set_var("TEST_KEY_BASE_SEPOLIA", "key2");

        let key1 = provider.get_private_key("ethereum-mainnet").await.unwrap();
        assert_eq!(key1, "key1");

        let key2 = provider.get_private_key("base-sepolia").await.unwrap();
        assert_eq!(key2, "key2");

        // Clean up
        env::remove_var("TEST_KEY_ETHEREUM_MAINNET");
        env::remove_var("TEST_KEY_BASE_SEPOLIA");
    }

    #[tokio::test]
    async fn test_list_networks() {
        let provider = EnvSecretProvider::new(Some("TEST_LIST_".to_string()));

        // Set multiple test environment variables
        env::set_var("TEST_LIST_NETWORK1", "key1");
        env::set_var("TEST_LIST_NETWORK2", "key2");
        env::set_var("TEST_LIST_NETWORK3", "key3");
        env::set_var("OTHER_VAR", "not_included");

        let networks = provider.list_networks().await.unwrap();
        assert_eq!(networks.len(), 3);
        assert!(networks.contains(&"network1".to_string()));
        assert!(networks.contains(&"network2".to_string()));
        assert!(networks.contains(&"network3".to_string()));

        // Clean up
        env::remove_var("TEST_LIST_NETWORK1");
        env::remove_var("TEST_LIST_NETWORK2");
        env::remove_var("TEST_LIST_NETWORK3");
    }

    #[tokio::test]
    async fn test_missing_key() {
        let provider = EnvSecretProvider::new(Some("TEST_MISSING_".to_string()));

        let result = provider.get_private_key("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_store_not_supported() {
        let provider = EnvSecretProvider::new(None);

        let result = provider.store_private_key("test", "key").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be persistently modified"));
    }
}
