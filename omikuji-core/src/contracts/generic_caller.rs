//! Generic contract caller with automatic metrics and error handling
//!
//! This module provides a high-level abstraction for making contract calls,
//! automatically handling metrics recording, error context, and result decoding.

use crate::metrics::ContractMetrics;
use alloy::{
    network::{Network, TransactionBuilder},
    primitives::{Address, Bytes},
    providers::Provider,
    rpc::types::BlockId,
    transports::Transport,
};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error};

/// Generic contract caller that handles metrics and error handling
pub struct MetricsAwareContractCaller<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    provider: Arc<P>,
    contract_address: Address,
    network_name: String,
    feed_name: Option<String>,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_n: std::marker::PhantomData<N>,
}

impl<T, N, P> MetricsAwareContractCaller<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    /// Create a new metrics-aware contract caller
    pub fn new(
        provider: Arc<P>,
        contract_address: Address,
        network_name: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            contract_address,
            network_name: network_name.into(),
            feed_name: None,
            _phantom_t: std::marker::PhantomData,
            _phantom_n: std::marker::PhantomData,
        }
    }

    /// Set the feed name for metrics tracking
    pub fn with_feed_name(mut self, feed_name: impl Into<String>) -> Self {
        self.feed_name = Some(feed_name.into());
        self
    }

    /// Make a contract call with automatic metrics recording
    pub async fn call<F, R>(&self, call_data: Bytes, method_name: &str, decode_fn: F) -> Result<R>
    where
        F: FnOnce(&Bytes) -> Result<R>,
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let start = Instant::now();

        // Build transaction request
        let mut tx = N::TransactionRequest::default();
        tx.set_to(self.contract_address);
        tx.set_input(call_data);

        debug!(
            "Calling {} on contract {} (network: {})",
            method_name, self.contract_address, self.network_name
        );

        // Make the call
        match self.provider.call(&tx).block(BlockId::latest()).await {
            Ok(result) => {
                let duration = start.elapsed();

                // Record success metrics if feed name is provided
                if let Some(ref feed_name) = self.feed_name {
                    ContractMetrics::record_contract_read(
                        feed_name,
                        &self.network_name,
                        method_name,
                        true,
                        duration,
                        None,
                    );
                }

                debug!("Contract call {} succeeded in {:?}", method_name, duration);

                // Decode the result
                decode_fn(&result)
                    .with_context(|| format!("Failed to decode {method_name} response"))
            }
            Err(e) => {
                let duration = start.elapsed();
                let error_msg = format!("{e:?}");

                error!(
                    "Contract call {} failed after {:?}: {}",
                    method_name, duration, error_msg
                );

                // Record failure metrics if feed name is provided
                if let Some(ref feed_name) = self.feed_name {
                    ContractMetrics::record_contract_read(
                        feed_name,
                        &self.network_name,
                        method_name,
                        false,
                        duration,
                        Some(&error_msg),
                    );
                }

                Err(e).with_context(|| format!("Contract call {method_name} failed"))
            }
        }
    }

    /// Make a simple call that returns raw bytes (no decoding)
    pub async fn call_raw(&self, call_data: Bytes, method_name: &str) -> Result<Bytes>
    where
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        self.call(call_data, method_name, |bytes| Ok(bytes.clone()))
            .await
    }
}

/// Builder for creating contract calls with a fluent interface
pub struct ContractCallBuilder<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    caller: MetricsAwareContractCaller<T, N, P>,
    method_name: String,
    call_data: Option<Bytes>,
}

impl<T, N, P> ContractCallBuilder<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    /// Create a new call builder
    pub fn new(
        provider: Arc<P>,
        contract_address: Address,
        network_name: impl Into<String>,
        method_name: impl Into<String>,
    ) -> Self {
        Self {
            caller: MetricsAwareContractCaller::new(provider, contract_address, network_name),
            method_name: method_name.into(),
            call_data: None,
        }
    }

    /// Set the feed name for metrics
    pub fn with_feed_name(mut self, feed_name: impl Into<String>) -> Self {
        self.caller = self.caller.with_feed_name(feed_name);
        self
    }

    /// Set the call data
    pub fn with_data(mut self, data: Bytes) -> Self {
        self.call_data = Some(data);
        self
    }

    /// Execute the call and decode the result
    pub async fn execute<F, R>(self, decode_fn: F) -> Result<R>
    where
        F: FnOnce(&Bytes) -> Result<R>,
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let call_data = self.call_data.context("Call data not set")?;

        self.caller
            .call(call_data, &self.method_name, decode_fn)
            .await
    }

    /// Execute the call and return raw bytes
    pub async fn execute_raw(self) -> Result<Bytes>
    where
        N::TransactionRequest: Default + TransactionBuilder<N>,
    {
        let call_data = self.call_data.context("Call data not set")?;

        self.caller.call_raw(call_data, &self.method_name).await
    }
}

/// Create a standard contract reader for common read operations
pub fn create_contract_reader<T, N, P>(
    provider: Arc<P>,
    contract_address: Address,
    network_name: impl Into<String>,
) -> MetricsAwareContractCaller<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    MetricsAwareContractCaller::new(provider, contract_address, network_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_metrics_aware_caller_creation() {
        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse().unwrap()),
        );
        let address = address!("0000000000000000000000000000000000000000");

        let _caller = MetricsAwareContractCaller::<_, alloy::network::Ethereum, _>::new(
            provider,
            address,
            "test-network",
        )
        .with_feed_name("test-feed");
    }

    #[test]
    fn test_call_builder_creation() {
        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse().unwrap()),
        );
        let address = address!("0000000000000000000000000000000000000000");

        let _builder = ContractCallBuilder::<_, alloy::network::Ethereum, _>::new(
            provider,
            address,
            "test-network",
            "balanceOf",
        )
        .with_feed_name("test-feed")
        .with_data(Bytes::from(vec![0x01, 0x02, 0x03]));
    }
}
