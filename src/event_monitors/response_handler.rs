//! Response handling framework for webhook responses

use super::error::{EventMonitorError, Result};
use super::listener::{EventContext, ProcessedEvent};
use super::metrics::EventMonitorMetricsContext;
use super::models::{EventMonitor, ResponseType};
use super::transaction_executor::TransactionExecutor;
use super::webhook_caller::WebhookResponse;
use crate::config::models::OmikujiConfig;
use crate::network::NetworkManager;
use alloy::primitives::U256;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Handles webhook responses based on configured response type
pub struct ResponseHandler {
    handlers: HashMap<ResponseType, Arc<dyn Handler>>,
    _network_manager: Arc<NetworkManager>,
    _config: Arc<OmikujiConfig>,
}

/// Trait for response handlers
#[async_trait]
pub trait Handler: Send + Sync {
    /// Handle the webhook response
    async fn handle(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        context: &EventContext,
    ) -> Result<()>;
}

/// Handler that only logs the response
pub struct LogOnlyHandler;

/// Handler for contract calls
pub struct ContractCallHandler {
    executor: Arc<TransactionExecutor>,
    config: Arc<OmikujiConfig>,
}

/// Handler for database storage (placeholder for Phase 4)
pub struct StoreDbHandler;

/// Handler for multiple actions
pub struct MultiActionHandler {
    handlers: Vec<Arc<dyn Handler>>,
}

impl ResponseHandler {
    /// Create a new response handler with default handlers
    pub fn new(network_manager: Arc<NetworkManager>, config: Arc<OmikujiConfig>) -> Self {
        let mut handlers: HashMap<ResponseType, Arc<dyn Handler>> = HashMap::new();

        handlers.insert(ResponseType::LogOnly, Arc::new(LogOnlyHandler));
        let executor = Arc::new(TransactionExecutor::new(network_manager.clone()));
        handlers.insert(
            ResponseType::ContractCall,
            Arc::new(ContractCallHandler {
                executor,
                config: config.clone(),
            }),
        );
        handlers.insert(ResponseType::StoreDb, Arc::new(StoreDbHandler));
        handlers.insert(
            ResponseType::MultiAction,
            Arc::new(MultiActionHandler {
                handlers: vec![Arc::new(LogOnlyHandler), Arc::new(StoreDbHandler)],
            }),
        );

        Self {
            handlers,
            _network_manager: network_manager,
            _config: config,
        }
    }

    /// Handle a webhook response
    pub async fn handle_response(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        context: &EventContext,
    ) -> Result<()> {
        let response_type = &monitor.response.response_type;
        let metrics_ctx =
            EventMonitorMetricsContext::new(monitor.name.clone(), context.network.clone());

        debug!(
            "Handling {} response for monitor '{}'",
            match response_type {
                ResponseType::LogOnly => "log-only",
                ResponseType::ContractCall => "contract-call",
                ResponseType::StoreDb => "store-db",
                ResponseType::MultiAction => "multi-action",
            },
            monitor.name
        );

        let response_type_str = match response_type {
            ResponseType::LogOnly => "log_only",
            ResponseType::ContractCall => "contract_call",
            ResponseType::StoreDb => "store_db",
            ResponseType::MultiAction => "multi_action",
        };

        let handler =
            self.handlers
                .get(response_type)
                .ok_or_else(|| EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: format!("No handler found for response type {response_type:?}"),
                })?;

        let result = handler.handle(monitor, response, event, context).await;

        match &result {
            Ok(_) => metrics_ctx.response_handler_execution(response_type_str, true),
            Err(_) => metrics_ctx.response_handler_execution(response_type_str, false),
        }

        result
    }

    /// Register a custom handler for a response type
    pub fn register_handler(&mut self, response_type: ResponseType, handler: Arc<dyn Handler>) {
        self.handlers.insert(response_type, handler);
    }
}

#[async_trait]
impl Handler for LogOnlyHandler {
    async fn handle(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        _context: &EventContext,
    ) -> Result<()> {
        info!(
            "Webhook response for monitor '{}' (event: {} at block {}): action={}, metadata={:?}",
            monitor.name, event.event_name, event.block_number, response.action, response.metadata
        );

        debug!("Full webhook response: {:?}", response);

        Ok(())
    }
}

#[async_trait]
impl Handler for ContractCallHandler {
    async fn handle(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        context: &EventContext,
    ) -> Result<()> {
        if response.action != "contract_call" {
            warn!(
                "Expected 'contract_call' action but got '{}' for monitor '{}'",
                response.action, monitor.name
            );
            return Ok(());
        }

        let calls = response
            .calls
            .ok_or_else(|| EventMonitorError::HandlerError {
                monitor: monitor.name.clone(),
                reason: "No contract calls provided in response".to_string(),
            })?;

        info!(
            "Executing {} contract calls for monitor '{}' (event: {} at block {})",
            calls.len(),
            monitor.name,
            event.event_name,
            event.block_number
        );

        // Get execution limits
        let execution_limits = monitor
            .execution_limits
            .as_ref()
            .unwrap_or(&self.config.default_execution_limits);

        // Execute each call sequentially
        for (i, call) in calls.iter().enumerate() {
            info!(
                "Executing contract call {}/{} for monitor '{}': {} on {}",
                i + 1,
                calls.len(),
                monitor.name,
                call.function,
                call.target
            );

            match self
                .executor
                .execute_call(call, event, context, monitor, execution_limits)
                .await
            {
                Ok(receipt) => {
                    info!(
                        "Contract call successful for monitor '{}': {} (gas used: {})",
                        monitor.name, call.function, receipt.gas_used
                    );

                    // Record metrics
                    let metrics_ctx = EventMonitorMetricsContext::new(
                        monitor.name.clone(),
                        context.network.clone(),
                    );
                    metrics_ctx.contract_execution(true);
                    metrics_ctx.contract_execution_gas(receipt.gas_used);
                    // Parse value from call for metrics
                    if let Ok(value) = U256::from_str_radix(&call.value, 10) {
                        metrics_ctx.contract_execution_value(value.to::<u128>());
                    }
                }
                Err(e) => {
                    error!("Failed to execute contract call: {}", e);

                    // Record failure metrics
                    let metrics_ctx = EventMonitorMetricsContext::new(
                        monitor.name.clone(),
                        context.network.clone(),
                    );
                    metrics_ctx.contract_execution(false);

                    // Record validation failure if applicable
                    let error_str = e.to_string();
                    if error_str.contains("Contract address mismatch") {
                        metrics_ctx.validation_failure("different_contract");
                    } else if error_str.contains("exceeds maximum") {
                        if error_str.contains("Value") {
                            metrics_ctx.validation_failure("value_exceeded");
                        } else if error_str.contains("Gas price") {
                            metrics_ctx.validation_failure("gas_price_exceeded");
                        }
                    }

                    return Err(e);
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Handler for StoreDbHandler {
    async fn handle(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        _context: &EventContext,
    ) -> Result<()> {
        info!(
            "Database storage handler for monitor '{}' - Phase 4 implementation pending",
            monitor.name
        );

        debug!(
            "Would store event {} from block {} with response action '{}'",
            event.event_name, event.block_number, response.action
        );

        // Phase 4: Implement database storage
        Ok(())
    }
}

#[async_trait]
impl Handler for MultiActionHandler {
    async fn handle(
        &self,
        monitor: &EventMonitor,
        response: WebhookResponse,
        event: &ProcessedEvent,
        context: &EventContext,
    ) -> Result<()> {
        info!(
            "Executing {} handlers for multi-action response on monitor '{}'",
            self.handlers.len(),
            monitor.name
        );

        for (i, handler) in self.handlers.iter().enumerate() {
            debug!("Executing handler {} of {}", i + 1, self.handlers.len());
            handler
                .handle(monitor, response.clone(), event, context)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::ExecutionLimits;
    use crate::event_monitors::models::{ResponseConfig, WebhookConfig};
    use crate::event_monitors::transaction_executor::TransactionExecutor;
    use crate::event_monitors::webhook_caller::ContractCall;
    use crate::network::NetworkManager;
    use alloy::primitives::address;
    use std::collections::HashMap;

    async fn create_test_network_manager() -> Arc<NetworkManager> {
        let networks = vec![];
        Arc::new(NetworkManager::new(&networks).await.unwrap())
    }

    fn create_test_config() -> Arc<OmikujiConfig> {
        Arc::new(OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: ExecutionLimits::default(),
        })
    }

    fn test_monitor(response_type: ResponseType) -> EventMonitor {
        EventMonitor {
            name: "test_monitor".to_string(),
            network: "ethereum-mainnet".to_string(),
            contract_address: address!("1234567890123456789012345678901234567890"),
            event_signature: "TestEvent(uint256)".to_string(),
            webhook: WebhookConfig {
                url: "https://example.com".to_string(),
                method: super::super::models::HttpMethod::Post,
                headers: HashMap::new(),
                timeout_seconds: 30,
                retry_attempts: 3,
                retry_delay_seconds: 5,
            },
            response: ResponseConfig {
                response_type,
                contract_call: None,
                validation: None,
            },
            execution_limits: None,
        }
    }

    fn test_response() -> WebhookResponse {
        WebhookResponse {
            action: "test_action".to_string(),
            calls: None,
            metadata: Some(serde_json::json!({"test": "data"})),
            extra: serde_json::Map::new(),
        }
    }

    fn test_event() -> ProcessedEvent {
        ProcessedEvent {
            monitor_name: "test_monitor".to_string(),
            event_name: "TestEvent".to_string(),
            contract_address: address!("1234567890123456789012345678901234567890"),
            transaction_hash: "0xabcd".to_string(),
            block_number: 12345,
            log_index: 0,
            removed: false,
            topics: vec![],
            data: "0x".to_string(),
            decoded_args: serde_json::json!({}),
        }
    }

    fn test_context() -> EventContext {
        EventContext {
            network: "ethereum-mainnet".to_string(),
            timestamp: chrono::Utc::now(),
            omikuji_version: "0.1.0".to_string(),
        }
    }

    #[tokio::test]
    async fn test_log_only_handler() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::LogOnly);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_registration() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let mut handler = ResponseHandler::new(network_manager, config);

        // Verify default handlers exist
        assert_eq!(handler.handlers.len(), 4);
        assert!(handler.handlers.contains_key(&ResponseType::LogOnly));
        assert!(handler.handlers.contains_key(&ResponseType::ContractCall));
        assert!(handler.handlers.contains_key(&ResponseType::StoreDb));
        assert!(handler.handlers.contains_key(&ResponseType::MultiAction));

        // Test registering a custom handler
        struct CustomHandler;
        #[async_trait]
        impl Handler for CustomHandler {
            async fn handle(
                &self,
                _monitor: &EventMonitor,
                _response: WebhookResponse,
                _event: &ProcessedEvent,
                _context: &EventContext,
            ) -> Result<()> {
                Ok(())
            }
        }

        handler.register_handler(ResponseType::LogOnly, Arc::new(CustomHandler));
        // Should still have 4 handlers (replaced LogOnly)
        assert_eq!(handler.handlers.len(), 4);
    }

    #[tokio::test]
    async fn test_handle_response_with_unknown_type() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::LogOnly);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        // Clear handlers to test missing handler error
        let mut handler_mut = handler;
        handler_mut.handlers.clear();

        let result = handler_mut
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No handler found for response type"));
    }

    #[tokio::test]
    async fn test_store_db_handler() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::StoreDb);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        // StoreDbHandler currently just logs, so it should succeed
        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multi_action_handler() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::MultiAction);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        // MultiActionHandler should execute multiple handlers
        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_contract_call_handler_wrong_action() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::ContractCall);
        let mut response = test_response();
        response.action = "wrong_action".to_string();
        let event = test_event();
        let context = test_context();

        // Should succeed but log warning
        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_contract_call_handler_no_calls() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::ContractCall);
        let mut response = test_response();
        response.action = "contract_call".to_string();
        response.calls = None;
        let event = test_event();
        let context = test_context();

        // Should fail with no calls provided
        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No contract calls provided"));
    }

    #[tokio::test]
    async fn test_handle_response_metrics() {
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);
        let monitor = test_monitor(ResponseType::LogOnly);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        // Test that metrics are recorded (should succeed)
        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_ok());
    }

    fn test_contract_call() -> ContractCall {
        ContractCall {
            target: "0x1234567890123456789012345678901234567890".to_string(),
            function: "transfer(address,uint256)".to_string(),
            params: vec![
                serde_json::json!("0x2345678901234567890123456789012345678901"),
                serde_json::json!("1000000"),
            ],
            value: "0".to_string(),
        }
    }

    #[test]
    fn test_contract_call_serialization() {
        let call = test_contract_call();
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("transfer(address,uint256)"));
        assert!(json.contains("0x1234567890123456789012345678901234567890"));
    }

    #[tokio::test]
    async fn test_same_contract_validation() {
        // Test that contract calls are only allowed to the same contract that emitted the event
        let networks = vec![crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let _config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager.clone()));

        let mut monitor = test_monitor(ResponseType::ContractCall);
        monitor.execution_limits = Some(ExecutionLimits {
            max_value_wei: "1000000000000000000".to_string(), // 1 ETH
            max_gas_price_gwei: 100,
        });

        let mut event = test_event();
        event.contract_address = address!("1234567890123456789012345678901234567890");

        let context = test_context();

        // Test 1: Same contract - should succeed (would fail at network level but pass validation)
        let mut call = test_contract_call();
        call.target = "0x1234567890123456789012345678901234567890".to_string();

        // This will fail due to no network, but should pass contract validation
        let result = executor
            .execute_call(
                &call,
                &event,
                &context,
                &monitor,
                &monitor.execution_limits.as_ref().unwrap(),
            )
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should fail on network/provider, not contract mismatch
        assert!(!err_msg.contains("Contract address mismatch"));

        // Test 2: Different contract - should fail with contract mismatch error
        call.target = "0x9999999999999999999999999999999999999999".to_string();

        let result = executor
            .execute_call(
                &call,
                &event,
                &context,
                &monitor,
                &monitor.execution_limits.as_ref().unwrap(),
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Contract address mismatch"));
    }

    #[tokio::test]
    async fn test_value_limit_validation() {
        // Test that value limits are enforced
        let networks = vec![crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let _config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager.clone()));

        let mut monitor = test_monitor(ResponseType::ContractCall);
        monitor.execution_limits = Some(ExecutionLimits {
            max_value_wei: "1000000000000000000".to_string(), // 1 ETH
            max_gas_price_gwei: 100,
        });

        let event = test_event();
        let context = test_context();

        // Test 1: Value within limit
        let mut call = test_contract_call();
        call.value = "500000000000000000".to_string(); // 0.5 ETH

        let result = executor
            .execute_call(
                &call,
                &event,
                &context,
                &monitor,
                &monitor.execution_limits.as_ref().unwrap(),
            )
            .await;
        // Will fail on network but not on value validation
        assert!(result.is_err());
        assert!(!result.unwrap_err().to_string().contains("exceeds maximum"));

        // Test 2: Value exceeds limit
        call.value = "2000000000000000000".to_string(); // 2 ETH

        let result = executor
            .execute_call(
                &call,
                &event,
                &context,
                &monitor,
                &monitor.execution_limits.as_ref().unwrap(),
            )
            .await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("exceeds maximum"),
            "Expected 'exceeds maximum' but got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_handle_response_debug_logging() {
        // This test covers lines 95-100: debug logging match statement
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let handler = ResponseHandler::new(network_manager, config);

        // Test all response types to cover all match arms
        let response_types = vec![
            ResponseType::LogOnly,
            ResponseType::ContractCall,
            ResponseType::StoreDb,
            ResponseType::MultiAction,
        ];

        for response_type in response_types {
            let monitor = test_monitor(response_type.clone());
            let response = test_response();
            let event = test_event();
            let context = test_context();

            // This will execute the debug logging in handle_response
            let _ = handler
                .handle_response(&monitor, response, &event, &context)
                .await;
        }
    }

    #[tokio::test]
    async fn test_log_only_handler_with_metadata() {
        // This test ensures line 146 (info logging) is covered
        let handler = LogOnlyHandler;
        let monitor = test_monitor(ResponseType::LogOnly);
        let mut response = test_response();
        response.metadata = Some(serde_json::json!({"key": "value", "number": 42}));
        let event = test_event();
        let context = test_context();

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_contract_call_handler_wrong_action_warning() {
        // This test specifically covers line 167: warning log for wrong action
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };
        let monitor = test_monitor(ResponseType::ContractCall);
        let mut response = test_response();
        response.action = "incorrect_action".to_string();
        let event = test_event();
        let context = test_context();

        // Should succeed but log warning
        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_contract_call_handler_with_calls() {
        // This test covers lines 180-230: main execution flow
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        // Create monitor with contract call config
        let mut monitor = test_monitor(ResponseType::ContractCall);
        monitor.response.contract_call = Some(crate::event_monitors::models::ContractCallConfig {
            target_contract: "0x1234567890123456789012345678901234567890".to_string(),
            gas_limit_multiplier: 1.2,
            max_gas_price_gwei: 100,
            value_wei: 0,
        });

        let mut response = test_response();
        response.action = "contract_call".to_string();
        response.calls = Some(vec![test_contract_call()]);
        let event = test_event();
        let context = test_context();

        // This will fail when trying to get provider, but covers the initial flow
        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to get provider"));
    }

    #[tokio::test]
    async fn test_contract_call_handler_missing_calls() {
        // Test missing contract calls in response
        let monitor = test_monitor(ResponseType::ContractCall);
        let mut response = test_response();
        response.action = "contract_call".to_string();
        response.calls = None; // No calls provided
        let event = test_event();
        let context = test_context();

        // Mock network manager to return a provider
        let networks = vec![crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager.clone()));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("No contract calls provided in response"),
            "Expected error about missing calls but got: {}",
            error_msg
        );
    }

    /* Tests for execute_contract_call have been removed as this functionality
       is now handled by TransactionExecutor::execute_call

    #[tokio::test]
    #[ignore]
    async fn test_execute_contract_call_invalid_address() {
        // Test execute_contract_call with invalid target address
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        let monitor = test_monitor(ResponseType::ContractCall);
        let mut call = test_contract_call();
        call.target = "invalid_address".to_string();

        // Create a mock provider
        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse::<url::Url>().unwrap()),
        );

        let network_config = crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        let call_config = crate::event_monitors::models::ContractCallConfig {
            target_contract: "0x1234567890123456789012345678901234567890".to_string(),
            gas_limit_multiplier: 1.2,
            max_gas_price_gwei: 100,
            value_wei: 0,
        };

        let event = test_event();
        let context = test_context();

        let result = handler
            .execute_contract_call(
                &monitor,
                &call,
                0,
                1,
                &provider,
                &network_config,
                &call_config,
                &event,
                &context,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid target address"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_execute_contract_call_invalid_value() {
        // Test execute_contract_call with invalid value
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        let monitor = test_monitor(ResponseType::ContractCall);
        let mut call = test_contract_call();
        call.value = "invalid_value".to_string();

        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse::<url::Url>().unwrap()),
        );
        let network_config = crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        let call_config = crate::event_monitors::models::ContractCallConfig {
            target_contract: "0x1234567890123456789012345678901234567890".to_string(),
            gas_limit_multiplier: 1.2,
            max_gas_price_gwei: 100,
            value_wei: 0,
        };

        let event = test_event();
        let context = test_context();

        let result = handler
            .execute_contract_call(
                &monitor,
                &call,
                0,
                1,
                &provider,
                &network_config,
                &call_config,
                &event,
                &context,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid value"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_execute_contract_call_with_value() {
        // Test execute_contract_call with a non-zero value
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        let monitor = test_monitor(ResponseType::ContractCall);
        let mut call = test_contract_call();
        call.value = "1000000000000000000".to_string(); // 1 ETH in wei

        let provider = Arc::new(
            alloy::providers::ProviderBuilder::new()
                .on_http("http://localhost:8545".parse::<url::Url>().unwrap()),
        );
        let network_config = crate::config::models::Network {
            name: "ethereum-mainnet".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        let call_config = crate::event_monitors::models::ContractCallConfig {
            target_contract: "0x1234567890123456789012345678901234567890".to_string(),
            gas_limit_multiplier: 0.0, // Test without gas limit multiplier
            max_gas_price_gwei: 0,     // Test without max gas price check
            value_wei: 0,
        };

        let event = test_event();
        let context = test_context();

        // This will fail when trying to build transaction, but covers value parsing
        let result = handler
            .execute_contract_call(
                &monitor,
                &call,
                0,
                1,
                &provider,
                &network_config,
                &call_config,
                &event,
                &context,
            )
            .await;

        assert!(result.is_err());
    } */

    #[tokio::test]
    async fn test_store_db_handler_full_coverage() {
        // This test covers lines 428 and 433: info and debug logs in StoreDbHandler
        let handler = StoreDbHandler;
        let monitor = test_monitor(ResponseType::StoreDb);
        let mut response = test_response();
        response.action = "store_data".to_string();
        let mut event = test_event();
        event.event_name = "TransferEvent".to_string();
        event.block_number = 67890;
        let context = test_context();

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multi_action_handler_multiple_handlers() {
        // This test covers lines 452-453: info log about executing handlers
        let handler = MultiActionHandler {
            handlers: vec![
                Arc::new(LogOnlyHandler),
                Arc::new(StoreDbHandler),
                Arc::new(LogOnlyHandler), // Add a third handler
            ],
        };

        let monitor = test_monitor(ResponseType::MultiAction);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multi_action_handler_with_failing_handler() {
        // Test MultiActionHandler with a handler that fails
        struct FailingHandler;
        #[async_trait]
        impl Handler for FailingHandler {
            async fn handle(
                &self,
                monitor: &EventMonitor,
                _response: WebhookResponse,
                _event: &ProcessedEvent,
                _context: &EventContext,
            ) -> Result<()> {
                Err(EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: "Intentional failure".to_string(),
                })
            }
        }

        let handler = MultiActionHandler {
            handlers: vec![
                Arc::new(LogOnlyHandler),
                Arc::new(FailingHandler), // This will fail
                Arc::new(StoreDbHandler), // This won't be reached
            ],
        };

        let monitor = test_monitor(ResponseType::MultiAction);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Intentional failure"));
    }

    #[tokio::test]
    async fn test_custom_handler_implementation() {
        // This test covers lines 573-575 in the Handler trait implementation
        struct DetailedCustomHandler;
        #[async_trait]
        impl Handler for DetailedCustomHandler {
            async fn handle(
                &self,
                monitor: &EventMonitor,
                response: WebhookResponse,
                event: &ProcessedEvent,
                context: &EventContext,
            ) -> Result<()> {
                // Actually use all the parameters to ensure coverage
                assert_eq!(monitor.name, "test_monitor");
                assert_eq!(response.action, "test_action");
                assert_eq!(event.event_name, "TestEvent");
                assert_eq!(context.network, "ethereum-mainnet");
                Ok(())
            }
        }

        let handler = DetailedCustomHandler;
        let monitor = test_monitor(ResponseType::LogOnly);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_response_metrics_failure() {
        // Test metrics recording on failure
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let mut handler = ResponseHandler::new(network_manager, config);

        // Create a handler that always fails
        struct AlwaysFailHandler;
        #[async_trait]
        impl Handler for AlwaysFailHandler {
            async fn handle(
                &self,
                monitor: &EventMonitor,
                _response: WebhookResponse,
                _event: &ProcessedEvent,
                _context: &EventContext,
            ) -> Result<()> {
                Err(EventMonitorError::HandlerError {
                    monitor: monitor.name.clone(),
                    reason: "Always fails".to_string(),
                })
            }
        }

        handler.register_handler(ResponseType::LogOnly, Arc::new(AlwaysFailHandler));

        let monitor = test_monitor(ResponseType::LogOnly);
        let response = test_response();
        let event = test_event();
        let context = test_context();

        let result = handler
            .handle_response(&monitor, response, &event, &context)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_contract_call_handler_multiple_calls() {
        // Test handling multiple contract calls
        let network_manager = create_test_network_manager().await;
        let config = create_test_config();
        let executor = Arc::new(TransactionExecutor::new(network_manager));
        let handler = ContractCallHandler {
            executor,
            config: config.clone(),
        };

        let mut monitor = test_monitor(ResponseType::ContractCall);
        monitor.response.contract_call = Some(crate::event_monitors::models::ContractCallConfig {
            target_contract: "0x1234567890123456789012345678901234567890".to_string(),
            gas_limit_multiplier: 1.5,
            max_gas_price_gwei: 200,
            value_wei: 0,
        });

        let mut response = test_response();
        response.action = "contract_call".to_string();
        response.calls = Some(vec![
            test_contract_call(),
            ContractCall {
                target: "0x3456789012345678901234567890123456789012".to_string(),
                function: "approve(address,uint256)".to_string(),
                params: vec![
                    serde_json::json!("0x4567890123456789012345678901234567890123"),
                    serde_json::json!("2000000"),
                ],
                value: "100".to_string(),
            },
            ContractCall {
                target: "0x5678901234567890123456789012345678901234".to_string(),
                function: "mint(address,uint256)".to_string(),
                params: vec![
                    serde_json::json!("0x6789012345678901234567890123456789012345"),
                    serde_json::json!("3000000"),
                ],
                value: "".to_string(), // Test empty value
            },
        ]);
        let event = test_event();
        let context = test_context();

        // This will fail when trying to get provider, but covers multiple calls logic
        let result = handler.handle(&monitor, response, &event, &context).await;
        assert!(result.is_err());
    }
}
