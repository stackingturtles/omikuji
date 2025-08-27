use super::contract_config::ContractConfigReader;
use super::fetcher::Fetcher;
use super::monitor::FeedMonitor;
use crate::config::models::{Datafeed, OmikujiConfig};
use crate::database::{DatabasePool, FeedLogRepository, TransactionLogRepository};
use crate::gas_price::GasPriceManager;
use crate::network::NetworkManager;
use crate::transaction::queue::TransactionQueue;
use alloy::primitives::I256;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Manages all datafeed monitors
pub struct FeedManager {
    config: OmikujiConfig,
    network_manager: Arc<NetworkManager>,
    fetcher: Arc<Fetcher>,
    repository: Option<Arc<FeedLogRepository>>,
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
    gas_price_manager: Option<Arc<GasPriceManager>>,
    transaction_queue: Option<Arc<TransactionQueue>>,
    handles: Vec<JoinHandle<()>>,
}

impl FeedManager {
    /// Creates a new FeedManager with the given configuration
    pub fn new(config: OmikujiConfig, network_manager: Arc<NetworkManager>) -> Self {
        Self {
            config,
            network_manager,
            fetcher: Arc::new(Fetcher::new()),
            repository: None,
            tx_log_repo: None,
            gas_price_manager: None,
            transaction_queue: None,
            handles: Vec::new(),
        }
    }

    /// Sets the database repository for feed logging
    pub fn with_repository(mut self, pool: DatabasePool) -> Self {
        self.repository = Some(Arc::new(FeedLogRepository::new(pool.clone())));
        self.tx_log_repo = Some(Arc::new(TransactionLogRepository::new(pool)));
        self
    }

    /// Sets the gas price manager for USD cost tracking
    pub fn with_gas_price_manager(mut self, gas_price_manager: Arc<GasPriceManager>) -> Self {
        self.gas_price_manager = Some(gas_price_manager);
        self
    }

    /// Sets the transaction queue for coordinated submissions
    pub fn with_transaction_queue(mut self, queue: Arc<TransactionQueue>) -> Self {
        self.transaction_queue = Some(queue);
        self
    }

    /// Starts monitoring all configured datafeeds
    /// Each datafeed runs in its own tokio task
    pub async fn start(&mut self) {
        info!(
            "Starting feed manager with {} datafeeds",
            self.config.datafeeds.len()
        );

        let contract_reader = ContractConfigReader::new(&self.network_manager);

        for mut datafeed in self.config.datafeeds.clone() {
            // Configure the datafeed (either from contract or YAML)
            match self
                .configure_datafeed(&mut datafeed, &contract_reader)
                .await
            {
                Ok(_) => {
                    let handle = self.spawn_monitor(datafeed);
                    self.handles.push(handle);
                }
                Err(feed_name) => {
                    // Feed was skipped due to error - already logged
                    info!("Skipping datafeed '{}'", feed_name);
                }
            }
        }

        info!(
            "Feed manager initialization complete. {} feeds running.",
            self.handles.len()
        );
    }

    /// Configures a datafeed by reading from contract or using YAML values
    /// Returns Ok(()) if successful, Err(feed_name) if the feed should be skipped
    async fn configure_datafeed(
        &self,
        datafeed: &mut Datafeed,
        contract_reader: &ContractConfigReader<'_>,
    ) -> Result<(), String> {
        if datafeed.read_contract_config {
            // Try to read from contract
            match contract_reader
                .read_config(&datafeed.networks, &datafeed.contract_address)
                .await
            {
                Ok(contract_config) => {
                    self.apply_contract_config(datafeed, contract_config);
                    Ok(())
                }
                Err(e) => {
                    error!(
                        "Failed to read contract config for datafeed '{}': {}. Skipping this feed.",
                        datafeed.name, e
                    );
                    Err(datafeed.name.clone())
                }
            }
        } else {
            // Use YAML config
            self.log_datafeed_config(datafeed, "config.yaml values");
            Ok(())
        }
    }

    /// Applies contract configuration to a datafeed
    fn apply_contract_config(
        &self,
        datafeed: &mut Datafeed,
        config: crate::datafeed::contract_config::ContractConfig,
    ) {
        self.log_config_values(
            &datafeed.name,
            "contract config",
            config.decimals,
            config.min_value,
            config.max_value,
        );

        datafeed.decimals = Some(config.decimals);
        datafeed.min_value = Some(config.min_value);
        datafeed.max_value = Some(config.max_value);
    }

    /// Logs datafeed configuration from YAML
    fn log_datafeed_config(&self, datafeed: &Datafeed, source: &str) {
        self.log_config_values(
            &datafeed.name,
            source,
            datafeed.decimals.unwrap_or(0),
            datafeed.min_value.unwrap_or(I256::ZERO),
            datafeed.max_value.unwrap_or(I256::ZERO),
        );
    }

    /// Common logging for configuration values
    fn log_config_values(
        &self,
        name: &str,
        source: &str,
        decimals: impl std::fmt::Display,
        min_value: impl std::fmt::Display,
        max_value: impl std::fmt::Display,
    ) {
        info!(
            "Using {} for datafeed '{}': decimals={}, min_value={}, max_value={}",
            source, name, decimals, min_value, max_value
        );
    }

    /// Spawns a monitor task for a single datafeed
    fn spawn_monitor(&self, datafeed: Datafeed) -> JoinHandle<()> {
        let mut monitor = FeedMonitor::new(
            datafeed.clone(),
            Arc::clone(&self.fetcher),
            Arc::clone(&self.network_manager),
            self.config.clone(),
            self.repository.clone(),
            self.tx_log_repo.clone(),
        );

        // Set gas price manager if available
        if let Some(ref gas_price_manager) = self.gas_price_manager {
            monitor = monitor.with_gas_price_manager(Arc::clone(gas_price_manager));
        }
        
        // Set transaction queue if available
        if let Some(ref queue) = self.transaction_queue {
            monitor = monitor.with_transaction_queue(Arc::clone(queue));
        }

        let feed_name = datafeed.name.clone();

        tokio::spawn(async move {
            info!("Spawning monitor task for datafeed '{}'", feed_name);
            monitor.start().await;
        })
    }

    /// Waits for all monitors to complete (they run indefinitely)
    #[allow(dead_code)]
    pub async fn wait(&mut self) {
        for handle in self.handles.drain(..) {
            if let Err(e) = handle.await {
                error!("Monitor task panicked: {}", e);
            }
        }
    }
}
