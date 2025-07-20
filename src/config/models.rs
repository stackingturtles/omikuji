use alloy::primitives::I256;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use super::metrics_config::MetricsConfig;
use crate::event_monitors::models::EventMonitor;
use crate::gas_price::models::GasPriceFeedConfig;
use crate::scheduled_tasks::models::ScheduledTask;

/// The main configuration structure for Omikuji
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OmikujiConfig {
    /// Networks supported by this Omikuji instance
    #[validate]
    pub networks: Vec<Network>,

    /// Datafeeds managed by this Omikuji instance
    #[validate]
    pub datafeeds: Vec<Datafeed>,

    /// Database cleanup configuration
    #[serde(default)]
    #[validate]
    pub database_cleanup: DatabaseCleanupConfig,

    /// Key storage configuration
    #[serde(default)]
    #[validate]
    pub key_storage: KeyStorageConfig,

    /// Metrics configuration
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Gas price feed configuration
    #[serde(default)]
    pub gas_price_feeds: GasPriceFeedConfig,

    /// Scheduled tasks configuration
    #[serde(default)]
    pub scheduled_tasks: Vec<ScheduledTask>,

    /// Event monitors configuration
    #[serde(default)]
    pub event_monitors: Vec<EventMonitor>,

    /// Default execution limits for event monitors
    #[serde(default)]
    pub default_execution_limits: ExecutionLimits,
}

/// Configuration for database cleanup task
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DatabaseCleanupConfig {
    /// Cron schedule for cleanup task (default: "0 0 * * * *" - every hour)
    #[serde(default = "default_cleanup_schedule")]
    pub schedule: String,

    /// Whether cleanup is enabled (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for DatabaseCleanupConfig {
    fn default() -> Self {
        Self {
            schedule: default_cleanup_schedule(),
            enabled: true,
        }
    }
}

fn default_cleanup_schedule() -> String {
    "0 0 * * * *".to_string() // Every hour at minute 0
}

/// Configuration for key storage
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct KeyStorageConfig {
    /// Storage type: "keyring", "env", "vault", or "aws-secrets" (default: "env" for backward compatibility)
    #[serde(default = "default_key_storage_type")]
    #[validate(custom = "validate_key_storage_type")]
    pub storage_type: String,

    /// Keyring configuration (only used when storage_type is "keyring")
    #[serde(default)]
    pub keyring: KeyringConfig,

    /// Vault configuration (only used when storage_type is "vault")
    #[serde(default)]
    pub vault: VaultConfig,

    /// AWS Secrets Manager configuration (only used when storage_type is "aws-secrets")
    #[serde(default)]
    pub aws_secrets: AwsSecretsConfig,
}

impl Default for KeyStorageConfig {
    fn default() -> Self {
        Self {
            storage_type: default_key_storage_type(),
            keyring: KeyringConfig::default(),
            vault: VaultConfig::default(),
            aws_secrets: AwsSecretsConfig::default(),
        }
    }
}

/// Keyring-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyringConfig {
    /// Service name for keyring (default: "omikuji")
    #[serde(default = "default_keyring_service")]
    pub service: String,
}

impl Default for KeyringConfig {
    fn default() -> Self {
        Self {
            service: default_keyring_service(),
        }
    }
}

/// Vault-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Vault server URL
    #[serde(default = "default_vault_url")]
    pub url: String,

    /// Mount path for KV v2 secrets engine (default: "secret")
    #[serde(default = "default_vault_mount_path")]
    pub mount_path: String,

    /// Path prefix for secrets (e.g., "omikuji")
    #[serde(default = "default_vault_path_prefix")]
    pub path_prefix: String,

    /// Authentication method: "token" or "approle"
    #[serde(default = "default_vault_auth_method")]
    pub auth_method: String,

    /// Token for authentication (can use ${VAULT_TOKEN} for env var)
    pub token: Option<String>,

    /// Cache TTL in seconds (default: 300)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            url: default_vault_url(),
            mount_path: default_vault_mount_path(),
            path_prefix: default_vault_path_prefix(),
            auth_method: default_vault_auth_method(),
            token: None,
            cache_ttl_seconds: default_cache_ttl(),
        }
    }
}

/// AWS Secrets Manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsSecretsConfig {
    /// AWS region (optional, will use default AWS config if not specified)
    pub region: Option<String>,

    /// Secret name prefix (e.g., "omikuji/")
    #[serde(default = "default_aws_prefix")]
    pub prefix: String,

    /// Cache TTL in seconds (default: 300)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
}

impl Default for AwsSecretsConfig {
    fn default() -> Self {
        Self {
            region: None,
            prefix: default_aws_prefix(),
            cache_ttl_seconds: default_cache_ttl(),
        }
    }
}

fn default_key_storage_type() -> String {
    "env".to_string()
}

fn default_keyring_service() -> String {
    "omikuji".to_string()
}

fn default_vault_url() -> String {
    "https://vault.example.com".to_string()
}

fn default_vault_mount_path() -> String {
    "secret".to_string()
}

fn default_vault_path_prefix() -> String {
    "omikuji".to_string()
}

fn default_vault_auth_method() -> String {
    "token".to_string()
}

fn default_aws_prefix() -> String {
    "omikuji/".to_string()
}

fn default_cache_ttl() -> u64 {
    300
}

fn validate_key_storage_type(storage_type: &str) -> Result<(), ValidationError> {
    match storage_type {
        "keyring" | "env" | "vault" | "aws-secrets" => Ok(()),
        _ => Err(ValidationError::new(
            "storage_type must be 'keyring', 'env', 'vault', or 'aws-secrets'",
        )),
    }
}

/// Configuration for a network node (RPC/WebSocket endpoints)
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NetworkNode {
    /// Node name (e.g., "Local Anvil", "Infura", "Alchemy")
    #[validate(length(min = 1))]
    pub name: String,

    /// RPC URL for the node
    #[validate(url)]
    pub rpc_url: String,

    /// WebSocket URL for the node (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(url)]
    pub ws_url: Option<String>,
}

/// Configuration for a blockchain network
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Network {
    /// Network name (e.g., "ethereum", "base")
    #[validate(length(min = 1))]
    pub name: String,

    /// Transaction type to use ("legacy" or "eip1559")
    #[serde(default = "default_transaction_type")]
    #[validate(custom = "validate_transaction_type")]
    pub transaction_type: String,

    /// Network nodes (RPC/WebSocket endpoints)
    #[validate(length(min = 1, message = "At least one node must be configured"))]
    pub nodes: Vec<NetworkNode>,

    /// Gas configuration for this network
    #[serde(default)]
    #[validate]
    pub gas_config: GasConfig,

    /// Gas token ID for price feeds (e.g., "ethereum" for CoinGecko)
    #[serde(default = "default_gas_token")]
    pub gas_token: String,

    /// Gas token symbol (e.g., "ETH", "BNB")
    #[serde(default = "default_gas_token_symbol")]
    pub gas_token_symbol: String,

    /// Legacy fields for backward compatibility (deprecated)
    #[serde(skip_serializing, skip_deserializing)]
    pub rpc_url: Option<String>,

    #[serde(skip_serializing, skip_deserializing)]
    pub ws_url: Option<String>,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            name: "localhost".to_string(),
            transaction_type: default_transaction_type(),
            nodes: vec![NetworkNode {
                name: "Local Node".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
                ws_url: None,
            }],
            gas_config: GasConfig::default(),
            gas_token: default_gas_token(),
            gas_token_symbol: default_gas_token_symbol(),
            rpc_url: None,
            ws_url: None,
        }
    }
}

/// Gas configuration for a network
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct GasConfig {
    /// Gas limit override (optional, will estimate if not provided)
    pub gas_limit: Option<u64>,

    /// For legacy transactions: gas price in gwei (optional, will estimate if not provided)
    pub gas_price_gwei: Option<f64>,

    /// For EIP-1559: max fee per gas in gwei (optional, will estimate if not provided)
    pub max_fee_per_gas_gwei: Option<f64>,

    /// For EIP-1559: max priority fee per gas in gwei (optional, will estimate if not provided)
    pub max_priority_fee_per_gas_gwei: Option<f64>,

    /// Gas estimation multiplier (default: 1.2 for 20% buffer)
    #[serde(default = "default_gas_multiplier")]
    #[validate(range(min = 1.0, max = 5.0))]
    pub gas_multiplier: f64,

    /// Fee bumping configuration
    #[serde(default)]
    #[validate]
    pub fee_bumping: FeeBumpingConfig,
}

/// Fee bumping configuration for stuck transactions
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FeeBumpingConfig {
    /// Enable automatic fee bumping for stuck transactions
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum number of retry attempts
    #[serde(default = "default_max_retries")]
    #[validate(range(min = 0, max = 10))]
    pub max_retries: u8,

    /// Initial wait time before first retry (in seconds)
    #[serde(default = "default_initial_wait")]
    #[validate(range(min = 10, max = 600))]
    pub initial_wait_seconds: u64,

    /// Fee increase percentage for each retry
    #[serde(default = "default_fee_increase_percent")]
    #[validate(range(min = 5.0, max = 100.0))]
    pub fee_increase_percent: f64,
}

fn default_transaction_type() -> String {
    "eip1559".to_string()
}

fn default_gas_token() -> String {
    "ethereum".to_string()
}

fn default_gas_token_symbol() -> String {
    "ETH".to_string()
}

fn default_gas_multiplier() -> f64 {
    crate::constants::gas::GAS_ESTIMATION_MULTIPLIER
}

fn default_true() -> bool {
    true
}

fn default_max_retries() -> u8 {
    crate::constants::gas::MAX_FEE_BUMP_ATTEMPTS as u8
}

fn default_initial_wait() -> u64 {
    crate::constants::time::RPC_TIMEOUT_SECS
}

fn default_fee_increase_percent() -> f64 {
    (crate::constants::gas::FEE_BUMP_MULTIPLIER - 1.0) * 100.0
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            gas_limit: None,
            gas_price_gwei: None,
            max_fee_per_gas_gwei: None,
            max_priority_fee_per_gas_gwei: None,
            gas_multiplier: default_gas_multiplier(),
            fee_bumping: FeeBumpingConfig::default(),
        }
    }
}

impl Default for FeeBumpingConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            max_retries: default_max_retries(),
            initial_wait_seconds: default_initial_wait(),
            fee_increase_percent: default_fee_increase_percent(),
        }
    }
}

/// Configuration for a datafeed
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Datafeed {
    /// Datafeed name
    #[validate(length(min = 1))]
    pub name: String,

    /// Network this datafeed operates on (must match a network name)
    #[validate(length(min = 1))]
    pub networks: String,

    /// Frequency to check the datafeed (in seconds)
    #[validate(range(min = 1))]
    pub check_frequency: u64,

    /// Smart contract address for the datafeed
    #[validate(custom = "validate_eth_address")]
    pub contract_address: String,

    /// Contract type (e.g., "fluxmon")
    #[validate(length(min = 1))]
    pub contract_type: String,

    /// Whether to read configuration from the contract
    pub read_contract_config: bool,

    /// Minimum time between updates (in seconds)
    #[validate(range(min = 1))]
    pub minimum_update_frequency: u64,

    /// Threshold percentage deviation to trigger an update
    #[validate(range(min = 0.0, max = 100.0))]
    pub deviation_threshold_pct: f64,

    /// URL to fetch the price feed data
    #[validate(url)]
    pub feed_url: String,

    /// JSON path to extract the price from the feed response
    #[validate(length(min = 1))]
    pub feed_json_path: String,

    /// JSON path to extract the timestamp from the feed response (optional)
    pub feed_json_path_timestamp: Option<String>,

    /// Number of decimals to use (optional, used when read_contract_config is false)
    pub decimals: Option<u8>,

    /// Minimum valid value (optional, used when read_contract_config is false)
    pub min_value: Option<I256>,

    /// Maximum valid value (optional, used when read_contract_config is false)
    pub max_value: Option<I256>,

    /// Data retention window in days (default: 7)
    #[serde(default = "default_data_retention_days")]
    #[validate(range(min = 1, max = 365))]
    pub data_retention_days: u32,
}

fn default_data_retention_days() -> u32 {
    7
}

/// Validates that a string is a valid Ethereum address
fn validate_eth_address(address: &str) -> Result<(), ValidationError> {
    // Simple validation: check if it's a hex string starting with 0x and of correct length
    // For more comprehensive validation, we might need to check the checksum
    if !address.starts_with("0x")
        || address.len() != 42
        || !address[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(ValidationError::new("invalid_eth_address"));
    }
    Ok(())
}

/// Validates that transaction type is either "legacy" or "eip1559"
pub fn validate_transaction_type(tx_type: &str) -> Result<(), ValidationError> {
    match tx_type.to_lowercase().as_str() {
        "legacy" | "eip1559" => Ok(()),
        _ => Err(ValidationError::new("invalid_transaction_type")),
    }
}

/// Configuration for execution limits when processing webhook responses
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq)]
#[serde(default)]
pub struct ExecutionLimits {
    /// Maximum value in wei that can be sent with a contract call
    #[validate(length(min = 1))]
    pub max_value_wei: String,

    /// Maximum gas price in gwei that can be used for transactions
    #[validate(range(min = 1))]
    pub max_gas_price_gwei: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_value_wei: "100000000000000000".to_string(), // 0.1 ETH default
            max_gas_price_gwei: 100,                         // 100 gwei default
        }
    }
}
