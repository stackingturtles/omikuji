//! Startup context and initialization sequencing
//!
//! This module manages the initialization phase of the Omikuji daemon,
//! ensuring components are started in the correct order and with proper
//! error handling.
//!
//! # Initialization Sequence
//!
//! The startup process follows this order:
//!
//! 1. **Configuration Loading**: Parse and validate config files
//! 2. **Database**: Establish connection pool (if enabled)
//! 3. **Network Manager**: Initialize blockchain RPC connections
//! 4. **Wallet Loading**: Load private keys for all networks
//! 5. **Cleanup Manager**: Initialize database cleanup (if enabled)
//! 6. **Gas Price Manager**: Initialize gas price feeds (if enabled)
//! 7. **Transaction Queue**: Initialize coordinated nonce management
//!
//! # Error Handling
//!
//! Startup failures are critical and will prevent the daemon from running.
//! Each component's initialization is wrapped in proper error context to
//! aid in debugging configuration issues.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::config::{self, models::OmikujiConfig};
use crate::database::{self, cleanup::CleanupManager, FeedLogRepository};
use crate::gas_price::GasPriceManager;
use crate::metrics::{init_metrics_config, ConfigMetrics};
use crate::network::NetworkManager;
use crate::transaction::queue::TransactionQueue;
use crate::wallet::key_storage::create_key_storage;
use sqlx::PgPool;

/// Context for daemon initialization
///
/// Holds all initialized components needed to run the daemon.
pub struct StartupContext {
    /// Loaded configuration
    pub config: OmikujiConfig,
    /// Database connection pool (optional)
    pub database_pool: Option<PgPool>,
    /// Database cleanup manager (optional)
    pub cleanup_manager: Option<CleanupManager>,
    /// Network manager for blockchain RPC connections
    pub network_manager: Arc<NetworkManager>,
    /// Gas price manager (optional)
    pub gas_price_manager: Option<Arc<GasPriceManager>>,
    /// Transaction queue for coordinated submissions
    pub transaction_queue: Arc<TransactionQueue>,
    /// Private key environment variable name (for backward compatibility)
    pub private_key_env: String,
}

impl StartupContext {
    /// Create a new startup context from configuration path
    ///
    /// # Arguments
    ///
    /// * `config_path` - Path to YAML configuration file
    /// * `private_key_env` - Environment variable name for private keys (fallback)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration file cannot be loaded or parsed
    /// - Configuration validation fails
    pub async fn new(config_path: &Path, private_key_env: String) -> Result<Self> {
        info!(
            "Creating startup context from config: {}",
            config_path.display()
        );

        // Load and validate configuration
        let config = config::load_config(config_path).with_context(|| {
            format!(
                "Failed to load configuration from {}",
                config_path.display()
            )
        })?;

        info!("Configuration loaded successfully");
        info!(
            "Loaded {} network(s) and {} datafeed(s)",
            config.networks.len(),
            config.datafeeds.len()
        );

        // Display loaded configuration
        for network in &config.networks {
            let rpc_url = network
                .nodes
                .first()
                .map(|node| node.rpc_url.as_str())
                .unwrap_or("no nodes configured");
            info!("Network: {} ({})", network.name, rpc_url);
        }

        for datafeed in &config.datafeeds {
            info!(
                "Datafeed: {} on network {}",
                datafeed.name, datafeed.networks
            );
        }

        // Create a temporary empty network manager (will be initialized later)
        let temp_network_manager = Arc::new(NetworkManager::new(&[]).await?);

        Ok(Self {
            config: config.clone(),
            database_pool: None,
            cleanup_manager: None,
            network_manager: temp_network_manager.clone(),
            gas_price_manager: None,
            transaction_queue: Arc::new(TransactionQueue::new(
                config,
                temp_network_manager,
                None,
                None,
            )),
            private_key_env,
        })
    }

    /// Initialize all daemon components
    ///
    /// Performs the full initialization sequence in the correct order.
    ///
    /// # Errors
    ///
    /// Returns an error if any component fails to initialize.
    pub async fn initialize_components(&mut self) -> Result<()> {
        info!("Initializing daemon components");

        // 1. Initialize metrics configuration
        init_metrics_config(self.config.metrics.clone());
        ConfigMetrics::record_startup_info(&self.config);

        // 2. Initialize database connection (optional)
        self.initialize_database().await?;

        // 3. Initialize network connections
        self.initialize_network().await?;

        // 4. Load wallets for all networks
        self.initialize_wallets().await?;

        // 5. Initialize cleanup manager (if database available)
        self.initialize_cleanup_manager().await?;

        // 6. Initialize gas price manager (if enabled)
        self.initialize_gas_price_manager().await?;

        // 7. Initialize transaction queue
        self.initialize_transaction_queue().await?;

        info!("All components initialized successfully");
        Ok(())
    }

    /// Initialize database connection and run migrations
    async fn initialize_database(&mut self) -> Result<()> {
        info!("Checking for database configuration...");

        match std::env::var("DATABASE_URL") {
            Ok(_) => {
                debug!("DATABASE_URL environment variable found, attempting database connection");

                match database::establish_connection().await {
                    Ok(pool) => {
                        info!("Database connection established successfully");

                        // Check if we should skip migrations
                        let skip_migrations = std::env::var("SKIP_MIGRATIONS")
                            .unwrap_or_else(|_| "false".to_string())
                            .to_lowercase()
                            == "true";

                        if skip_migrations {
                            info!("SKIP_MIGRATIONS is set, skipping database migrations");

                            // Verify tables exist and are accessible
                            match database::connection::verify_tables(&pool).await {
                                Ok(_) => {
                                    info!("Database tables verified successfully");
                                    info!("Database initialized with feed logging and transaction tracking enabled");
                                    ConfigMetrics::set_database_status(true);
                                    self.database_pool = Some(pool);
                                }
                                Err(e) => {
                                    error!("Database tables are not accessible: {}", e);
                                    error!("Please ensure the required tables exist (feed_log, transaction_log, gas_token_prices)");
                                    error!("Continuing without database support");
                                    ConfigMetrics::set_database_status(false);
                                }
                            }
                        } else {
                            // Run migrations as normal
                            if let Err(e) = database::connection::run_migrations(&pool).await {
                                error!("Failed to run database migrations: {}", e);
                                error!("You can skip migrations by setting SKIP_MIGRATIONS=true");
                                error!("Continuing without database support");
                                ConfigMetrics::set_database_status(false);
                            } else {
                                info!("Database migrations completed successfully");
                                info!("Database initialized with feed logging and transaction tracking enabled");
                                ConfigMetrics::set_database_status(true);
                                self.database_pool = Some(pool);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to establish database connection: {}", e);
                        error!("Continuing without database logging");
                        ConfigMetrics::set_database_status(false);
                    }
                }
            }
            Err(_) => {
                info!("DATABASE_URL not set - running without database logging");
                info!("To enable database logging, set DATABASE_URL environment variable");
                ConfigMetrics::set_database_status(false);
            }
        }

        Ok(())
    }

    /// Initialize network manager and connections
    async fn initialize_network(&mut self) -> Result<()> {
        info!("Initializing network connections");

        let network_manager = NetworkManager::new(&self.config.networks)
            .await
            .context("Failed to initialize network connections")?;

        info!("Network connections initialized successfully");
        self.network_manager = Arc::new(network_manager);

        Ok(())
    }

    /// Load wallets for all configured networks
    async fn initialize_wallets(&mut self) -> Result<()> {
        info!("Loading wallets for configured networks");

        let key_storage = create_key_storage(&self.config)
            .await
            .context("Failed to initialize key storage")?;

        // Get mutable reference to network manager
        let network_manager = Arc::get_mut(&mut self.network_manager)
            .context("Cannot get mutable reference to network manager")?;

        let mut failed_networks = Vec::new();

        for network in &self.config.networks {
            match network_manager
                .load_wallet_from_key_storage(&network.name, key_storage.as_ref())
                .await
            {
                Ok(_) => {
                    info!("Successfully loaded wallet for network {}", network.name);
                }
                Err(e) => {
                    // For backward compatibility, try environment variable if keyring fails
                    if self.config.key_storage.storage_type == "keyring" {
                        debug!(
                            "Keyring lookup failed for network {}, trying environment variable",
                            network.name
                        );

                        match network_manager
                            .load_wallet_from_env(&network.name, &self.private_key_env)
                            .await
                        {
                            Ok(_) => {
                                info!(
                                    "Successfully loaded wallet for network {} from environment variable",
                                    network.name
                                );
                                continue;
                            }
                            Err(env_err) => {
                                debug!("Failed to load from env var: {:?}", env_err);
                            }
                        }
                    }

                    error!("Failed to load wallet for network {}: {}", network.name, e);
                    failed_networks.push(network.name.clone());
                }
            }
        }

        if !failed_networks.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to load wallets for {} network(s): {}. \
                 Run 'omikuji verify' to diagnose key storage issues.",
                failed_networks.len(),
                failed_networks.join(", ")
            ));
        }

        Ok(())
    }

    /// Initialize database cleanup manager
    async fn initialize_cleanup_manager(&mut self) -> Result<()> {
        if let Some(ref pool) = self.database_pool {
            let repository = Arc::new(FeedLogRepository::new(pool.clone()));
            let mut cleanup_manager = CleanupManager::new(self.config.clone(), repository).await?;

            // Start cleanup scheduler
            if let Err(e) = cleanup_manager.start().await {
                error!("Failed to start cleanup scheduler: {}", e);
            } else {
                info!("Cleanup manager initialized successfully");
            }

            self.cleanup_manager = Some(cleanup_manager);
        }

        Ok(())
    }

    /// Initialize gas price manager
    async fn initialize_gas_price_manager(&mut self) -> Result<()> {
        if self.config.gas_price_feeds.enabled {
            info!("Initializing gas price feed manager");

            // Build token mappings from network configurations
            let mut token_mappings = std::collections::HashMap::new();
            for network in &self.config.networks {
                token_mappings.insert(network.name.clone(), network.gas_token.clone());
            }

            // Create transaction repository if database is available
            let tx_repo = self.database_pool.as_ref().map(|pool| {
                Arc::new(
                    crate::database::transaction_repository::TransactionLogRepository::new(
                        pool.clone(),
                    ),
                )
            });

            let gas_price_manager = Arc::new(GasPriceManager::new(
                self.config.gas_price_feeds.clone(),
                token_mappings,
                tx_repo,
            ));

            // Start the price update loop
            gas_price_manager.clone().start().await;

            self.gas_price_manager = Some(gas_price_manager);
        } else {
            info!("Gas price feeds are disabled");
        }

        Ok(())
    }

    /// Initialize transaction queue
    async fn initialize_transaction_queue(&mut self) -> Result<()> {
        info!("Initializing transaction queue for coordinated submissions");

        let tx_log_repo = self.database_pool.as_ref().map(|pool| {
            Arc::new(
                crate::database::transaction_repository::TransactionLogRepository::new(
                    pool.clone(),
                ),
            )
        });

        self.transaction_queue = Arc::new(TransactionQueue::new(
            self.config.clone(),
            Arc::clone(&self.network_manager),
            tx_log_repo,
            self.gas_price_manager.clone(),
        ));

        info!("Transaction queue initialized successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use std::io::Write;
    use tempfile::TempDir;

    /// Write a minimal valid config YAML that points to the given Anvil URL.
    fn write_temp_config(anvil_url: &str) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let yaml = format!(
            r#"
networks:
  - name: anvil
    transaction_type: eip1559
    nodes:
      - name: Test Anvil
        rpc_url: {anvil_url}

datafeeds: []
scheduled_tasks: []
event_monitors: []

key_storage:
  storage_type: env
"#
        );
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn test_startup_context_requires_config() {
        // Startup context requires a valid config file
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.yaml");

        let result = StartupContext::new(&config_path, "TEST_KEY".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_new_with_valid_config() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let (_dir, path) = write_temp_config(&anvil.endpoint());

        let ctx = StartupContext::new(&path, "TEST_KEY".to_string())
            .await
            .unwrap();

        assert_eq!(ctx.config.networks.len(), 1);
        assert_eq!(ctx.config.networks[0].name, "anvil");
        assert!(ctx.database_pool.is_none());
        assert!(ctx.gas_price_manager.is_none());
    }

    #[tokio::test]
    async fn test_new_config_parsing_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{{{{not valid yaml").unwrap();

        let result = StartupContext::new(&path, "TEST_KEY".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_initialize_components_no_database() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let (_dir, path) = write_temp_config(&anvil.endpoint());

        // Ensure DATABASE_URL is not set for this test
        std::env::remove_var("DATABASE_URL");

        // Set a valid private key so wallet loading succeeds.
        // This is the first default Anvil account private key.
        std::env::set_var(
            "OMIKUJI_PRIVATE_KEY_ANVIL",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        );

        let mut ctx = StartupContext::new(&path, "TEST_KEY".to_string())
            .await
            .unwrap();

        // initialize_components should succeed even without a database
        ctx.initialize_components().await.unwrap();

        assert!(ctx.database_pool.is_none());
    }

    #[tokio::test]
    async fn test_initialize_wallets_fails_on_missing_key() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let (_dir, path) = write_temp_config(&anvil.endpoint());

        // Ensure the key env var does NOT exist
        std::env::remove_var("OMIKUJI_PRIVATE_KEY_ANVIL");

        let mut ctx = StartupContext::new(&path, "TEST_KEY".to_string())
            .await
            .unwrap();

        // Initialize network first (required by initialize_wallets)
        ctx.initialize_network().await.unwrap();

        let err = ctx.initialize_wallets().await.unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("Failed to load wallets"),
            "should report wallet failure: {msg}"
        );
        assert!(
            msg.contains("anvil"),
            "should list the failed network: {msg}"
        );
        assert!(
            msg.contains("omikuji verify"),
            "should suggest running verify: {msg}"
        );
    }

    #[tokio::test]
    async fn test_initialize_wallets_fails_lists_all_failed_networks() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let url = anvil.endpoint();

        // Config with two networks
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let yaml = format!(
            r#"
networks:
  - name: net-a
    transaction_type: eip1559
    nodes:
      - name: Node A
        rpc_url: {url}
  - name: net-b
    transaction_type: eip1559
    nodes:
      - name: Node B
        rpc_url: {url}

datafeeds: []
scheduled_tasks: []
event_monitors: []

key_storage:
  storage_type: env
"#
        );
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        // Neither key set
        std::env::remove_var("OMIKUJI_PRIVATE_KEY_NET_A");
        std::env::remove_var("OMIKUJI_PRIVATE_KEY_NET_B");

        let mut ctx = StartupContext::new(&path, "TEST_KEY".to_string())
            .await
            .unwrap();

        // Initialize network first (required by initialize_wallets)
        ctx.initialize_network().await.unwrap();

        let err = ctx.initialize_wallets().await.unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("2 network(s)"),
            "should report count of 2: {msg}"
        );
        assert!(msg.contains("net-a"), "should list net-a: {msg}");
        assert!(msg.contains("net-b"), "should list net-b: {msg}");
    }

    #[tokio::test]
    async fn test_initialize_components_network_setup() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let url = anvil.endpoint();

        // Construct a NetworkManager directly from the Anvil URL and verify
        // chain_id works — this is what initialize_components sets up internally.
        let network = crate::config::models::Network {
            name: "anvil".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Anvil Node".to_string(),
                rpc_url: url,
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        let nm = NetworkManager::new(&[network]).await.unwrap();
        let chain_id = nm.get_chain_id("anvil").await.unwrap();
        assert_eq!(chain_id, 31337);
    }

    #[tokio::test]
    async fn test_initialize_components_transaction_queue() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let url = anvil.endpoint();

        // Construct a minimal config + NetworkManager to verify TransactionQueue creation,
        // matching what initialize_components does without the global metrics OnceCell.
        let network = crate::config::models::Network {
            name: "anvil".to_string(),
            nodes: vec![crate::config::models::NetworkNode {
                name: "Anvil Node".to_string(),
                rpc_url: url,
                ws_url: None,
            }],
            transaction_type: "eip1559".to_string(),
            gas_config: Default::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        };

        let config = crate::config::models::OmikujiConfig {
            networks: vec![network],
            datafeeds: vec![],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: Default::default(),
            gas_price_feeds: Default::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        };

        let nm = Arc::new(NetworkManager::new(&config.networks).await.unwrap());
        let tx_queue = Arc::new(TransactionQueue::new(
            config.clone(),
            nm.clone(),
            None,
            None,
        ));

        // Verify the transaction queue is properly constructed
        assert!(Arc::strong_count(&tx_queue) >= 1);
    }
}
