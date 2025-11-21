//! Community edition plugin implementations
//!
//! This module contains the Community edition's plugin implementations.
//! These plugins provide basic functionality without external cloud dependencies:
//! - Environment variable secret provider
//! - OS keyring secret provider
//! - Single-node cluster provider (no-op)

pub mod env_secrets;
pub mod keyring_secrets;
pub mod single_node;

// Re-export providers
pub use env_secrets::EnvSecretProvider;
pub use keyring_secrets::KeyringSecretProvider;

// Re-export factories
use anyhow::Result;
use async_trait::async_trait;
use omikuji_core::plugin_registry::PluginRegistry;
use omikuji_core::traits::secrets::{
    SecretProvider, SecretProviderConfig, SecretProviderFactory, SecretProviderType,
};
use std::sync::Arc;

/// Factory for environment variable secret provider
pub struct EnvSecretProviderFactory;

#[async_trait]
impl SecretProviderFactory for EnvSecretProviderFactory {
    async fn create(&self, config: SecretProviderConfig) -> Result<Box<dyn SecretProvider>> {
        match config {
            SecretProviderConfig::Env { prefix } => Ok(Box::new(EnvSecretProvider::new(prefix))),
            _ => anyhow::bail!("Invalid configuration for EnvSecretProvider"),
        }
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::Env
    }
}

/// Factory for keyring secret provider
pub struct KeyringSecretProviderFactory;

#[async_trait]
impl SecretProviderFactory for KeyringSecretProviderFactory {
    async fn create(&self, config: SecretProviderConfig) -> Result<Box<dyn SecretProvider>> {
        match config {
            SecretProviderConfig::Keyring { service } => {
                Ok(Box::new(KeyringSecretProvider::new(Some(service))))
            }
            _ => anyhow::bail!("Invalid configuration for KeyringSecretProvider"),
        }
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::Keyring
    }
}

pub use single_node::SingleNodeFactory;

/// Register all community plugins with the provided registry
///
/// This function registers:
/// - 2 secret providers: env, keyring
/// - 1 cluster provider: single-node
pub fn register_community_plugins(registry: &PluginRegistry) -> Result<()> {
    // Register secret providers
    registry.register_secret_factory(Arc::new(EnvSecretProviderFactory))?;
    registry.register_secret_factory(Arc::new(KeyringSecretProviderFactory))?;

    // Register cluster provider (single-node)
    registry.register_cluster_factory(Arc::new(SingleNodeFactory::new()))?;

    tracing::info!("Registered 3 community plugins (2 secret providers + 1 cluster provider)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use omikuji_core::plugin_registry::PluginRegistry;

    #[tokio::test]
    async fn test_register_community_plugins() {
        // Create a fresh registry for testing
        let registry = PluginRegistry::new();
        let result = register_community_plugins(&registry);
        assert!(
            result.is_ok(),
            "Failed to register community plugins: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_env_factory() {
        let factory = EnvSecretProviderFactory;
        assert_eq!(factory.provider_type(), SecretProviderType::Env);

        let config = SecretProviderConfig::Env {
            prefix: Some("TEST".to_string()),
        };
        let provider = factory.create(config).await;
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_keyring_factory() {
        let factory = KeyringSecretProviderFactory;
        assert_eq!(factory.provider_type(), SecretProviderType::Keyring);

        let config = SecretProviderConfig::Keyring {
            service: "test-service".to_string(),
        };
        let provider = factory.create(config).await;
        assert!(provider.is_ok());
    }
}
