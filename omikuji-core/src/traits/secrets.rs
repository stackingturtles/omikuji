//! Secret management trait for plugin-based key storage
//!
//! This trait allows different implementations of secret storage,
//! from simple environment variables to cloud-based secret managers.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for secret providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SecretProviderConfig {
    /// Environment variable provider
    Env {
        /// Optional prefix for environment variables
        prefix: Option<String>,
    },
    /// File-based provider
    File {
        /// Path to the secrets file
        path: String,
    },
    /// OS Keyring provider
    Keyring {
        /// Service name in the keyring
        service: String,
    },
    /// AWS Secrets Manager (Pro Edition plugin)
    AwsSecrets {
        /// AWS region
        region: Option<String>,
        /// Secret name prefix
        prefix: Option<String>,
        /// Cache TTL in seconds
        cache_ttl_seconds: Option<u64>,
    },
    /// HashiCorp Vault (Community/Pro Edition plugin)
    Vault {
        /// Vault server URL
        url: String,
        /// Authentication method
        auth_method: String,
        /// Mount path
        mount_path: String,
    },
}

/// Information about a stored secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInfo {
    /// Network or identifier for this secret
    pub network: String,
    /// When the secret was last updated
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Core trait for secret management providers
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// Get a private key for a specific network
    ///
    /// # Arguments
    /// * `network` - The network identifier (e.g., "ethereum-mainnet", "base-sepolia")
    ///
    /// # Returns
    /// The private key as a string, or an error if not found
    async fn get_private_key(&self, network: &str) -> Result<String>;

    /// Store a private key for a specific network
    ///
    /// # Arguments
    /// * `network` - The network identifier
    /// * `private_key` - The private key to store
    ///
    /// # Returns
    /// Ok(()) on success, or an error
    async fn store_private_key(&self, network: &str, private_key: &str) -> Result<()>;

    /// Remove a private key for a specific network
    ///
    /// # Arguments
    /// * `network` - The network identifier
    ///
    /// # Returns
    /// Ok(()) on success (even if key didn't exist), or an error
    async fn remove_private_key(&self, network: &str) -> Result<()>;

    /// List all available networks with stored keys
    ///
    /// # Returns
    /// A vector of network identifiers
    async fn list_networks(&self) -> Result<Vec<String>>;

    /// Get information about a stored secret
    ///
    /// # Arguments
    /// * `network` - The network identifier
    ///
    /// # Returns
    /// Information about the secret, or None if not found
    async fn get_secret_info(&self, network: &str) -> Result<Option<SecretInfo>>;

    /// Check if the provider is available and properly configured
    ///
    /// # Returns
    /// Ok(()) if the provider is ready, or an error describing the issue
    async fn health_check(&self) -> Result<()>;

    /// Get the name of this provider
    ///
    /// # Returns
    /// A human-readable name for this provider (e.g., "env", "keyring", "aws-secrets")
    fn provider_name(&self) -> &str;

    /// Get the provider type
    ///
    /// # Returns
    /// The provider type identifier
    fn provider_type(&self) -> SecretProviderType;

    /// Check if this provider supports key rotation
    ///
    /// # Returns
    /// true if the provider can rotate keys automatically
    fn supports_rotation(&self) -> bool {
        false
    }

    /// Rotate a key for a specific network (if supported)
    ///
    /// # Arguments
    /// * `network` - The network identifier
    ///
    /// # Returns
    /// The new private key, or an error if rotation is not supported
    async fn rotate_key(&self, _network: &str) -> Result<String> {
        anyhow::bail!("Key rotation not supported by {}", self.provider_name())
    }
}

/// Types of secret providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretProviderType {
    /// Environment variables
    Env,
    /// File-based storage
    File,
    /// OS keyring
    Keyring,
    /// AWS Secrets Manager
    AwsSecrets,
    /// HashiCorp Vault
    Vault,
    /// Custom provider
    Custom,
}

/// Factory trait for creating secret providers
#[async_trait]
pub trait SecretProviderFactory: Send + Sync {
    /// Create a secret provider from configuration
    async fn create(&self, config: SecretProviderConfig) -> Result<Box<dyn SecretProvider>>;

    /// Get the provider type this factory creates
    fn provider_type(&self) -> SecretProviderType;
}
