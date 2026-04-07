use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use alloy::{
    primitives::Address,
    providers::{
        fillers::FillProvider, utils::JoinedRecommendedFillers, Provider, ProviderBuilder,
        RootProvider,
    },
    signers::local::PrivateKeySigner,
};
use anyhow::{Context, Result};
use secrecy::ExposeSecret;
use thiserror::Error;
use tracing::{error, info};
use url::Url;

use crate::config::models::Network;
use crate::metrics::NetworkMetrics;
use crate::network::ws_provider::{WsConnectionPool, WsProvider};
use crate::wallet::key_storage::KeyStorage;

/// Errors that can occur when interacting with network providers
#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Network not found: {0}")]
    NetworkNotFound(String),

    #[error("Provider error: {0}")]
    #[allow(dead_code)]
    ProviderError(String),

    #[error("RPC connection failed: {0}")]
    ConnectionFailed(String),
}

/// Type alias for the alloy provider we will use
pub type EthProvider = FillProvider<JoinedRecommendedFillers, RootProvider>;

/// Manages the connections to different EVM networks
pub struct NetworkManager {
    /// Map of network name to provider
    providers: HashMap<String, Arc<EthProvider>>,

    /// Private keys for each network (stored securely)
    private_keys: HashMap<String, String>,

    /// RPC URLs for each network (needed for creating signed providers)
    rpc_urls: HashMap<String, String>,

    /// Wallet addresses for each network
    wallet_addresses: HashMap<String, Address>,

    /// Network configurations
    networks: HashMap<String, Network>,

    /// WebSocket connection pool
    ws_pool: WsConnectionPool,
}

impl NetworkManager {
    /// Create a new network manager from a list of network configurations
    pub async fn new(networks: &[Network]) -> Result<Self> {
        let mut providers = HashMap::new();
        let private_keys = HashMap::new();
        let mut rpc_urls = HashMap::new();
        let wallet_addresses = HashMap::new();
        let mut network_configs = HashMap::new();

        for network in networks {
            // Use the first node's RPC URL for now
            let first_node = network.nodes.first().ok_or_else(|| {
                anyhow::anyhow!("Network {} has no nodes configured", network.name)
            })?;

            let provider = Self::create_provider(&first_node.rpc_url)
                .await
                .with_context(|| {
                    format!("Failed to create provider for network {}", network.name)
                })?;

            providers.insert(network.name.clone(), Arc::new(provider));
            rpc_urls.insert(network.name.clone(), first_node.rpc_url.clone());
            network_configs.insert(network.name.clone(), network.clone());
        }

        Ok(Self {
            providers,
            private_keys,
            rpc_urls,
            wallet_addresses,
            networks: network_configs,
            ws_pool: WsConnectionPool::new(),
        })
    }

    /// Load a wallet from an environment variable
    pub async fn load_wallet_from_env(&mut self, network_name: &str, env_var: &str) -> Result<()> {
        info!(
            "Attempting to load wallet for network {} from env var {}",
            network_name, env_var
        );

        // Check if the network exists
        if !self.providers.contains_key(network_name) {
            return Err(NetworkError::NetworkNotFound(network_name.to_string()).into());
        }

        let private_key = std::env::var(env_var)
            .with_context(|| format!("Environment variable {env_var} not found"))?;

        info!(
            "Successfully read private key from env var {} (length: {})",
            env_var,
            private_key.len()
        );

        let trimmed = private_key.trim();
        let signer = trimmed.parse::<PrivateKeySigner>().with_context(|| {
            format!(
                "Failed to parse private key for network {network_name} as signer \
                     (key length: {}, starts_with 0x: {}). \
                     Expected a 64-char hex string optionally prefixed with 0x.",
                trimmed.len(),
                trimmed.starts_with("0x")
            )
        })?;

        // Store the wallet address
        let wallet_address = signer.address();
        self.wallet_addresses
            .insert(network_name.to_string(), wallet_address);

        // Store the private key (we'll create providers with wallets on demand)
        self.private_keys
            .insert(network_name.to_string(), trimmed.to_string());

        info!(
            "Successfully loaded wallet for network {} with address {}",
            network_name, wallet_address
        );

        Ok(())
    }

    /// Load wallet from key storage
    pub async fn load_wallet_from_key_storage(
        &mut self,
        network_name: &str,
        key_storage: &dyn KeyStorage,
    ) -> Result<()> {
        // Check if the network exists
        if !self.providers.contains_key(network_name) {
            return Err(NetworkError::NetworkNotFound(network_name.to_string()).into());
        }

        let private_key_secret = key_storage
            .get_key(network_name)
            .await
            .with_context(|| format!("Failed to retrieve key for network {network_name}"))?;

        let private_key = private_key_secret.expose_secret();
        let trimmed = private_key.trim();

        let signer = trimmed.parse::<PrivateKeySigner>().with_context(|| {
            format!(
                "Failed to parse private key for network {network_name} as signer \
                     (key length: {}, starts_with 0x: {}). \
                     Expected a 64-char hex string optionally prefixed with 0x.",
                trimmed.len(),
                trimmed.starts_with("0x")
            )
        })?;

        // Store the wallet address
        let wallet_address = signer.address();
        self.wallet_addresses
            .insert(network_name.to_string(), wallet_address);

        // Store the private key (we'll create providers with wallets on demand)
        self.private_keys
            .insert(network_name.to_string(), trimmed.to_string());

        info!(
            "Successfully loaded wallet for network {} with address {} from key storage",
            network_name, wallet_address
        );

        Ok(())
    }

    /// Get the chain ID for a given network
    pub async fn get_chain_id(&self, network_name: &str) -> Result<u64> {
        let start = Instant::now();
        let provider = self.get_provider(network_name)?;

        match provider.get_chain_id().await {
            Ok(chain_id) => {
                let duration = start.elapsed();
                NetworkMetrics::record_rpc_request(
                    network_name,
                    "eth_chainId",
                    true,
                    duration,
                    None,
                );
                Ok(chain_id)
            }
            Err(e) => {
                let duration = start.elapsed();
                let error_type = NetworkMetrics::classify_rpc_error(&e.to_string());
                NetworkMetrics::record_rpc_request(
                    network_name,
                    "eth_chainId",
                    false,
                    duration,
                    Some(error_type),
                );
                Err(e).with_context(|| format!("Failed to get chain ID for network {network_name}"))
            }
        }
    }

    /// Get the block number for a given network
    pub async fn get_block_number(&self, network_name: &str) -> Result<u64> {
        let start = Instant::now();
        let provider = self.get_provider(network_name)?;

        match provider.get_block_number().await {
            Ok(block_number) => {
                let duration = start.elapsed();
                NetworkMetrics::record_rpc_request(
                    network_name,
                    "eth_blockNumber",
                    true,
                    duration,
                    None,
                );

                // Update chain head metric
                NetworkMetrics::update_chain_head(network_name, block_number);

                Ok(block_number)
            }
            Err(e) => {
                let duration = start.elapsed();
                let error_type = NetworkMetrics::classify_rpc_error(&e.to_string());
                NetworkMetrics::record_rpc_request(
                    network_name,
                    "eth_blockNumber",
                    false,
                    duration,
                    Some(error_type),
                );
                Err(e).with_context(|| {
                    format!("Failed to get block number for network {network_name}")
                })
            }
        }
    }

    /// Get a provider for a given network
    pub fn get_provider(&self, network_name: &str) -> Result<Arc<EthProvider>> {
        self.providers
            .get(network_name)
            .cloned()
            .ok_or_else(|| NetworkError::NetworkNotFound(network_name.to_string()).into())
    }

    /// Get the private key for a network
    pub fn get_private_key(&self, network_name: &str) -> Result<String> {
        self.private_keys.get(network_name).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "No private key found for network {network_name}. Call load_wallet_from_env first"
            )
        })
    }

    /// Get the RPC URL for a network
    pub fn get_rpc_url(&self, network_name: &str) -> Result<&str> {
        self.rpc_urls
            .get(network_name)
            .map(|s| s.as_str())
            .ok_or_else(|| NetworkError::NetworkNotFound(network_name.to_string()).into())
    }

    /// Get a signer for a given network
    #[allow(dead_code)]
    pub fn get_signer(&self, network_name: &str) -> Result<Arc<EthProvider>> {
        // For backward compatibility, check if we have a private key
        if self.private_keys.contains_key(network_name) {
            // Return the regular provider - the actual signing will be handled differently
            self.get_provider(network_name)
        } else {
            Err(anyhow::anyhow!(
                "No signer found for network {network_name}. Call load_wallet_from_env first"
            ))
        }
    }

    /// Get the wallet address for a given network
    pub fn get_wallet_address(&self, network_name: &str) -> Result<Address> {
        self.wallet_addresses
            .get(network_name)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No wallet address found for network {network_name}. Call load_wallet_from_env first"
                )
            })
    }

    /// Get a WebSocket provider for a given network
    pub async fn get_ws_provider(&self, network_name: &str) -> Result<Arc<WsProvider>> {
        let network = self
            .networks
            .get(network_name)
            .ok_or_else(|| NetworkError::NetworkNotFound(network_name.to_string()))?;

        // Use the first node that has a WebSocket URL
        let node_with_ws = network
            .nodes
            .iter()
            .find(|node| node.ws_url.is_some())
            .ok_or_else(|| {
                anyhow::anyhow!("Network {network_name} has no WebSocket URLs configured")
            })?;

        let ws_url = node_with_ws.ws_url.as_ref().unwrap();

        self.ws_pool.get_provider(network_name, ws_url).await
    }

    /// Check if a network has WebSocket support
    pub fn has_ws_support(&self, network_name: &str) -> bool {
        self.networks
            .get(network_name)
            .map(|network| network.nodes.iter().any(|node| node.ws_url.is_some()))
            .unwrap_or(false)
    }

    /// Get network configuration
    pub async fn get_network(&self, network_name: &str) -> Result<&Network> {
        self.networks
            .get(network_name)
            .ok_or_else(|| NetworkError::NetworkNotFound(network_name.to_string()).into())
    }

    /// Get all network names
    pub fn get_network_names(&self) -> Vec<String> {
        self.networks.keys().cloned().collect()
    }

    /// Get network names that have loaded wallets
    pub fn get_networks_with_wallets(&self) -> Vec<String> {
        self.wallet_addresses.keys().cloned().collect()
    }

    /// Create a provider from an RPC URL
    /// Check if a wallet has been loaded for a specific network
    #[cfg(test)]
    pub fn has_wallet(&self, network_name: &str) -> bool {
        self.wallet_addresses.contains_key(network_name)
    }

    /// Get stored private key for a network (for test assertions)
    #[cfg(test)]
    pub fn get_stored_private_key(&self, network_name: &str) -> Option<&str> {
        self.private_keys.get(network_name).map(|s| s.as_str())
    }

    /// Create a provider from an RPC URL
    async fn create_provider(rpc_url: &str) -> Result<EthProvider> {
        let url =
            Url::parse(rpc_url).with_context(|| format!("Failed to parse RPC URL: {rpc_url}"))?;

        let provider = ProviderBuilder::new().connect_http(url);

        // Test connection by getting the current block number
        let start = Instant::now();
        match provider.get_block_number().await {
            Ok(block_number) => {
                let _duration = start.elapsed();

                // Update endpoint health metric
                NetworkMetrics::update_endpoint_health("unknown", rpc_url, true);

                info!(
                    "Connected to RPC at {}, current block: {}",
                    rpc_url, block_number
                );
                Ok(provider)
            }
            Err(err) => {
                let _duration = start.elapsed();

                // Update endpoint health metric
                NetworkMetrics::update_endpoint_health("unknown", rpc_url, false);

                error!("Failed to connect to RPC at {}: {}", rpc_url, err);
                Err(NetworkError::ConnectionFailed(err.to_string()).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::key_storage::EnvVarStorage;
    use alloy::node_bindings::Anvil;

    /// First default Anvil account private key (without 0x prefix).
    const ANVIL_KEY_NO_PREFIX: &str =
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    /// First default Anvil account private key (with 0x prefix).
    const ANVIL_KEY_WITH_PREFIX: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    fn anvil_network(rpc_url: String) -> Network {
        crate::config::models::Network {
            name: "anvil".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Anvil".to_string(),
                rpc_url,
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }
    }

    // ── trim tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_load_wallet_from_key_storage_trims_whitespace() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let network = anvil_network(anvil.endpoint());
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        // Set env var with trailing newline (common AWS Secrets Manager artifact)
        let env_var = "OMIKUJI_PRIVATE_KEY_ANVIL";
        std::env::set_var(env_var, format!("{ANVIL_KEY_WITH_PREFIX}\n"));
        let storage = EnvVarStorage::new();

        nm.load_wallet_from_key_storage("anvil", &storage)
            .await
            .expect("should succeed after trimming");

        assert!(nm.has_wallet("anvil"));
        // Stored key should be trimmed
        assert_eq!(
            nm.get_stored_private_key("anvil").unwrap(),
            ANVIL_KEY_WITH_PREFIX
        );

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn test_load_wallet_from_key_storage_trims_spaces() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        // Use a unique network name to avoid env var collisions with parallel tests
        let mut network = anvil_network(anvil.endpoint());
        network.name = "trim-spaces".to_string();
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "OMIKUJI_PRIVATE_KEY_TRIM_SPACES";
        std::env::set_var(env_var, format!("  {ANVIL_KEY_WITH_PREFIX}  "));
        let storage = EnvVarStorage::new();

        nm.load_wallet_from_key_storage("trim-spaces", &storage)
            .await
            .expect("should succeed after trimming leading/trailing spaces");

        assert!(nm.has_wallet("trim-spaces"));
        assert_eq!(
            nm.get_stored_private_key("trim-spaces").unwrap(),
            ANVIL_KEY_WITH_PREFIX
        );

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn test_load_wallet_from_env_trims_whitespace() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let network = anvil_network(anvil.endpoint());
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "TEST_TRIM_KEY_ANVIL";
        std::env::set_var(env_var, format!("{ANVIL_KEY_WITH_PREFIX}\n"));

        nm.load_wallet_from_env("anvil", env_var)
            .await
            .expect("should succeed after trimming");

        assert!(nm.has_wallet("anvil"));
        assert_eq!(
            nm.get_stored_private_key("anvil").unwrap(),
            ANVIL_KEY_WITH_PREFIX
        );

        std::env::remove_var(env_var);
    }

    // ── error context tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_load_wallet_invalid_key_error_includes_network_name() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let mut network = anvil_network(anvil.endpoint());
        network.name = "badkey-net".to_string();
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "OMIKUJI_PRIVATE_KEY_BADKEY_NET";
        std::env::set_var(env_var, "not-a-valid-hex-key");
        let storage = EnvVarStorage::new();

        let err = nm
            .load_wallet_from_key_storage("badkey-net", &storage)
            .await
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("badkey-net"),
            "error should mention network name: {msg}"
        );
        assert!(
            msg.contains("key length:"),
            "error should include key length: {msg}"
        );
        assert!(
            msg.contains("starts_with 0x:"),
            "error should mention 0x prefix status: {msg}"
        );

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn test_load_wallet_from_env_invalid_key_error_includes_network_name() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let network = anvil_network(anvil.endpoint());
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "TEST_INVALID_KEY_ANVIL";
        std::env::set_var(env_var, "garbage");

        let err = nm.load_wallet_from_env("anvil", env_var).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("anvil"),
            "error should mention network name: {msg}"
        );
        assert!(
            msg.contains("key length: 7"),
            "error should include key length: {msg}"
        );

        std::env::remove_var(env_var);
    }

    // ── get_networks_with_wallets tests ─────────────────────────────

    #[tokio::test]
    async fn test_get_networks_with_wallets_empty_initially() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let network = anvil_network(anvil.endpoint());
        let nm = NetworkManager::new(&[network]).await.unwrap();

        // Network exists but no wallet loaded
        assert_eq!(nm.get_network_names().len(), 1);
        assert!(nm.get_networks_with_wallets().is_empty());
    }

    #[tokio::test]
    async fn test_get_networks_with_wallets_after_load() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let mut network = anvil_network(anvil.endpoint());
        network.name = "wallets-after".to_string();
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "OMIKUJI_PRIVATE_KEY_WALLETS_AFTER";
        std::env::set_var(env_var, ANVIL_KEY_NO_PREFIX);
        let storage = EnvVarStorage::new();

        nm.load_wallet_from_key_storage("wallets-after", &storage)
            .await
            .unwrap();

        let with_wallets = nm.get_networks_with_wallets();
        assert_eq!(with_wallets.len(), 1);
        assert!(with_wallets.contains(&"wallets-after".to_string()));

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn test_load_wallet_without_0x_prefix() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let mut network = anvil_network(anvil.endpoint());
        network.name = "no-prefix".to_string();
        let mut nm = NetworkManager::new(&[network]).await.unwrap();

        let env_var = "OMIKUJI_PRIVATE_KEY_NO_PREFIX";
        std::env::set_var(env_var, ANVIL_KEY_NO_PREFIX);
        let storage = EnvVarStorage::new();

        nm.load_wallet_from_key_storage("no-prefix", &storage)
            .await
            .expect("key without 0x prefix should parse");

        assert!(nm.has_wallet("no-prefix"));

        std::env::remove_var(env_var);
    }
}
