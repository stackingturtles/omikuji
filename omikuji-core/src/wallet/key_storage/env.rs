use anyhow::{anyhow, Result};
use async_trait::async_trait;
use secrecy::SecretString;
use std::env;
use tracing::{debug, warn};

use super::KeyStorage;

#[derive(Debug, Clone)]
pub struct EnvVarStorage;

impl EnvVarStorage {
    pub fn new() -> Self {
        Self
    }

    fn get_env_var_name(network: &str) -> String {
        format!(
            "OMIKUJI_PRIVATE_KEY_{}",
            network.to_uppercase().replace('-', "_")
        )
    }
}

impl Default for EnvVarStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KeyStorage for EnvVarStorage {
    async fn get_key(&self, network: &str) -> Result<SecretString> {
        let env_var = Self::get_env_var_name(network);
        debug!(
            "Looking for private key in environment variable: {}",
            env_var
        );

        // First try the network-specific env var
        if let Ok(key) = env::var(&env_var) {
            warn!(
                "Using private key from environment variable '{}'. \
                Consider migrating to OS keyring for better security.",
                env_var
            );
            return Ok(SecretString::from(key));
        }

        // Fall back to the generic PRIVATE_KEY env var for backward compatibility
        if let Ok(key) = env::var("PRIVATE_KEY") {
            warn!(
                "Using private key from generic 'PRIVATE_KEY' environment variable. \
                This is deprecated. Please use '{}' or migrate to OS keyring.",
                env_var
            );
            return Ok(SecretString::from(key));
        }

        Err(anyhow!(
            "Private key not found. Looked for '{}' and 'PRIVATE_KEY' environment variables",
            env_var
        ))
    }

    async fn store_key(&self, _network: &str, _key: SecretString) -> Result<()> {
        Err(anyhow!(
            "Environment variable storage does not support storing keys. \
            Please set the environment variable manually or use keyring storage."
        ))
    }

    async fn remove_key(&self, _network: &str) -> Result<()> {
        Err(anyhow!(
            "Environment variable storage does not support removing keys. \
            Please unset the environment variable manually or use keyring storage."
        ))
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        let mut keys = Vec::new();

        // Check for network-specific keys
        for (key, _) in env::vars() {
            if key.starts_with("OMIKUJI_PRIVATE_KEY_") {
                let network = key
                    .strip_prefix("OMIKUJI_PRIVATE_KEY_")
                    .unwrap()
                    .to_lowercase()
                    .replace('_', "-");
                keys.push(network);
            }
        }

        // Check for generic key
        if env::var("PRIVATE_KEY").is_ok() && keys.is_empty() {
            keys.push("default".to_string());
        }

        Ok(keys)
    }
}
