use std::time::{Duration, Instant};

use alloy::signers::local::PrivateKeySigner;

use crate::config::models::KeyStorageConfig;
use crate::config::models::Network;
use crate::plugin_registry::global_registry;
use crate::traits::secrets::{SecretProviderConfig, SecretProviderType};

use super::types::{CheckCategory, CheckResult, CheckStatus};

pub async fn check_secrets(
    config: &KeyStorageConfig,
    networks: &[Network],
    timeout: Duration,
) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let registry = global_registry();

    // 1. Map storage_type to SecretProviderType and check factory
    let start = Instant::now();
    let provider_type = match config.storage_type.as_str() {
        "env" => SecretProviderType::Env,
        "keyring" => SecretProviderType::Keyring,
        "vault" => SecretProviderType::Vault,
        "aws-secrets" => SecretProviderType::AwsSecrets,
        other => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Provider factory".to_string(),
                status: CheckStatus::Fail,
                message: format!("Unknown storage type: '{other}'"),
                hint: Some("Valid types: env, keyring, vault, aws-secrets".to_string()),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    if !registry.has_secret_factory(&provider_type) {
        results.push(CheckResult {
            category: CheckCategory::Secrets,
            name: "Provider factory".to_string(),
            status: CheckStatus::Fail,
            message: format!("Provider '{provider_type:?}' not available"),
            hint: Some(
                "This provider may require the Pro edition or a plugin registration".to_string(),
            ),
            duration: start.elapsed(),
        });
        return results;
    }

    results.push(CheckResult {
        category: CheckCategory::Secrets,
        name: "Provider factory".to_string(),
        status: CheckStatus::Pass,
        message: format!("Factory registered for '{}'", config.storage_type),
        hint: None,
        duration: start.elapsed(),
    });

    // 2. Create provider and run health_check
    let start = Instant::now();
    let provider_config = match config.storage_type.as_str() {
        "env" => SecretProviderConfig::Env {
            prefix: Some("OMIKUJI_PRIVATE_KEY".to_string()),
        },
        "keyring" => SecretProviderConfig::Keyring {
            service: config.keyring.service.clone(),
        },
        "vault" => SecretProviderConfig::Vault {
            url: config.vault.url.clone(),
            auth_method: config.vault.auth_method.clone(),
            mount_path: config.vault.mount_path.clone(),
        },
        "aws-secrets" => SecretProviderConfig::AwsSecrets {
            region: config.aws_secrets.region.clone(),
            prefix: Some(config.aws_secrets.prefix.clone()),
            cache_ttl_seconds: Some(config.aws_secrets.cache_ttl_seconds),
        },
        _ => unreachable!(),
    };

    let provider = match registry.create_secret_provider(provider_config).await {
        Ok(p) => p,
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Provider creation".to_string(),
                status: CheckStatus::Fail,
                message: format!("Failed to create provider: {e}"),
                hint: Some("Check your key_storage configuration".to_string()),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    let start = Instant::now();
    match tokio::time::timeout(timeout, provider.health_check()).await {
        Ok(Ok(())) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Provider health".to_string(),
                status: CheckStatus::Pass,
                message: format!("{} provider is healthy", provider.provider_name()),
                hint: None,
                duration: start.elapsed(),
            });
        }
        Ok(Err(e)) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Provider health".to_string(),
                status: CheckStatus::Fail,
                message: format!("Health check failed: {e}"),
                hint: Some("Check provider credentials and connectivity".to_string()),
                duration: start.elapsed(),
            });
            return results;
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Provider health".to_string(),
                status: CheckStatus::Fail,
                message: format!("Health check timed out after {} s", timeout.as_secs()),
                hint: None,
                duration: start.elapsed(),
            });
            return results;
        }
    }

    // 3. Check if keys exist for configured networks and validate they are parseable
    let start = Instant::now();
    match tokio::time::timeout(timeout, provider.list_networks()).await {
        Ok(Ok(stored_networks)) => {
            for network in networks {
                let start_key = Instant::now();
                if stored_networks.contains(&network.name) {
                    // Key exists — now verify it's actually parseable as a signer
                    match tokio::time::timeout(timeout, provider.get_private_key(&network.name))
                        .await
                    {
                        Ok(Ok(key)) => {
                            let trimmed = key.trim();
                            match trimmed.parse::<PrivateKeySigner>() {
                                Ok(signer) => {
                                    results.push(CheckResult {
                                        category: CheckCategory::Secrets,
                                        name: format!("Key: {}", network.name),
                                        status: CheckStatus::Pass,
                                        message: format!(
                                            "Key valid (wallet: {:?})",
                                            signer.address()
                                        ),
                                        hint: None,
                                        duration: start_key.elapsed(),
                                    });
                                }
                                Err(e) => {
                                    results.push(CheckResult {
                                        category: CheckCategory::Secrets,
                                        name: format!("Key: {}", network.name),
                                        status: CheckStatus::Fail,
                                        message: format!(
                                            "Key found but invalid: {} (length: {}, starts_with 0x: {})",
                                            e,
                                            trimmed.len(),
                                            trimmed.starts_with("0x")
                                        ),
                                        hint: Some(
                                            "Expected a 64-char hex string optionally prefixed with 0x"
                                                .to_string(),
                                        ),
                                        duration: start_key.elapsed(),
                                    });
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            results.push(CheckResult {
                                category: CheckCategory::Secrets,
                                name: format!("Key: {}", network.name),
                                status: CheckStatus::Fail,
                                message: format!("Key exists but could not be read: {e}"),
                                hint: None,
                                duration: start_key.elapsed(),
                            });
                        }
                        Err(_) => {
                            results.push(CheckResult {
                                category: CheckCategory::Secrets,
                                name: format!("Key: {}", network.name),
                                status: CheckStatus::Warn,
                                message: format!(
                                    "Key read timed out after {} s",
                                    timeout.as_secs()
                                ),
                                hint: None,
                                duration: start_key.elapsed(),
                            });
                        }
                    }
                } else {
                    results.push(CheckResult {
                        category: CheckCategory::Secrets,
                        name: format!("Key: {}", network.name),
                        status: CheckStatus::Warn,
                        message: "No key found for this network".to_string(),
                        hint: Some(format!(
                            "Import with `omikuji key import -n {}`",
                            network.name
                        )),
                        duration: start_key.elapsed(),
                    });
                }
            }
        }
        Ok(Err(e)) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Key listing".to_string(),
                status: CheckStatus::Warn,
                message: format!("Could not list keys: {e}"),
                hint: Some("Provider may not support listing — key checks skipped".to_string()),
                duration: start.elapsed(),
            });
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Secrets,
                name: "Key listing".to_string(),
                status: CheckStatus::Warn,
                message: format!("list_networks timed out after {} s", timeout.as_secs()),
                hint: None,
                duration: start.elapsed(),
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::KeyStorageConfig;
    use std::time::Duration;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn test_check_secrets_unknown_storage_type() {
        let config = KeyStorageConfig {
            storage_type: "unknown-provider".to_string(),
            ..KeyStorageConfig::default()
        };
        let networks = vec![];

        let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert_eq!(results[0].category, CheckCategory::Secrets);
        assert_eq!(results[0].name, "Provider factory");
        assert!(results[0].message.contains("Unknown storage type"));
        assert!(results[0].message.contains("unknown-provider"));
        assert!(results[0].hint.is_some());
        assert!(results[0].hint.as_ref().unwrap().contains("Valid types"));
    }

    #[tokio::test]
    async fn test_check_secrets_vault_no_factory_registered() {
        // "vault" is a valid storage type, but the global registry may not have
        // a factory registered for it in the test environment. This tests the
        // "Provider not available" path.
        let config = KeyStorageConfig {
            storage_type: "vault".to_string(),
            ..KeyStorageConfig::default()
        };
        let networks = vec![];

        let results = check_secrets(&config, &networks, TEST_TIMEOUT).await;

        // We expect at least one result. If no factory is registered, we get a Fail
        // for "Provider factory". If a factory IS registered (e.g., community vault),
        // we may get further results. Either way, the first result should be about
        // the provider factory.
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Provider factory");
        assert_eq!(results[0].category, CheckCategory::Secrets);
    }
}
