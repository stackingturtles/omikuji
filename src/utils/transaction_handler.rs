use crate::database::transaction_repository::TransactionLogRepository;
use crate::gas_price::GasPriceManager;
use crate::metrics::gas_metrics::GasMetrics;
use crate::metrics::update_metrics::UpdateMetrics;
use alloy::rpc::types::TransactionReceipt;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// Context for the transaction (e.g., "datafeed" or "scheduled_task")
#[derive(Debug, Clone)]
pub enum TransactionContext {
    Datafeed {
        feed_name: String,
    },
    ScheduledTask {
        task_name: String,
    },
    EventMonitor {
        monitor_name: String,
        event_name: String,
        function_name: String,
    },
}

impl TransactionContext {
    pub fn name(&self) -> &str {
        match self {
            TransactionContext::Datafeed { feed_name } => feed_name,
            TransactionContext::ScheduledTask { task_name } => task_name,
            TransactionContext::EventMonitor { monitor_name, .. } => monitor_name,
        }
    }

    pub fn context_type(&self) -> &str {
        match self {
            TransactionContext::Datafeed { .. } => "datafeed",
            TransactionContext::ScheduledTask { .. } => "scheduled_task",
            TransactionContext::EventMonitor { .. } => "event_monitor",
        }
    }
}

/// Handles post-transaction processing including logging, metrics, and cost calculation
pub struct TransactionHandler<'a> {
    receipt: TransactionReceipt,
    context: TransactionContext,
    network: String,
    gas_price_manager: Option<&'a Arc<GasPriceManager>>,
    tx_log_repo: Option<&'a Arc<TransactionLogRepository>>,
    gas_limit: Option<u64>,
    transaction_type: Option<String>,
}

impl<'a> TransactionHandler<'a> {
    pub fn new(receipt: TransactionReceipt, context: TransactionContext, network: String) -> Self {
        Self {
            receipt,
            context,
            network,
            gas_price_manager: None,
            tx_log_repo: None,
            gas_limit: None,
            transaction_type: None,
        }
    }

    pub fn with_gas_price_manager(mut self, manager: Option<&'a Arc<GasPriceManager>>) -> Self {
        self.gas_price_manager = manager;
        self
    }

    pub fn with_tx_log_repo(mut self, repo: Option<&'a Arc<TransactionLogRepository>>) -> Self {
        self.tx_log_repo = repo;
        self
    }

    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }

    pub fn with_transaction_type(mut self, tx_type: String) -> Self {
        self.transaction_type = Some(tx_type);
        self
    }

    pub async fn process(self) -> Result<()> {
        let tx_hash = self.receipt.transaction_hash;
        let gas_used = self.receipt.gas_used;
        let effective_gas_price = self.receipt.effective_gas_price;

        // Log successful transaction
        info!(
            "Successfully submitted {} to contract. Tx hash: 0x{:x}, Gas used: {}",
            self.context.context_type(),
            tx_hash,
            gas_used
        );

        // Record metrics based on context
        match &self.context {
            TransactionContext::Datafeed { feed_name } => {
                // Record successful update attempt
                UpdateMetrics::record_update_attempt(feed_name, &self.network, true);

                // Record update lag if we have timestamp info
                if let Ok(current_time) = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                {
                    UpdateMetrics::record_update_lag(
                        feed_name,
                        &self.network,
                        current_time.saturating_sub(
                            crate::constants::time::FEED_TIMESTAMP_APPROXIMATION_SECS,
                        ),
                        current_time,
                    );
                }

                // Record contract update in metrics
                use crate::metrics::feed_metrics::FeedMetrics;
                FeedMetrics::record_contract_update(feed_name, &self.network);
            }
            TransactionContext::ScheduledTask { .. } => {
                // Scheduled task specific metrics can be added here
            }
            TransactionContext::EventMonitor { .. } => {
                // Event monitor specific metrics can be added here
            }
        }

        // Calculate and record USD cost if gas price manager is available
        if let Some(gas_price_manager) = self.gas_price_manager {
            let tx_hash_str = format!("0x{tx_hash:x}");

            if let Some(gas_cost_usd) = gas_price_manager
                .calculate_usd_cost(
                    &self.network,
                    self.context.name(),
                    &tx_hash_str,
                    gas_used as u64,
                    effective_gas_price,
                )
                .await
            {
                // Record USD cost metrics
                GasMetrics::record_usd_cost(
                    self.context.name(),
                    &self.network,
                    gas_used as u64,
                    effective_gas_price,
                    gas_cost_usd.gas_token_price_usd,
                );

                info!(
                    "Transaction cost: ${:.6} USD (gas: {}, price: {} wei, token: ${:.2})",
                    gas_cost_usd.total_cost_usd,
                    gas_used,
                    effective_gas_price,
                    gas_cost_usd.gas_token_price_usd
                );
            }
        }

        // Record gas metrics if gas_limit is available
        if let Some(gas_limit) = self.gas_limit {
            let tx_type = self.transaction_type.as_deref().unwrap_or("eip1559");
            GasMetrics::record_transaction(
                self.context.name(),
                &self.network,
                &self.receipt,
                alloy::primitives::U256::from(gas_limit),
                tx_type,
            );
        }

        // Log to transaction repository if available
        if let Some(tx_repo) = self.tx_log_repo {
            // Transaction logging can be implemented here if needed
            let _ = tx_repo; // Placeholder to avoid unused warning
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_context() {
        let datafeed_ctx = TransactionContext::Datafeed {
            feed_name: "eth_usd".to_string(),
        };
        assert_eq!(datafeed_ctx.name(), "eth_usd");
        assert_eq!(datafeed_ctx.context_type(), "datafeed");

        let task_ctx = TransactionContext::ScheduledTask {
            task_name: "daily_update".to_string(),
        };
        assert_eq!(task_ctx.name(), "daily_update");
        assert_eq!(task_ctx.context_type(), "scheduled_task");
    }
}
