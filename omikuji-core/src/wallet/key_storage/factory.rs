use anyhow::{anyhow, Context, Result};
use tracing::info;

use crate::config::models::OmikujiConfig;
use crate::wallet::key_storage::{EnvVarStorage, KeyStorage, KeyringStorage, VaultStorage};

/// Creates a key storage instance based on the configuration
pub async fn create_key_storage(config: &OmikujiConfig) -> Result<Box<dyn KeyStorage>> {
    match config.key_storage.storage_type.as_str() {
        "keyring" => {
            info!("Using OS keyring for key storage");
            Ok(Box::new(KeyringStorage::new(Some(
                config.key_storage.keyring.service.clone(),
            ))))
        }
        "env" => {
            info!("Using environment variables for key storage (consider migrating to vault/aws-secrets for production)");
            Ok(Box::new(EnvVarStorage::new()))
        }
        "vault" => {
            info!("Using HashiCorp Vault for key storage");
            let vault_config = &config.key_storage.vault;

            // Handle token from environment variable if specified
            let token = resolve_token_from_env(vault_config.token.as_ref());

            let vault_storage = VaultStorage::new(
                &vault_config.url,
                &vault_config.mount_path,
                &vault_config.path_prefix,
                &vault_config.auth_method,
                token,
                Some(vault_config.cache_ttl_seconds),
            )
            .await
            .context("Failed to initialize Vault storage")?;

            // Start cache cleanup task
            vault_storage.start_cache_cleanup().await;

            Ok(Box::new(vault_storage))
        }
        "aws-secrets" => Err(anyhow!(
            "AWS Secrets Manager storage is only available in Omikuji Pro Edition. \
                Please use 'vault', 'keyring', or 'env' for Community Edition."
        )),
        _ => Err(anyhow!(
            "Unknown key storage type: {}",
            config.key_storage.storage_type
        )),
    }
}

/// Resolves token from environment variable if specified
fn resolve_token_from_env(token: Option<&String>) -> Option<String> {
    token.and_then(|t| {
        if t.starts_with("${") && t.ends_with("}") {
            let var_name = &t[2..t.len() - 1];
            std::env::var(var_name).ok()
        } else {
            Some(t.clone())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{KeyStorageConfig, KeyringConfig, VaultConfig};

    fn create_test_config(storage_type: &str) -> OmikujiConfig {
        OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: KeyStorageConfig {
                storage_type: storage_type.to_string(),
                keyring: KeyringConfig::default(),
                vault: VaultConfig::default(),
                aws_secrets: Default::default(), // Will be ignored in Community Edition
            },
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_create_keyring_storage() {
        let config = create_test_config("keyring");
        let storage = create_key_storage(&config).await;
        assert!(storage.is_ok());
    }

    #[tokio::test]
    async fn test_create_env_storage() {
        let config = create_test_config("env");
        let storage = create_key_storage(&config).await;
        assert!(storage.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_storage_type() {
        let config = create_test_config("unknown");
        let storage = create_key_storage(&config).await;
        assert!(storage.is_err());
        let err = storage.err().unwrap();
        assert!(err.to_string().contains("Unknown key storage type"));
    }

    #[test]
    fn test_resolve_token_from_env() {
        // Test direct token
        let token = Some("direct-token".to_string());
        assert_eq!(
            resolve_token_from_env(token.as_ref()),
            Some("direct-token".to_string())
        );

        // Test environment variable syntax
        std::env::set_var("TEST_TOKEN", "env-token");
        let token = Some("${TEST_TOKEN}".to_string());
        assert_eq!(
            resolve_token_from_env(token.as_ref()),
            Some("env-token".to_string())
        );

        // Test missing environment variable
        let token = Some("${MISSING_TOKEN}".to_string());
        assert_eq!(resolve_token_from_env(token.as_ref()), None);

        // Test None
        assert_eq!(resolve_token_from_env(None), None);
    }
}
