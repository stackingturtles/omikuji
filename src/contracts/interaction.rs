//! Common contract interaction patterns
//!
//! This module provides abstractions for common contract interaction patterns
//! to reduce code duplication and improve consistency.

use crate::config::models::Network as NetworkConfig;
use crate::database::TransactionLogRepository;
use crate::gas::utils as gas_utils;
use crate::metrics::ContractMetrics;
use crate::utils::{TransactionContext, TransactionLogger};
use alloy::{
    network::{Network, ReceiptResponse, TransactionBuilder},
    primitives::{Address, Bytes},
    providers::Provider,
    rpc::types::BlockId,
    transports::Transport,
};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

/// Common contract interaction builder
pub struct ContractInteraction<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    provider: Arc<P>,
    contract_address: Address,
    network_config: NetworkConfig,
    feed_name: Option<String>,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_n: std::marker::PhantomData<N>,
}

impl<T, N, P> ContractInteraction<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    /// Create a new contract interaction builder
    pub fn new(provider: Arc<P>, contract_address: Address, network_config: NetworkConfig) -> Self {
        Self {
            provider,
            contract_address,
            network_config,
            feed_name: None,
            _phantom_t: std::marker::PhantomData,
            _phantom_n: std::marker::PhantomData,
        }
    }

    /// Set the feed name for metrics tracking
    pub fn with_feed_name(mut self, feed_name: String) -> Self {
        self.feed_name = Some(feed_name);
        self
    }

    /// Call a read-only contract function with metrics tracking
    pub async fn call_with_metrics<C, R>(
        &self,
        call_data: C,
        function_name: &str,
        decode_fn: impl FnOnce(&Bytes) -> Result<R>,
    ) -> Result<R>
    where
        C: Into<Bytes>,
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let start = Instant::now();
        let mut tx = N::TransactionRequest::default();
        tx.set_to(self.contract_address);
        tx.set_input(call_data.into());

        match self.provider.call(&tx).block(BlockId::latest()).await {
            Ok(result) => {
                let duration = start.elapsed();

                // Record metrics if context is provided
                if let Some(ref feed_name) = self.feed_name {
                    ContractMetrics::record_contract_read(
                        feed_name,
                        &self.network_config.name,
                        function_name,
                        true,
                        duration,
                        None,
                    );
                }

                decode_fn(&result).context("Failed to decode contract response")
            }
            Err(e) => {
                let duration = start.elapsed();

                // Record metrics if context is provided
                if let Some(ref feed_name) = self.feed_name {
                    ContractMetrics::record_contract_read(
                        feed_name,
                        &self.network_config.name,
                        function_name,
                        false,
                        duration,
                        Some(&e.to_string()),
                    );
                }

                Err(e.into())
            }
        }
    }

    /// Submit a transaction with gas estimation, retry logic, and standardized handling
    pub async fn submit_transaction_with_handling<C>(
        &self,
        call_data: C,
        context: TransactionContext,
        gas_limit_override: Option<u64>,
        _tx_log_repo: Option<Arc<TransactionLogRepository>>,
        _gas_price_manager: Option<&Arc<crate::gas_price::GasPriceManager>>,
    ) -> Result<N::ReceiptResponse>
    where
        C: Into<Bytes>,
        N::TransactionRequest: Default + TransactionBuilder<N>,
        N::ReceiptResponse: std::fmt::Debug,
    {
        let call_bytes = call_data.into();

        // Log the submission
        TransactionLogger::log_submission(
            context.context_type(),
            context.name(),
            &self.network_config.name,
            None,
        );

        // Build base transaction
        let mut tx = N::TransactionRequest::default();
        tx.set_to(self.contract_address);
        tx.set_input(call_bytes);

        // Apply gas configuration
        let _gas_limit = self.apply_gas_config(&mut tx, gas_limit_override).await?;

        // Send transaction with retry logic
        let receipt = self.send_with_retry(tx, &context).await?;

        // For now, we'll log the basic transaction info
        // TODO: Integrate with TransactionHandler when we have proper type conversion
        info!(
            "Transaction completed successfully for {} '{}'",
            context.context_type(),
            context.name()
        );

        Ok(receipt)
    }

    /// Apply gas configuration to a transaction
    async fn apply_gas_config(
        &self,
        tx: &mut N::TransactionRequest,
        gas_limit_override: Option<u64>,
    ) -> Result<u64>
    where
        N::TransactionRequest: TransactionBuilder<N>,
    {
        // Get gas limit from override or config
        let gas_limit = gas_limit_override
            .or(self.network_config.gas_config.gas_limit)
            .unwrap_or(crate::constants::gas::DEFAULT_GAS_LIMIT);

        tx.set_gas_limit(gas_limit);

        // Apply gas pricing based on transaction type
        match self.network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                if let Some(gas_price) = self.network_config.gas_config.gas_price_gwei {
                    let price_wei = gas_utils::gwei_to_wei(gas_price);
                    tx.set_gas_price(price_wei.to::<u128>());
                    debug!("Set legacy gas price: {} gwei", gas_price);
                }
            }
            "eip1559" => {
                if let Some(max_fee) = self.network_config.gas_config.max_fee_per_gas_gwei {
                    let max_fee_wei = gas_utils::gwei_to_wei(max_fee);
                    tx.set_max_fee_per_gas(max_fee_wei.to::<u128>());
                    debug!("Set max fee per gas: {} gwei", max_fee);
                }
                if let Some(priority_fee) =
                    self.network_config.gas_config.max_priority_fee_per_gas_gwei
                {
                    let priority_fee_wei = gas_utils::gwei_to_wei(priority_fee);
                    tx.set_max_priority_fee_per_gas(priority_fee_wei.to::<u128>());
                    debug!("Set max priority fee per gas: {} gwei", priority_fee);
                }
            }
            _ => {
                debug!("Unknown transaction type, using provider defaults");
            }
        }

        Ok(gas_limit)
    }

    /// Send transaction with retry logic for stuck transactions
    async fn send_with_retry(
        &self,
        mut tx: N::TransactionRequest,
        context: &TransactionContext,
    ) -> Result<N::ReceiptResponse>
    where
        N::TransactionRequest: TransactionBuilder<N> + Clone,
        N::ReceiptResponse: std::fmt::Debug,
    {
        let fee_bumping_config = &self.network_config.gas_config.fee_bumping;
        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!("Transaction attempt #{} for {}", attempt, context.name());

            // Apply fee bump if this is a retry
            if attempt > 1 && fee_bumping_config.enabled {
                self.apply_fee_bump(&mut tx, attempt);
            }

            // Send transaction
            match self.provider.send_transaction(tx.clone()).await {
                Ok(pending_tx) => {
                    let tx_hash = *pending_tx.tx_hash();
                    info!("Transaction submitted: 0x{:x}", tx_hash);

                    // Wait for confirmation with timeout
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(fee_bumping_config.initial_wait_seconds),
                        pending_tx.get_receipt(),
                    )
                    .await
                    {
                        Ok(Ok(receipt)) => {
                            if receipt.status() {
                                TransactionLogger::log_confirmation(tx_hash, receipt.gas_used());
                                return Ok(receipt);
                            } else {
                                error!("Transaction failed: 0x{:x}", tx_hash);
                                return Err(anyhow::anyhow!("Transaction reverted"));
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to get receipt: {}", e);
                            if attempt >= fee_bumping_config.max_retries as u32 {
                                return Err(e.into());
                            }
                            // Continue to retry
                        }
                        Err(_) => {
                            if attempt >= fee_bumping_config.max_retries as u32 {
                                return Err(anyhow::anyhow!("Transaction confirmation timeout"));
                            }
                            info!("Transaction confirmation timeout, retrying with higher fee");
                            // Continue to retry with fee bump
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to send transaction: {}", e);
                    if attempt >= fee_bumping_config.max_retries as u32 {
                        TransactionLogger::log_failure(
                            context.context_type(),
                            context.name(),
                            &e.to_string(),
                        );
                        return Err(e.into());
                    }
                    // Continue to retry
                }
            }
        }
    }

    /// Apply fee bump to a transaction for retry
    fn apply_fee_bump(&self, tx: &mut N::TransactionRequest, attempt: u32)
    where
        N::TransactionRequest: TransactionBuilder<N>,
    {
        let increase_percent = self
            .network_config
            .gas_config
            .fee_bumping
            .fee_increase_percent;

        match self.network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                // Bump gas price for legacy transactions
                // Note: This is a simplified implementation. In production, you'd want to
                // retrieve the current gas price from the transaction and multiply it.
                if let Some(base_price) = self.network_config.gas_config.gas_price_gwei {
                    let base_wei = gas_utils::gwei_to_wei(base_price);
                    let bumped_wei =
                        gas_utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_gas_price(bumped_wei.to::<u128>());
                    TransactionLogger::log_fee_bump(attempt, base_wei, bumped_wei);
                }
            }
            "eip1559" => {
                // Bump both max fee and priority fee for EIP-1559
                if let Some(base_max_fee) = self.network_config.gas_config.max_fee_per_gas_gwei {
                    let base_wei = gas_utils::gwei_to_wei(base_max_fee);
                    let bumped_wei =
                        gas_utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_max_fee_per_gas(bumped_wei.to::<u128>());

                    TransactionLogger::log_fee_bump(attempt, base_wei, bumped_wei);
                }
                if let Some(base_priority) =
                    self.network_config.gas_config.max_priority_fee_per_gas_gwei
                {
                    let base_wei = gas_utils::gwei_to_wei(base_priority);
                    let bumped_wei =
                        gas_utils::calculate_fee_bump(base_wei, attempt, increase_percent);
                    tx.set_max_priority_fee_per_gas(bumped_wei.to::<u128>());
                }
            }
            _ => {}
        }
    }
}

/// Simplified contract caller for read-only operations
pub struct ContractReader<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    interaction: ContractInteraction<T, N, P>,
}

impl<T, N, P> ContractReader<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    pub fn new(provider: Arc<P>, contract_address: Address, network_name: String) -> Self {
        // Create a minimal network config for read operations
        let network_config = NetworkConfig {
            name: network_name,
            nodes: vec![crate::config::models::NetworkNode {
                name: "Default Node".to_string(),
                rpc_url: String::new(), // Not needed for reads
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            rpc_url: None,
            ws_url: None,
        };

        Self {
            interaction: ContractInteraction::new(provider, contract_address, network_config),
        }
    }

    pub fn with_feed_name(mut self, feed_name: String) -> Self {
        self.interaction = self.interaction.with_feed_name(feed_name);
        self
    }

    pub async fn call<C, R>(
        self,
        call_data: C,
        function_name: &str,
        decode_fn: impl FnOnce(&Bytes) -> Result<R>,
    ) -> Result<R>
    where
        C: Into<Bytes>,
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        self.interaction
            .call_with_metrics(call_data, function_name, decode_fn)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_contract_interaction_builder() {
        // This is a compile-time test to ensure the builder pattern works
        let contract_address = address!("0000000000000000000000000000000000000000");
        let network_config = NetworkConfig::default();

        // Test that we can create and configure a contract interaction
        let _interaction = ContractInteraction::<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >::new(
            Arc::new(alloy::providers::RootProvider::new_http(
                "http://localhost:8545".parse().unwrap(),
            )),
            contract_address,
            network_config,
        )
        .with_feed_name("test_feed".to_string());
    }
}
