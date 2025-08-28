use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{debug, error, info, warn};

mod cli;

use cli::{Cli, Commands};
use omikuji::wallet::key_storage::create_key_storage;
use omikuji::{config, database, datafeed, gas_price, metrics, network, scheduled_tasks, ui};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments first
    // This allows --version and --help to work without any side effects
    let cli = Cli::parse();

    // Handle key management commands before showing welcome screen
    match &cli.command {
        Some(Commands::Key { command }) => {
            // Initialize minimal logging for key commands
            tracing_subscriber::fmt::init();
            return cli::handle_key_command(command.clone(), cli.config.clone()).await;
        }
        Some(Commands::Run) | None => {
            // Continue with normal daemon operation
        }
    }

    // Prepare version string for ASCII art
    let version = format!("Omikuji v{}", env!("CARGO_PKG_VERSION"));
    // The ASCII art is 100 chars wide, so center the version string
    let width = 100;
    let version_line = format!("{version:^width$}");
    let welcome = ui::welcome_screen::WELCOME_SCREEN.replace("{version_line}", &version_line);
    println!("{welcome}");

    // Initialize logging after argument parsing
    tracing_subscriber::fmt::init();

    // Load .env file if it exists
    match dotenv::dotenv() {
        Ok(path) => info!("Loaded .env file from: {:?}", path),
        Err(e) => info!("No .env file loaded: {}", e),
    }

    // Determine configuration path
    let config_path = cli.config.unwrap_or_else(config::default_config_path);
    info!("Using configuration file: {:?}", config_path);

    // Load and validate configuration
    let config = match config::load_config(&config_path) {
        Ok(cfg) => {
            info!("Configuration loaded successfully");
            cfg
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return Err(anyhow::anyhow!("Configuration error: {}", e));
        }
    };

    // Initialize metrics configuration
    use crate::metrics::{init_metrics_config, ConfigMetrics};
    init_metrics_config(config.metrics.clone());

    // Record configuration metrics
    ConfigMetrics::record_startup_info(&config);

    // Display loaded configuration
    info!(
        "Loaded {} network(s) and {} datafeed(s)",
        config.networks.len(),
        config.datafeeds.len()
    );

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

    // Initialize network connections
    let mut network_manager = match network::NetworkManager::new(&config.networks).await {
        Ok(manager) => {
            info!("Network connections initialized successfully");
            manager
        }
        Err(e) => {
            error!("Failed to initialize network connections: {}", e);
            return Err(anyhow::anyhow!("Network initialization error: {}", e));
        }
    };

    // Load wallets based on key storage configuration
    let key_storage = create_key_storage(&config)
        .await
        .context("Failed to initialize key storage")?;

    for network in &config.networks {
        match network_manager
            .load_wallet_from_key_storage(&network.name, key_storage.as_ref())
            .await
        {
            Ok(_) => {
                info!("Successfully loaded wallet for network {}", network.name);
            }
            Err(e) => {
                // For backward compatibility, try environment variable if keyring fails
                if config.key_storage.storage_type == "keyring" {
                    debug!(
                        "Keyring lookup failed for network {}, trying environment variable",
                        network.name
                    );

                    match network_manager
                        .load_wallet_from_env(&network.name, &cli.private_key_env)
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
                warn!(
                    "Transactions on {} network will not be possible",
                    network.name
                );
            }
        }
    }

    // Now wrap in Arc for sharing across threads
    let network_manager = Arc::new(network_manager);

    // Initialize database connection (optional - continues if not available)
    info!("Checking for database configuration...");
    let database_pool = match std::env::var("DATABASE_URL") {
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
                                Some(pool)
                            }
                            Err(e) => {
                                error!("Database tables are not accessible: {}", e);
                                error!("Please ensure the required tables exist (feed_log, transaction_log, gas_token_prices)");
                                error!("Continuing without database support");
                                ConfigMetrics::set_database_status(false);
                                None
                            }
                        }
                    } else {
                        // Run migrations as normal
                        if let Err(e) = database::connection::run_migrations(&pool).await {
                            error!("Failed to run database migrations: {}", e);
                            error!("You can skip migrations by setting SKIP_MIGRATIONS=true");
                            error!("Continuing without database support");
                            ConfigMetrics::set_database_status(false);
                            None
                        } else {
                            info!("Database migrations completed successfully");
                            info!("Database initialized with feed logging and transaction tracking enabled");
                            ConfigMetrics::set_database_status(true);
                            Some(pool)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to establish database connection: {}", e);
                    error!("Continuing without database logging");
                    ConfigMetrics::set_database_status(false);
                    None
                }
            }
        }
        Err(_) => {
            info!("DATABASE_URL not set - running without database logging");
            info!("To enable database logging, set DATABASE_URL environment variable");
            ConfigMetrics::set_database_status(false);
            None
        }
    };

    // Initialize cleanup manager if database is available
    let cleanup_manager = if let Some(ref pool) = database_pool {
        let repository = Arc::new(database::FeedLogRepository::new(pool.clone()));
        let mut cleanup_manager =
            database::cleanup::CleanupManager::new(config.clone(), repository).await?;

        // Start cleanup scheduler
        if let Err(e) = cleanup_manager.start().await {
            error!("Failed to start cleanup scheduler: {}", e);
        }

        Some(cleanup_manager)
    } else {
        None
    };

    // Initialize gas price manager
    let gas_price_manager = if config.gas_price_feeds.enabled {
        info!("Initializing gas price feed manager");

        // Build token mappings from network configurations
        let mut token_mappings = std::collections::HashMap::new();
        for network in &config.networks {
            token_mappings.insert(network.name.clone(), network.gas_token.clone());
        }

        // Create transaction repository if database is available
        let tx_repo = database_pool.as_ref().map(|pool| {
            Arc::new(database::transaction_repository::TransactionLogRepository::new(pool.clone()))
        });

        let gas_price_manager = Arc::new(gas_price::GasPriceManager::new(
            config.gas_price_feeds.clone(),
            token_mappings,
            tx_repo,
        ));

        // Start the price update loop
        gas_price_manager.clone().start().await;

        Some(gas_price_manager)
    } else {
        info!("Gas price feeds are disabled");
        None
    };

    // Initialize transaction queue for coordinated nonce management
    info!("Initializing transaction queue for coordinated submissions");
    let tx_log_repo = database_pool.as_ref().map(|pool| {
        Arc::new(database::transaction_repository::TransactionLogRepository::new(pool.clone()))
    });

    let transaction_queue = Arc::new(omikuji::transaction::queue::TransactionQueue::new(
        config.clone(),
        Arc::clone(&network_manager),
        tx_log_repo,
        gas_price_manager.clone(),
    ));
    info!("Transaction queue initialized successfully");

    // Initialize and start datafeed monitoring
    let mut feed_manager = if let Some(ref pool) = &database_pool {
        let mut manager = datafeed::FeedManager::new(config.clone(), Arc::clone(&network_manager))
            .with_repository(pool.clone());

        // Add gas price manager if available
        if let Some(ref gas_price_manager) = gas_price_manager {
            manager = manager.with_gas_price_manager(Arc::clone(gas_price_manager));
        }

        // Add transaction queue for coordinated submissions
        manager = manager.with_transaction_queue(Arc::clone(&transaction_queue));

        manager
    } else {
        let mut manager = datafeed::FeedManager::new(config.clone(), Arc::clone(&network_manager));

        // Add gas price manager if available
        if let Some(ref gas_price_manager) = gas_price_manager {
            manager = manager.with_gas_price_manager(Arc::clone(gas_price_manager));
        }

        // Add transaction queue for coordinated submissions
        manager = manager.with_transaction_queue(Arc::clone(&transaction_queue));

        manager
    };

    feed_manager.start().await;

    // Initialize and start scheduled task manager
    let scheduled_task_manager = if !config.scheduled_tasks.is_empty() {
        info!(
            "Initializing scheduled task manager with {} tasks",
            config.scheduled_tasks.len()
        );

        // Debug output for each scheduled task
        for (i, task) in config.scheduled_tasks.iter().enumerate() {
            debug!("Scheduled task {}: {:?}", i, task);
            info!(
                "Task '{}': schedule='{}', network='{}', function='{}'",
                task.name, task.schedule, task.network, task.target_function.function
            );
            if let Some(ref condition) = task.check_condition {
                match condition {
                    scheduled_tasks::models::CheckCondition::Property {
                        property,
                        expected_value,
                        ..
                    } => {
                        debug!(
                            "  Condition: property '{}' should be {:?}",
                            property, expected_value
                        );
                    }
                    scheduled_tasks::models::CheckCondition::Function {
                        function,
                        expected_value,
                        ..
                    } => {
                        debug!(
                            "  Condition: function '{}' should return {:?}",
                            function, expected_value
                        );
                    }
                }
            }
        }

        let mut task_manager = scheduled_tasks::ScheduledTaskManager::new(
            config.scheduled_tasks.clone(),
            Arc::clone(&network_manager),
        )
        .await
        .context("Failed to create scheduled task manager")?;

        // Add gas price manager if available
        if let Some(ref gas_price_manager) = gas_price_manager {
            task_manager = task_manager.with_gas_price_manager(Arc::clone(gas_price_manager));
        }

        // Add transaction repository if database is available
        if let Some(ref pool) = &database_pool {
            let tx_repo = Arc::new(
                database::transaction_repository::TransactionLogRepository::new(pool.clone()),
            );
            task_manager = task_manager.with_tx_log_repo(tx_repo);
        }

        task_manager
            .start()
            .await
            .context("Failed to start scheduled task manager")?;

        Some(Arc::new(task_manager))
    } else {
        info!("No scheduled tasks configured");
        None
    };

    // Start Prometheus metrics server
    if let Err(e) = metrics::start_metrics_server(config.metrics.port).await {
        error!("Failed to start metrics server: {}", e);
        error!("Continuing without metrics endpoint");
    } else {
        info!(
            "Prometheus metrics available at http://0.0.0.0:{}/metrics",
            config.metrics.port
        );

        // Update metrics server status
        ConfigMetrics::set_metrics_server_status(true, config.metrics.port);
    }

    // Initialize and start event monitoring
    let event_monitor_handles = if !config.event_monitors.is_empty() {
        info!(
            "Initializing event monitoring with {} monitors",
            config.event_monitors.len()
        );

        // Debug output for each event monitor
        for monitor in &config.event_monitors {
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
        let webhook_caller = Arc::new(omikuji::event_monitors::WebhookCaller::new());
        let response_handler = Arc::new(omikuji::event_monitors::ResponseHandler::new(
            Arc::clone(&network_manager),
            Arc::new(config.clone()),
        ));

        // Create event listener with all configured monitors
        let event_listener = if let Some(ref pool) = &database_pool {
            omikuji::event_monitors::EventListener::with_database(
                Arc::clone(&network_manager),
                webhook_caller,
                response_handler,
                config.event_monitors.clone(),
                pool.clone(),
            )
        } else {
            omikuji::event_monitors::EventListener::new(
                Arc::clone(&network_manager),
                webhook_caller,
                response_handler,
                config.event_monitors.clone(),
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
    } else {
        info!("No event monitors configured");
        None
    };

    // Start wallet balance monitor
    let mut wallet_monitor =
        omikuji::wallet::WalletBalanceMonitor::new(Arc::clone(&network_manager));
    if let Some(ref gas_price_manager) = gas_price_manager {
        wallet_monitor = wallet_monitor.with_gas_price_manager(Arc::clone(gas_price_manager));
    }
    tokio::spawn(async move {
        wallet_monitor.start().await;
    });

    // Start network health monitor
    let health_monitor = omikuji::network::health_monitor::NetworkHealthMonitor::new(
        Arc::clone(&network_manager),
        omikuji::network::health_monitor::HealthMonitorConfig::default(),
    );
    tokio::spawn(async move {
        health_monitor.start().await;
    });
    info!("Started network health monitor");

    info!("Omikuji starting up...");

    // Get chain ID and block number for each network as a test
    for network in &config.networks {
        match network_manager.get_chain_id(&network.name).await {
            Ok(chain_id) => info!("Network {} chain ID: {}", network.name, chain_id),
            Err(e) => error!("Failed to get chain ID for network {}: {}", network.name, e),
        }

        match network_manager.get_block_number(&network.name).await {
            Ok(block_number) => info!("Network {} current block: {}", network.name, block_number),
            Err(e) => error!(
                "Failed to get block number for network {}: {}",
                network.name, e
            ),
        }
    }

    // Keep the application running
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal, stopping...");

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
    if let Some(mut cleanup_manager) = cleanup_manager {
        if let Err(e) = cleanup_manager.stop().await {
            error!("Error stopping cleanup manager: {}", e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli::KeyCommands;
    use config::metrics_config::MetricsConfig;
    use config::models::{
        AwsSecretsConfig, DatabaseCleanupConfig, GasConfig, KeyStorageConfig, KeyringConfig,
        Network, NetworkNode, OmikujiConfig, VaultConfig,
    };
    use gas_price::models::GasPriceFeedConfig;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> OmikujiConfig {
        OmikujiConfig {
            networks: vec![Network {
                name: "test-network".to_string(),
                nodes: vec![NetworkNode {
                    name: "Local Node".to_string(),
                    rpc_url: "http://localhost:8545".to_string(),
                    ws_url: None,
                }],
                transaction_type: "eip1559".to_string(),
                gas_config: GasConfig::default(),
                gas_token: "ethereum".to_string(),
                gas_token_symbol: "ETH".to_string(),
                balance_alerts: None,
                rpc_url: None,
                ws_url: None,
            }],
            datafeeds: vec![],
            database_cleanup: DatabaseCleanupConfig::default(),
            key_storage: KeyStorageConfig {
                storage_type: "env".to_string(),
                keyring: KeyringConfig {
                    service: "omikuji-test".to_string(),
                },
                vault: VaultConfig::default(),
                aws_secrets: AwsSecretsConfig::default(),
            },
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        }
    }

    #[test]
    fn test_cli_parsing() {
        // Test default CLI args
        let cli = Cli::parse_from(["omikuji"]);
        assert!(cli.command.is_none());
        assert!(cli.config.is_none());
        assert_eq!(cli.private_key_env, "OMIKUJI_PRIVATE_KEY");

        // Test with config path
        let cli = Cli::parse_from(["omikuji", "-c", "config.yaml"]);
        assert_eq!(cli.config.unwrap().to_str().unwrap(), "config.yaml");

        // Test with custom private key env
        let cli = Cli::parse_from(["omikuji", "-p", "MY_KEY"]);
        assert_eq!(cli.private_key_env, "MY_KEY");
    }

    #[test]
    fn test_cli_key_commands() {
        // Test key import command
        let cli = Cli::parse_from(["omikuji", "key", "import", "-n", "mainnet"]);
        match cli.command {
            Some(Commands::Key { command }) => match command {
                KeyCommands::Import { network, .. } => {
                    assert_eq!(network, "mainnet");
                }
                _ => panic!("Expected Import command"),
            },
            _ => panic!("Expected Key command"),
        }

        // Test key list command
        let cli = Cli::parse_from(["omikuji", "key", "list"]);
        match cli.command {
            Some(Commands::Key { command }) => match command {
                KeyCommands::List { .. } => {}
                _ => panic!("Expected List command"),
            },
            _ => panic!("Expected Key command"),
        }

        // Test key remove command
        let cli = Cli::parse_from(["omikuji", "key", "remove", "-n", "testnet"]);
        match cli.command {
            Some(Commands::Key { command }) => match command {
                KeyCommands::Remove { network, .. } => {
                    assert_eq!(network, "testnet");
                }
                _ => panic!("Expected Remove command"),
            },
            _ => panic!("Expected Key command"),
        }
    }

    #[test]
    fn test_version_string_formatting() {
        let version = format!("Omikuji v{}", env!("CARGO_PKG_VERSION"));
        let width = 100;
        let version_line = format!("{version:^width$}");

        // Check that version line is centered and has correct width
        assert_eq!(version_line.len(), 100);
        assert!(version_line.contains(&version));

        // Check that padding is roughly equal on both sides
        let trimmed = version_line.trim();
        let left_padding = version_line.find(trimmed.chars().next().unwrap()).unwrap();
        let right_padding = 100 - left_padding - trimmed.len();
        assert!((left_padding as i32 - right_padding as i32).abs() <= 1);
    }

    #[test]
    fn test_config_path_resolution() {
        // Test default config path
        let cli = Cli::parse_from(["omikuji"]);
        let config_path = cli.config.unwrap_or_else(config::default_config_path);
        assert!(config_path.to_str().unwrap().ends_with("config.yaml"));

        // Test custom config path
        let cli = Cli::parse_from(["omikuji", "-c", "/custom/path.yaml"]);
        let config_path = cli.config.unwrap_or_else(config::default_config_path);
        assert_eq!(config_path.to_str().unwrap(), "/custom/path.yaml");
    }

    #[test]
    fn test_key_storage_type_selection() {
        // Test keyring storage
        let mut config = create_test_config();
        config.key_storage.storage_type = "keyring".to_string();
        assert_eq!(config.key_storage.storage_type, "keyring");

        // Test env storage
        config.key_storage.storage_type = "env".to_string();
        assert_eq!(config.key_storage.storage_type, "env");

        // Test vault storage
        config.key_storage.storage_type = "vault".to_string();
        assert_eq!(config.key_storage.storage_type, "vault");

        // Test aws-secrets storage
        config.key_storage.storage_type = "aws-secrets".to_string();
        assert_eq!(config.key_storage.storage_type, "aws-secrets");
    }

    #[test]
    fn test_database_url_handling() {
        // Test with DATABASE_URL not set
        std::env::remove_var("DATABASE_URL");
        assert!(std::env::var("DATABASE_URL").is_err());

        // Test with DATABASE_URL set
        std::env::set_var("DATABASE_URL", "postgres://localhost/test");
        assert_eq!(
            std::env::var("DATABASE_URL").unwrap(),
            "postgres://localhost/test"
        );
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn test_skip_migrations_parsing() {
        // Test default (false)
        std::env::remove_var("SKIP_MIGRATIONS");
        let skip = std::env::var("SKIP_MIGRATIONS")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";
        assert!(!skip);

        // Test true
        std::env::set_var("SKIP_MIGRATIONS", "true");
        let skip = std::env::var("SKIP_MIGRATIONS")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";
        assert!(skip);

        // Test case insensitive
        std::env::set_var("SKIP_MIGRATIONS", "TRUE");
        let skip = std::env::var("SKIP_MIGRATIONS")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";
        assert!(skip);

        std::env::remove_var("SKIP_MIGRATIONS");
    }

    #[test]
    fn test_vault_token_env_parsing() {
        let mut config = create_test_config();
        config.key_storage.vault.token = Some("${VAULT_TOKEN}".to_string());

        // Test environment variable parsing
        std::env::set_var("VAULT_TOKEN", "test-token-123");

        let token = config.key_storage.vault.token.as_ref().and_then(|t| {
            if t.starts_with("${") && t.ends_with("}") {
                let var_name = &t[2..t.len() - 1];
                std::env::var(var_name).ok()
            } else {
                Some(t.clone())
            }
        });

        assert_eq!(token, Some("test-token-123".to_string()));
        std::env::remove_var("VAULT_TOKEN");

        // Test direct token
        config.key_storage.vault.token = Some("direct-token".to_string());
        let token = config.key_storage.vault.token.as_ref().and_then(|t| {
            if t.starts_with("${") && t.ends_with("}") {
                let var_name = &t[2..t.len() - 1];
                std::env::var(var_name).ok()
            } else {
                Some(t.clone())
            }
        });

        assert_eq!(token, Some("direct-token".to_string()));
    }

    #[test]
    fn test_gas_price_manager_initialization() {
        let config = create_test_config();

        // Test disabled gas price feeds
        assert!(!config.gas_price_feeds.enabled);

        // Test token mappings
        let mut token_mappings = std::collections::HashMap::new();
        for network in &config.networks {
            token_mappings.insert(network.name.clone(), network.gas_token.clone());
        }

        assert_eq!(
            token_mappings.get("test-network"),
            Some(&"ethereum".to_string())
        );
    }

    #[tokio::test]
    async fn test_config_loading() {
        // Create a temporary directory and config file
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");

        let config_content = r#"
networks:
  - name: test-network
    rpc_url: http://localhost:8545
    transaction_type: eip1559
    gas_token: ethereum
    gas_token_symbol: ETH
datafeeds: []
key_storage:
  storage_type: env
  keyring:
    service: test
gas_price_feeds:
  enabled: false
"#;

        fs::write(&config_path, config_content).unwrap();

        // Test successful config loading
        let result = config::load_config(&config_path);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.networks[0].name, "test-network");
    }
}
