//! Builder patterns for complex configuration objects
//!
//! This module provides builder patterns for creating complex configuration objects
//! with sensible defaults and validation. These builders make it easier to create
//! configurations programmatically and reduce boilerplate code.

use super::metrics_config::MetricsConfig;
use super::models::*;
use crate::gas_price::models::GasPriceFeedConfig;
use crate::scheduled_tasks::models::ScheduledTask;
use alloy::primitives::I256;
use serde_json::Value;

/// Builder for creating OmikujiConfig objects with a fluent API
#[derive(Debug, Clone)]
pub struct OmikujiConfigBuilder {
    networks: Vec<Network>,
    datafeeds: Vec<Datafeed>,
    database_cleanup: DatabaseCleanupConfig,
    key_storage: KeyStorageConfig,
    metrics: MetricsConfig,
    gas_price_feeds: GasPriceFeedConfig,
    scheduled_tasks: Vec<ScheduledTask>,
}

impl OmikujiConfigBuilder {
    /// Create a new OmikujiConfigBuilder with default values
    pub fn new() -> Self {
        Self {
            networks: Vec::new(),
            datafeeds: Vec::new(),
            database_cleanup: DatabaseCleanupConfig::default(),
            key_storage: KeyStorageConfig::default(),
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: Vec::new(),
        }
    }

    /// Add a network to the configuration
    pub fn add_network(mut self, network: Network) -> Self {
        self.networks.push(network);
        self
    }

    /// Add multiple networks to the configuration
    pub fn add_networks(mut self, networks: Vec<Network>) -> Self {
        self.networks.extend(networks);
        self
    }

    /// Add a datafeed to the configuration
    pub fn add_datafeed(mut self, datafeed: Datafeed) -> Self {
        self.datafeeds.push(datafeed);
        self
    }

    /// Add multiple datafeeds to the configuration
    pub fn add_datafeeds(mut self, datafeeds: Vec<Datafeed>) -> Self {
        self.datafeeds.extend(datafeeds);
        self
    }

    /// Set the database cleanup configuration
    pub fn with_database_cleanup(mut self, config: DatabaseCleanupConfig) -> Self {
        self.database_cleanup = config;
        self
    }

    /// Set the key storage configuration
    pub fn with_key_storage(mut self, config: KeyStorageConfig) -> Self {
        self.key_storage = config;
        self
    }

    /// Set the metrics configuration
    pub fn with_metrics(mut self, config: MetricsConfig) -> Self {
        self.metrics = config;
        self
    }

    /// Set the gas price feeds configuration
    pub fn with_gas_price_feeds(mut self, config: GasPriceFeedConfig) -> Self {
        self.gas_price_feeds = config;
        self
    }

    /// Add a scheduled task to the configuration
    pub fn add_scheduled_task(mut self, task: ScheduledTask) -> Self {
        self.scheduled_tasks.push(task);
        self
    }

    /// Add multiple scheduled tasks to the configuration
    pub fn add_scheduled_tasks(mut self, tasks: Vec<ScheduledTask>) -> Self {
        self.scheduled_tasks.extend(tasks);
        self
    }

    /// Build the OmikujiConfig
    pub fn build(self) -> OmikujiConfig {
        OmikujiConfig {
            networks: self.networks,
            datafeeds: self.datafeeds,
            database_cleanup: self.database_cleanup,
            key_storage: self.key_storage,
            metrics: self.metrics,
            gas_price_feeds: self.gas_price_feeds,
            scheduled_tasks: self.scheduled_tasks,
            event_monitors: Vec::new(),
        }
    }
}

impl Default for OmikujiConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating Network configurations with a fluent API
#[derive(Debug, Clone)]
pub struct NetworkBuilder {
    name: String,
    rpc_url: String,
    ws_url: Option<String>,
    transaction_type: String,
    gas_config: GasConfig,
    gas_token: String,
    gas_token_symbol: String,
}

impl NetworkBuilder {
    /// Create a new NetworkBuilder with default values
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: None,
            transaction_type: "eip1559".to_string(),
            gas_config: GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
        }
    }

    /// Set the RPC URL for this network
    pub fn with_rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = url.into();
        self
    }

    /// Set the WebSocket URL for this network
    pub fn with_ws_url(mut self, url: impl Into<String>) -> Self {
        self.ws_url = Some(url.into());
        self
    }

    /// Set the transaction type (legacy, eip1559)
    pub fn with_transaction_type(mut self, tx_type: impl Into<String>) -> Self {
        self.transaction_type = tx_type.into();
        self
    }

    /// Set the gas configuration
    pub fn with_gas_config(mut self, config: GasConfig) -> Self {
        self.gas_config = config;
        self
    }

    /// Set the gas token details
    pub fn with_gas_token(mut self, token: impl Into<String>, symbol: impl Into<String>) -> Self {
        self.gas_token = token.into();
        self.gas_token_symbol = symbol.into();
        self
    }

    /// Build the Network configuration
    pub fn build(self) -> Network {
        Network {
            name: self.name,
            nodes: vec![crate::config::models::NetworkNode {
                name: "Default Node".to_string(),
                rpc_url: self.rpc_url,
                ws_url: None,
            }],
            transaction_type: self.transaction_type,
            gas_config: self.gas_config,
            gas_token: self.gas_token,
            gas_token_symbol: self.gas_token_symbol,
            rpc_url: None,
            ws_url: None,
        }
    }

    /// Create a common Ethereum mainnet configuration
    pub fn ethereum_mainnet(rpc_url: impl Into<String>) -> Network {
        Self::new("ethereum-mainnet")
            .with_rpc_url(rpc_url)
            .with_gas_token("ethereum", "ETH")
            .build()
    }

    /// Create a common BASE mainnet configuration
    pub fn base_mainnet(rpc_url: impl Into<String>) -> Network {
        Self::new("base-mainnet")
            .with_rpc_url(rpc_url)
            .with_gas_token("ethereum", "ETH")
            .build()
    }

    /// Create a common Polygon mainnet configuration
    pub fn polygon_mainnet(rpc_url: impl Into<String>) -> Network {
        Self::new("polygon-mainnet")
            .with_rpc_url(rpc_url)
            .with_gas_token("polygon", "MATIC")
            .build()
    }

    /// Create a common development/localhost configuration
    pub fn localhost(port: u16) -> Network {
        Self::new("localhost")
            .with_rpc_url(format!("http://localhost:{port}"))
            .with_transaction_type("legacy")
            .build()
    }
}

/// Builder for creating Datafeed configurations with a fluent API
#[derive(Debug, Clone)]
pub struct DatafeedBuilder {
    name: String,
    networks: String,
    check_frequency: u64,
    contract_address: String,
    contract_type: String,
    read_contract_config: bool,
    minimum_update_frequency: u64,
    deviation_threshold_pct: f64,
    feed_url: String,
    feed_json_path: String,
    feed_json_path_timestamp: Option<String>,
    decimals: Option<u8>,
    min_value: Option<I256>,
    max_value: Option<I256>,
    data_retention_days: u32,
}

impl DatafeedBuilder {
    /// Create a new DatafeedBuilder with default values
    pub fn new(name: impl Into<String>, network: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            networks: network.into(),
            check_frequency: 60, // 1 minute
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            contract_type: "fluxmon".to_string(),
            read_contract_config: true,
            minimum_update_frequency: 300, // 5 minutes
            deviation_threshold_pct: 0.5,  // 0.5%
            feed_url: "https://api.example.com/price".to_string(),
            feed_json_path: "$.price".to_string(),
            feed_json_path_timestamp: None,
            decimals: None,
            min_value: None,
            max_value: None,
            data_retention_days: 7,
        }
    }

    /// Set the check frequency in seconds
    pub fn with_check_frequency(mut self, seconds: u64) -> Self {
        self.check_frequency = seconds;
        self
    }

    /// Set the contract address
    pub fn with_contract_address(mut self, address: impl Into<String>) -> Self {
        self.contract_address = address.into();
        self
    }

    /// Set the contract type
    pub fn with_contract_type(mut self, contract_type: impl Into<String>) -> Self {
        self.contract_type = contract_type.into();
        self
    }

    /// Set whether to read configuration from the contract
    pub fn with_read_contract_config(mut self, read_config: bool) -> Self {
        self.read_contract_config = read_config;
        self
    }

    /// Set the minimum update frequency in seconds
    pub fn with_minimum_update_frequency(mut self, seconds: u64) -> Self {
        self.minimum_update_frequency = seconds;
        self
    }

    /// Set the deviation threshold percentage
    pub fn with_deviation_threshold(mut self, percent: f64) -> Self {
        self.deviation_threshold_pct = percent;
        self
    }

    /// Set the feed URL
    pub fn with_feed_url(mut self, url: impl Into<String>) -> Self {
        self.feed_url = url.into();
        self
    }

    /// Set the JSON path for extracting the price
    pub fn with_feed_json_path(mut self, path: impl Into<String>) -> Self {
        self.feed_json_path = path.into();
        self
    }

    /// Set the JSON path for extracting the timestamp
    pub fn with_feed_json_path_timestamp(mut self, path: Option<String>) -> Self {
        self.feed_json_path_timestamp = path;
        self
    }

    /// Set the number of decimals
    pub fn with_decimals(mut self, decimals: u8) -> Self {
        self.decimals = Some(decimals);
        self
    }

    /// Set the minimum valid value
    pub fn with_min_value(mut self, min: I256) -> Self {
        self.min_value = Some(min);
        self
    }

    /// Set the maximum valid value
    pub fn with_max_value(mut self, max: I256) -> Self {
        self.max_value = Some(max);
        self
    }

    /// Set the data retention period in days
    pub fn with_data_retention_days(mut self, days: u32) -> Self {
        self.data_retention_days = days;
        self
    }

    /// Build the Datafeed configuration
    pub fn build(self) -> Datafeed {
        Datafeed {
            name: self.name,
            networks: self.networks,
            check_frequency: self.check_frequency,
            contract_address: self.contract_address,
            contract_type: self.contract_type,
            read_contract_config: self.read_contract_config,
            minimum_update_frequency: self.minimum_update_frequency,
            deviation_threshold_pct: self.deviation_threshold_pct,
            feed_url: self.feed_url,
            feed_json_path: self.feed_json_path,
            feed_json_path_timestamp: self.feed_json_path_timestamp,
            decimals: self.decimals,
            min_value: self.min_value,
            max_value: self.max_value,
            data_retention_days: self.data_retention_days,
        }
    }

    /// Create a common ETH/USD price feed configuration
    pub fn eth_usd_feed(
        network: impl Into<String>,
        contract_address: impl Into<String>,
    ) -> Datafeed {
        Self::new("eth_usd", network)
            .with_contract_address(contract_address)
            .with_feed_url(
                "https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd",
            )
            .with_feed_json_path("$.ethereum.usd")
            .with_decimals(8)
            .with_min_value(I256::try_from(100_000_000).unwrap()) // $1 with 8 decimals
            .with_max_value(I256::try_from(1_000_000_000_000u64).unwrap()) // $10,000 with 8 decimals
            .build()
    }

    /// Create a common BTC/USD price feed configuration
    pub fn btc_usd_feed(
        network: impl Into<String>,
        contract_address: impl Into<String>,
    ) -> Datafeed {
        Self::new("btc_usd", network)
            .with_contract_address(contract_address)
            .with_feed_url(
                "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd",
            )
            .with_feed_json_path("$.bitcoin.usd")
            .with_decimals(8)
            .with_min_value(I256::try_from(1_000_000_000u64).unwrap()) // $10 with 8 decimals
            .with_max_value(I256::try_from(10_000_000_000_000u64).unwrap()) // $100,000 with 8 decimals
            .build()
    }
}

/// Builder for creating ScheduledTask configurations with a fluent API
#[derive(Debug, Clone)]
pub struct ScheduledTaskBuilder {
    name: String,
    network: String,
    schedule: String,
    check_condition: Option<crate::scheduled_tasks::models::CheckCondition>,
    target_function: Option<crate::scheduled_tasks::models::TargetFunction>,
    gas_config: Option<crate::scheduled_tasks::models::GasConfig>,
}

impl ScheduledTaskBuilder {
    /// Create a new ScheduledTaskBuilder with default values
    pub fn new(name: impl Into<String>, network: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            network: network.into(),
            schedule: "0 0 * * * *".to_string(), // Every hour
            check_condition: None,
            target_function: None,
            gas_config: None,
        }
    }

    /// Set the cron schedule expression
    pub fn with_schedule(mut self, schedule: impl Into<String>) -> Self {
        self.schedule = schedule.into();
        self
    }

    /// Set a daily schedule at a specific hour
    pub fn daily_at_hour(mut self, hour: u8) -> Self {
        self.schedule = format!("0 0 {hour} * * *");
        self
    }

    /// Set an hourly schedule
    pub fn hourly(mut self) -> Self {
        self.schedule = "0 0 * * * *".to_string();
        self
    }

    /// Set a schedule to run every N minutes
    pub fn every_minutes(mut self, minutes: u8) -> Self {
        self.schedule = format!("0 */{minutes} * * * *");
        self
    }

    /// Set a property-based check condition
    pub fn with_property_condition(
        mut self,
        contract_address: impl Into<String>,
        property: impl Into<String>,
        expected_value: Value,
    ) -> Self {
        self.check_condition = Some(crate::scheduled_tasks::models::CheckCondition::Property {
            contract_address: contract_address.into(),
            property: property.into(),
            expected_value,
        });
        self
    }

    /// Set a function-based check condition
    pub fn with_function_condition(
        mut self,
        contract_address: impl Into<String>,
        function: impl Into<String>,
        expected_value: Value,
    ) -> Self {
        self.check_condition = Some(crate::scheduled_tasks::models::CheckCondition::Function {
            contract_address: contract_address.into(),
            function: function.into(),
            expected_value,
        });
        self
    }

    /// Set the target function to call
    pub fn with_target_function(
        mut self,
        contract_address: impl Into<String>,
        function: impl Into<String>,
    ) -> Self {
        self.target_function = Some(crate::scheduled_tasks::models::TargetFunction {
            contract_address: contract_address.into(),
            function: function.into(),
            parameters: Vec::new(),
        });
        self
    }

    /// Add a parameter to the target function
    pub fn add_parameter(mut self, param_type: impl Into<String>, value: Value) -> Self {
        if let Some(ref mut target_function) = self.target_function {
            target_function
                .parameters
                .push(crate::scheduled_tasks::models::Parameter {
                    param_type: param_type.into(),
                    value,
                });
        }
        self
    }

    /// Set the gas configuration
    pub fn with_gas_config(mut self, config: crate::scheduled_tasks::models::GasConfig) -> Self {
        self.gas_config = Some(config);
        self
    }

    /// Set gas configuration with specific values
    pub fn with_gas_settings(
        mut self,
        gas_limit: Option<u64>,
        max_gas_price_gwei: Option<u64>,
        priority_fee_gwei: Option<u64>,
    ) -> Self {
        self.gas_config = Some(crate::scheduled_tasks::models::GasConfig {
            gas_limit,
            max_gas_price_gwei,
            priority_fee_gwei,
        });
        self
    }

    /// Build the ScheduledTask configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the target function is not set or if the configuration is invalid
    pub fn build(self) -> Result<ScheduledTask, String> {
        let target_function = self.target_function.ok_or("Target function is required")?;

        let task = ScheduledTask {
            name: self.name,
            network: self.network,
            schedule: self.schedule,
            check_condition: self.check_condition,
            target_function,
            gas_config: self.gas_config,
        };

        // Validate the task before returning
        task.validate()?;
        Ok(task)
    }
}

/// Builder for creating KeyStorageConfig configurations with a fluent API
#[derive(Debug, Clone)]
pub struct KeyStorageConfigBuilder {
    storage_type: String,
    keyring: KeyringConfig,
    vault: VaultConfig,
    aws_secrets: AwsSecretsConfig,
}

impl KeyStorageConfigBuilder {
    /// Create a new KeyStorageConfigBuilder with default values
    pub fn new() -> Self {
        Self {
            storage_type: "env".to_string(),
            keyring: KeyringConfig::default(),
            vault: VaultConfig::default(),
            aws_secrets: AwsSecretsConfig::default(),
        }
    }

    /// Configure for environment variable storage
    pub fn env_storage(mut self) -> Self {
        self.storage_type = "env".to_string();
        self
    }

    /// Configure for keyring storage
    pub fn keyring_storage(mut self, service: Option<String>) -> Self {
        self.storage_type = "keyring".to_string();
        if let Some(service) = service {
            self.keyring.service = service;
        }
        self
    }

    /// Configure for Vault storage
    pub fn vault_storage(mut self, url: impl Into<String>, token: Option<String>) -> Self {
        self.storage_type = "vault".to_string();
        self.vault.url = url.into();
        self.vault.token = token;
        self
    }

    /// Configure for AWS Secrets Manager storage
    pub fn aws_secrets_storage(mut self, region: Option<String>, prefix: Option<String>) -> Self {
        self.storage_type = "aws-secrets".to_string();
        if let Some(region) = region {
            self.aws_secrets.region = Some(region);
        }
        if let Some(prefix) = prefix {
            self.aws_secrets.prefix = prefix;
        }
        self
    }

    /// Set Vault configuration details
    pub fn with_vault_config(
        mut self,
        mount_path: impl Into<String>,
        path_prefix: impl Into<String>,
        auth_method: impl Into<String>,
    ) -> Self {
        self.vault.mount_path = mount_path.into();
        self.vault.path_prefix = path_prefix.into();
        self.vault.auth_method = auth_method.into();
        self
    }

    /// Set cache TTL for Vault or AWS Secrets
    pub fn with_cache_ttl(mut self, ttl_seconds: u64) -> Self {
        self.vault.cache_ttl_seconds = ttl_seconds;
        self.aws_secrets.cache_ttl_seconds = ttl_seconds;
        self
    }

    /// Build the KeyStorageConfig
    pub fn build(self) -> KeyStorageConfig {
        KeyStorageConfig {
            storage_type: self.storage_type,
            keyring: self.keyring,
            vault: self.vault,
            aws_secrets: self.aws_secrets,
        }
    }
}

impl Default for KeyStorageConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_omikuji_config_builder() {
        let network = NetworkBuilder::new("test-network")
            .with_rpc_url("https://test.example.com")
            .build();

        let datafeed = DatafeedBuilder::new("test_feed", "test-network")
            .with_contract_address("0x1234567890123456789012345678901234567890")
            .build();

        let config = OmikujiConfigBuilder::new()
            .add_network(network)
            .add_datafeed(datafeed)
            .build();

        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.datafeeds.len(), 1);
        assert_eq!(config.networks[0].name, "test-network");
        assert_eq!(config.datafeeds[0].name, "test_feed");
    }

    #[test]
    fn test_network_builder() {
        let network = NetworkBuilder::new("ethereum-mainnet")
            .with_rpc_url("https://eth-mainnet.alchemyapi.io/v2/test")
            .with_gas_token("ethereum", "ETH")
            .build();

        assert_eq!(network.name, "ethereum-mainnet");
        assert_eq!(network.rpc_url, "https://eth-mainnet.alchemyapi.io/v2/test");
        assert_eq!(network.gas_token, "ethereum");
        assert_eq!(network.gas_token_symbol, "ETH");
    }

    #[test]
    fn test_network_builder_presets() {
        let eth_network = NetworkBuilder::ethereum_mainnet("https://eth.example.com");
        assert_eq!(eth_network.name, "ethereum-mainnet");
        assert_eq!(eth_network.gas_token_symbol, "ETH");

        let base_network = NetworkBuilder::base_mainnet("https://base.example.com");
        assert_eq!(base_network.name, "base-mainnet");
        assert_eq!(base_network.gas_token_symbol, "ETH");

        let polygon_network = NetworkBuilder::polygon_mainnet("https://polygon.example.com");
        assert_eq!(polygon_network.name, "polygon-mainnet");
        assert_eq!(polygon_network.gas_token_symbol, "MATIC");

        let localhost = NetworkBuilder::localhost(8545);
        assert_eq!(localhost.name, "localhost");
        assert_eq!(localhost.rpc_url, "http://localhost:8545");
        assert_eq!(localhost.transaction_type, "legacy");
    }

    #[test]
    fn test_datafeed_builder() {
        let datafeed = DatafeedBuilder::new("eth_usd", "ethereum-mainnet")
            .with_contract_address("0x1234567890123456789012345678901234567890")
            .with_check_frequency(30)
            .with_deviation_threshold(1.0)
            .build();

        assert_eq!(datafeed.name, "eth_usd");
        assert_eq!(datafeed.networks, "ethereum-mainnet");
        assert_eq!(datafeed.check_frequency, 30);
        assert_eq!(datafeed.deviation_threshold_pct, 1.0);
    }

    #[test]
    fn test_datafeed_builder_presets() {
        let eth_feed = DatafeedBuilder::eth_usd_feed(
            "ethereum-mainnet",
            "0x1234567890123456789012345678901234567890",
        );
        assert_eq!(eth_feed.name, "eth_usd");
        assert_eq!(eth_feed.decimals, Some(8));
        assert!(eth_feed.feed_url.contains("ethereum"));

        let btc_feed = DatafeedBuilder::btc_usd_feed(
            "ethereum-mainnet",
            "0x1234567890123456789012345678901234567890",
        );
        assert_eq!(btc_feed.name, "btc_usd");
        assert_eq!(btc_feed.decimals, Some(8));
        assert!(btc_feed.feed_url.contains("bitcoin"));
    }

    #[test]
    fn test_scheduled_task_builder() {
        let task = ScheduledTaskBuilder::new("test_task", "ethereum-mainnet")
            .with_target_function("0x1234567890123456789012345678901234567890", "execute()")
            .daily_at_hour(2)
            .build()
            .unwrap();

        assert_eq!(task.name, "test_task");
        assert_eq!(task.network, "ethereum-mainnet");
        assert_eq!(task.schedule, "0 0 2 * * *");
        assert_eq!(task.target_function.function, "execute()");
    }

    #[test]
    fn test_scheduled_task_builder_with_conditions() {
        let task = ScheduledTaskBuilder::new("conditional_task", "ethereum-mainnet")
            .with_property_condition(
                "0x1234567890123456789012345678901234567890",
                "isReady",
                json!(true),
            )
            .with_target_function("0x1234567890123456789012345678901234567890", "execute()")
            .every_minutes(15)
            .build()
            .unwrap();

        assert!(task.check_condition.is_some());
        assert_eq!(task.schedule, "0 */15 * * * *");
    }

    #[test]
    fn test_key_storage_config_builder() {
        let config = KeyStorageConfigBuilder::new()
            .vault_storage("https://vault.example.com", Some("test-token".to_string()))
            .with_cache_ttl(600)
            .build();

        assert_eq!(config.storage_type, "vault");
        assert_eq!(config.vault.url, "https://vault.example.com");
        assert_eq!(config.vault.token, Some("test-token".to_string()));
        assert_eq!(config.vault.cache_ttl_seconds, 600);
    }

    #[test]
    fn test_key_storage_config_builder_aws() {
        let config = KeyStorageConfigBuilder::new()
            .aws_secrets_storage(Some("us-west-2".to_string()), Some("myapp/".to_string()))
            .with_cache_ttl(300)
            .build();

        assert_eq!(config.storage_type, "aws-secrets");
        assert_eq!(config.aws_secrets.region, Some("us-west-2".to_string()));
        assert_eq!(config.aws_secrets.prefix, "myapp/");
        assert_eq!(config.aws_secrets.cache_ttl_seconds, 300);
    }

    #[test]
    fn test_scheduled_task_builder_fails_without_target_function() {
        let result = ScheduledTaskBuilder::new("test_task", "ethereum-mainnet")
            .daily_at_hour(2)
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Target function is required"));
    }
}
