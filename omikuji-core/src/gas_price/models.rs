use serde::{Deserialize, Serialize};

/// Represents a gas token price in USD
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasTokenPrice {
    /// CoinGecko ID for the token (e.g., "ethereum")
    pub token_id: String,
    /// Token symbol (e.g., "ETH")
    pub symbol: String,
    /// Price in USD
    pub price_usd: f64,
    /// Timestamp when the price was fetched
    pub timestamp: u64,
    /// Source of the price data (e.g., "coingecko")
    pub source: String,
}

/// Configuration for gas price feeds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPriceFeedConfig {
    /// Whether gas price feeds are enabled
    #[serde(default)]
    pub enabled: bool,
    /// Update frequency in seconds (default: 3600 - 1 hour)
    /// This also determines how long prices are cached
    #[serde(default = "default_update_frequency")]
    pub update_frequency: u64,
    /// Price provider to use (default: "coingecko")
    #[serde(default = "default_provider")]
    pub provider: String,
    /// CoinGecko-specific configuration
    #[serde(default)]
    pub coingecko: CoinGeckoConfig,
    /// Whether to fallback to cached prices on fetch failure
    #[serde(default = "default_fallback_to_cache")]
    pub fallback_to_cache: bool,
    /// Whether to persist prices to database
    #[serde(default)]
    pub persist_to_database: bool,
}

/// CoinGecko-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinGeckoConfig {
    /// API key for CoinGecko (optional, for pro tier)
    pub api_key: Option<String>,
    /// Base URL for CoinGecko API
    #[serde(default = "default_coingecko_base_url")]
    pub base_url: String,
}

/// Price fetch result
#[derive(Debug)]
pub struct PriceFetchResult {
    pub prices: Vec<GasTokenPrice>,
    pub errors: Vec<PriceFetchError>,
}

/// Errors that can occur during price fetching
#[derive(Debug, thiserror::Error)]
pub enum PriceFetchError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),
    #[error("Failed to parse response: {0}")]
    ParseError(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Invalid API key")]
    InvalidApiKey,
    #[error("Token not found: {0}")]
    TokenNotFound(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Trait for price providers
#[async_trait::async_trait]
pub trait PriceProvider: Send + Sync {
    /// Fetch prices for the given token IDs
    async fn fetch_prices(
        &self,
        token_ids: &[String],
    ) -> Result<Vec<GasTokenPrice>, PriceFetchError>;

    /// Get the name of this provider
    fn name(&self) -> &str;
}

/// Gas cost in USD
#[derive(Debug, Clone)]
pub struct GasCostUsd {
    pub network: String,
    pub feed_name: String,
    pub transaction_hash: String,
    pub gas_used: u64,
    pub gas_price_wei: u128,
    pub gas_token_price_usd: f64,
    pub total_cost_usd: f64,
    pub timestamp: u64,
}

// Default functions
fn default_update_frequency() -> u64 {
    3600 // 1 hour
}

fn default_provider() -> String {
    "coingecko".to_string()
}

fn default_fallback_to_cache() -> bool {
    true
}

fn default_coingecko_base_url() -> String {
    "https://api.coingecko.com/api/v3".to_string()
}

impl Default for GasPriceFeedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            update_frequency: default_update_frequency(),
            provider: default_provider(),
            coingecko: CoinGeckoConfig::default(),
            fallback_to_cache: default_fallback_to_cache(),
            persist_to_database: false,
        }
    }
}

impl Default for CoinGeckoConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: default_coingecko_base_url(),
        }
    }
}
