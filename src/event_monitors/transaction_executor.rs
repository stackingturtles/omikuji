//! Transaction executor for contract calls from webhook responses

use super::error::{EventMonitorError, Result};
use super::listener::{EventContext, ProcessedEvent};
use super::models::EventMonitor;
use super::webhook_caller::ContractCall;
use crate::config::models::ExecutionLimits;
use crate::database::{DatabasePool, TransactionLogRepository};
use crate::gas::transaction_builder::GasAwareTransactionBuilder;
use crate::gas_price::GasPriceManager;
use crate::metrics::MetricsContext;
use crate::network::NetworkManager;
use crate::utils::{TransactionContext, TransactionHandler, TransactionLogger};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

/// Executes contract calls based on webhook responses
pub struct TransactionExecutor {
    network_manager: Arc<NetworkManager>,
    gas_price_manager: Option<Arc<GasPriceManager>>,
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
    execution_repo: Option<Arc<ContractExecutionRepository>>,
}

/// Repository for tracking contract executions
pub struct ContractExecutionRepository {
    _pool: Arc<DatabasePool>,
}

/// Record of a contract execution
pub struct ContractExecution {
    pub transaction_hash: String,
    pub event_tx_hash: String,
    pub monitor_name: String,
    pub function_signature: String,
    pub gas_used: u128,
    pub value_wei: String,
    pub status: String,
    pub error_message: Option<String>,
}

impl TransactionExecutor {
    /// Create a new transaction executor
    pub fn new(network_manager: Arc<NetworkManager>) -> Self {
        Self {
            network_manager,
            gas_price_manager: None,
            tx_log_repo: None,
            execution_repo: None,
        }
    }

    /// Set the gas price manager
    pub fn with_gas_price_manager(mut self, manager: Arc<GasPriceManager>) -> Self {
        self.gas_price_manager = Some(manager);
        self
    }

    /// Set the transaction log repository
    pub fn with_tx_log_repo(mut self, repo: Arc<TransactionLogRepository>) -> Self {
        self.tx_log_repo = Some(repo);
        self
    }

    /// Set the contract execution repository
    pub fn with_execution_repo(mut self, repo: Arc<ContractExecutionRepository>) -> Self {
        self.execution_repo = Some(repo);
        self
    }

    /// Execute a contract call from webhook response
    pub async fn execute_call(
        &self,
        call: &ContractCall,
        event: &ProcessedEvent,
        context: &EventContext,
        monitor: &EventMonitor,
        execution_limits: &ExecutionLimits,
    ) -> Result<alloy::rpc::types::TransactionReceipt> {
        // Parse and validate target address
        let target_address =
            call.target
                .parse::<Address>()
                .map_err(|e| EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: format!("Invalid target address '{}': {}", call.target, e),
                })?;

        // Validate same-contract restriction
        if target_address != event.contract_address {
            let event_contract = format!("0x{}", hex::encode(event.contract_address));
            return Err(EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!(
                    "Contract address mismatch: Event from {}, Call to {}",
                    event_contract, call.target
                ),
            });
        }

        // Get provider and network config
        let provider = self
            .network_manager
            .get_provider(&context.network)
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Failed to get provider: {}", e),
            })?;
        let network_config = self
            .network_manager
            .get_network(&context.network)
            .await
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Failed to get network config: {}", e),
            })?;

        // Parse and validate value
        let value = if call.value.is_empty() || call.value == "0" {
            U256::ZERO
        } else {
            U256::from_str_radix(&call.value, 10).map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Invalid value '{}': {}", call.value, e),
            })?
        };

        // Check value against limit
        let max_value = U256::from_str_radix(&execution_limits.max_value_wei, 10).map_err(|e| {
            EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Invalid max_value_wei: {}", e),
            }
        })?;

        if value > max_value {
            return Err(EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Value {} exceeds maximum {}", value, max_value),
            });
        }

        // Encode function call
        let call_data = super::abi_decoder::AbiDecoder::encode_function_call(
            &call.function,
            &call.params,
            &monitor.name,
        )?;

        // Create metrics context
        let _metrics_ctx =
            MetricsContext::new(&monitor.name, &context.network).with_method(&call.function);

        // Log submission
        let value_str = if value.is_zero() {
            None
        } else {
            Some(value.to_string())
        };
        TransactionLogger::log_submission(
            "event_monitor",
            &monitor.name,
            &context.network,
            value_str.as_deref(),
        );

        // Build transaction
        let start_time = Instant::now();
        let tx_builder = GasAwareTransactionBuilder::new(
            provider.clone(),
            target_address,
            call_data,
            network_config.clone(),
        )
        .with_value(value);

        // Check gas price limit
        let current_gas_price =
            provider
                .get_gas_price()
                .await
                .map_err(|e| EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: format!("Failed to get gas price: {}", e),
                })?;

        let max_gas_price_wei =
            crate::gas::utils::gwei_to_wei(execution_limits.max_gas_price_gwei as f64);
        if U256::from(current_gas_price) > max_gas_price_wei {
            return Err(EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!(
                    "Gas price {} gwei exceeds maximum {} gwei",
                    crate::gas::utils::wei_to_gwei(U256::from(current_gas_price)),
                    execution_limits.max_gas_price_gwei
                ),
            });
        }

        // Build and submit transaction
        let tx_request = tx_builder
            .build()
            .await
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Failed to build transaction: {}", e),
            })?;

        // Create signer provider
        let signer_provider = self.create_signer_provider(&context.network).await?;

        // Submit transaction
        let pending_tx = signer_provider
            .send_transaction(tx_request)
            .await
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Failed to send transaction: {}", e),
            })?;

        let tx_hash = *pending_tx.tx_hash();
        info!("Transaction submitted: 0x{}", hex::encode(tx_hash));

        // Wait for receipt
        let receipt =
            pending_tx
                .get_receipt()
                .await
                .map_err(|e| EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: format!("Failed to get receipt: {}", e),
                })?;

        // Handle transaction result
        let _submission_time = start_time.elapsed();
        let tx_context = TransactionContext::EventMonitor {
            monitor_name: monitor.name.clone(),
            event_name: event.event_name.clone(),
            function_name: call.function.clone(),
        };

        // Process transaction with handler
        TransactionHandler::new(receipt.clone(), tx_context, context.network.clone())
            .with_gas_price_manager(self.gas_price_manager.as_ref())
            .with_tx_log_repo(self.tx_log_repo.as_ref())
            .with_gas_limit(300_000) // TODO: Get actual gas limit
            .with_transaction_type(network_config.transaction_type.clone())
            .process()
            .await
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Failed to process transaction: {}", e),
            })?;

        // Record execution in database
        if let Some(repo) = &self.execution_repo {
            let execution = ContractExecution {
                transaction_hash: format!("0x{}", hex::encode(tx_hash)),
                event_tx_hash: event.transaction_hash.clone(),
                monitor_name: monitor.name.clone(),
                function_signature: call.function.clone(),
                gas_used: receipt.gas_used,
                value_wei: value.to_string(),
                status: if receipt.status() {
                    "success"
                } else {
                    "failed"
                }
                .to_string(),
                error_message: if !receipt.status() {
                    Some("Transaction reverted".to_string())
                } else {
                    None
                },
            };

            if let Err(e) = repo.save_execution(execution).await {
                error!("Failed to save contract execution: {}", e);
            }
        }

        // Check transaction status
        if !receipt.status() {
            return Err(EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: format!("Transaction reverted: 0x{}", hex::encode(tx_hash)),
            });
        }

        Ok(receipt)
    }

    /// Create a signer provider for transactions
    async fn create_signer_provider(
        &self,
        network_name: &str,
    ) -> Result<
        impl Provider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
                alloy::network::Ethereum,
            > + Clone,
    > {
        use alloy::network::EthereumWallet;
        use alloy::providers::ProviderBuilder;
        use alloy::signers::local::PrivateKeySigner;

        let private_key = self
            .network_manager
            .get_private_key(network_name)
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: network_name.to_string(),
                reason: format!("Failed to get private key: {}", e),
            })?;

        let rpc_url = self
            .network_manager
            .get_rpc_url(network_name)
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: network_name.to_string(),
                reason: format!("Failed to get RPC URL: {}", e),
            })?;

        let signer = private_key.parse::<PrivateKeySigner>().map_err(|e| {
            EventMonitorError::HandlerError {
                monitor: network_name.to_string(),
                reason: format!("Failed to parse private key: {}", e),
            }
        })?;

        let wallet = EthereumWallet::from(signer);

        let url = rpc_url
            .parse::<url::Url>()
            .map_err(|e| EventMonitorError::HandlerError {
                monitor: network_name.to_string(),
                reason: format!("Failed to parse RPC URL: {}", e),
            })?;

        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(url);

        Ok(provider)
    }
}

impl ContractExecutionRepository {
    /// Create a new repository
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { _pool: pool }
    }

    /// Save a contract execution record
    pub async fn save_execution(&self, execution: ContractExecution) -> Result<()> {
        // TODO: Implement database storage
        debug!(
            "Would save execution: tx={}, monitor={}, function={}",
            execution.transaction_hash, execution.monitor_name, execution.function_signature
        );
        Ok(())
    }

    /// Get executions by event transaction hash
    pub async fn get_by_event_tx(&self, event_tx_hash: &str) -> Result<Vec<ContractExecution>> {
        // TODO: Implement database query
        debug!("Would query executions for event tx: {}", event_tx_hash);
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkManager;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_transaction_executor_creation() {
        let networks = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let executor = TransactionExecutor::new(network_manager);

        assert!(executor.gas_price_manager.is_none());
        assert!(executor.tx_log_repo.is_none());
        assert!(executor.execution_repo.is_none());
    }

    #[tokio::test]
    async fn test_with_builders() {
        let networks = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());

        // Mock dependencies
        let gas_manager = Arc::new(GasPriceManager::new(
            Default::default(),
            HashMap::new(),
            None,
        ));
        let pool = DatabasePool::connect_lazy("postgres://user:pass@localhost/test").unwrap();
        let tx_repo = Arc::new(TransactionLogRepository::new(pool.clone()));
        let exec_repo = Arc::new(ContractExecutionRepository::new(Arc::new(pool)));

        let executor = TransactionExecutor::new(network_manager)
            .with_gas_price_manager(gas_manager)
            .with_tx_log_repo(tx_repo)
            .with_execution_repo(exec_repo);

        assert!(executor.gas_price_manager.is_some());
        assert!(executor.tx_log_repo.is_some());
        assert!(executor.execution_repo.is_some());
    }
}
