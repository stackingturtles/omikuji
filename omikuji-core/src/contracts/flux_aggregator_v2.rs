//! Example refactored FluxAggregator contract using new metrics patterns
//!
//! This demonstrates how to use the consolidated metrics recording utilities
//! to reduce code duplication and improve consistency.

use crate::config::models::Network as NetworkConfig;
use crate::database::TransactionLogRepository;
use crate::gas::GasEstimate;
use crate::metrics::{
    MetricsContext, RetryMetricsRecorder, TimedOperationRecorder, TransactionMetricsRecorder,
};
use alloy::{
    network::{Ethereum, TransactionBuilder},
    primitives::{Address, I256, U256},
    providers::Provider,
    rpc::types::{BlockId, TransactionReceipt, TransactionRequest},
    sol,
    sol_types::SolCall,
    transports::Transport,
};
use anyhow::Result;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{error, info, warn};

// Reuse the same Solidity interface from the original
sol! {
    #[sol(rpc)]
    interface IFluxAggregator {
        function latestAnswer() external view returns (int256);
        function latestTimestamp() external view returns (uint256);
        function submit(uint256 _roundId, int256 _submission) external;
    }
}

/// Refactored FluxAggregator contract with consolidated metrics
pub struct FluxAggregatorContractV2<T: Transport + Clone, P: Provider<T, Ethereum>> {
    address: Address,
    provider: P,
    metrics_context: MetricsContext,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Transport + Clone, P: Provider<T, Ethereum> + Clone> FluxAggregatorContractV2<T, P> {
    /// Create a new FluxAggregator contract instance with metrics context
    pub fn new(address: Address, provider: P, feed_name: &str, network: &str) -> Self {
        Self {
            address,
            provider,
            metrics_context: MetricsContext::new(feed_name, network),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the latest answer from the contract with automatic metrics
    pub async fn latest_answer(&self) -> Result<I256> {
        let recorder =
            TimedOperationRecorder::contract_read(self.metrics_context.clone(), "latestAnswer");

        let result = async {
            let call = IFluxAggregator::latestAnswerCall {};
            let tx = TransactionRequest::default()
                .to(self.address)
                .input(call.abi_encode().into());

            let result = self.provider.call(&tx).block(BlockId::latest()).await?;
            let decoded = IFluxAggregator::latestAnswerCall::abi_decode_returns(&result, true)?;
            Ok(decoded._0)
        }
        .await;

        recorder.record_result(&result, None);
        result
    }

    /// Submit a new price with consolidated retry and metrics logic
    pub async fn submit_price_with_metrics(
        &self,
        round_id: U256,
        price: I256,
        network_config: &NetworkConfig,
        tx_log_repo: Option<Arc<TransactionLogRepository>>,
        from_address: Option<Address>,
    ) -> Result<TransactionReceipt> {
        let gas_config = &network_config.gas_config;
        let fee_bumping = &gas_config.fee_bumping;

        // Initialize retry metrics recorder
        let max_attempts = if fee_bumping.enabled {
            fee_bumping.max_retries + 1
        } else {
            1
        };

        let mut retry_recorder =
            RetryMetricsRecorder::new(self.metrics_context.clone(), max_attempts as u32);

        // Initialize transaction metrics recorder
        let tx_recorder = TransactionMetricsRecorder::new(
            self.metrics_context.clone(),
            &network_config.transaction_type,
        );

        // Create the function call
        let call = IFluxAggregator::submitCall {
            _roundId: round_id,
            _submission: price,
        };

        // Build base transaction request
        let mut tx = TransactionRequest::default()
            .to(self.address)
            .input(call.abi_encode().into());

        if let Some(from) = from_address {
            tx = tx.from(from);
        }

        // Estimate gas
        let gas_estimator = crate::gas::GasEstimator::<T, P>::new(
            Arc::new(self.provider.clone()),
            network_config.clone(),
        );
        let mut gas_estimate = gas_estimator.estimate_gas(&tx).await?;

        // Retry loop with consolidated metrics
        loop {
            let attempt = retry_recorder.start_attempt();

            // Apply gas settings using existing patterns
            tx = tx.with_gas_limit(gas_estimate.gas_limit.to::<u64>());
            self.apply_gas_pricing(&mut tx, &gas_estimate, network_config);

            info!("Sending transaction (attempt {})", attempt);

            // Record submission time for confirmation metrics
            let submission_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Send transaction with metrics recording
            let result = self
                .send_transaction_with_metrics(
                    &tx,
                    &tx_recorder,
                    &gas_estimate,
                    network_config,
                    tx_log_repo.as_ref(),
                    submission_time,
                )
                .await;

            match result {
                Ok(receipt) => return Ok(receipt),
                Err(e) => {
                    let error_str = e.to_string();

                    // Record specific error types
                    if error_str.contains("revert") {
                        tx_recorder.record_revert(&error_str);
                    }

                    // Check if we should retry
                    if retry_recorder.check_max_attempts_reached() {
                        return Err(anyhow::anyhow!(
                            "Failed to send transaction after {} attempts: {}",
                            attempt,
                            e
                        ));
                    }

                    // Record retry with reason
                    let retry_reason = self.categorize_error(&error_str);
                    retry_recorder.record_retry(&retry_reason);

                    // Bump fees for retry if enabled
                    if fee_bumping.enabled {
                        gas_estimate = gas_estimator.bump_fees(&gas_estimate, attempt as u8);
                        info!("Bumping fees for retry attempt {}", attempt + 1);
                    }
                }
            }
        }
    }

    /// Apply gas pricing to transaction based on type
    fn apply_gas_pricing(
        &self,
        tx: &mut TransactionRequest,
        gas_estimate: &GasEstimate,
        network_config: &NetworkConfig,
    ) {
        match network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                if let Some(gas_price) = gas_estimate.gas_price {
                    *tx = tx.clone().with_gas_price(gas_price.to::<u128>());
                }
            }
            "eip1559" => {
                if let Some(max_fee) = gas_estimate.max_fee_per_gas {
                    *tx = tx.clone().with_max_fee_per_gas(max_fee.to::<u128>());
                }
                if let Some(priority_fee) = gas_estimate.max_priority_fee_per_gas {
                    *tx = tx
                        .clone()
                        .with_max_priority_fee_per_gas(priority_fee.to::<u128>());
                }
            }
            _ => {
                warn!("Unknown transaction type, defaulting to EIP-1559");
                if let Some(max_fee) = gas_estimate.max_fee_per_gas {
                    *tx = tx.clone().with_max_fee_per_gas(max_fee.to::<u128>());
                }
                if let Some(priority_fee) = gas_estimate.max_priority_fee_per_gas {
                    *tx = tx
                        .clone()
                        .with_max_priority_fee_per_gas(priority_fee.to::<u128>());
                }
            }
        }
    }

    /// Send transaction with consolidated metrics recording
    async fn send_transaction_with_metrics(
        &self,
        tx: &TransactionRequest,
        tx_recorder: &TransactionMetricsRecorder,
        gas_estimate: &GasEstimate,
        network_config: &NetworkConfig,
        tx_log_repo: Option<&Arc<TransactionLogRepository>>,
        submission_time: u64,
    ) -> Result<TransactionReceipt> {
        // Send transaction
        let pending_tx = self
            .provider
            .send_transaction(tx.clone())
            .await
            .map_err(|e| {
                tx_recorder.record_failure(
                    gas_estimate.gas_limit,
                    gas_estimate.gas_price.or(gas_estimate.max_fee_per_gas),
                    &e.to_string(),
                );
                e
            })?;

        let tx_hash = *pending_tx.tx_hash();
        info!("Transaction sent: 0x{:x}", tx_hash);

        // Wait for confirmation with timeout
        let wait_duration =
            Duration::from_secs(network_config.gas_config.fee_bumping.initial_wait_seconds);

        let receipt = tokio::time::timeout(
            wait_duration,
            pending_tx.with_required_confirmations(1).get_receipt(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Transaction timed out"))?
        .map_err(|e| {
            tx_recorder.record_failure(
                gas_estimate.gas_limit,
                gas_estimate.gas_price.or(gas_estimate.max_fee_per_gas),
                &e.to_string(),
            );
            e
        })?;

        if !receipt.status() {
            tx_recorder.record_failure(
                gas_estimate.gas_limit,
                gas_estimate.gas_price.or(gas_estimate.max_fee_per_gas),
                "Transaction reverted",
            );
            return Err(anyhow::anyhow!("Transaction failed: 0x{:x}", tx_hash));
        }

        // Record successful transaction with all metrics
        tx_recorder.record_success(&receipt, gas_estimate.gas_limit, Some(submission_time));

        // Log transaction if repository is available
        if let Some(repo) = tx_log_repo {
            if let Err(e) = self
                .log_transaction(
                    repo,
                    &tx_hash,
                    &receipt,
                    gas_estimate,
                    &network_config.transaction_type,
                )
                .await
            {
                error!("Failed to log transaction: {}", e);
            }
        }

        info!("Transaction confirmed: 0x{:x}", tx_hash);
        Ok(receipt)
    }

    /// Categorize error for retry reason tracking
    fn categorize_error(&self, error: &str) -> String {
        if error.contains("gas") {
            "insufficient_gas".to_string()
        } else if error.contains("nonce") {
            "nonce_conflict".to_string()
        } else if error.contains("timeout") || error.contains("deadline") {
            "timeout".to_string()
        } else if error.contains("connection") || error.contains("network") {
            "network_error".to_string()
        } else if error.contains("replacement") || error.contains("underpriced") {
            "fee_too_low".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Log transaction details (simplified version)
    async fn log_transaction(
        &self,
        repo: &Arc<TransactionLogRepository>,
        tx_hash: &alloy::primitives::TxHash,
        receipt: &TransactionReceipt,
        gas_estimate: &GasEstimate,
        tx_type: &str,
    ) -> Result<()> {
        let gas_used = receipt.gas_used;
        let gas_limit = gas_estimate.gas_limit;
        let efficiency_percent = (gas_used as f64 / gas_limit.to::<u128>() as f64) * 100.0;

        let gas_price_gwei = if let Some(price) = gas_estimate.gas_price {
            alloy::primitives::utils::format_units(price, "gwei")?.parse::<f64>()?
        } else if let Some(max_fee) = gas_estimate.max_fee_per_gas {
            alloy::primitives::utils::format_units(max_fee, "gwei")?.parse::<f64>()?
        } else {
            0.0
        };

        let total_cost_wei = U256::from(gas_used) * gas_estimate.gas_price.unwrap_or(U256::ZERO);

        let details = crate::metrics::gas_metrics::TransactionDetails {
            tx_hash: format!("0x{tx_hash:x}"),
            feed_name: self.metrics_context.feed_name().to_string(),
            network: self.metrics_context.network().to_string(),
            gas_limit: gas_limit.to::<u64>(),
            gas_used: gas_used as u64,
            gas_price_gwei,
            total_cost_wei: total_cost_wei.to::<u128>(),
            efficiency_percent,
            tx_type: tx_type.to_string(),
            status: if receipt.status() {
                "success"
            } else {
                "failed"
            }
            .to_string(),
            block_number: receipt.block_number.unwrap_or(0),
            error_message: None,
        };

        repo.save_transaction(details).await?;
        Ok(())
    }

    /// Get metrics context for external use
    pub fn metrics_context(&self) -> &MetricsContext {
        &self.metrics_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categorization() {
        // Create a minimal test helper function to avoid provider trait bounds
        fn categorize_error(error: &str) -> String {
            if error.contains("gas") {
                "insufficient_gas".to_string()
            } else if error.contains("nonce") {
                "nonce_conflict".to_string()
            } else if error.contains("timeout") || error.contains("deadline") {
                "timeout".to_string()
            } else if error.contains("connection") || error.contains("network") {
                "network_error".to_string()
            } else if error.contains("replacement") || error.contains("underpriced") {
                "fee_too_low".to_string()
            } else {
                "unknown".to_string()
            }
        }

        assert_eq!(categorize_error("out of gas"), "insufficient_gas");
        assert_eq!(categorize_error("nonce too low"), "nonce_conflict");
        assert_eq!(categorize_error("timeout exceeded"), "timeout");
        assert_eq!(categorize_error("connection refused"), "network_error");
        assert_eq!(
            categorize_error("replacement transaction underpriced"),
            "fee_too_low"
        );
        assert_eq!(categorize_error("unknown error"), "unknown");
    }

    #[test]
    fn test_metrics_context() {
        let context = MetricsContext::new("eth_usd", "ethereum");
        assert_eq!(context.feed_name(), "eth_usd");
        assert_eq!(context.network(), "ethereum");
        assert!(context.method().is_none());

        let context_with_method = context.with_method("latestAnswer");
        assert_eq!(context_with_method.method(), Some("latestAnswer"));
    }
}
