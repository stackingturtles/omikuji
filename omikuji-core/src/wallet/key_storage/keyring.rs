use anyhow::{anyhow, Result};
use async_trait::async_trait;
use keyring::Entry;
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, error, info, warn};

use super::KeyStorage;

const DEFAULT_SERVICE: &str = "omikuji";

#[derive(Debug, Clone)]
pub struct KeyringStorage {
    service: String,
}

impl KeyringStorage {
    pub fn new(service: Option<String>) -> Self {
        Self {
            service: service.unwrap_or_else(|| DEFAULT_SERVICE.to_string()),
        }
    }

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
}

#[async_trait]
impl KeyStorage for KeyringStorage {
    async fn get_key(&self, network: &str) -> Result<SecretString> {
        debug!("Retrieving key for network '{}'", network);

        // Check if we're in an SSH session (common cause of keyring issues)
        if std::env::var("SSH_CONNECTION").is_ok() {
            debug!("Running in SSH session - keyring may not be available");
        }

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
        Ok(SecretString::from(password))
    }

    async fn store_key(&self, network: &str, key: SecretString) -> Result<()> {
        debug!("Storing key for network '{}'", network);

        // Check for common environment issues
        if std::env::var("SSH_CONNECTION").is_ok()
            && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err()
        {
            warn!("SSH session without D-Bus detected - keyring may not persist data");
        }

        if std::path::Path::new("/.dockerenv").exists() {
            warn!("Running in Docker container - keyring may not be available");
        }

        let entry = self.get_entry(network)?;

        entry.set_password(key.expose_secret()).map_err(|e| {
            let error_string = e.to_string();
            if error_string.contains("D-Bus") || error_string.contains("dbus") {
                error!("Keyring unavailable: D-Bus session not found (common in SSH/container)");
            }
            anyhow!("Failed to store key for network '{}': {}", network, e)
        })?;

        info!("Successfully stored key for network '{}'", network);

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

    async fn remove_key(&self, network: &str) -> Result<()> {
        info!("Removing key for network '{}' from keyring", network);

        let entry = self.get_entry(network)?;
        entry
            .delete_credential()
            .map_err(|e| anyhow!("Failed to remove key for network '{}': {}", network, e))?;

        info!("Successfully removed key for network '{}'", network);
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        // Note: The keyring crate doesn't support listing all keys directly
        // This would need to be tracked separately or use platform-specific APIs
        Err(anyhow!(
            "Listing keys is not directly supported by the keyring crate. \
            Consider maintaining a separate list of networks in configuration."
        ))
    }
}
