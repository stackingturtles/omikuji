//! Refactored network provider using consolidated error handling
//!
//! This demonstrates how to use the new error handling utilities to reduce
//! boilerplate and improve consistency.

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
use anyhow::Result;
use secrecy::ExposeSecret;
use thiserror::Error;
use tracing::{error, info};
use url::Url;

use crate::config::models::Network;
use omikuji_core::error_context::{self, ErrorContextExt};
use omikuji_core::error_handlers::{validation, ErrorWrapper, NetworkOperationError};
use crate::metrics::NetworkMetrics;
use crate::wallet::key_storage::KeyStorage;

/// Type alias for the alloy provider we will use
pub type EthProvider = FillProvider<JoinedRecommendedFillers, RootProvider>;

/// Manages the connections to different EVM networks (refactored version)
pub struct NetworkManagerV2 {
    /// Map of network name to provider
    providers: HashMap<String, Arc<EthProvider>>,
    /// Private keys for each network (stored securely)
    private_keys: HashMap<String, String>,
    /// RPC URLs for each network (needed for creating signed providers)
    rpc_urls: HashMap<String, String>,
    /// Wallet addresses for each network
    wallet_addresses: HashMap<String, Address>,
}

impl NetworkManagerV2 {
    /// Create a new network manager from a list of network configurations
    pub async fn new(networks: &[Network]) -> Result<Self> {
        let mut providers = HashMap::new();
        let private_keys = HashMap::new();
        let mut rpc_urls = HashMap::new();
        let wallet_addresses = HashMap::new();

        for network in networks {
            // Use the new error context helper
            let provider = Self::create_provider(&network.rpc_url)
                .await
                .map_err(|e| NetworkOperationError::provider_creation(&network.name, e))?;

            providers.insert(network.name.clone(), Arc::new(provider));
            rpc_urls.insert(network.name.clone(), network.rpc_url.clone());
        }

        Ok(Self {
            providers,
            private_keys,
            rpc_urls,
            wallet_addresses,
        })
    }

    /// Load a wallet from an environment variable
    pub async fn load_wallet_from_env(&mut self, network_name: &str, env_var: &str) -> Result<()> {
        info!(
            "Attempting to load wallet for network {} from env var {}",
            network_name, env_var
        );

        // Use validation helper
        validation::require_network_exists(
            network_name,
            &self.providers.keys().cloned().collect::<Vec<_>>(),
        )?;

        let private_key = std::env::var(env_var)
            .context_fmt(|| error_context::config::missing_field(env_var))?;

        info!(
            "Successfully read private key from env var {} (length: {})",
            env_var,
            private_key.len()
        );

        self.load_wallet(network_name, private_key).await
    }

    /// Load a wallet from key storage
    pub async fn load_wallet_from_key_storage(
        &mut self,
        network_name: &str,
        key_storage: &dyn KeyStorage,
    ) -> Result<()> {
        info!(
            "Attempting to load wallet for network {} from key storage",
            network_name
        );

        // Use validation helper
        validation::require_network_exists(
            network_name,
            &self.providers.keys().cloned().collect::<Vec<_>>(),
        )?;

        // Use error context helper
        let private_key = key_storage
            .get_key(network_name)
            .await
            .context_fmt(|| error_context::key_storage::retrieve(network_name))?
            .ok_or_else(|| NetworkOperationError::key_not_found(network_name))?;

        self.load_wallet(network_name, private_key.expose_secret().clone())
            .await
    }

    /// Internal method to load a wallet
    async fn load_wallet(&mut self, network_name: &str, private_key: String) -> Result<()> {
        // Parse the private key using error context
        let signer: PrivateKeySigner = private_key
            .parse()
            .context_fmt(|| error_context::key_storage::parse())?;

        let wallet_address = signer.address();

        // Update metrics and state
        NetworkMetrics::set_wallet_status(network_name, true);
        self.wallet_addresses
            .insert(network_name.to_string(), wallet_address);
        self.private_keys
            .insert(network_name.to_string(), private_key);

        info!(
            "Successfully loaded wallet for network {} with address {}",
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
                // Use the new error context
                Err(e).context_network("get_chain_id", network_name)
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
                // Use the new error context
                Err(e).context_network("get_block_number", network_name)
            }
        }
    }

    /// Get a provider for a given network
    pub fn get_provider(&self, network_name: &str) -> Result<Arc<EthProvider>> {
        self.providers
            .get(network_name)
            .cloned()
            .ok_or_else(|| error_context::not_found("Network provider", network_name).into())
    }

    /// Get the private key for a network
    pub fn get_private_key(&self, network_name: &str) -> Result<String> {
        self.private_keys
            .get(network_name)
            .cloned()
            .ok_or_else(|| error_context::not_found("Private key", network_name).into())
            .context_fmt(|| error_context::messages::operation_failed("get_private_key", network_name))
    }

    /// Get the RPC URL for a network
    pub fn get_rpc_url(&self, network_name: &str) -> Result<&str> {
        self.rpc_urls
            .get(network_name)
            .map(|s| s.as_str())
            .ok_or_else(|| error_context::not_found("RPC URL", network_name).into())
    }

    /// Create a provider from an RPC URL
    async fn create_provider(rpc_url: &str) -> Result<EthProvider> {
        let url = Url::parse(rpc_url)
            .map_err(|e| NetworkOperationError::invalid_rpc_url(rpc_url, e))?;

        let provider = ProviderBuilder::new().connect_http(url);

        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_usage() {
        // Test that error contexts are properly formatted
        let err = error_context::network::provider_creation("mainnet");
        assert_eq!(err, "Failed to create provider for network 'mainnet'");

        let err = error_context::not_found("Network", "testnet");
        assert_eq!(err, "Network 'testnet' not found");
    }

    #[tokio::test]
    async fn test_network_not_found_error() {
        let networks = vec![];
        let manager = NetworkManagerV2::new(&networks).await.unwrap();

        let result = manager.get_provider("nonexistent");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Network provider 'nonexistent' not found"));
    }
}