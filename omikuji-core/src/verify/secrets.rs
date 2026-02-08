use std::time::{Duration, Instant};

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

    // 3. Check if keys exist for configured networks
    let start = Instant::now();
    match tokio::time::timeout(timeout, provider.list_networks()).await {
        Ok(Ok(stored_networks)) => {
            for network in networks {
                if stored_networks.contains(&network.name) {
                    results.push(CheckResult {
                        category: CheckCategory::Secrets,
                        name: format!("Key: {}", network.name),
                        status: CheckStatus::Pass,
                        message: "Key found".to_string(),
                        hint: None,
                        duration: start.elapsed(),
                    });
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
                        duration: start.elapsed(),
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
