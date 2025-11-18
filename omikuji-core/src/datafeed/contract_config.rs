use alloy::primitives::I256;
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::info;

use super::contract_utils::{create_contract_with_provider, parse_address};
use crate::network::NetworkManager;

/// Configuration values read from a FluxAggregator contract
#[derive(Debug, Clone)]
pub struct ContractConfig {
    pub decimals: u8,
    pub min_value: I256, // Store as I256 to match contract's int256
    pub max_value: I256, // Store as I256 to match contract's int256
}

/// Reads configuration from FluxAggregator contracts
pub struct ContractConfigReader<'a> {
    network_manager: &'a Arc<NetworkManager>,
}

impl<'a> ContractConfigReader<'a> {
    /// Creates a new ContractConfigReader
    pub fn new(network_manager: &'a Arc<NetworkManager>) -> Self {
        Self { network_manager }
    }

    /// Reads configuration from a FluxAggregator contract
    ///
    /// # Arguments
    /// * `network_name` - The network the contract is deployed on
    /// * `contract_address` - The address of the FluxAggregator contract
    ///
    /// # Returns
    /// The contract configuration or an error
    pub async fn read_config(
        &self,
        network_name: &str,
        contract_address: &str,
    ) -> Result<ContractConfig> {
        info!(
            "Reading contract config from {} on network {}",
            contract_address, network_name
        );

        // Parse the contract address
        let address = parse_address(contract_address)?;

        // Get provider for the network
        let provider = self
            .network_manager
            .get_provider(network_name)
            .with_context(|| format!("Failed to get provider for network: {network_name}"))?;

        // Create contract instance
        let contract = create_contract_with_provider(address, provider.as_ref().clone());

        // Read decimals
        let decimals = contract
            .decimals()
            .await
            .with_context(|| "Failed to read decimals from contract")?;

        // Read min submission value
        let min_value = contract
            .min_submission_value()
            .await
            .with_context(|| "Failed to read minSubmissionValue from contract")?;

        // Read max submission value
        let max_value = contract
            .max_submission_value()
            .await
            .with_context(|| "Failed to read maxSubmissionValue from contract")?;

        let config = ContractConfig {
            decimals,
            min_value,
            max_value,
        };

        info!(
            "Successfully read contract config: decimals={}, min_value={}, max_value={}",
            config.decimals, config.min_value, config.max_value
        );

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_config_struct() {
        let config = ContractConfig {
            decimals: 8,
            min_value: I256::try_from(-1000000).unwrap(),
            max_value: I256::try_from(1000000).unwrap(),
        };

        assert_eq!(config.decimals, 8);
        assert_eq!(config.min_value, I256::try_from(-1000000).unwrap());
        assert_eq!(config.max_value, I256::try_from(1000000).unwrap());
    }

    #[test]
    fn test_contract_config_with_large_values() {
        // Test with values that would overflow i64
        let large_value = I256::try_from(10000000000000000000i128).unwrap();
        let config = ContractConfig {
            decimals: 6,
            min_value: I256::ZERO,
            max_value: large_value,
        };

        assert_eq!(config.decimals, 6);
        assert_eq!(config.min_value, I256::ZERO);
        assert_eq!(config.max_value, large_value);
    }
}
