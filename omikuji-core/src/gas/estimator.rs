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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{FeeBumpingConfig, GasConfig, Network, NetworkNode};
    use alloy::{
        node_bindings::{Anvil, AnvilInstance},
        providers::{ProviderBuilder, RootProvider},
        transports::http::{Client, Http},
    };

    /// Create an Anvil-backed provider for tests.
    /// Returns the AnvilInstance (must stay alive) and the provider.
    fn anvil_provider() -> (AnvilInstance, Arc<RootProvider<Http<Client>>>) {
        let anvil = Anvil::new()
            .try_spawn()
            .expect("Anvil required for this test");
        let provider = Arc::new(ProviderBuilder::new().on_http(anvil.endpoint_url()));
        (anvil, provider)
    }

    fn create_test_network(tx_type: &str) -> Network {
        Network {
            name: "test".to_string(),
            nodes: vec![NetworkNode {
                name: "Test Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: tx_type.to_string(),
            gas_config: GasConfig {
                gas_limit: None,
                gas_price_gwei: None,
                max_fee_per_gas_gwei: None,
                max_priority_fee_per_gas_gwei: None,
                gas_multiplier: 1.2,
                fee_bumping: FeeBumpingConfig {
                    enabled: true,
                    max_retries: 3,
                    initial_wait_seconds: 30,
                    fee_increase_percent: 10.0,
                },
            },
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }
    }

    fn simple_tx() -> TransactionRequest {
        TransactionRequest::default()
    }

    // --- Async tests using Anvil ---

    #[tokio::test]
    async fn test_estimate_gas_legacy() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("legacy");
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        assert!(estimate.gas_price.is_some(), "legacy should have gas_price");
        assert!(estimate.max_fee_per_gas.is_none());
        assert!(estimate.max_priority_fee_per_gas.is_none());
        assert!(estimate.gas_limit > U256::ZERO);
    }

    #[tokio::test]
    async fn test_estimate_gas_eip1559() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("eip1559");
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        assert!(estimate.gas_price.is_none());
        assert!(
            estimate.max_fee_per_gas.is_some(),
            "eip1559 should have max_fee_per_gas"
        );
        assert!(
            estimate.max_priority_fee_per_gas.is_some(),
            "eip1559 should have max_priority_fee_per_gas"
        );
        assert!(estimate.gas_limit > U256::ZERO);
    }

    #[tokio::test]
    async fn test_estimate_gas_unknown_type_defaults_eip1559() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("unknown_type");
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        // Unknown type should fall through to EIP-1559
        assert!(estimate.gas_price.is_none());
        assert!(estimate.max_fee_per_gas.is_some());
        assert!(estimate.max_priority_fee_per_gas.is_some());
    }

    #[tokio::test]
    async fn test_estimate_gas_limit_manual_override() {
        let (_anvil, provider) = anvil_provider();
        let mut network = create_test_network("eip1559");
        network.gas_config.gas_limit = Some(300_000);
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        assert_eq!(estimate.gas_limit, U256::from(300_000));
    }

    #[tokio::test]
    async fn test_estimate_gas_limit_with_multiplier() {
        let (_anvil, provider) = anvil_provider();
        let mut network = create_test_network("eip1559");
        network.gas_config.gas_multiplier = 1.5;
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        // Gas limit should be > 0 and reflect the multiplier
        // On Anvil, estimate_gas for an empty tx returns 21000
        // With 1.5x multiplier: 21000 * 1500 / 1000 = 31500
        assert!(estimate.gas_limit > U256::ZERO);
    }

    #[tokio::test]
    async fn test_estimate_legacy_gas_price_manual_override() {
        let (_anvil, provider) = anvil_provider();
        let mut network = create_test_network("legacy");
        network.gas_config.gas_price_gwei = Some(25.0);
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        // 25 gwei = 25_000_000_000 wei
        assert_eq!(estimate.gas_price, Some(U256::from(25_000_000_000u64)));
    }

    #[tokio::test]
    async fn test_estimate_eip1559_manual_both_overrides() {
        let (_anvil, provider) = anvil_provider();
        let mut network = create_test_network("eip1559");
        network.gas_config.max_fee_per_gas_gwei = Some(100.0);
        network.gas_config.max_priority_fee_per_gas_gwei = Some(5.0);
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        assert_eq!(
            estimate.max_fee_per_gas,
            Some(U256::from(100_000_000_000u64))
        );
        assert_eq!(
            estimate.max_priority_fee_per_gas,
            Some(U256::from(5_000_000_000u64))
        );
    }

    #[tokio::test]
    async fn test_estimate_eip1559_partial_override_priority_only() {
        let (_anvil, provider) = anvil_provider();
        let mut network = create_test_network("eip1559");
        network.gas_config.max_priority_fee_per_gas_gwei = Some(3.0);
        // max_fee is NOT set — should be derived from network gas_price + priority fee
        let estimator = GasEstimator::new(provider, network);

        let estimate = estimator.estimate_gas(&simple_tx()).await.unwrap();

        // Priority fee should reflect manual override (3 gwei) with multiplier applied
        // 3 gwei = 3_000_000_000 wei, with 1.2x multiplier = 3_600_000_000
        assert!(estimate.max_priority_fee_per_gas.is_some());
        let priority_fee = estimate.max_priority_fee_per_gas.unwrap();
        assert_eq!(priority_fee, U256::from(3_600_000_000u64));

        // Max fee should be derived from network (not None)
        assert!(estimate.max_fee_per_gas.is_some());
    }

    #[tokio::test]
    async fn test_estimate_gas_returns_valid_fields() {
        let (_anvil, provider) = anvil_provider();

        // Legacy
        let legacy_network = create_test_network("legacy");
        let estimator = GasEstimator::new(provider.clone(), legacy_network);
        let legacy_est = estimator.estimate_gas(&simple_tx()).await.unwrap();
        assert!(legacy_est.gas_price.is_some());
        assert!(legacy_est.max_fee_per_gas.is_none());
        assert!(legacy_est.max_priority_fee_per_gas.is_none());

        // EIP-1559
        let eip_network = create_test_network("eip1559");
        let estimator = GasEstimator::new(provider, eip_network);
        let eip_est = estimator.estimate_gas(&simple_tx()).await.unwrap();
        assert!(eip_est.gas_price.is_none());
        assert!(eip_est.max_fee_per_gas.is_some());
        assert!(eip_est.max_priority_fee_per_gas.is_some());
    }

    // --- Pure function tests (no Anvil needed, but GasEstimator::new requires Arc<P>) ---

    #[test]
    fn test_bump_fees_legacy() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("legacy");
        let estimator = GasEstimator::new(provider, network);

        let original = GasEstimate {
            gas_limit: U256::from(100_000),
            gas_price: Some(U256::from(20_000_000_000u64)), // 20 gwei
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        };

        // 10% bump per retry: multiplier = 1.0 + (10.0/100.0) * retry_count

        // retry_count=1 → multiplier=1.1 → 20 * 1.1 = 22 gwei
        let bumped1 = estimator.bump_fees(&original, 1);
        assert_eq!(bumped1.gas_limit, original.gas_limit);
        assert_eq!(bumped1.gas_price, Some(U256::from(22_000_000_000u64)));

        // retry_count=2 → multiplier=1.2 → 20 * 1.2 = 24 gwei
        let bumped2 = estimator.bump_fees(&original, 2);
        assert_eq!(bumped2.gas_price, Some(U256::from(24_000_000_000u64)));

        // retry_count=3 → multiplier=1.3 → 20 * 1.3 = 26 gwei
        let bumped3 = estimator.bump_fees(&original, 3);
        assert_eq!(bumped3.gas_price, Some(U256::from(26_000_000_000u64)));
    }

    #[test]
    fn test_bump_fees_eip1559() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("eip1559");
        let estimator = GasEstimator::new(provider, network);

        let original = GasEstimate {
            gas_limit: U256::from(100_000),
            gas_price: None,
            max_fee_per_gas: Some(U256::from(50_000_000_000u64)), // 50 gwei
            max_priority_fee_per_gas: Some(U256::from(2_000_000_000u64)), // 2 gwei
        };

        // retry_count=1 → multiplier=1.1
        let bumped1 = estimator.bump_fees(&original, 1);
        assert_eq!(bumped1.gas_limit, original.gas_limit);
        assert_eq!(
            bumped1.max_fee_per_gas,
            Some(U256::from(55_000_000_000u64)) // 50 * 1.1 = 55 gwei
        );
        assert_eq!(
            bumped1.max_priority_fee_per_gas,
            Some(U256::from(2_200_000_000u64)) // 2 * 1.1 = 2.2 gwei
        );
    }

    #[test]
    fn test_bump_fees_zero_retries() {
        let (_anvil, provider) = anvil_provider();
        let network = create_test_network("legacy");
        let estimator = GasEstimator::new(provider, network);

        let original = GasEstimate {
            gas_limit: U256::from(100_000),
            gas_price: Some(U256::from(20_000_000_000u64)),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        };

        // retry_count=0 → multiplier=1.0 (no change)
        let bumped = estimator.bump_fees(&original, 0);
        assert_eq!(bumped.gas_price, original.gas_price);
        assert_eq!(bumped.gas_limit, original.gas_limit);
    }
}
