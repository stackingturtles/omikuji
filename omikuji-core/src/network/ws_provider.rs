//! WebSocket provider management for real-time event monitoring

use std::sync::Arc;
use std::time::Duration;

use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider, WsConnect},
    pubsub::PubSubFrontend,
};
use anyhow::{Context, Result};
use backon::{ExponentialBuilder, Retryable};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

/// Type alias for WebSocket provider
pub type WsProvider = RootProvider<PubSubFrontend>;

/// Manages WebSocket connections with automatic reconnection
pub struct WsProviderManager {
    url: String,
    provider: Arc<RwLock<Option<Arc<WsProvider>>>>,
}

impl WsProviderManager {
    /// Create a new WebSocket provider manager
    pub fn new(ws_url: String) -> Self {
        Self {
            url: ws_url,
            provider: Arc::new(RwLock::new(None)),
        }
    }

    /// Connect to the WebSocket endpoint
    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to WebSocket endpoint: {}", self.url);

        let provider = self.create_provider().await?;

        let mut guard = self.provider.write().await;
        *guard = Some(Arc::new(provider));

        info!("Successfully connected to WebSocket endpoint");
        Ok(())
    }

    /// Get the current provider, connecting if necessary
    pub async fn get_provider(&self) -> Result<Arc<WsProvider>> {
        let guard = self.provider.read().await;

        if let Some(provider) = guard.as_ref() {
            // Check if the connection is still alive
            match provider.get_chain_id().await {
                Ok(_) => return Ok(provider.clone()),
                Err(e) => {
                    warn!("WebSocket connection appears to be dead: {}", e);
                    drop(guard); // Release read lock before reconnecting
                }
            }
        } else {
            drop(guard); // Release read lock before connecting
        }

        // Need to connect or reconnect
        self.reconnect().await?;

        // Now get the provider again
        let guard = self.provider.read().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Failed to establish WebSocket connection"))
    }

    /// Reconnect with exponential backoff
    async fn reconnect(&self) -> Result<()> {
        warn!("Attempting to reconnect to WebSocket endpoint");

        // Configure exponential backoff with max delay of 60 seconds
        // Retry indefinitely (no max_times limit)
        let backoff = ExponentialBuilder::default()
            .with_max_delay(Duration::from_secs(60))
            .with_max_times(usize::MAX);

        // Use backon's retry mechanism
        let connect_fn = || async {
            match self.connect().await {
                Ok(()) => {
                    info!("Successfully reconnected to WebSocket endpoint");
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to connect to WebSocket: {}", e);
                    warn!("Will retry connection with exponential backoff");
                    Err(e)
                }
            }
        };

        connect_fn.retry(backoff).await
    }

    /// Create a new WebSocket provider
    async fn create_provider(&self) -> Result<WsProvider> {
        let url = Url::parse(&self.url)
            .with_context(|| format!("Invalid WebSocket URL: {}", self.url))?;

        debug!("Creating WebSocket connection to: {}", url);

        let ws_connect = WsConnect::new(url);
        let provider = ProviderBuilder::new()
            .on_ws(ws_connect)
            .await
            .with_context(|| "Failed to create WebSocket provider")?;

        Ok(provider)
    }

    /// Disconnect and clean up
    pub async fn disconnect(&self) {
        info!("Disconnecting WebSocket provider");
        let mut guard = self.provider.write().await;
        *guard = None;
    }
}

/// WebSocket connection pool for managing multiple connections
pub struct WsConnectionPool {
    connections: Arc<RwLock<std::collections::HashMap<String, Arc<WsProviderManager>>>>,
}

impl Default for WsConnectionPool {
    fn default() -> Self {
        Self {
            connections: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

impl WsConnectionPool {
    /// Create a new connection pool
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a WebSocket provider for a given URL
    pub async fn get_provider(&self, network_name: &str, ws_url: &str) -> Result<Arc<WsProvider>> {
        let key = format!("{network_name}-{ws_url}");

        // Check if we already have a manager for this URL
        {
            let guard = self.connections.read().await;
            if let Some(manager) = guard.get(&key) {
                return manager.get_provider().await;
            }
        }

        // Create a new manager
        let manager = Arc::new(WsProviderManager::new(ws_url.to_string()));
        manager.connect().await?;

        // Store it in the pool
        {
            let mut guard = self.connections.write().await;
            guard.insert(key.clone(), manager.clone());
        }

        manager.get_provider().await
    }

    /// Disconnect all connections
    pub async fn disconnect_all(&self) {
        let guard = self.connections.read().await;
        for (_, manager) in guard.iter() {
            manager.disconnect().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use alloy::providers::Provider;

    fn spawn_anvil_ws() -> (alloy::node_bindings::AnvilInstance, String) {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let ws_url = anvil.ws_endpoint();
        (anvil, ws_url)
    }

    #[tokio::test]
    async fn test_ws_provider_manager_creation() {
        let manager = WsProviderManager::new("ws://localhost:8545".to_string());
        assert_eq!(manager.url, "ws://localhost:8545");
    }

    #[tokio::test]
    async fn test_connection_pool_creation() {
        let pool = WsConnectionPool::new();
        let guard = pool.connections.read().await;
        assert_eq!(guard.len(), 0);
    }

    #[tokio::test]
    async fn test_ws_connect_with_anvil() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let manager = WsProviderManager::new(ws_url);
        let result = manager.connect().await;
        assert!(result.is_ok(), "connect() should succeed with Anvil WS");
    }

    #[tokio::test]
    async fn test_ws_get_provider_returns_working_provider() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let manager = WsProviderManager::new(ws_url);
        manager.connect().await.unwrap();

        let provider = manager.get_provider().await.unwrap();
        let chain_id = provider.get_chain_id().await.unwrap();
        assert_eq!(chain_id, 31337, "Anvil default chain ID should be 31337");
    }

    #[tokio::test]
    async fn test_ws_get_provider_reconnects_after_disconnect() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let manager = WsProviderManager::new(ws_url);
        manager.connect().await.unwrap();

        // Disconnect
        manager.disconnect().await;

        // get_provider should reconnect automatically
        let provider = manager.get_provider().await.unwrap();
        let chain_id = provider.get_chain_id().await.unwrap();
        assert_eq!(chain_id, 31337);
    }

    #[tokio::test]
    async fn test_ws_disconnect_clears_provider() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let manager = WsProviderManager::new(ws_url);
        manager.connect().await.unwrap();

        // Verify provider exists
        {
            let guard = manager.provider.read().await;
            assert!(guard.is_some(), "Provider should exist after connect");
        }

        // Disconnect
        manager.disconnect().await;

        // Verify provider is cleared
        {
            let guard = manager.provider.read().await;
            assert!(guard.is_none(), "Provider should be None after disconnect");
        }
    }

    #[tokio::test]
    async fn test_pool_get_provider_with_anvil() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let pool = WsConnectionPool::new();

        let provider = pool.get_provider("anvil", &ws_url).await.unwrap();
        let chain_id = provider.get_chain_id().await.unwrap();
        assert_eq!(chain_id, 31337);

        // Verify the connection is stored in the pool
        let guard = pool.connections.read().await;
        assert_eq!(guard.len(), 1);
    }

    #[tokio::test]
    async fn test_pool_disconnect_all() {
        let (_anvil, ws_url) = spawn_anvil_ws();
        let pool = WsConnectionPool::new();

        pool.get_provider("anvil", &ws_url).await.unwrap();

        // Disconnect all
        pool.disconnect_all().await;

        // The pool still has the manager entry, but the provider inside should be None
        let guard = pool.connections.read().await;
        let manager = guard.values().next().unwrap();
        let provider_guard = manager.provider.read().await;
        assert!(
            provider_guard.is_none(),
            "Provider should be None after disconnect_all"
        );
    }
}
