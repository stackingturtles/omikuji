use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, RootProvider},
    signers::local::PrivateKeySigner,
    transports::http::{Client, Http},
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
pub type EthProvider = RootProvider<Http<Client>>;

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
            let first_node = network.nodes.first()
                .ok_or_else(|| anyhow::anyhow!("Network {} has no nodes configured", network.name))?;
            
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

        let signer = private_key
            .parse::<PrivateKeySigner>()
            .with_context(|| "Failed to parse private key as signer")?;

        // Store the wallet address
        let wallet_address = signer.address();
        self.wallet_addresses
            .insert(network_name.to_string(), wallet_address);

        // Store the private key (we'll create providers with wallets on demand)
        self.private_keys
            .insert(network_name.to_string(), private_key);

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

        let signer = private_key
            .parse::<PrivateKeySigner>()
            .with_context(|| "Failed to parse private key as signer")?;

        // Store the wallet address
        let wallet_address = signer.address();
        self.wallet_addresses
            .insert(network_name.to_string(), wallet_address);

        // Store the private key (we'll create providers with wallets on demand)
        self.private_keys
            .insert(network_name.to_string(), private_key.to_string());

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
                "No private key found for network {}. Call load_wallet_from_env first",
                network_name
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
                "No signer found for network {}. Call load_wallet_from_env first",
                network_name
            ))
        }
    }

    /// Get all configured network names
    pub fn get_network_names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Get the wallet address for a given network
    pub fn get_wallet_address(&self, network_name: &str) -> Result<Address> {
        self.wallet_addresses
            .get(network_name)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No wallet address found for network {}. Call load_wallet_from_env first",
                    network_name
                )
            })
    }
    
    /// Get a WebSocket provider for a given network
    pub async fn get_ws_provider(&self, network_name: &str) -> Result<Arc<WsProvider>> {
        let network = self.networks
            .get(network_name)
            .ok_or_else(|| NetworkError::NetworkNotFound(network_name.to_string()))?;
        
        // Use the first node that has a WebSocket URL
        let node_with_ws = network.nodes.iter()
            .find(|node| node.ws_url.is_some())
            .ok_or_else(|| anyhow::anyhow!("Network {} has no WebSocket URLs configured", network_name))?;
        
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

    /// Create a provider from an RPC URL
    async fn create_provider(rpc_url: &str) -> Result<EthProvider> {
        let url =
            Url::parse(rpc_url).with_context(|| format!("Failed to parse RPC URL: {rpc_url}"))?;

        let provider = ProviderBuilder::new().on_http(url);

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
