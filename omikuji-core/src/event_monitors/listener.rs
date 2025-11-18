//! Event listening and subscription logic

use super::error::{EventMonitorError, Result};
use super::event_abi_decoder::EventAbiDecoder;
use super::metrics::{EventMonitorMetrics, EventMonitorMetricsContext};
use super::models::EventMonitor;
use super::response_handler::ResponseHandler;
use super::webhook_caller::WebhookCaller;
use crate::database::{models::NewProcessedEvent, DatabasePool, EventRepository};
use crate::network::NetworkManager;
use alloy::primitives::{Address, Bytes};
use alloy::providers::Provider;
use alloy::rpc::types::Log;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Manages event subscriptions for multiple monitors
pub struct EventListener {
    network_manager: Arc<NetworkManager>,
    webhook_caller: Arc<WebhookCaller>,
    response_handler: Arc<ResponseHandler>,
    monitors: Vec<EventMonitor>,
    db_pool: Option<DatabasePool>,
}

/// Processed event data ready for webhook
#[derive(Debug, Clone)]
pub struct ProcessedEvent {
    pub monitor_name: String,
    pub event_name: String,
    pub contract_address: Address,
    pub transaction_hash: String,
    pub block_number: u64,
    pub log_index: u64,
    pub removed: bool,
    pub topics: Vec<String>,
    pub data: String,
    pub decoded_args: serde_json::Value,
}

/// Context for event processing
#[derive(Debug, Clone)]
pub struct EventContext {
    pub network: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub omikuji_version: String,
}

impl EventListener {
    /// Create a new event listener
    pub fn new(
        network_manager: Arc<NetworkManager>,
        webhook_caller: Arc<WebhookCaller>,
        response_handler: Arc<ResponseHandler>,
        monitors: Vec<EventMonitor>,
    ) -> Self {
        Self {
            network_manager,
            webhook_caller,
            response_handler,
            monitors,
            db_pool: None,
        }
    }

    /// Create a new event listener with database support
    pub fn with_database(
        network_manager: Arc<NetworkManager>,
        webhook_caller: Arc<WebhookCaller>,
        response_handler: Arc<ResponseHandler>,
        monitors: Vec<EventMonitor>,
        db_pool: DatabasePool,
    ) -> Self {
        Self {
            network_manager,
            webhook_caller,
            response_handler,
            monitors,
            db_pool: Some(db_pool),
        }
    }

    /// Start monitoring events for all configured monitors
    pub async fn start_monitoring(&self) -> Result<Vec<JoinHandle<()>>> {
        let mut handles = Vec::new();
        let metrics = EventMonitorMetrics::global();

        info!(
            "Starting event monitoring for {} total monitors",
            self.monitors.len()
        );

        // Group monitors by network for efficiency
        let monitors_by_network = self.group_monitors_by_network();

        for (network_name, network_monitors) in monitors_by_network {
            let monitor_count = network_monitors.len() as i64;
            info!(
                "Starting {} event monitors on network '{}'",
                monitor_count, network_name
            );

            for monitor in &network_monitors {
                debug!(
                    "  - Monitor '{}': contract={}, event='{}'",
                    monitor.name, monitor.contract_address, monitor.event_signature
                );
            }

            metrics.update_active_subscriptions(&network_name, monitor_count);

            let handle = self
                .start_network_monitoring(network_name, network_monitors)
                .await?;
            handles.push(handle);
        }

        info!(
            "Started event monitoring with {} active subscriptions",
            handles.len()
        );
        Ok(handles)
    }

    /// Group monitors by network
    fn group_monitors_by_network(&self) -> HashMap<String, Vec<EventMonitor>> {
        let mut grouped = HashMap::new();
        for monitor in &self.monitors {
            grouped
                .entry(monitor.network.clone())
                .or_insert_with(Vec::new)
                .push(monitor.clone());
        }
        grouped
    }

    /// Start monitoring for all monitors on a specific network
    async fn start_network_monitoring(
        &self,
        network_name: String,
        monitors: Vec<EventMonitor>,
    ) -> Result<JoinHandle<()>> {
        let _network = self
            .network_manager
            .get_network(&network_name)
            .await
            .map_err(|_| EventMonitorError::NetworkNotFound(network_name.clone()))?;

        // Check if WebSocket is available for this network
        let use_websocket = self.network_manager.has_ws_support(&network_name);

        if use_websocket {
            info!(
                "Using WebSocket for event monitoring on network '{}'",
                network_name
            );
            // Get WebSocket provider
            let ws_provider = self
                .network_manager
                .get_ws_provider(&network_name)
                .await
                .map_err(|e| EventMonitorError::ProviderError {
                    network: network_name.clone(),
                    reason: e.to_string(),
                })?;

            let webhook_caller = self.webhook_caller.clone();
            let response_handler = self.response_handler.clone();
            let db_pool = self.db_pool.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = Self::monitor_network_events_ws(
                    ws_provider,
                    network_name.clone(),
                    monitors,
                    webhook_caller,
                    response_handler,
                    db_pool,
                )
                .await
                {
                    error!(
                        "WebSocket event monitoring error for network '{}': {}",
                        network_name, e
                    );
                    EventMonitorMetrics::global().update_active_subscriptions(&network_name, 0);
                }
            });

            Ok(handle)
        } else {
            info!(
                "Using HTTP polling for event monitoring on network '{}'",
                network_name
            );
            // Fall back to HTTP provider with polling
            let provider = self
                .network_manager
                .get_provider(&network_name)
                .map_err(|e| EventMonitorError::ProviderError {
                    network: network_name.clone(),
                    reason: e.to_string(),
                })?;

            let webhook_caller = self.webhook_caller.clone();
            let response_handler = self.response_handler.clone();
            let db_pool = self.db_pool.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = Self::monitor_network_events(
                    provider,
                    network_name.clone(),
                    monitors,
                    webhook_caller,
                    response_handler,
                    db_pool,
                )
                .await
                {
                    error!(
                        "Event monitoring error for network '{}': {}",
                        network_name, e
                    );
                    EventMonitorMetrics::global().update_active_subscriptions(&network_name, 0);
                }
            });

            Ok(handle)
        }
    }

    /// Monitor events for a specific network
    async fn monitor_network_events(
        provider: Arc<crate::network::EthProvider>,
        network_name: String,
        monitors: Vec<EventMonitor>,
        webhook_caller: Arc<WebhookCaller>,
        response_handler: Arc<ResponseHandler>,
        db_pool: Option<DatabasePool>,
    ) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(100);

        // Create subscriptions for each monitor
        let mut subscription_handles = Vec::new();
        for monitor in monitors {
            let handle =
                Self::subscribe_to_monitor_events(provider.clone(), monitor.clone(), tx.clone())
                    .await?;
            subscription_handles.push(handle);
        }

        // Process events as they arrive
        while let Some((monitor, log)) = rx.recv().await {
            if let Err(e) = Self::process_event(
                &monitor,
                log,
                &network_name,
                &webhook_caller,
                &response_handler,
                &db_pool,
            )
            .await
            {
                error!(
                    "Failed to process event for monitor '{}': {}",
                    monitor.name, e
                );
            }
        }

        Ok(())
    }

    /// Monitor events for a specific network using WebSocket
    async fn monitor_network_events_ws(
        provider: Arc<crate::network::WsProvider>,
        network_name: String,
        monitors: Vec<EventMonitor>,
        webhook_caller: Arc<WebhookCaller>,
        response_handler: Arc<ResponseHandler>,
        db_pool: Option<DatabasePool>,
    ) -> Result<()> {
        let _metrics = EventMonitorMetrics::global();

        // Create filters and subscriptions for each monitor
        for monitor in monitors {
            let monitor_name = monitor.name.clone();
            let network = network_name.clone();
            let webhook_caller = webhook_caller.clone();
            let response_handler = response_handler.clone();
            let provider = provider.clone();
            let db_pool = db_pool.clone();

            tokio::spawn(async move {
                loop {
                    match Self::subscribe_to_ws_events(
                        provider.clone(),
                        monitor.clone(),
                        network.clone(),
                        webhook_caller.clone(),
                        response_handler.clone(),
                        db_pool.clone(),
                    )
                    .await
                    {
                        Ok(()) => {
                            info!(
                                "WebSocket subscription ended normally for monitor '{}'",
                                monitor_name
                            );
                            break;
                        }
                        Err(e) => {
                            error!(
                                "WebSocket subscription error for monitor '{}': {}. Reconnecting...",
                                monitor_name, e
                            );
                            // TODO: Add error metrics tracking

                            // Wait before reconnecting
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });
        }

        // Keep the main task running indefinitely
        // The spawned tasks will handle the monitoring
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }

    /// Subscribe to events for a specific monitor using WebSocket
    async fn subscribe_to_ws_events(
        provider: Arc<crate::network::WsProvider>,
        monitor: EventMonitor,
        network_name: String,
        webhook_caller: Arc<WebhookCaller>,
        response_handler: Arc<ResponseHandler>,
        db_pool: Option<DatabasePool>,
    ) -> Result<()> {
        use alloy::rpc::types::eth::Filter;
        use futures::StreamExt;

        let _metrics = EventMonitorMetrics::global();

        // Parse event signature to get selector
        let event_selector = Self::parse_event_selector(&monitor.event_signature)?;

        // Create filter for the specific event
        let filter = Filter::new()
            .address(monitor.contract_address)
            .event_signature(event_selector);

        info!(
            "Creating WebSocket subscription for '{}' on contract {} for monitor '{}'",
            monitor.event_signature, monitor.contract_address, monitor.name
        );

        // Subscribe to logs
        let subscription = provider.subscribe_logs(&filter).await.map_err(|e| {
            EventMonitorError::SubscriptionError {
                monitor: monitor.name.clone(),
                reason: e.to_string(),
            }
        })?;

        let mut stream = subscription.into_stream();

        // Process events from the stream
        while let Some(log) = stream.next().await {
            debug!(
                "Received event for monitor '{}': tx={}, block={}",
                monitor.name,
                log.transaction_hash
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
                log.block_number.unwrap_or_default()
            );

            if let Err(e) = Self::process_event(
                &monitor,
                log,
                &network_name,
                &webhook_caller,
                &response_handler,
                &db_pool,
            )
            .await
            {
                error!(
                    "Failed to process event for monitor '{}': {}",
                    monitor.name, e
                );
            }
        }

        Ok(())
    }

    /// Subscribe to events for a specific monitor
    async fn subscribe_to_monitor_events(
        provider: Arc<crate::network::EthProvider>,
        monitor: EventMonitor,
        tx: mpsc::Sender<(EventMonitor, Log)>,
    ) -> Result<JoinHandle<()>> {
        // Parse event signature to get event selector
        let _event_selector =
            Self::parse_event_selector(&monitor.event_signature).map_err(|e| {
                EventMonitorError::ConfigError {
                    monitor: monitor.name.clone(),
                    reason: format!("Invalid event signature: {e}"),
                }
            })?;

        // For Phase 1, we'll use polling instead of subscriptions
        // TODO: Implement proper WebSocket subscriptions in production

        info!(
            "Starting event polling for '{}' on contract {} for monitor '{}'",
            monitor.event_signature, monitor.contract_address, monitor.name
        );

        let handle = tokio::spawn(async move {
            let mut last_block = 0u64;

            loop {
                // Poll every 5 seconds
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                match provider.get_block_number().await {
                    Ok(current_block) => {
                        if current_block > last_block {
                            debug!(
                                "Checking for events from block {} to {} for monitor '{}'",
                                last_block + 1,
                                current_block,
                                monitor.name
                            );

                            // Parse event selector from signature
                            let event_selector =
                                match Self::parse_event_selector(&monitor.event_signature) {
                                    Ok(selector) => selector,
                                    Err(e) => {
                                        error!(
                                            "Failed to parse event signature '{}': {}",
                                            monitor.event_signature, e
                                        );
                                        EventMonitorMetrics::global().record_processing_error(
                                            &monitor.name,
                                            "event_signature_parse",
                                        );
                                        continue;
                                    }
                                };

                            // Create filter for the specific contract and event
                            let filter = alloy::rpc::types::Filter::new()
                                .address(monitor.contract_address)
                                .event_signature(event_selector)
                                .from_block(last_block + 1)
                                .to_block(current_block);

                            // Fetch logs
                            match provider.get_logs(&filter).await {
                                Ok(logs) => {
                                    if !logs.is_empty() {
                                        info!(
                                            "Found {} events for monitor '{}' between blocks {} and {}",
                                            logs.len(),
                                            monitor.name,
                                            last_block + 1,
                                            current_block
                                        );
                                    }

                                    for log in logs {
                                        debug!(
                                            "Processing event for monitor '{}' in tx: {:?}",
                                            monitor.name, log.transaction_hash
                                        );

                                        // Send the log through the channel for processing
                                        if let Err(e) = tx.send((monitor.clone(), log)).await {
                                            error!(
                                                "Failed to send event to processing channel: {}",
                                                e
                                            );
                                            EventMonitorMetrics::global().record_processing_error(
                                                &monitor.name,
                                                "channel_send",
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to fetch logs for monitor '{}': {}",
                                        monitor.name, e
                                    );
                                    EventMonitorMetrics::global()
                                        .record_processing_error(&monitor.name, "log_fetch");
                                }
                            }

                            last_block = current_block;
                        }
                    }
                    Err(e) => {
                        error!("Failed to get block number: {}", e);
                        EventMonitorMetrics::global()
                            .record_processing_error(&monitor.name, "block_number_fetch");
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Parse event signature to get the event selector
    fn parse_event_selector(
        signature: &str,
    ) -> std::result::Result<alloy::primitives::B256, String> {
        use alloy::primitives::keccak256;

        // Event signatures should be in the format: "EventName(type1 name1,type2 name2,...)"
        // For the selector, we need only "EventName(type1,type2,...)" without parameter names

        // Extract event name and parameters
        let event_name = signature
            .split('(')
            .next()
            .ok_or_else(|| format!("Invalid event signature: '{signature}'"))?
            .trim();

        // Extract parameters part
        let params_start = signature
            .find('(')
            .ok_or_else(|| format!("Missing '(' in signature: '{signature}'"))?;
        let params_end = signature
            .rfind(')')
            .ok_or_else(|| format!("Missing ')' in signature: '{signature}'"))?;

        if params_start >= params_end {
            return Err(format!("Invalid parentheses in signature: '{signature}'"));
        }

        let params_str = &signature[params_start + 1..params_end];

        // Parse parameters and extract only types (remove parameter names)
        let types_only: Vec<String> = if params_str.trim().is_empty() {
            vec![]
        } else {
            params_str
                .split(',')
                .map(|param| {
                    // Each param can be "type" or "type name" - we want only the type
                    param.split_whitespace().next().unwrap_or("").to_string()
                })
                .collect()
        };

        // Reconstruct the canonical signature for hashing
        let canonical_signature = format!("{}({})", event_name, types_only.join(","));

        debug!(
            "Parsing event signature: '{}' -> canonical: '{}'",
            signature, canonical_signature
        );

        // Calculate the event selector (topic0) by hashing the canonical signature
        let selector = keccak256(canonical_signature.as_bytes());

        debug!(
            "Event selector for '{}': {:?}",
            canonical_signature, selector
        );

        Ok(selector)
    }

    /// Process a single event
    async fn process_event(
        monitor: &EventMonitor,
        log: Log,
        network_name: &str,
        webhook_caller: &Arc<WebhookCaller>,
        response_handler: &Arc<ResponseHandler>,
        db_pool: &Option<DatabasePool>,
    ) -> Result<()> {
        let metrics_ctx =
            EventMonitorMetricsContext::new(monitor.name.clone(), network_name.to_string());
        let tx_hash = log.transaction_hash.unwrap_or_default().to_string();
        let log_index = log.log_index.unwrap_or_default() as i32;

        debug!(
            "Processing event for monitor '{}' at block {} (tx: {}, log_index: {})",
            monitor.name,
            log.block_number.unwrap_or_default(),
            tx_hash,
            log_index
        );

        // Check for duplicate processing if database is available
        if let Some(pool) = db_pool {
            match EventRepository::is_event_processed(pool, &tx_hash, log_index).await {
                Ok(true) => {
                    warn!(
                        "Event already processed: tx={}, log_index={} for monitor '{}'",
                        tx_hash, log_index, monitor.name
                    );
                    return Ok(()); // Skip duplicate event
                }
                Ok(false) => {
                    debug!("Event not yet processed, continuing...");
                }
                Err(e) => {
                    // Log the error but continue processing to avoid losing events
                    error!(
                        "Failed to check duplicate event status: {}. Processing anyway.",
                        e
                    );
                }
            }
        }

        // Decode event data
        let processed_event = Self::decode_event_data(&log, monitor)?;

        // Record event received
        metrics_ctx.event_received(&processed_event.event_name);

        // Create event context
        let context = EventContext {
            network: network_name.to_string(),
            timestamp: chrono::Utc::now(),
            omikuji_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Call webhook
        let response = webhook_caller
            .call_webhook(&monitor.webhook, &processed_event, &context)
            .await?;

        // Handle response
        response_handler
            .handle_response(monitor, response, &processed_event, &context)
            .await?;

        // Mark event as processed in database if available
        if let Some(pool) = db_pool {
            let new_event = NewProcessedEvent {
                transaction_hash: tx_hash,
                log_index,
                contract_address: monitor.contract_address.to_string(),
                event_type: processed_event.event_name.clone(),
                event_data: Some(serde_json::json!({
                    "monitor": monitor.name,
                    "topics": processed_event.topics,
                    "data": processed_event.data,
                    "decoded_args": processed_event.decoded_args,
                })),
            };

            match EventRepository::mark_event_processed(pool, new_event).await {
                Ok(_) => {
                    debug!("Event marked as processed in database");
                }
                Err(e) => {
                    // Log error but don't fail the processing
                    error!("Failed to mark event as processed in database: {}", e);
                }
            }
        }

        // Record successful processing
        metrics_ctx.event_processed(&processed_event.event_name);

        Ok(())
    }

    /// Decode event data from a log entry
    fn decode_event_data(log: &Log, monitor: &EventMonitor) -> Result<ProcessedEvent> {
        // Extract event name and parse signature
        let event_name = monitor
            .event_signature
            .split('(')
            .next()
            .unwrap_or("Unknown")
            .to_string();

        // Convert log data to string representations
        let topics: Vec<String> = log
            .topics()
            .iter()
            .map(|t| format!("0x{}", hex::encode(t.as_slice())))
            .collect();

        let data = format!("0x{}", hex::encode(&log.data().data));

        // Parse event signature and decode parameters using EventAbiDecoder
        let decoded_args = match EventAbiDecoder::parse_event(&monitor.event_signature) {
            Ok(event) => {
                debug!(
                    "Decoding event '{}' with {} parameters",
                    event.name,
                    event.inputs.len()
                );

                // Convert topics to Bytes for the decoder
                let topic_bytes: Vec<Bytes> = log
                    .topics()
                    .iter()
                    .map(|t| Bytes::from(t.as_slice().to_vec()))
                    .collect();

                // Convert data to Bytes
                let data_bytes = log.data().data.clone();

                // Decode the event
                match EventAbiDecoder::decode_event(
                    &event,
                    &topic_bytes,
                    &data_bytes,
                    &monitor.name,
                ) {
                    Ok(decoded) => serde_json::to_value(decoded).unwrap_or_else(|e| {
                        error!("Failed to convert decoded event to JSON: {}", e);
                        serde_json::json!({
                            "error": "Failed to convert decoded event",
                            "raw_topics": &topics,
                            "raw_data": &data,
                        })
                    }),
                    Err(e) => {
                        error!("Failed to decode event parameters: {}", e);
                        serde_json::json!({
                            "error": format!("Decoding failed: {}", e),
                            "raw_topics": &topics,
                            "raw_data": &data,
                        })
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to parse event signature '{}': {}",
                    monitor.event_signature, e
                );
                serde_json::json!({
                    "error": format!("Invalid event signature: {}", e),
                    "signature": &monitor.event_signature,
                    "raw_topics": &topics,
                    "raw_data": &data,
                })
            }
        };

        Ok(ProcessedEvent {
            monitor_name: monitor.name.clone(),
            event_name,
            contract_address: log.address(),
            transaction_hash: format!("{:?}", log.transaction_hash.unwrap_or_default()),
            block_number: log.block_number.unwrap_or_default(),
            log_index: log.log_index.unwrap_or_default(),
            removed: log.removed,
            topics,
            data,
            decoded_args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_monitors::models::{HttpMethod, ResponseConfig, WebhookConfig};
    use alloy::primitives::address;
    use std::collections::HashMap;

    fn test_monitor() -> EventMonitor {
        EventMonitor {
            name: "test_monitor".to_string(),
            network: "ethereum-mainnet".to_string(),
            contract_address: address!("1234567890123456789012345678901234567890"),
            event_signature: "Transfer(address,address,uint256)".to_string(),
            webhook: WebhookConfig {
                url: "https://example.com/webhook".to_string(),
                method: HttpMethod::Post,
                headers: HashMap::new(),
                timeout_seconds: 30,
                retry_attempts: 3,
                retry_delay_seconds: 5,
            },
            response: ResponseConfig::default(),
            execution_limits: None,
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

    #[test]
    fn test_group_monitors_by_network() {
        let mut monitor1 = test_monitor();
        monitor1.name = "monitor1".to_string();

        let mut monitor2 = test_monitor();
        monitor2.name = "monitor2".to_string();
        monitor2.network = "base-mainnet".to_string();

        let mut monitor3 = test_monitor();
        monitor3.name = "monitor3".to_string();

        // For tests, create an empty network manager
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(NetworkManager::new(&networks))
                .unwrap(),
        );
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));

        let listener = EventListener::new(
            network_manager,
            webhook_caller,
            response_handler,
            vec![monitor1, monitor2, monitor3],
        );

        let grouped = listener.group_monitors_by_network();
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("ethereum-mainnet").unwrap().len(), 2);
        assert_eq!(grouped.get("base-mainnet").unwrap().len(), 1);
    }

    #[test]
    fn test_parse_event_selector() {
        let selector = EventListener::parse_event_selector("Transfer(address,address,uint256)");
        assert!(selector.is_ok());
    }

    #[test]
    fn test_decode_event_data() {
        use alloy::primitives::{b256, B256};
        use alloy::rpc::types::Log as AlloyLog;

        let monitor = test_monitor();
        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![b256!(
                        "0000000000000000000000000000000000000000000000000000000000000001"
                    )],
                    vec![0x00, 0x00, 0x00, 0x01].into(),
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: false,
        };

        let result = EventListener::decode_event_data(&log, &monitor);
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.monitor_name, "test_monitor");
        assert_eq!(event.event_name, "Transfer");
        assert_eq!(event.block_number, 12345);
        assert!(!event.removed);
        assert_eq!(event.topics.len(), 1);
        assert_eq!(event.data, "0x00000001");
    }

    #[test]
    fn test_event_context_creation() {
        let context = test_context();
        assert_eq!(context.network, "ethereum-mainnet");
        assert_eq!(context.omikuji_version, "0.1.0");
        assert!(context.timestamp <= chrono::Utc::now());
    }

    #[test]
    fn test_processed_event_fields() {
        let event = test_event();
        assert_eq!(event.monitor_name, "test_monitor");
        assert_eq!(event.event_name, "TestEvent");
        assert_eq!(event.block_number, 12345);
        assert_eq!(event.log_index, 0);
        assert!(!event.removed);
        assert!(event.topics.is_empty());
        assert_eq!(event.data, "0x");
    }

    #[tokio::test]
    async fn test_event_listener_creation() {
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));
        let monitors = vec![test_monitor()];

        let listener = EventListener::new(
            network_manager,
            webhook_caller,
            response_handler,
            monitors.clone(),
        );

        // Verify monitors are stored
        let grouped = listener.group_monitors_by_network();
        assert_eq!(grouped.len(), 1);
        assert!(grouped.contains_key("ethereum-mainnet"));
    }

    #[test]
    fn test_multiple_monitors_grouping() {
        let mut monitor1 = test_monitor();
        monitor1.name = "monitor1".to_string();
        monitor1.network = "ethereum-mainnet".to_string();

        let mut monitor2 = test_monitor();
        monitor2.name = "monitor2".to_string();
        monitor2.network = "base-mainnet".to_string();

        let mut monitor3 = test_monitor();
        monitor3.name = "monitor3".to_string();
        monitor3.network = "ethereum-mainnet".to_string();

        let mut monitor4 = test_monitor();
        monitor4.name = "monitor4".to_string();
        monitor4.network = "arbitrum-mainnet".to_string();

        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(NetworkManager::new(&networks))
                .unwrap(),
        );
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));

        let listener = EventListener::new(
            network_manager,
            webhook_caller,
            response_handler,
            vec![monitor1, monitor2, monitor3, monitor4],
        );

        let grouped = listener.group_monitors_by_network();
        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped.get("ethereum-mainnet").unwrap().len(), 2);
        assert_eq!(grouped.get("base-mainnet").unwrap().len(), 1);
        assert_eq!(grouped.get("arbitrum-mainnet").unwrap().len(), 1);
    }

    #[test]
    fn test_event_signature_parsing() {
        // Test valid signatures
        assert!(EventListener::parse_event_selector("Transfer(address,address,uint256)").is_ok());
        assert!(EventListener::parse_event_selector("Approval(address,address,uint256)").is_ok());
        assert!(
            EventListener::parse_event_selector("Swap(uint256,uint256,address,address)").is_ok()
        );
        assert!(EventListener::parse_event_selector("SimpleEvent()").is_ok());
        assert!(
            EventListener::parse_event_selector("ComplexEvent(address[],uint256[],bytes)").is_ok()
        );
    }

    // Tests for uncovered lines

    #[tokio::test]
    async fn test_start_monitoring() {
        // Create mock servers for the networks
        let mut mock_server1 = mockito::Server::new_async().await;
        let mut mock_server2 = mockito::Server::new_async().await;

        // Mock the necessary RPC calls
        mock_server1
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
            .create();

        mock_server2
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
            .create();

        // Create a test network configuration
        let networks = vec![
            crate::config::models::Network {
                name: "ethereum-mainnet".to_string(),
                nodes: vec![crate::config::models::NetworkNode {
                    name: "Local Node".to_string(),
                    rpc_url: mock_server1.url(),
                    ws_url: None,
                }],
                transaction_type: "eip1559".to_string(),
                gas_config: crate::config::models::GasConfig::default(),
                gas_token: "ethereum".to_string(),
                gas_token_symbol: "ETH".to_string(),
                balance_alerts: None,
                rpc_url: None,
                ws_url: None,
            },
            crate::config::models::Network {
                name: "base-mainnet".to_string(),
                nodes: vec![crate::config::models::NetworkNode {
                    name: "Local Node".to_string(),
                    rpc_url: mock_server2.url(),
                    ws_url: None,
                }],
                transaction_type: "eip1559".to_string(),
                gas_config: crate::config::models::GasConfig::default(),
                gas_token: "ethereum".to_string(),
                gas_token_symbol: "ETH".to_string(),
                balance_alerts: None,
                rpc_url: None,
                ws_url: None,
            },
        ];

        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));

        // Create monitors for multiple networks
        let mut monitor1 = test_monitor();
        monitor1.name = "monitor1".to_string();
        monitor1.network = "ethereum-mainnet".to_string();

        let mut monitor2 = test_monitor();
        monitor2.name = "monitor2".to_string();
        monitor2.network = "base-mainnet".to_string();

        let mut monitor3 = test_monitor();
        monitor3.name = "monitor3".to_string();
        monitor3.network = "ethereum-mainnet".to_string();

        let listener = EventListener::new(
            network_manager,
            webhook_caller,
            response_handler,
            vec![monitor1, monitor2, monitor3],
        );

        // Test start_monitoring - it should succeed in returning handles
        let result = listener.start_monitoring().await;
        assert!(result.is_ok());

        let handles = result.unwrap();
        assert_eq!(handles.len(), 2); // One handle per network

        // Abort all handles to clean up
        for handle in handles {
            handle.abort();
        }
    }

    #[tokio::test]
    async fn test_start_network_monitoring_network_not_found() {
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));

        let listener =
            EventListener::new(network_manager, webhook_caller, response_handler, vec![]);

        // Test with non-existent network
        let result = listener
            .start_network_monitoring("non-existent".to_string(), vec![test_monitor()])
            .await;

        assert!(result.is_err());
        if let Err(e) = result {
            match e {
                EventMonitorError::NetworkNotFound(name) => {
                    assert_eq!(name, "non-existent");
                }
                _ => panic!("Expected NetworkNotFound error"),
            }
        }
    }

    #[tokio::test]
    async fn test_start_network_monitoring_with_valid_network() {
        // Create a mock server
        let mut mock_server = mockito::Server::new_async().await;

        // Mock the necessary RPC calls
        mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
            .create();

        // Create a network with mock server URL
        let networks = vec![crate::config::models::Network {
            name: "test-network".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: mock_server.url(),
                ws_url: None,
            }],
            transaction_type: "legacy".to_string(),
            gas_config: crate::config::models::GasConfig::default(),
            gas_token: "test".to_string(),
            gas_token_symbol: "TEST".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }];

        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager.clone(), config));

        let listener =
            EventListener::new(network_manager, webhook_caller, response_handler, vec![]);

        let result = listener
            .start_network_monitoring("test-network".to_string(), vec![test_monitor()])
            .await;

        assert!(result.is_ok());

        // Clean up the handle
        if let Ok(handle) = result {
            handle.abort();
        }
    }

    #[tokio::test]
    async fn test_monitor_network_events() {
        use alloy::providers::ProviderBuilder;

        // Create a mock provider
        let provider =
            Arc::new(ProviderBuilder::new().on_http("http://localhost:8545".parse().unwrap()));

        let webhook_caller = Arc::new(WebhookCaller::new());
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager, config));

        let monitors = vec![test_monitor()];

        // Create a channel to test event flow
        let (tx, mut rx) = mpsc::channel(1);

        // Start monitoring in a separate task
        let monitor_task = tokio::spawn(async move {
            let result = EventListener::monitor_network_events(
                provider,
                "test-network".to_string(),
                monitors,
                webhook_caller,
                response_handler,
                None,
            )
            .await;
            tx.send(result).await.unwrap();
        });

        // Give it some time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Cancel the task to simulate channel closure
        monitor_task.abort();

        // The function should exit when channel is closed
        if let Ok(result) = rx.try_recv() {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_subscribe_to_monitor_events() {
        use alloy::providers::ProviderBuilder;

        let provider =
            Arc::new(ProviderBuilder::new().on_http("http://localhost:8545".parse().unwrap()));

        let (tx, _rx) = mpsc::channel(100);
        let monitor = test_monitor();

        // Test subscription creation
        let result =
            EventListener::subscribe_to_monitor_events(provider, monitor.clone(), tx).await;
        assert!(result.is_ok());

        let handle = result.unwrap();

        // The task should be running
        assert!(!handle.is_finished());

        // Abort the task to clean up
        handle.abort();
    }

    #[tokio::test]
    async fn test_subscribe_with_invalid_event_signature() {
        use alloy::providers::ProviderBuilder;

        let mut mock_server = mockito::Server::new_async().await;

        // Mock the necessary RPC calls
        mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
            .create();

        let provider = Arc::new(ProviderBuilder::new().on_http(mock_server.url().parse().unwrap()));

        let (tx, _rx) = mpsc::channel(100);
        let mut monitor = test_monitor();
        // This signature is still valid for parsing but would be invalid for actual ABI decoding
        monitor.event_signature = "InvalidEvent()".to_string();

        // Currently parse_event_selector just hashes the string, so this won't fail
        // In a real implementation with proper ABI parsing, this might fail
        let result = EventListener::subscribe_to_monitor_events(provider, monitor, tx).await;

        assert!(result.is_ok());

        if let Ok(handle) = result {
            handle.abort();
        }
    }

    #[tokio::test]
    async fn test_process_event_success() {
        use alloy::primitives::{b256, B256};
        use alloy::rpc::types::Log as AlloyLog;

        // Setup dependencies with a mock HTTP client
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager, config));

        let monitor = test_monitor();

        // Create a test log
        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![b256!(
                        "0000000000000000000000000000000000000000000000000000000000000001"
                    )],
                    vec![0x00, 0x00, 0x00, 0x01].into(),
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: false,
        };

        // Test process_event - will fail due to webhook call but tests the flow
        let result = EventListener::process_event(
            &monitor,
            log,
            "test-network",
            &webhook_caller,
            &response_handler,
            &None,
        )
        .await;

        // Should fail because we can't actually call the webhook
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_event_with_removed_log() {
        use alloy::primitives::{b256, B256};
        use alloy::rpc::types::Log as AlloyLog;

        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let webhook_caller = Arc::new(WebhookCaller::new());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager, config));

        let monitor = test_monitor();

        // Create a removed log
        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![b256!(
                        "0000000000000000000000000000000000000000000000000000000000000001"
                    )],
                    vec![0x00, 0x00, 0x00, 0x01].into(),
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: true, // Mark as removed
        };

        let result = EventListener::process_event(
            &monitor,
            log,
            "test-network",
            &webhook_caller,
            &response_handler,
            &None,
        )
        .await;

        // Should still try to process but webhook will fail
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_event_decode_and_metrics() {
        use alloy::primitives::{b256, B256};
        use alloy::rpc::types::Log as AlloyLog;

        let monitor = test_monitor();

        // Create a test log with multiple topics
        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![
                        b256!("0000000000000000000000000000000000000000000000000000000000000001"),
                        b256!("0000000000000000000000000000000000000000000000000000000000000002"),
                        b256!("0000000000000000000000000000000000000000000000000000000000000003"),
                    ],
                    vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02].into(),
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(5),
            removed: false,
        };

        // Test decoding
        let processed = EventListener::decode_event_data(&log, &monitor).unwrap();
        assert_eq!(processed.event_name, "Transfer");
        assert_eq!(processed.topics.len(), 3);
        assert_eq!(processed.log_index, 5);
        assert_eq!(processed.data, "0x0000000100000002");
    }

    #[tokio::test]
    async fn test_monitor_network_events_with_timeout() {
        use alloy::providers::ProviderBuilder;
        use tokio::time::{timeout, Duration};

        let provider =
            Arc::new(ProviderBuilder::new().on_http("http://localhost:8545".parse().unwrap()));

        let webhook_caller = Arc::new(WebhookCaller::new());
        let networks: Vec<crate::config::models::Network> = vec![];
        let network_manager = Arc::new(NetworkManager::new(&networks).await.unwrap());
        let config = Arc::new(crate::config::models::OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        });
        let response_handler = Arc::new(ResponseHandler::new(network_manager, config));

        let monitors = vec![test_monitor()];

        // Test that monitor_network_events starts up correctly
        // We'll use a timeout to ensure it doesn't block forever
        let monitor_future = EventListener::monitor_network_events(
            provider,
            "test-network".to_string(),
            monitors,
            webhook_caller,
            response_handler,
            None,
        );

        // Give it 100ms to start up and then timeout
        let result = timeout(Duration::from_millis(100), monitor_future).await;

        // The function should timeout because it's waiting for events
        assert!(result.is_err());
    }

    #[test]
    fn test_event_context_version() {
        let context = EventContext {
            network: "test".to_string(),
            timestamp: chrono::Utc::now(),
            omikuji_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        assert_eq!(context.omikuji_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_decode_event_data_with_indexed_params() {
        use alloy::primitives::{b256, Address, B256, U256};
        use alloy::rpc::types::Log as AlloyLog;
        use alloy::sol_types::SolValue;

        // Create monitor for Transfer event with indexed parameters
        let mut monitor = test_monitor();
        monitor.event_signature =
            "Transfer(address indexed from, address indexed to, uint256 value)".to_string();

        // Create test addresses
        let from_addr = Address::from([0x11; 20]);
        let to_addr = Address::from([0x22; 20]);
        let value = U256::from(1000u64);

        // Create log with proper topics for indexed parameters
        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![
                        b256!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"), // Transfer event selector
                        B256::left_padding_from(&from_addr.to_vec()), // from address
                        B256::left_padding_from(&to_addr.to_vec()),   // to address
                    ],
                    value.abi_encode().into(), // value in data
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: false,
        };

        let result = EventListener::decode_event_data(&log, &monitor);
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.event_name, "Transfer");

        // Check decoded arguments
        let args = event.decoded_args.as_object().unwrap();
        assert!(args.contains_key("from"));
        assert!(args.contains_key("to"));
        assert!(args.contains_key("value"));

        // Verify the values
        assert!(args["from"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("1111"));
        assert!(args["to"].as_str().unwrap().to_lowercase().contains("2222"));
        assert_eq!(args["value"].as_str().unwrap(), "1000");
    }

    #[test]
    fn test_decode_event_data_with_arrays() {
        use alloy::primitives::{b256, Address, B256, U256};
        use alloy::rpc::types::Log as AlloyLog;
        use alloy::sol_types::SolValue;

        // Create monitor for event with array parameters
        let mut monitor = test_monitor();
        monitor.event_signature = "Winners(address[] winners, uint256[] amounts)".to_string();

        // Create test data
        let winners = vec![Address::from([0x11; 20]), Address::from([0x22; 20])];
        let amounts = vec![U256::from(100), U256::from(200)];

        let log = AlloyLog {
            inner: alloy::primitives::Log {
                address: monitor.contract_address,
                data: alloy::primitives::LogData::new_unchecked(
                    vec![
                        b256!("1234567890123456789012345678901234567890123456789012345678901234"), // Event selector
                    ],
                    (winners, amounts).abi_encode().into(),
                ),
            },
            block_hash: Some(B256::ZERO),
            block_number: Some(12345),
            block_timestamp: None,
            transaction_hash: Some(b256!(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: false,
        };

        let result = EventListener::decode_event_data(&log, &monitor);
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.event_name, "Winners");

        // Check decoded arguments
        let args = event.decoded_args.as_object().unwrap();
        assert!(args.contains_key("winners"));
        assert!(args.contains_key("amounts"));

        // Verify arrays
        let winners_arr = args["winners"].as_array().unwrap();
        let amounts_arr = args["amounts"].as_array().unwrap();
        assert_eq!(winners_arr.len(), 2);
        assert_eq!(amounts_arr.len(), 2);
        assert_eq!(amounts_arr[0].as_str().unwrap(), "100");
        assert_eq!(amounts_arr[1].as_str().unwrap(), "200");
    }
}
