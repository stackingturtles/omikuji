use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};
use vaultrs::kv2;

use super::KeyStorage;

/// Cache entry for a secret
#[derive(Clone)]
struct CacheEntry {
    secret: SecretString,
    cached_at: Instant,
}

/// Vault-based key storage implementation
pub struct VaultStorage {
    client: VaultClient,
    mount_path: String,
    path_prefix: String,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    cache_ttl: Duration,
}

impl VaultStorage {
    /// Create a new Vault storage instance
    pub async fn new(
        url: &str,
        mount_path: &str,
        path_prefix: &str,
        auth_method: &str,
        token: Option<String>,
        cache_ttl_seconds: Option<u64>,
    ) -> Result<Self> {
        // Build Vault client settings
        let mut settings_builder = VaultClientSettingsBuilder::default();
        settings_builder.address(url);

        // Handle authentication
        match auth_method {
            "token" => {
                let token = token.ok_or_else(|| anyhow!("Token required for token auth method"))?;
                settings_builder.token(token);
            }
            _ => {
                return Err(anyhow!("Unsupported auth method: {}", auth_method));
            }
        }

        let settings = settings_builder
            .build()
            .context("Failed to build Vault client settings")?;

        let client = VaultClient::new(settings).context("Failed to create Vault client")?;

        // Test connection with a simple health check
        // Note: vaultrs doesn't have a direct token lookup method in the public API
        // We'll test the connection on first actual operation
        info!("Vault client created for {}", url);

        Ok(Self {
            client,
            mount_path: mount_path.to_string(),
            path_prefix: path_prefix.trim_end_matches('/').to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(cache_ttl_seconds.unwrap_or(300)), // Default 5 minutes
        })
    }

    /// Get the full path for a network's key
    fn get_secret_path(&self, network: &str) -> String {
        if self.path_prefix.is_empty() {
            network.to_string()
        } else {
            format!("{}/{}", self.path_prefix, network)
        }
    }

    /// Check if a cache entry is still valid
    fn is_cache_valid(&self, entry: &CacheEntry) -> bool {
        entry.cached_at.elapsed() < self.cache_ttl
    }

    /// Log audit event for key access
    fn audit_log(&self, operation: &str, network: &str, success: bool) {
        info!(
            target: "omikuji::audit",
            operation = operation,
            network = network,
            success = success,
            storage_type = "vault",
            timestamp = chrono::Utc::now().to_rfc3339(),
            "Key storage operation"
        );
    }
}

#[async_trait]
impl KeyStorage for VaultStorage {
    async fn get_key(&self, network: &str) -> Result<SecretString> {
        debug!("Retrieving key for network '{}' from Vault", network);

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(network) {
                if self.is_cache_valid(entry) {
                    debug!("Returning cached key for network '{}'", network);
                    self.audit_log("get_key_cached", network, true);
                    return Ok(entry.secret.clone());
                }
            }
        }

        // Fetch from Vault
        let secret_path = self.get_secret_path(network);

        match kv2::read::<HashMap<String, String>>(&self.client, &self.mount_path, &secret_path)
            .await
        {
            Ok(secret) => {
                // Extract the private key from the secret data
                let private_key = secret
                    .get("private_key")
                    .or_else(|| secret.get("key"))
                    .or_else(|| secret.get("value"))
                    .ok_or_else(|| {
                        anyhow!(
                            "No 'private_key', 'key', or 'value' field found in Vault secret for network '{}'",
                            network
                        )
                    })?;

                let secret_string = SecretString::from(private_key.clone());

                // Update cache
                {
                    let mut cache = self.cache.write().await;
                    cache.insert(
                        network.to_string(),
                        CacheEntry {
                            secret: secret_string.clone(),
                            cached_at: Instant::now(),
                        },
                    );
                }

                info!(
                    "Successfully retrieved key for network '{}' from Vault",
                    network
                );
                self.audit_log("get_key", network, true);
                Ok(secret_string)
            }
            Err(e) => {
                warn!(
                    "Failed to retrieve key from Vault for network '{}': {}",
                    network, e
                );

                // Check cache for fallback
                let cache = self.cache.read().await;
                if let Some(entry) = cache.get(network) {
                    warn!(
                        "Using cached key for network '{}' due to Vault error",
                        network
                    );
                    self.audit_log("get_key_fallback", network, true);
                    return Ok(entry.secret.clone());
                }

                self.audit_log("get_key", network, false);
                Err(anyhow!(
                    "Failed to retrieve key for network '{}': {}",
                    network,
                    e
                ))
            }
        }
    }

    async fn store_key(&self, network: &str, key: SecretString) -> Result<()> {
        debug!("Storing key for network '{}' in Vault", network);

        let secret_path = self.get_secret_path(network);
        let mut data = HashMap::new();
        data.insert("private_key".to_string(), key.expose_secret().to_string());

        // Add metadata
        data.insert("network".to_string(), network.to_string());
        data.insert("created_at".to_string(), chrono::Utc::now().to_rfc3339());
        data.insert("created_by".to_string(), "omikuji".to_string());

        match kv2::set(&self.client, &self.mount_path, &secret_path, &data).await {
            Ok(_) => {
                // Update cache
                {
                    let mut cache = self.cache.write().await;
                    cache.insert(
                        network.to_string(),
                        CacheEntry {
                            secret: key,
                            cached_at: Instant::now(),
                        },
                    );
                }

                info!("Successfully stored key for network '{}' in Vault", network);
                self.audit_log("store_key", network, true);
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to store key in Vault for network '{}': {}",
                    network, e
                );
                self.audit_log("store_key", network, false);
                Err(anyhow!(
                    "Failed to store key for network '{}': {}",
                    network,
                    e
                ))
            }
        }
    }

    async fn remove_key(&self, network: &str) -> Result<()> {
        debug!("Removing key for network '{}' from Vault", network);

        let secret_path = self.get_secret_path(network);

        // Note: We don't need to preserve metadata for delete operation

        match kv2::delete_latest(&self.client, &self.mount_path, &secret_path).await {
            Ok(_) => {
                // Remove from cache
                {
                    let mut cache = self.cache.write().await;
                    cache.remove(network);
                }

                info!(
                    "Successfully removed key for network '{}' from Vault",
                    network
                );
                self.audit_log("remove_key", network, true);
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to remove key from Vault for network '{}': {}",
                    network, e
                );
                self.audit_log("remove_key", network, false);
                Err(anyhow!(
                    "Failed to remove key for network '{}': {}",
                    network,
                    e
                ))
            }
        }
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        debug!("Listing keys from Vault");

        match kv2::list(&self.client, &self.mount_path, &self.path_prefix).await {
            Ok(keys) => {
                let networks: Vec<String> = keys
                    .into_iter()
                    .filter(|k| !k.ends_with('/')) // Filter out directories
                    .collect();

                info!("Found {} keys in Vault", networks.len());
                self.audit_log("list_keys", "", true);
                Ok(networks)
            }
            Err(e) => {
                error!("Failed to list keys from Vault: {}", e);
                self.audit_log("list_keys", "", false);
                Err(anyhow!("Failed to list keys: {}", e))
            }
        }
    }
}

// Clean up expired cache entries periodically
impl VaultStorage {
    pub async fn start_cache_cleanup(&self) {
        let cache = self.cache.clone();
        let cache_ttl = self.cache_ttl;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
            loop {
                interval.tick().await;
                let mut cache = cache.write().await;
                let now = Instant::now();
                cache.retain(|_, entry| now.duration_since(entry.cached_at) < cache_ttl);
            }
        });
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
