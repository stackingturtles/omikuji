//! Metrics recording utilities for consistent metrics collection
//!
//! This module provides abstractions for recording metrics across different
//! components, reducing code duplication and ensuring consistent patterns.

use std::time::{Duration, Instant};

use alloy::primitives::U256;
use alloy::rpc::types::TransactionReceipt;
use anyhow::Result;

use crate::metrics::gas_metrics::GasMetrics;
use crate::metrics::{ContractMetrics, FeedMetrics};

/// Context information for metrics recording
#[derive(Debug, Clone)]
pub struct MetricsContext {
    pub feed_name: String,
    pub network: String,
    pub method: Option<String>,
}

impl MetricsContext {
    pub fn new(feed_name: impl Into<String>, network: impl Into<String>) -> Self {
        Self {
            feed_name: feed_name.into(),
            network: network.into(),
            method: None,
        }
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    pub fn feed_name(&self) -> &str {
        &self.feed_name
    }

    pub fn network(&self) -> &str {
        &self.network
    }

    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }
}

/// Builder for timed operations with automatic metrics recording
pub struct TimedOperationRecorder {
    context: MetricsContext,
    start_time: Instant,
    operation_type: OperationType,
}

#[derive(Debug, Clone)]
pub enum OperationType {
    ContractRead { method: String },
    ContractWrite,
    FeedUpdate,
    NetworkOperation,
}

impl TimedOperationRecorder {
    /// Start timing a contract read operation
    pub fn contract_read(context: MetricsContext, method: impl Into<String>) -> Self {
        Self {
            context,
            start_time: Instant::now(),
            operation_type: OperationType::ContractRead {
                method: method.into(),
            },
        }
    }

    /// Start timing a contract write operation
    pub fn contract_write(context: MetricsContext) -> Self {
        Self {
            context,
            start_time: Instant::now(),
            operation_type: OperationType::ContractWrite,
        }
    }

    /// Start timing a feed update operation
    pub fn feed_update(context: MetricsContext) -> Self {
        Self {
            context,
            start_time: Instant::now(),
            operation_type: OperationType::FeedUpdate,
        }
    }

    /// Start timing a network operation
    pub fn network_operation(context: MetricsContext) -> Self {
        Self {
            context,
            start_time: Instant::now(),
            operation_type: OperationType::NetworkOperation,
        }
    }

    /// Record successful completion of the operation
    pub fn record_success(self, additional_data: Option<&str>) {
        let duration = self.start_time.elapsed();

        match &self.operation_type {
            OperationType::ContractRead { method } => {
                ContractMetrics::record_contract_read(
                    &self.context.feed_name,
                    &self.context.network,
                    method,
                    true,
                    duration,
                    None,
                );
            }
            OperationType::ContractWrite => {
                ContractMetrics::record_contract_write(
                    &self.context.feed_name,
                    &self.context.network,
                    true,
                    duration,
                    additional_data,
                );
            }
            OperationType::FeedUpdate => {
                // Feed updates don't have a direct metrics equivalent
                // This could be extended based on specific needs
            }
            OperationType::NetworkOperation => {
                // Network operations could be recorded with NetworkMetrics
                // if such a module exists
            }
        }
    }

    /// Record failed completion of the operation
    pub fn record_failure(self, error: &str) {
        let duration = self.start_time.elapsed();

        match &self.operation_type {
            OperationType::ContractRead { method } => {
                ContractMetrics::record_contract_read(
                    &self.context.feed_name,
                    &self.context.network,
                    method,
                    false,
                    duration,
                    Some(error),
                );
            }
            OperationType::ContractWrite => {
                ContractMetrics::record_contract_write(
                    &self.context.feed_name,
                    &self.context.network,
                    false,
                    duration,
                    None,
                );
            }
            OperationType::FeedUpdate => {
                // Could record feed update failures if needed
            }
            OperationType::NetworkOperation => {
                // Could record network operation failures if needed
            }
        }
    }

    /// Record the operation result based on a Result type
    pub fn record_result<T, E: std::fmt::Display>(
        self,
        result: &Result<T, E>,
        tx_hash: Option<&str>,
    ) {
        match result {
            Ok(_) => self.record_success(tx_hash),
            Err(e) => self.record_failure(&e.to_string()),
        }
    }

    /// Get the elapsed time so far
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get the context
    pub fn context(&self) -> &MetricsContext {
        &self.context
    }
}

/// Fluent interface for recording transaction metrics
pub struct TransactionMetricsRecorder {
    context: MetricsContext,
    tx_type: String,
}

impl TransactionMetricsRecorder {
    pub fn new(context: MetricsContext, tx_type: impl Into<String>) -> Self {
        Self {
            context,
            tx_type: tx_type.into(),
        }
    }

    /// Record a successful transaction with receipt
    pub fn record_success(
        &self,
        receipt: &TransactionReceipt,
        gas_limit: U256,
        submission_time: Option<u64>,
    ) {
        // Record gas metrics
        GasMetrics::record_transaction(
            &self.context.feed_name,
            &self.context.network,
            receipt,
            gas_limit,
            &self.tx_type,
        );

        // Record confirmation time if submission time is provided
        if let Some(sub_time) = submission_time {
            let confirmation_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            ContractMetrics::record_confirmation_time(
                &self.context.feed_name,
                &self.context.network,
                sub_time,
                confirmation_time,
            );
        }
    }

    /// Record a failed transaction
    pub fn record_failure(&self, gas_limit: U256, estimated_gas_price: Option<U256>, error: &str) {
        GasMetrics::record_failed_transaction(
            &self.context.feed_name,
            &self.context.network,
            gas_limit,
            estimated_gas_price,
            &self.tx_type,
            error,
        );
    }

    /// Record a transaction retry
    pub fn record_retry(&self, reason: &str, attempt: u32) {
        ContractMetrics::record_transaction_retry(
            &self.context.feed_name,
            &self.context.network,
            reason,
            attempt,
        );
    }

    /// Record a transaction revert
    pub fn record_revert(&self, reason: &str) {
        ContractMetrics::record_transaction_revert(
            &self.context.feed_name,
            &self.context.network,
            reason,
        );
    }
}

/// Builder for retry operation metrics
pub struct RetryMetricsRecorder {
    context: MetricsContext,
    max_attempts: u32,
    current_attempt: u32,
}

impl RetryMetricsRecorder {
    pub fn new(context: MetricsContext, max_attempts: u32) -> Self {
        Self {
            context,
            max_attempts,
            current_attempt: 0,
        }
    }

    /// Record the start of a retry attempt
    pub fn start_attempt(&mut self) -> u32 {
        self.current_attempt += 1;
        self.current_attempt
    }

    /// Record a retry due to specific reason
    pub fn record_retry(&self, reason: &str) {
        ContractMetrics::record_transaction_retry(
            &self.context.feed_name,
            &self.context.network,
            reason,
            self.current_attempt,
        );
    }

    /// Check if max attempts reached and record if so
    pub fn check_max_attempts_reached(&self) -> bool {
        let reached = self.current_attempt >= self.max_attempts;
        if reached {
            ContractMetrics::record_transaction_retry(
                &self.context.feed_name,
                &self.context.network,
                "max_attempts_reached",
                self.current_attempt,
            );
        }
        reached
    }

    pub fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    pub fn context(&self) -> &MetricsContext {
        &self.context
    }
}

/// Utility for recording feed value updates
pub struct FeedMetricsRecorder {
    context: MetricsContext,
}

impl FeedMetricsRecorder {
    pub fn new(context: MetricsContext) -> Self {
        Self { context }
    }

    /// Record a feed value update
    pub fn record_feed_value(&self, value: f64, timestamp: u64) {
        FeedMetrics::set_feed_value(
            &self.context.feed_name,
            &self.context.network,
            value,
            timestamp,
        );
    }

    /// Record a contract value update
    pub fn record_contract_value(&self, value: f64, round: u64, timestamp: u64) {
        FeedMetrics::set_contract_value(
            &self.context.feed_name,
            &self.context.network,
            value,
            round,
            timestamp,
        );
    }

    /// Update deviation between feed and contract values
    pub fn update_deviation(&self, feed_value: f64, contract_value: f64) {
        FeedMetrics::update_deviation(
            &self.context.feed_name,
            &self.context.network,
            feed_value,
            contract_value,
        );
    }

    /// Record wallet balance
    pub fn record_wallet_balance(&self, address: &str, balance_wei: u128) {
        FeedMetrics::set_wallet_balance(&self.context.network, address, balance_wei);
    }

    pub fn context(&self) -> &MetricsContext {
        &self.context
    }
}

/// Extension trait for adding metrics recording to operations
pub trait MetricsRecordingExt<T> {
    /// Execute the operation with automatic metrics recording
    fn with_metrics(
        self,
        recorder: TimedOperationRecorder,
    ) -> impl std::future::Future<Output = Result<T>> + Send
    where
        Self: std::future::Future<Output = Result<T>> + Send;
}

impl<F, T> MetricsRecordingExt<T> for F
where
    F: std::future::Future<Output = Result<T>> + Send,
{
    async fn with_metrics(self, recorder: TimedOperationRecorder) -> Result<T> {
        let result = self.await;
        recorder.record_result(&result, None);
        result
    }
}

/// Macro for creating metrics context quickly
#[macro_export]
macro_rules! metrics_context {
    ($feed:expr, $network:expr) => {
        $crate::metrics::recorder::MetricsContext::new($feed, $network)
    };
    ($feed:expr, $network:expr, $method:expr) => {
        $crate::metrics::recorder::MetricsContext::new($feed, $network).with_method($method)
    };
}

/// Macro for timing operations with metrics
#[macro_export]
macro_rules! timed_contract_read {
    ($context:expr, $method:expr, $operation:expr) => {{
        let recorder =
            $crate::metrics::recorder::TimedOperationRecorder::contract_read($context, $method);
        let result = $operation.await;
        recorder.record_result(&result, None);
        result
    }};
}

/// Macro for timing contract writes with metrics
#[macro_export]
macro_rules! timed_contract_write {
    ($context:expr, $operation:expr) => {{
        let recorder = $crate::metrics::recorder::TimedOperationRecorder::contract_write($context);
        let result = $operation.await;
        match &result {
            Ok(receipt) => {
                recorder.record_success(Some(&format!("0x{:x}", receipt.transaction_hash)))
            }
            Err(e) => recorder.record_failure(&e.to_string()),
        }
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_context_creation() {
        let context = MetricsContext::new("eth_usd", "ethereum");
        assert_eq!(context.feed_name(), "eth_usd");
        assert_eq!(context.network(), "ethereum");
        assert!(context.method().is_none());

        let context_with_method = context.with_method("latestAnswer");
        assert_eq!(context_with_method.method(), Some("latestAnswer"));
    }

    #[test]
    fn test_timed_operation_recorder_creation() {
        let context = MetricsContext::new("test_feed", "testnet");

        let read_recorder = TimedOperationRecorder::contract_read(context.clone(), "testMethod");
        assert_eq!(read_recorder.context().feed_name(), "test_feed");
        assert!(read_recorder.elapsed() < Duration::from_millis(100));

        let write_recorder = TimedOperationRecorder::contract_write(context);
        assert_eq!(write_recorder.context().network(), "testnet");
    }

    #[test]
    fn test_transaction_metrics_recorder() {
        let context = MetricsContext::new("eth_usd", "ethereum");
        let recorder = TransactionMetricsRecorder::new(context, "eip1559");

        // Test failure recording
        recorder.record_failure(
            U256::from(200000),
            Some(U256::from(30_000_000_000u64)),
            "insufficient funds",
        );

        // Test retry recording
        recorder.record_retry("network_congestion", 2);
        recorder.record_revert("unauthorized caller");
    }

    #[test]
    fn test_retry_metrics_recorder() {
        let context = MetricsContext::new("btc_usd", "bitcoin");
        let mut retry_recorder = RetryMetricsRecorder::new(context, 3);

        assert_eq!(retry_recorder.start_attempt(), 1);
        assert_eq!(retry_recorder.start_attempt(), 2);
        assert_eq!(retry_recorder.current_attempt(), 2);
        assert!(!retry_recorder.check_max_attempts_reached());

        retry_recorder.start_attempt();
        assert!(retry_recorder.check_max_attempts_reached());
    }

    #[test]
    fn test_feed_metrics_recorder() {
        let context = MetricsContext::new("link_usd", "ethereum");
        let recorder = FeedMetricsRecorder::new(context);

        recorder.record_feed_value(25.5, 1700000000);
        recorder.record_contract_value(25.0, 12345, 1700000100);
        recorder.update_deviation(25.5, 25.0);
        recorder.record_wallet_balance(
            "0x1234567890123456789012345678901234567890",
            1000000000000000000,
        );
    }

    #[test]
    fn test_macro_context_creation() {
        let context = metrics_context!("test_feed", "testnet");
        assert_eq!(context.feed_name(), "test_feed");
        assert_eq!(context.network(), "testnet");

        let context_with_method = metrics_context!("test_feed", "testnet", "testMethod");
        assert_eq!(context_with_method.method(), Some("testMethod"));
    }

    #[tokio::test]
    async fn test_metrics_recording_ext() {
        let context = MetricsContext::new("test_feed", "testnet");
        let recorder = TimedOperationRecorder::contract_read(context, "testMethod");

        // Test successful operation
        let success_result: Result<String> = Ok("success".to_string());
        let future = async { success_result };
        let result = future.with_metrics(recorder).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_operation_type_variants() {
        let context = MetricsContext::new("test", "test");

        let read_recorder = TimedOperationRecorder::contract_read(context.clone(), "method");
        match read_recorder.operation_type {
            OperationType::ContractRead { method } => assert_eq!(method, "method"),
            _ => panic!("Wrong operation type"),
        }

        let write_recorder = TimedOperationRecorder::contract_write(context.clone());
        matches!(write_recorder.operation_type, OperationType::ContractWrite);

        let feed_recorder = TimedOperationRecorder::feed_update(context.clone());
        matches!(feed_recorder.operation_type, OperationType::FeedUpdate);

        let network_recorder = TimedOperationRecorder::network_operation(context);
        matches!(
            network_recorder.operation_type,
            OperationType::NetworkOperation
        );
    }

    #[test]
    fn test_retry_metrics_edge_cases() {
        let context = MetricsContext::new("test", "test");

        // Test with max_attempts = 1
        let mut single_retry = RetryMetricsRecorder::new(context.clone(), 1);
        single_retry.start_attempt();
        assert!(single_retry.check_max_attempts_reached());

        // Test with max_attempts = 0 (edge case)
        let no_retry = RetryMetricsRecorder::new(context, 0);
        assert!(no_retry.check_max_attempts_reached()); // Should immediately be at max
    }
}
