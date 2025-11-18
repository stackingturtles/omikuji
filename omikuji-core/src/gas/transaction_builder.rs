//! Transaction builder utility for consistent transaction construction
//!
//! This module provides a builder pattern for constructing blockchain transactions
//! with proper gas configuration, supporting both legacy and EIP-1559 transaction types.

use crate::config::models::{GasConfig, Network as NetworkConfig};
use crate::gas::{utils, GasEstimate};
use alloy::{
    network::{Network, TransactionBuilder},
    primitives::{Address, Bytes, U256},
    providers::Provider,
    transports::Transport,
};
use anyhow::Result;
use std::sync::Arc;
use tracing::debug;

/// Transaction builder that handles gas configuration
pub struct GasAwareTransactionBuilder<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + Clone,
{
    to: Address,
    data: Bytes,
    value: Option<U256>,
    network_config: NetworkConfig,
    gas_config_override: Option<GasConfig>,
    gas_limit_override: Option<u64>,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_n: std::marker::PhantomData<N>,
    _phantom_p: std::marker::PhantomData<P>,
}

impl<T, N, P> GasAwareTransactionBuilder<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N> + Clone,
{
    /// Create a new transaction builder
    pub fn new(_provider: Arc<P>, to: Address, data: Bytes, network_config: NetworkConfig) -> Self {
        Self {
            to,
            data,
            value: None,
            network_config,
            gas_config_override: None,
            gas_limit_override: None,
            _phantom_t: std::marker::PhantomData,
            _phantom_n: std::marker::PhantomData,
            _phantom_p: std::marker::PhantomData,
        }
    }

    /// Set the transaction value (ETH to send)
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = Some(value);
        self
    }

    /// Override the gas configuration
    pub fn with_gas_config(mut self, gas_config: GasConfig) -> Self {
        self.gas_config_override = Some(gas_config);
        self
    }

    /// Override the gas limit
    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit_override = Some(gas_limit);
        self
    }

    /// Build the transaction with proper gas configuration
    pub async fn build(self) -> Result<N::TransactionRequest>
    where
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let gas_config = self
            .gas_config_override
            .as_ref()
            .unwrap_or(&self.network_config.gas_config);

        // Create base transaction
        let mut tx = N::TransactionRequest::default();
        tx.set_to(self.to);
        tx.set_input(self.data.clone());

        if let Some(value) = self.value {
            tx.set_value(value);
        }

        // Note: Gas estimation requires a specific provider type
        // For now, we'll skip automatic estimation in the builder
        // and require the caller to provide a gas estimate

        // Apply default gas configuration without estimation
        let default_estimate = crate::gas::GasEstimate {
            gas_limit: U256::from(
                self.gas_limit_override
                    .unwrap_or(crate::constants::gas::DEFAULT_GAS_LIMIT),
            ),
            gas_price: gas_config.gas_price_gwei.map(utils::gwei_to_wei),
            max_fee_per_gas: gas_config.max_fee_per_gas_gwei.map(utils::gwei_to_wei),
            max_priority_fee_per_gas: gas_config
                .max_priority_fee_per_gas_gwei
                .map(utils::gwei_to_wei),
        };

        // Apply gas estimate to transaction
        self.apply_gas_estimate(&mut tx, &default_estimate, gas_config)?;

        Ok(tx)
    }

    /// Build transaction with a specific gas estimate
    pub fn build_with_estimate(self, gas_estimate: &GasEstimate) -> Result<N::TransactionRequest>
    where
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let gas_config = self
            .gas_config_override
            .as_ref()
            .unwrap_or(&self.network_config.gas_config);

        // Create base transaction
        let mut tx = N::TransactionRequest::default();
        tx.set_to(self.to);
        tx.set_input(self.data.clone());

        if let Some(value) = self.value {
            tx.set_value(value);
        }

        // Apply gas estimate
        self.apply_gas_estimate(&mut tx, gas_estimate, gas_config)?;

        Ok(tx)
    }

    /// Apply gas estimate to transaction
    fn apply_gas_estimate(
        &self,
        tx: &mut N::TransactionRequest,
        estimate: &GasEstimate,
        gas_config: &GasConfig,
    ) -> Result<()>
    where
        N::TransactionRequest: TransactionBuilder<N>,
    {
        // Set gas limit
        let gas_limit = self
            .gas_limit_override
            .or(gas_config.gas_limit)
            .unwrap_or(estimate.gas_limit.to::<u64>());
        tx.set_gas_limit(gas_limit);
        debug!("Set gas limit: {}", gas_limit);

        // Apply gas pricing based on transaction type
        match self.network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                let gas_price = estimate.gas_price.ok_or_else(|| {
                    anyhow::anyhow!("No gas price in estimate for legacy transaction")
                })?;
                tx.set_gas_price(gas_price.to::<u128>());
                debug!(
                    "Set legacy gas price: {} gwei",
                    utils::wei_to_gwei(gas_price)
                );
            }
            "eip1559" => {
                let max_fee = estimate.max_fee_per_gas.ok_or_else(|| {
                    anyhow::anyhow!("No max fee in estimate for EIP-1559 transaction")
                })?;
                let priority_fee = estimate.max_priority_fee_per_gas.ok_or_else(|| {
                    anyhow::anyhow!("No priority fee in estimate for EIP-1559 transaction")
                })?;

                tx.set_max_fee_per_gas(max_fee.to::<u128>());
                tx.set_max_priority_fee_per_gas(priority_fee.to::<u128>());

                debug!(
                    "Set EIP-1559 fees - max: {} gwei, priority: {} gwei",
                    utils::wei_to_gwei(max_fee),
                    utils::wei_to_gwei(priority_fee)
                );
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown transaction type: {}",
                    self.network_config.transaction_type
                ));
            }
        }

        Ok(())
    }

    /// Apply fee bump to an existing transaction
    pub fn apply_fee_bump(
        tx: &mut N::TransactionRequest,
        network_config: &NetworkConfig,
        attempt: u32,
    ) -> Result<()>
    where
        N::TransactionRequest: TransactionBuilder<N>,
    {
        let fee_config = &network_config.gas_config.fee_bumping;
        if !fee_config.enabled || attempt <= 1 {
            return Ok(());
        }

        let increase_percent = fee_config.fee_increase_percent;

        match network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                // For legacy, we need to get the current gas price and bump it
                // This is simplified - in practice, you'd extract from the tx
                if let Some(base_price) = network_config.gas_config.gas_price_gwei {
                    let base_wei = utils::gwei_to_wei(base_price);
                    let bumped_wei = utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_gas_price(bumped_wei.to::<u128>());
                    debug!(
                        "Bumped legacy gas price from {} gwei to {} gwei (attempt #{})",
                        base_price,
                        utils::wei_to_gwei(bumped_wei),
                        attempt
                    );
                }
            }
            "eip1559" => {
                // For EIP-1559, bump both max fee and priority fee
                if let Some(base_max_fee) = network_config.gas_config.max_fee_per_gas_gwei {
                    let base_wei = utils::gwei_to_wei(base_max_fee);
                    let bumped_wei = utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_max_fee_per_gas(bumped_wei.to::<u128>());
                    debug!(
                        "Bumped max fee from {} gwei to {} gwei",
                        base_max_fee,
                        utils::wei_to_gwei(bumped_wei)
                    );
                }

                if let Some(base_priority) = network_config.gas_config.max_priority_fee_per_gas_gwei
                {
                    let base_wei = utils::gwei_to_wei(base_priority);
                    let bumped_wei = utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_max_priority_fee_per_gas(bumped_wei.to::<u128>());
                    debug!(
                        "Bumped priority fee from {} gwei to {} gwei",
                        base_priority,
                        utils::wei_to_gwei(bumped_wei)
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/// Create a simple transaction request with default gas settings
pub fn build_simple_transaction<N>(
    to: Address,
    data: Bytes,
    gas_estimate: &GasEstimate,
    tx_type: &str,
) -> Result<N::TransactionRequest>
where
    N: Network,
    N::TransactionRequest: Default + TransactionBuilder<N>,
{
    let mut tx = N::TransactionRequest::default();
    tx.set_to(to);
    tx.set_input(data);
    tx.set_gas_limit(gas_estimate.gas_limit.to::<u64>());

    match tx_type.to_lowercase().as_str() {
        "legacy" => {
            if let Some(gas_price) = gas_estimate.gas_price {
                tx.set_gas_price(gas_price.to::<u128>());
            }
        }
        "eip1559" => {
            if let Some(max_fee) = gas_estimate.max_fee_per_gas {
                tx.set_max_fee_per_gas(max_fee.to::<u128>());
            }
            if let Some(priority_fee) = gas_estimate.max_priority_fee_per_gas {
                tx.set_max_priority_fee_per_gas(priority_fee.to::<u128>());
            }
        }
        _ => return Err(anyhow::anyhow!("Unknown transaction type: {}", tx_type)),
    }

    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_transaction_builder_creation() {
        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse().unwrap()),
        );
        let to = address!("0000000000000000000000000000000000000001");
        let data = Bytes::from(vec![0x01, 0x02, 0x03]);
        let network_config = NetworkConfig::default();

        let builder = GasAwareTransactionBuilder::<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            _,
        >::new(provider, to, data, network_config)
        .with_value(U256::from(1000))
        .with_gas_limit(100_000);

        // This is just a compile test
        assert!(builder.gas_limit_override.is_some());
    }

    #[test]
    fn test_fee_bump_calculation() {
        let base_price = 100.0; // 100 gwei
        let base_wei = utils::gwei_to_wei(base_price);

        // 10% increase per attempt
        let bumped = utils::calculate_fee_bump(base_wei, 2, 10.0);
        assert_eq!(utils::wei_to_gwei(bumped), 110.0);

        let bumped = utils::calculate_fee_bump(base_wei, 3, 10.0);
        assert_eq!(utils::wei_to_gwei(bumped), 121.0);
    }
}
