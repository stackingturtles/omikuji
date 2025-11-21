//! Daemon orchestrator for Omikuji runtime
//!
//! The `Daemon` is the central component that coordinates all aspects of the
//! Omikuji system. It manages:
//!
//! - Network connections to EVM blockchains
//! - Datafeed monitoring and updates
//! - Event monitoring for smart contracts
//! - Scheduled task execution
//! - Transaction queue processing
//! - Metrics collection and reporting
//! - Graceful shutdown handling
//!
//! # Lifecycle
//!
//! The daemon follows a strict lifecycle:
//!
//! 1. **Construction**: Built via `DaemonBuilder`
//! 2. **Initialization**: All components are initialized
//! 3. **Running**: Main event loop processes events
//! 4. **Shutdown**: Graceful cleanup on signal
//!
//! # Example
//!
//! ```rust,no_run
//! # use omikuji_core::runtime::DaemonBuilder;
//! # async fn example() -> anyhow::Result<()> {
//! let daemon = DaemonBuilder::new()
//!     .build()
//!     .await?;
//!
//! // Run until shutdown signal (SIGINT, SIGTERM)
//! daemon.run().await?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use super::{ShutdownHandle, StartupContext};
use crate::config::models::OmikujiConfig;
use crate::datafeed::FeedManager;
use crate::event_monitors::{EventListener, ResponseHandler, WebhookCaller};
use crate::metrics;
use crate::network::health_monitor;
use crate::scheduled_tasks::ScheduledTaskManager;
use crate::wallet::WalletBalanceMonitor;

/// Main daemon orchestrator
///
/// The daemon coordinates all Omikuji operations and manages the lifecycle
/// of all monitoring components.
pub struct Daemon {
    /// Configuration
    config: OmikujiConfig,
    /// Startup context with all initialized components
    context: StartupContext,
    /// Shutdown coordination handle
    shutdown: ShutdownHandle,
}

impl Daemon {
    /// Create a new daemon instance
    ///
    /// This is typically called by `DaemonBuilder::build()` rather than directly.
    ///
    /// # Arguments
    ///
    /// * `context` - Initialized startup context
    /// * `shutdown` - Handle for coordinating shutdown
    pub(crate) fn new(context: StartupContext, shutdown: ShutdownHandle) -> Self {
        debug!("Creating new Daemon instance");
        let config = context.config.clone();
        Self {
            config,
            context,
            shutdown,
        }
    }

    /// Run the daemon until shutdown signal
    ///
    /// This is the main entry point for the daemon. It will:
    ///
    /// 1. Start datafeed monitoring
    /// 2. Start event monitoring
    /// 3. Start scheduled tasks
    /// 4. Start metrics server
    /// 5. Start health monitors
    /// 6. Wait for shutdown signal
    /// 7. Gracefully stop all components
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Component initialization fails
    /// - Critical runtime error occurs
    /// - Shutdown cleanup fails
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Omikuji daemon");

        // Start datafeed monitoring
        let mut feed_manager = self.start_datafeed_monitoring();
        feed_manager.start().await;

        // Start scheduled task manager
        let scheduled_task_manager = self.start_scheduled_tasks().await?;

        // Start metrics server
        self.start_metrics_server().await?;

        // Start event monitoring
        let event_monitor_handles = self.start_event_monitoring().await;

        // Start wallet balance monitor
        let wallet_monitor_handle = self.start_wallet_monitor();

        // Start network health monitor
        let health_monitor_handle = self.start_health_monitor();

        // Test network connectivity
        self.test_network_connectivity().await;

        info!("Omikuji daemon started successfully");

        // Wait for shutdown signal
        self.shutdown.wait_for_shutdown().await;
        info!("Received shutdown signal, stopping...");

        // Graceful shutdown
        self.shutdown_components(
            scheduled_task_manager,
            event_monitor_handles,
            wallet_monitor_handle,
            health_monitor_handle,
        )
        .await;

        info!("Daemon shutdown complete");
        Ok(())
    }

    /// Start datafeed monitoring
    fn start_datafeed_monitoring(&self) -> FeedManager {
        let mut manager = if let Some(pool) = &self.context.database_pool {
            FeedManager::new(
                self.config.clone(),
                Arc::clone(&self.context.network_manager),
            )
            .with_repository(pool.clone())
        } else {
            FeedManager::new(
                self.config.clone(),
                Arc::clone(&self.context.network_manager),
            )
        };

        // Add gas price manager if available
        if let Some(ref gas_price_manager) = self.context.gas_price_manager {
            manager = manager.with_gas_price_manager(Arc::clone(gas_price_manager));
        }

        // Add transaction queue for coordinated submissions
        manager = manager.with_transaction_queue(Arc::clone(&self.context.transaction_queue));

        manager
    }

    /// Start scheduled task manager
    async fn start_scheduled_tasks(&self) -> Result<Option<Arc<ScheduledTaskManager>>> {
        if self.config.scheduled_tasks.is_empty() {
            info!("No scheduled tasks configured");
            return Ok(None);
        }

        info!(
            "Initializing scheduled task manager with {} tasks",
            self.config.scheduled_tasks.len()
        );

        // Debug output for each scheduled task
        for (i, task) in self.config.scheduled_tasks.iter().enumerate() {
            debug!("Scheduled task {}: {:?}", i, task);
            info!(
                "Task '{}': schedule='{}', network='{}', function='{}'",
                task.name, task.schedule, task.network, task.target_function.function
            );
        }

        let mut task_manager = ScheduledTaskManager::new(
            self.config.scheduled_tasks.clone(),
            Arc::clone(&self.context.network_manager),
        )
        .await?;

        // Add gas price manager if available
        if let Some(ref gas_price_manager) = self.context.gas_price_manager {
            task_manager = task_manager.with_gas_price_manager(Arc::clone(gas_price_manager));
        }

        // Add transaction repository if database is available
        if let Some(pool) = &self.context.database_pool {
            let tx_repo = Arc::new(
                crate::database::transaction_repository::TransactionLogRepository::new(
                    pool.clone(),
                ),
            );
            task_manager = task_manager.with_tx_log_repo(tx_repo);
        }

        task_manager.start().await?;

        Ok(Some(Arc::new(task_manager)))
    }

    /// Start Prometheus metrics server
    async fn start_metrics_server(&self) -> Result<()> {
        if let Err(e) = metrics::start_metrics_server(self.config.metrics.port).await {
            error!("Failed to start metrics server: {}", e);
            error!("Continuing without metrics endpoint");
        } else {
            info!(
                "Prometheus metrics available at http://0.0.0.0:{}/metrics",
                self.config.metrics.port
            );
            metrics::ConfigMetrics::set_metrics_server_status(true, self.config.metrics.port);
        }

        Ok(())
    }

    /// Start event monitoring
    async fn start_event_monitoring(&self) -> Option<Vec<JoinHandle<()>>> {
        if self.config.event_monitors.is_empty() {
            info!("No event monitors configured");
            return None;
        }

        info!(
            "Initializing event monitoring with {} monitors",
            self.config.event_monitors.len()
        );

        // Debug output for each event monitor
        for monitor in &self.config.event_monitors {
            info!(
                "Event monitor '{}': network='{}', contract='{}', event='{}'",
                monitor.name, monitor.network, monitor.contract_address, monitor.event_signature
            );
            debug!(
                "  Webhook: {:?} {}",
                monitor.webhook.method, monitor.webhook.url
            );
        }

        // Create event monitoring components
        let webhook_caller = Arc::new(WebhookCaller::new());
        let response_handler = Arc::new(ResponseHandler::new(
            Arc::clone(&self.context.network_manager),
            Arc::new(self.config.clone()),
        ));

        // Create event listener with all configured monitors
        let event_listener = if let Some(pool) = &self.context.database_pool {
            EventListener::with_database(
                Arc::clone(&self.context.network_manager),
                webhook_caller,
                response_handler,
                self.config.event_monitors.clone(),
                pool.clone(),
            )
        } else {
            EventListener::new(
                Arc::clone(&self.context.network_manager),
                webhook_caller,
                response_handler,
                self.config.event_monitors.clone(),
            )
        };

        // Start monitoring
        match event_listener.start_monitoring().await {
            Ok(handles) => {
                info!(
                    "Started event monitoring with {} active subscriptions",
                    handles.len()
                );
                Some(handles)
            }
            Err(e) => {
                error!("Failed to start event monitoring: {}", e);
                None
            }
        }
    }

    /// Start wallet balance monitor
    fn start_wallet_monitor(&self) -> JoinHandle<()> {
        let mut wallet_monitor =
            WalletBalanceMonitor::new(Arc::clone(&self.context.network_manager));
        if let Some(ref gas_price_manager) = self.context.gas_price_manager {
            wallet_monitor = wallet_monitor.with_gas_price_manager(Arc::clone(gas_price_manager));
        }
        tokio::spawn(async move {
            wallet_monitor.start().await;
        })
    }

    /// Start network health monitor
    fn start_health_monitor(&self) -> JoinHandle<()> {
        let health_monitor = health_monitor::NetworkHealthMonitor::new(
            Arc::clone(&self.context.network_manager),
            health_monitor::HealthMonitorConfig::default(),
        );
        info!("Started network health monitor");
        tokio::spawn(async move {
            health_monitor.start().await;
        })
    }

    /// Test network connectivity for all configured networks
    async fn test_network_connectivity(&self) {
        for network in &self.config.networks {
            match self
                .context
                .network_manager
                .get_chain_id(&network.name)
                .await
            {
                Ok(chain_id) => info!("Network {} chain ID: {}", network.name, chain_id),
                Err(e) => error!("Failed to get chain ID for network {}: {}", network.name, e),
            }

            match self
                .context
                .network_manager
                .get_block_number(&network.name)
                .await
            {
                Ok(block_number) => {
                    info!("Network {} current block: {}", network.name, block_number)
                }
                Err(e) => error!(
                    "Failed to get block number for network {}: {}",
                    network.name, e
                ),
            }
        }
    }

    /// Gracefully shutdown all components
    async fn shutdown_components(
        &mut self,
        scheduled_task_manager: Option<Arc<ScheduledTaskManager>>,
        event_monitor_handles: Option<Vec<JoinHandle<()>>>,
        wallet_monitor_handle: JoinHandle<()>,
        health_monitor_handle: JoinHandle<()>,
    ) {
        // Stop scheduled task manager if running
        if let Some(task_manager) = scheduled_task_manager {
            if let Err(e) = task_manager.stop().await {
                error!("Error stopping scheduled task manager: {}", e);
            } else {
                info!("Scheduled task manager stopped successfully");
            }
        }

        // Stop event monitors if running
        if let Some(handles) = event_monitor_handles {
            info!("Stopping event monitors...");
            for handle in handles {
                handle.abort();
            }
            info!("Event monitors stopped successfully");
        }

        // Stop cleanup manager if running
        if let Some(mut cleanup_manager) = self.context.cleanup_manager.take() {
            if let Err(e) = cleanup_manager.stop().await {
                error!("Error stopping cleanup manager: {}", e);
            }
        }

        // Stop wallet monitor
        wallet_monitor_handle.abort();

        // Stop health monitor
        health_monitor_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    // Note: Full daemon testing requires valid configuration
    // Integration tests should be in the main binary
}
