use alloy::primitives::{TxHash, U256};
use tracing::{debug, error, info, warn};

/// Standard transaction logging utilities
pub struct TransactionLogger;

impl TransactionLogger {
    /// Log transaction submission
    pub fn log_submission(
        context_type: &str,
        context_name: &str,
        network: &str,
        value: Option<&str>,
    ) {
        if let Some(val) = value {
            info!(
                "Submitting {} update for '{}' on {}: {}",
                context_type, context_name, network, val
            );
        } else {
            info!(
                "Submitting {} for '{}' on {}",
                context_type, context_name, network
            );
        }
    }

    /// Log transaction confirmation
    pub fn log_confirmation(tx_hash: TxHash, gas_used: u128) {
        info!(
            "Successfully submitted to contract. Tx hash: 0x{:x}, Gas used: {}",
            tx_hash, gas_used
        );
    }

    /// Log transaction failure
    pub fn log_failure(context_type: &str, context_name: &str, error: &str) {
        error!(
            "Failed to submit {} for '{}': {}",
            context_type, context_name, error
        );
    }

    /// Log fee bumping attempt
    pub fn log_fee_bump(attempt: u32, old_price: U256, new_price: U256) {
        warn!(
            "Attempting fee bump #{}: {} -> {} wei",
            attempt, old_price, new_price
        );
    }

    /// Log when conditions are met for execution
    pub fn log_condition_met(context_type: &str, context_name: &str, condition_desc: &str) {
        info!(
            "{} '{}' condition met: {}. Proceeding with execution.",
            context_type, context_name, condition_desc
        );
    }

    /// Log when conditions are not met
    pub fn log_condition_not_met(context_type: &str, context_name: &str, condition_desc: &str) {
        info!(
            "{} '{}' condition not met: {}. Skipping execution.",
            context_type, context_name, condition_desc
        );
    }

    /// Log when starting execution
    pub fn log_execution_start(context_type: &str, context_name: &str) {
        debug!(
            "=== Starting {} execution: {} ===",
            context_type, context_name
        );
    }

    /// Log when execution completes successfully
    pub fn log_execution_complete(context_type: &str, context_name: &str, tx_hash: TxHash) {
        info!(
            "Successfully executed {} '{}', tx hash: 0x{:x}",
            context_type, context_name, tx_hash
        );
    }
}
