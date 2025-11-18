use crate::config::models::{GasConfig, Network};
use alloy::{
    primitives::{
        utils::{format_units, parse_units},
        U256,
    },
    providers::Provider,
    rpc::types::TransactionRequest,
    transports::Transport,
};
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Gas estimate for a transaction
#[derive(Debug, Clone)]
pub struct GasEstimate {
    /// Gas limit for the transaction
    pub gas_limit: U256,
    /// For legacy transactions
    pub gas_price: Option<U256>,
    /// For EIP-1559 transactions
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
}

/// Gas estimator that handles both legacy and EIP-1559 transactions
pub struct GasEstimator<T: Transport + Clone, P: Provider<T> + Clone> {
    provider: Arc<P>,
    network_config: Network,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> GasEstimator<T, P> {
    pub fn new(provider: Arc<P>, network_config: Network) -> Self {
        Self {
            provider,
            network_config,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Estimate gas for a transaction
    pub async fn estimate_gas(&self, tx: &TransactionRequest) -> Result<GasEstimate> {
        let gas_config = &self.network_config.gas_config;

        // Estimate gas limit
        let gas_limit = self.estimate_gas_limit(tx, gas_config).await?;

        // Estimate fees based on transaction type
        let gas_estimate = match self.network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                let gas_price = self.estimate_legacy_gas_price(gas_config).await?;
                GasEstimate {
                    gas_limit,
                    gas_price: Some(gas_price),
                    max_fee_per_gas: None,
                    max_priority_fee_per_gas: None,
                }
            }
            "eip1559" => {
                let (max_fee, max_priority_fee) = self.estimate_eip1559_fees(gas_config).await?;
                GasEstimate {
                    gas_limit,
                    gas_price: None,
                    max_fee_per_gas: Some(max_fee),
                    max_priority_fee_per_gas: Some(max_priority_fee),
                }
            }
            _ => {
                // This should be caught by validation, but handle it gracefully
                warn!("Unknown transaction type, defaulting to EIP-1559");
                let (max_fee, max_priority_fee) = self.estimate_eip1559_fees(gas_config).await?;
                GasEstimate {
                    gas_limit,
                    gas_price: None,
                    max_fee_per_gas: Some(max_fee),
                    max_priority_fee_per_gas: Some(max_priority_fee),
                }
            }
        };

        info!(
            "Gas estimate for {}: limit={}, legacy_price={:?}, max_fee={:?}, priority_fee={:?}",
            self.network_config.name,
            gas_estimate.gas_limit,
            gas_estimate.gas_price,
            gas_estimate.max_fee_per_gas,
            gas_estimate.max_priority_fee_per_gas
        );

        Ok(gas_estimate)
    }

    /// Estimate gas limit for a transaction
    async fn estimate_gas_limit(
        &self,
        tx: &TransactionRequest,
        gas_config: &GasConfig,
    ) -> Result<U256> {
        // Use manual override if provided
        if let Some(manual_limit) = gas_config.gas_limit {
            info!("Using manual gas limit: {}", manual_limit);
            return Ok(U256::from(manual_limit));
        }

        // Otherwise estimate
        match self.provider.estimate_gas(tx).await {
            Ok(estimated) => {
                // Apply multiplier for safety margin
                let multiplier = gas_config.gas_multiplier;
                let estimated_u256 = U256::from(estimated);
                let with_buffer = estimated_u256
                    .saturating_mul(U256::from((multiplier * 1000.0) as u64))
                    / U256::from(1000);
                info!(
                    "Estimated gas limit: {} (with {}x multiplier: {})",
                    estimated, multiplier, with_buffer
                );
                Ok(with_buffer)
            }
            Err(e) => {
                error!("Failed to estimate gas limit: {}", e);
                // Fallback to a reasonable default
                let default_limit = U256::from(200_000);
                warn!("Using fallback gas limit: {}", default_limit);
                Ok(default_limit)
            }
        }
    }

    /// Estimate gas price for legacy transactions
    async fn estimate_legacy_gas_price(&self, gas_config: &GasConfig) -> Result<U256> {
        // Use manual override if provided
        if let Some(manual_price) = gas_config.gas_price_gwei {
            let price_wei = parse_units(&manual_price.to_string(), "gwei")?;
            info!("Using manual gas price: {} gwei", manual_price);
            return Ok(price_wei.into());
        }

        // Otherwise get from network
        match self.provider.get_gas_price().await {
            Ok(gas_price) => {
                // Apply multiplier
                let multiplier = gas_config.gas_multiplier;
                let gas_price_u256 = U256::from(gas_price);
                let with_buffer = gas_price_u256
                    .saturating_mul(U256::from((multiplier * 1000.0) as u64))
                    / U256::from(1000);
                let gwei_price = format_units(with_buffer, "gwei")?;
                info!(
                    "Network gas price: {} gwei (with {}x multiplier: {} gwei)",
                    format_units(gas_price_u256, "gwei")?,
                    multiplier,
                    gwei_price
                );
                Ok(with_buffer)
            }
            Err(e) => {
                error!("Failed to get gas price: {}", e);
                // Fallback to 20 gwei
                let fallback = parse_units("20", "gwei")?;
                warn!("Using fallback gas price: 20 gwei");
                Ok(fallback.into())
            }
        }
    }

    /// Estimate fees for EIP-1559 transactions
    async fn estimate_eip1559_fees(&self, gas_config: &GasConfig) -> Result<(U256, U256)> {
        // Check for manual overrides
        let manual_max_fee = gas_config
            .max_fee_per_gas_gwei
            .map(|gwei| parse_units(&gwei.to_string(), "gwei").map(Into::into))
            .transpose()?;

        let manual_priority_fee = gas_config
            .max_priority_fee_per_gas_gwei
            .map(|gwei| parse_units(&gwei.to_string(), "gwei").map(Into::into))
            .transpose()?;

        if let (Some(max_fee), Some(priority_fee)) = (manual_max_fee, manual_priority_fee) {
            info!(
                "Using manual EIP-1559 fees: max_fee={} gwei, priority_fee={} gwei",
                gas_config.max_fee_per_gas_gwei.unwrap(),
                gas_config.max_priority_fee_per_gas_gwei.unwrap()
            );
            return Ok((max_fee, priority_fee));
        }

        // Try to get fee history for EIP-1559 estimation
        // Note: get_fee_data is not available in ethers 2.0, we'll use gas_price and estimate priority fee
        match self.provider.get_gas_price().await {
            Ok(gas_price) => {
                let multiplier = gas_config.gas_multiplier;

                // Estimate base fee and priority fee
                // Priority fee is typically 1-2 gwei, we'll use 2 gwei as default
                let base_priority_fee =
                    manual_priority_fee.unwrap_or_else(|| parse_units("2", "gwei").unwrap().into());

                // Max fee should be current gas price + priority fee + buffer
                let base_max_fee =
                    manual_max_fee.unwrap_or_else(|| U256::from(gas_price) + base_priority_fee);

                // Apply multiplier
                let max_fee = base_max_fee.saturating_mul(U256::from((multiplier * 1000.0) as u64))
                    / U256::from(1000);
                let priority_fee = base_priority_fee
                    .saturating_mul(U256::from((multiplier * 1000.0) as u64))
                    / U256::from(1000);

                info!(
                    "EIP-1559 fees: max_fee={} gwei, priority_fee={} gwei ({}x multiplier applied)",
                    format_units(max_fee, "gwei")?,
                    format_units(priority_fee, "gwei")?,
                    multiplier
                );

                Ok((max_fee, priority_fee))
            }
            Err(e) => {
                error!("Failed to get gas price for EIP-1559 estimation: {}", e);
                // Fallback values
                let max_fee = parse_units("50", "gwei")?.into();
                let priority_fee = parse_units("2", "gwei")?.into();
                warn!("Using fallback EIP-1559 fees: max_fee=50 gwei, priority_fee=2 gwei");
                Ok((max_fee, priority_fee))
            }
        }
    }

    /// Bump fees for a retry attempt
    pub fn bump_fees(&self, original: &GasEstimate, retry_count: u8) -> GasEstimate {
        let bump_percent = self
            .network_config
            .gas_config
            .fee_bumping
            .fee_increase_percent;
        let multiplier = 1.0 + (bump_percent / 100.0) * retry_count as f64;

        GasEstimate {
            gas_limit: original.gas_limit, // Keep same gas limit
            gas_price: original.gas_price.map(|p| {
                p.saturating_mul(U256::from((multiplier * 1000.0) as u64)) / U256::from(1000)
            }),
            max_fee_per_gas: original.max_fee_per_gas.map(|p| {
                p.saturating_mul(U256::from((multiplier * 1000.0) as u64)) / U256::from(1000)
            }),
            max_priority_fee_per_gas: original.max_priority_fee_per_gas.map(|p| {
                p.saturating_mul(U256::from((multiplier * 1000.0) as u64)) / U256::from(1000)
            }),
        }
    }
}
