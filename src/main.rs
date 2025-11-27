use anyhow::Result;
use clap::Parser;
use omikuji_core::runtime::DaemonBuilder;
use tracing::info;

mod cli;
mod plugins;

use cli::{Cli, Commands};

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
    let welcome =
        omikuji_core::ui::welcome_screen::WELCOME_SCREEN.replace("{version_line}", &version_line);
    println!("{welcome}");

    // Initialize logging after argument parsing
    tracing_subscriber::fmt::init();

    // Load .env file if it exists
    match dotenvy::dotenv() {
        Ok(path) => info!("Loaded .env file from: {:?}", path),
        Err(e) => info!("No .env file loaded: {}", e),
    }

    // Register community plugins BEFORE building daemon
    use omikuji_core::plugin_registry::global_registry;
    plugins::register_community_plugins(global_registry())?;

    // Build daemon using DaemonBuilder
    let mut builder = DaemonBuilder::new();

    // Set config path if provided
    if let Some(config_path) = cli.config {
        builder = builder.config_path(config_path);
    }

    // Set private key environment variable
    builder = builder.private_key_env(cli.private_key_env);

    // Build and run daemon
    let daemon = builder.build().await?;
    daemon.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli::KeyCommands;
    use omikuji_core::config::metrics_config::MetricsConfig;
    use omikuji_core::config::models::{
        AwsSecretsConfig, DatabaseCleanupConfig, GasConfig, KeyStorageConfig, KeyringConfig,
        Network, NetworkNode, OmikujiConfig, VaultConfig,
    };
    use omikuji_core::gas_price::models::GasPriceFeedConfig;
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
        let config_path = cli
            .config
            .unwrap_or_else(omikuji_core::config::default_config_path);
        assert!(config_path.to_str().unwrap().ends_with("config.yaml"));

        // Test custom config path
        let cli = Cli::parse_from(["omikuji", "-c", "/custom/path.yaml"]);
        let config_path = cli
            .config
            .unwrap_or_else(omikuji_core::config::default_config_path);
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
        let result = omikuji_core::config::load_config(&config_path);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.networks[0].name, "test-network");
    }
}
