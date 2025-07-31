//! Test data builders for creating common test objects

use crate::config::metrics_config::MetricsConfig;
use crate::config::models::{
    DatabaseCleanupConfig, GasConfig, Network, NetworkNode, OmikujiConfig,
};
use crate::gas_price::models::GasPriceFeedConfig;

/// Builder for creating test Network configurations
#[derive(Debug, Clone)]
pub struct NetworkBuilder {
    name: String,
    rpc_url: String,
    transaction_type: String,
    gas_config: GasConfig,
    gas_token: String,
    gas_token_symbol: String,
}

impl NetworkBuilder {
    /// Create a new NetworkBuilder with default test values
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            rpc_url: format!("http://localhost:8545/{name}"),
            transaction_type: "eip1559".to_string(),
            gas_config: GasConfig::default(),
            gas_token: "ethereum".to_string(),
            gas_token_symbol: "ETH".to_string(),
        }
    }

    /// Set the RPC URL for this network
    pub fn with_rpc_url(mut self, url: &str) -> Self {
        self.rpc_url = url.to_string();
        self
    }

    /// Set the transaction type (legacy, eip1559, etc.)
    pub fn with_transaction_type(mut self, tx_type: &str) -> Self {
        self.transaction_type = tx_type.to_string();
        self
    }

    /// Set the gas configuration
    pub fn with_gas_config(mut self, gas_config: GasConfig) -> Self {
        self.gas_config = gas_config;
        self
    }

    /// Set the gas token details
    pub fn with_gas_token(mut self, token: &str, symbol: &str) -> Self {
        self.gas_token = token.to_string();
        self.gas_token_symbol = symbol.to_string();
        self
    }

    /// Build the Network configuration
    pub fn build(self) -> Network {
        Network {
            name: self.name,
            nodes: vec![NetworkNode {
                name: "Default Node".to_string(),
                rpc_url: self.rpc_url,
                ws_url: None,
            }],
            transaction_type: self.transaction_type,
            gas_config: self.gas_config,
            gas_token: self.gas_token,
            gas_token_symbol: self.gas_token_symbol,
            balance_alerts: None,
            rpc_url: None,
            ws_url: None,
        }
    }

    /// Create a common Ethereum mainnet test network
    pub fn ethereum_mainnet() -> Network {
        Self::new("ethereum-mainnet")
            .with_rpc_url("https://eth-mainnet.alchemyapi.io/v2/test")
            .with_gas_token("ethereum", "ETH")
            .build()
    }

    /// Create a common BASE test network
    pub fn base_mainnet() -> Network {
        Self::new("base-mainnet")
            .with_rpc_url("https://base-mainnet.g.alchemy.com/v2/test")
            .with_gas_token("ethereum", "ETH")
            .build()
    }
}

/// Builder for creating test OmikujiConfig objects
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    networks: Vec<Network>,
    database_cleanup: DatabaseCleanupConfig,
    metrics: MetricsConfig,
    gas_price_feeds: GasPriceFeedConfig,
    scheduled_tasks: Vec<crate::scheduled_tasks::models::ScheduledTask>,
}

impl ConfigBuilder {
    /// Create a new ConfigBuilder with default test values
    pub fn new() -> Self {
        Self {
            networks: vec![],
            database_cleanup: DatabaseCleanupConfig {
                enabled: false,
                schedule: "0 2 * * *".to_string(),
            },
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
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

    /// Set the database cleanup configuration
    pub fn with_database_cleanup(mut self, config: DatabaseCleanupConfig) -> Self {
        self.database_cleanup = config;
        self
    }

    /// Set the metrics configuration
    pub fn with_metrics(mut self, metrics: MetricsConfig) -> Self {
        self.metrics = metrics;
        self
    }

    /// Set the gas price feeds configuration
    pub fn with_gas_price_feeds(mut self, config: GasPriceFeedConfig) -> Self {
        self.gas_price_feeds = config;
        self
    }

    /// Add a scheduled task
    pub fn add_scheduled_task(
        mut self,
        task: crate::scheduled_tasks::models::ScheduledTask,
    ) -> Self {
        self.scheduled_tasks.push(task);
        self
    }

    /// Build the OmikujiConfig
    pub fn build(self) -> OmikujiConfig {
        OmikujiConfig {
            networks: self.networks,
            datafeeds: vec![], // Empty datafeeds for test configs
            database_cleanup: self.database_cleanup,
            key_storage: Default::default(),
            metrics: self.metrics,
            gas_price_feeds: self.gas_price_feeds,
            scheduled_tasks: self.scheduled_tasks,
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        }
    }

    /// Create a minimal test configuration with common defaults
    pub fn minimal() -> OmikujiConfig {
        Self::new()
            .add_network(NetworkBuilder::ethereum_mainnet())
            .build()
    }

    /// Create a multi-network test configuration
    pub fn multi_network() -> OmikujiConfig {
        Self::new()
            .add_networks(vec![
                NetworkBuilder::ethereum_mainnet(),
                NetworkBuilder::base_mainnet(),
            ])
            .build()
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating test GasConfig objects
#[derive(Debug, Clone)]
pub struct GasConfigBuilder {
    gas_limit: Option<u64>,
    gas_price_gwei: Option<f64>,
    max_fee_per_gas_gwei: Option<f64>,
    max_priority_fee_per_gas_gwei: Option<f64>,
    gas_multiplier: f64,
    fee_bumping: crate::config::models::FeeBumpingConfig,
}

impl GasConfigBuilder {
    /// Create a new GasConfigBuilder with default values
    pub fn new() -> Self {
        Self {
            gas_limit: None,
            gas_price_gwei: None,
            max_fee_per_gas_gwei: None,
            max_priority_fee_per_gas_gwei: None,
            gas_multiplier: 1.2,
            fee_bumping: Default::default(),
        }
    }

    /// Set manual gas limit
    pub fn with_gas_limit(mut self, limit: u64) -> Self {
        self.gas_limit = Some(limit);
        self
    }

    /// Set manual gas price for legacy transactions (in gwei)
    pub fn with_gas_price(mut self, price: f64) -> Self {
        self.gas_price_gwei = Some(price);
        self
    }

    /// Set manual max fee per gas for EIP-1559 transactions (in gwei)
    pub fn with_max_fee_per_gas(mut self, fee: f64) -> Self {
        self.max_fee_per_gas_gwei = Some(fee);
        self
    }

    /// Set manual max priority fee per gas for EIP-1559 transactions (in gwei)
    pub fn with_max_priority_fee_per_gas(mut self, fee: f64) -> Self {
        self.max_priority_fee_per_gas_gwei = Some(fee);
        self
    }

    /// Set gas multiplier
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.gas_multiplier = multiplier;
        self
    }

    /// Set fee bumping configuration
    pub fn with_fee_bumping(mut self, config: crate::config::models::FeeBumpingConfig) -> Self {
        self.fee_bumping = config;
        self
    }

    /// Build the GasConfig
    pub fn build(self) -> GasConfig {
        GasConfig {
            gas_limit: self.gas_limit,
            gas_price_gwei: self.gas_price_gwei,
            max_fee_per_gas_gwei: self.max_fee_per_gas_gwei,
            max_priority_fee_per_gas_gwei: self.max_priority_fee_per_gas_gwei,
            gas_multiplier: self.gas_multiplier,
            fee_bumping: self.fee_bumping,
        }
    }
}

impl Default for GasConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_builder() {
        let network = NetworkBuilder::new("test-network")
            .with_rpc_url("http://localhost:8545")
            .with_transaction_type("legacy")
            .with_gas_token("ethereum", "ETH")
            .build();

        assert_eq!(network.name, "test-network");
        assert_eq!(network.nodes[0].rpc_url, "http://localhost:8545");
        assert_eq!(network.transaction_type, "legacy");
        assert_eq!(network.gas_token, "ethereum");
        assert_eq!(network.gas_token_symbol, "ETH");
    }

    #[test]
    fn test_network_builder_presets() {
        let eth_network = NetworkBuilder::ethereum_mainnet();
        assert_eq!(eth_network.name, "ethereum-mainnet");
        assert_eq!(eth_network.gas_token_symbol, "ETH");

        let base_network = NetworkBuilder::base_mainnet();
        assert_eq!(base_network.name, "base-mainnet");
        assert_eq!(base_network.gas_token_symbol, "ETH");
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .add_network(NetworkBuilder::new("test").build())
            .build();

        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.networks[0].name, "test");
        assert!(!config.database_cleanup.enabled);
    }

    #[test]
    fn test_config_builder_minimal() {
        let config = ConfigBuilder::minimal();
        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.networks[0].name, "ethereum-mainnet");
    }

    #[test]
    fn test_config_builder_multi_network() {
        let config = ConfigBuilder::multi_network();
        assert_eq!(config.networks.len(), 2);
        assert_eq!(config.networks[0].name, "ethereum-mainnet");
        assert_eq!(config.networks[1].name, "base-mainnet");
    }

    #[test]
    fn test_gas_config_builder() {
        let gas_config = GasConfigBuilder::new()
            .with_gas_limit(100000)
            .with_gas_price(20.0)
            .with_multiplier(1.5)
            .build();

        assert_eq!(gas_config.gas_limit, Some(100000));
        assert_eq!(gas_config.gas_price_gwei, Some(20.0));
        assert_eq!(gas_config.gas_multiplier, 1.5);
    }
}
