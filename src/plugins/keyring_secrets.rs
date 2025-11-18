//! OS Keyring secret provider for community edition

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use keyring::Entry;
use omikuji_core::traits::secrets::{SecretInfo, SecretProvider, SecretProviderType};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

const DEFAULT_SERVICE: &str = "omikuji";

/// OS Keyring implementation of SecretProvider
pub struct KeyringSecretProvider {
    service: String,
}

impl KeyringSecretProvider {
    /// Create a new keyring provider
    pub fn new(service: Option<String>) -> Self {
        Self {
            service: service.unwrap_or_else(|| DEFAULT_SERVICE.to_string()),
        }
    }

    /// Get a keyring entry for a network
    fn get_entry(&self, network: &str) -> Result<Entry> {
        debug!(
            "Creating keyring entry for service: '{}', network: '{}'",
            self.service, network
        );

        Entry::new(&self.service, network).map_err(|e| {
            error!(
                "Failed to create keyring entry for network '{}': {}",
                network, e
            );
            anyhow!("Failed to create keyring entry: {}", e)
        })
    }

    /// Check for common environment issues that affect keyring
    fn check_environment(&self) {
        if std::env::var("SSH_CONNECTION").is_ok() {
            debug!("Running in SSH session - keyring may not be available");
            if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
                warn!("SSH session without D-Bus detected - keyring may not persist data");
            }
        }

        if std::path::Path::new("/.dockerenv").exists() {
            warn!("Running in Docker container - keyring may not be available");
        }
    }
}

#[async_trait]
impl SecretProvider for KeyringSecretProvider {
    async fn get_private_key(&self, network: &str) -> Result<String> {
        debug!("Retrieving key for network '{}' from keyring", network);

        self.check_environment();

        let entry = self.get_entry(network)?;
        let password = entry.get_password().map_err(|e| {
            let error_string = e.to_string();

            // Provide helpful context for common errors
            if error_string.contains("not found") || error_string.contains("No such secret") {
                debug!("Key not found in keyring for network '{}'", network);
            } else if error_string.contains("D-Bus") || error_string.contains("dbus") {
                warn!("Keyring unavailable: D-Bus session not found (common in SSH sessions)");
            }

            anyhow!("Failed to retrieve key for network '{}': {}", network, e)
        })?;

        debug!("Successfully retrieved key for network '{}'", network);
        Ok(password)
    }

    async fn store_private_key(&self, network: &str, private_key: &str) -> Result<()> {
        debug!("Storing key for network '{}' in keyring", network);

        self.check_environment();

        let entry = self.get_entry(network)?;

        entry.set_password(private_key).map_err(|e| {
            let error_string = e.to_string();
            if error_string.contains("D-Bus") || error_string.contains("dbus") {
                error!("Keyring unavailable: D-Bus session not found (common in SSH/container)");
            }
            anyhow!("Failed to store key for network '{}': {}", network, e)
        })?;

        info!(
            "Successfully stored key for network '{}' in keyring",
            network
        );

        // Verify the key was actually stored (helps detect non-persistent backends)
        match entry.get_password() {
            Ok(_) => debug!("Key verification successful"),
            Err(e) => {
                warn!(
                    "Key verification failed - keyring may be using non-persistent backend: {}",
                    e
                );
            }
        }

        Ok(())
    }

    async fn remove_private_key(&self, network: &str) -> Result<()> {
        debug!("Removing key for network '{}' from keyring", network);

        let entry = self.get_entry(network)?;
        entry.delete_credential().map_err(|e| {
            error!(
                "Failed to remove key from keyring for network '{}': {}",
                network, e
            );
            anyhow!("Failed to remove key: {}", e)
        })?;

        info!(
            "Successfully removed key for network '{}' from keyring",
            network
        );
        Ok(())
    }

    async fn list_networks(&self) -> Result<Vec<String>> {
        // The keyring crate doesn't provide a way to list all entries
        // We'd need to maintain a separate index or use platform-specific APIs
        warn!("Listing all networks is not supported by the keyring provider");
        warn!("You need to know the network names to access their keys");

        // Return empty list as we can't enumerate keyring entries portably
        Ok(Vec::new())
    }

    async fn get_secret_info(&self, network: &str) -> Result<Option<SecretInfo>> {
        // Try to get the entry to see if it exists
        let entry = self.get_entry(network)?;

        match entry.get_password() {
            Ok(_) => {
                let mut metadata = HashMap::new();
                metadata.insert("service".to_string(), self.service.clone());
                metadata.insert("storage".to_string(), "keyring".to_string());

                Ok(Some(SecretInfo {
                    network: network.to_string(),
                    last_updated: None, // Keyring doesn't track update time
                    metadata,
                }))
            }
            Err(_) => Ok(None),
        }
    }

    async fn health_check(&self) -> Result<()> {
        self.check_environment();

        // Try to access the keyring with a test entry
        let test_entry = Entry::new(&self.service, "__health_check__")
            .map_err(|e| anyhow!("Keyring unavailable: {}", e))?;

        // Try to delete any existing test entry (ignore errors)
        let _ = test_entry.delete_credential();

        // Try to set and get a value
        test_entry
            .set_password("test")
            .map_err(|e| anyhow!("Cannot write to keyring: {}", e))?;

        test_entry
            .get_password()
            .map_err(|e| anyhow!("Cannot read from keyring: {}", e))?;

        // Clean up
        let _ = test_entry.delete_credential();

        Ok(())
    }

    fn provider_name(&self) -> &str {
        "keyring"
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::Keyring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Keyring tests require system keyring access
    async fn test_store_and_retrieve() {
        let provider = KeyringSecretProvider::new(Some("omikuji_test".to_string()));
        let network = "test_network";
        let test_key = "test_private_key_123";

        // Store key
        provider.store_private_key(network, test_key).await.unwrap();

        // Retrieve key
        let retrieved = provider.get_private_key(network).await.unwrap();
        assert_eq!(retrieved, test_key);

        // Clean up
        provider.remove_private_key(network).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Keyring tests require system keyring access
    async fn test_remove_key() {
        let provider = KeyringSecretProvider::new(Some("omikuji_test".to_string()));
        let network = "test_remove";

        // Store a key
        provider
            .store_private_key(network, "temporary_key")
            .await
            .unwrap();

        // Remove it
        provider.remove_private_key(network).await.unwrap();

        // Verify it's gone
        let result = provider.get_private_key(network).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // Keyring tests require system keyring access
    async fn test_health_check() {
        let provider = KeyringSecretProvider::new(Some("omikuji_test".to_string()));

        // Health check should succeed if keyring is available
        let result = provider.health_check().await;
        // We don't assert success as it depends on the system
        if result.is_err() {
            println!("Keyring not available: {:?}", result);
        }
    }
}
