use crate::database::transaction_repository::TransactionLogRepository;
use crate::gas_price::{
    cache::PriceCache,
    models::{GasCostUsd, GasPriceFeedConfig, GasTokenPrice, PriceFetchError, PriceProvider},
    providers::CoinGeckoProvider,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Manages gas token price fetching and caching
pub struct GasPriceManager {
    config: GasPriceFeedConfig,
    providers: Vec<Box<dyn PriceProvider>>,
    cache: Arc<PriceCache>,
    token_mappings: Arc<RwLock<HashMap<String, String>>>, // network -> token_id
    db_repo: Option<Arc<TransactionLogRepository>>,
}

impl GasPriceManager {
    /// Create a new gas price manager
    pub fn new(
        config: GasPriceFeedConfig,
        token_mappings: HashMap<String, String>,
        db_repo: Option<Arc<TransactionLogRepository>>,
    ) -> Self {
        let cache = Arc::new(PriceCache::with_options(
            config.update_frequency,
            config.fallback_to_cache,
        ));

        let mut providers: Vec<Box<dyn PriceProvider>> = Vec::new();

        // Initialize providers based on config
        match config.provider.as_str() {
            "coingecko" => {
                providers.push(Box::new(CoinGeckoProvider::new(config.coingecko.clone())));
            }
            _ => {
                warn!(
                    "Unknown price provider: {}, using coingecko",
                    config.provider
                );
                providers.push(Box::new(CoinGeckoProvider::new(config.coingecko.clone())));
            }
        }

        Self {
            config,
            providers,
            cache,
            token_mappings: Arc::new(RwLock::new(token_mappings)),
            db_repo,
        }
    }

    /// Start the price update loop
    pub async fn start(self: Arc<Self>) {
        if !self.config.enabled {
            info!("Gas price feeds are disabled");
            return;
        }

        info!(
            "Starting gas price manager with {} second update frequency",
            self.config.update_frequency
        );

        let manager_clone = self.clone();

        // Initial fetch
        if let Err(e) = manager_clone.update_prices().await {
            error!("Failed initial price fetch: {}", e);
        }

        // Start periodic updates
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(manager_clone.config.update_frequency));
            interval.tick().await; // Skip first tick

            loop {
                interval.tick().await;
                if let Err(e) = manager_clone.update_prices().await {
                    error!("Failed to update gas prices: {}", e);
                }
            }
        });

        // Start staleness monitoring (update metrics every 30 seconds)
        let manager_staleness = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));

            loop {
                interval.tick().await;
                manager_staleness.update_staleness_metrics().await;
            }
        });
    }

    /// Update prices for all configured tokens
    async fn update_prices(&self) -> Result<(), PriceFetchError> {
        // Get unique token IDs from mappings
        let mappings = self.token_mappings.read().await;
        let token_ids: Vec<String> = mappings
            .values()
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if token_ids.is_empty() {
            debug!("No tokens configured for price fetching");
            return Ok(());
        }

        info!("Fetching prices for {} tokens", token_ids.len());

        // Try each provider until one succeeds
        let mut last_error = None;
        for provider in &self.providers {
            match provider.fetch_prices(&token_ids).await {
                Ok(prices) => {
                    info!(
                        "Successfully fetched {} prices from {}",
                        prices.len(),
                        provider.name()
                    );

                    // Update cache
                    self.cache.insert_many(prices.clone()).await;

                    // Persist to database if enabled
                    if self.config.persist_to_database {
                        if let Some(_repo) = &self.db_repo {
                            if let Err(e) = self.persist_prices(&prices).await {
                                error!("Failed to persist prices to database: {}", e);
                            }
                        }
                    }

                    // Update metrics
                    self.update_metrics(&prices).await;

                    return Ok(());
                }
                Err(e) => {
                    warn!("Provider {} failed: {}", provider.name(), e);
                    last_error = Some(e);
                }
            }
        }

        // All providers failed
        if let Some(e) = last_error {
            if self.config.fallback_to_cache {
                warn!("All providers failed, using cached prices");
                Ok(())
            } else {
                Err(e)
            }
        } else {
            Err(PriceFetchError::ProviderError(
                "No providers configured".to_string(),
            ))
        }
    }

    /// Get the current price for a network's gas token
    pub async fn get_price(&self, network: &str) -> Option<GasTokenPrice> {
        let mappings = self.token_mappings.read().await;
        let token_id = mappings.get(network)?;
        self.cache.get(token_id).await
    }

    /// Get prices for multiple networks
    pub async fn get_prices(&self, networks: &[String]) -> HashMap<String, GasTokenPrice> {
        let mappings = self.token_mappings.read().await;
        let mut results = HashMap::new();

        for network in networks {
            if let Some(token_id) = mappings.get(network) {
                if let Some(price) = self.cache.get(token_id).await {
                    results.insert(network.clone(), price);
                }
            }
        }

        results
    }

    /// Calculate USD cost for a gas transaction
    pub async fn calculate_usd_cost(
        &self,
        network: &str,
        feed_name: &str,
        transaction_hash: &str,
        gas_used: u64,
        gas_price_wei: u128,
    ) -> Option<GasCostUsd> {
        let price = self.get_price(network).await?;

        // Convert wei to native token (1 token = 10^18 wei)
        let gas_cost_native = (gas_used as f64 * gas_price_wei as f64) / 1e18;
        let total_cost_usd = gas_cost_native * price.price_usd;

        Some(GasCostUsd {
            network: network.to_string(),
            feed_name: feed_name.to_string(),
            transaction_hash: transaction_hash.to_string(),
            gas_used,
            gas_price_wei,
            gas_token_price_usd: price.price_usd,
            total_cost_usd,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Persist prices to database
    async fn persist_prices(&self, prices: &[GasTokenPrice]) -> Result<(), String> {
        // TODO: Implement database persistence when transaction repository is extended
        debug!("Would persist {} prices to database", prices.len());
        Ok(())
    }

    /// Update Prometheus metrics
    async fn update_metrics(&self, prices: &[GasTokenPrice]) {
        use crate::metrics::gas_metrics::{GAS_PRICE_STALENESS_SECONDS, GAS_TOKEN_PRICE_USD};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mappings = self.token_mappings.read().await;
        let reverse_mappings: HashMap<&str, Vec<&str>> =
            mappings
                .iter()
                .fold(HashMap::new(), |mut acc, (network, token_id)| {
                    acc.entry(token_id.as_str())
                        .or_default()
                        .push(network.as_str());
                    acc
                });

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for price in prices {
            if let Some(networks) = reverse_mappings.get(price.token_id.as_str()) {
                for network in networks {
                    let network_str: &str = network;
                    let symbol_str: &str = price.symbol.as_str();
                    // Update price metric
                    GAS_TOKEN_PRICE_USD
                        .with_label_values(&[network_str, symbol_str])
                        .set(price.price_usd);

                    // Update staleness metric (time since price was fetched)
                    let staleness = now.saturating_sub(price.timestamp) as f64;
                    GAS_PRICE_STALENESS_SECONDS
                        .with_label_values(&[network_str, symbol_str])
                        .set(staleness);
                }
            }
        }
    }

    /// Check if price feeds are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> (usize, u64) {
        let size = self.cache.size().await;
        (size, self.config.update_frequency)
    }

    /// Create a manager with injected providers (for testing)
    #[cfg(test)]
    pub(crate) fn with_providers(
        config: GasPriceFeedConfig,
        providers: Vec<Box<dyn PriceProvider>>,
        token_mappings: HashMap<String, String>,
    ) -> Self {
        let cache = Arc::new(PriceCache::with_options(
            config.update_frequency,
            config.fallback_to_cache,
        ));

        Self {
            config,
            providers,
            cache,
            token_mappings: Arc::new(RwLock::new(token_mappings)),
            db_repo: None,
        }
    }

    /// Update staleness metrics for all cached prices
    async fn update_staleness_metrics(&self) {
        use crate::metrics::gas_metrics::GAS_PRICE_STALENESS_SECONDS;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mappings = self.token_mappings.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check staleness for each network
        for (network, token_id) in mappings.iter() {
            if let Some(price) = self.cache.get(token_id).await {
                let staleness = now.saturating_sub(price.timestamp) as f64;
                GAS_PRICE_STALENESS_SECONDS
                    .with_label_values(&[network, &price.symbol])
                    .set(staleness);

                // Warn if price is getting stale (over 80% of update frequency)
                if staleness > (self.config.update_frequency as f64 * 0.8) {
                    warn!(
                        "Gas price for {} ({}) is getting stale: {}s old (update frequency: {}s)",
                        network, price.symbol, staleness, self.config.update_frequency
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_price::models::{
        GasPriceFeedConfig, GasTokenPrice, PriceFetchError, PriceProvider,
    };
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A mock price provider for testing
    struct MockPriceProvider {
        name: String,
        prices: Result<Vec<GasTokenPrice>, PriceFetchError>,
    }

    impl MockPriceProvider {
        fn success(prices: Vec<GasTokenPrice>) -> Self {
            Self {
                name: "mock".to_string(),
                prices: Ok(prices),
            }
        }

        fn failure(error: PriceFetchError) -> Self {
            Self {
                name: "mock".to_string(),
                prices: Err(error),
            }
        }
    }

    #[async_trait::async_trait]
    impl PriceProvider for MockPriceProvider {
        async fn fetch_prices(
            &self,
            _token_ids: &[String],
        ) -> Result<Vec<GasTokenPrice>, PriceFetchError> {
            match &self.prices {
                Ok(prices) => Ok(prices.clone()),
                Err(_) => Err(PriceFetchError::ProviderError(
                    "Mock provider error".to_string(),
                )),
            }
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    fn now_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn make_price(token_id: &str, symbol: &str, price_usd: f64) -> GasTokenPrice {
        GasTokenPrice {
            token_id: token_id.to_string(),
            symbol: symbol.to_string(),
            price_usd,
            timestamp: now_timestamp(),
            source: "mock".to_string(),
        }
    }

    fn default_mappings() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("ethereum".to_string(), "ethereum".to_string());
        m.insert("bsc".to_string(), "binancecoin".to_string());
        m
    }

    fn enabled_config() -> GasPriceFeedConfig {
        GasPriceFeedConfig {
            enabled: true,
            update_frequency: 3600,
            fallback_to_cache: false,
            ..GasPriceFeedConfig::default()
        }
    }

    #[tokio::test]
    async fn test_update_prices_success() {
        let prices = vec![
            make_price("ethereum", "ETH", 2500.0),
            make_price("binancecoin", "BNB", 300.0),
        ];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        let result = manager.update_prices().await;
        assert!(result.is_ok());

        // Verify cache was populated
        let eth_price = manager.cache.get("ethereum").await;
        assert!(eth_price.is_some());
        assert!((eth_price.unwrap().price_usd - 2500.0).abs() < f64::EPSILON);

        let bnb_price = manager.cache.get("binancecoin").await;
        assert!(bnb_price.is_some());
        assert!((bnb_price.unwrap().price_usd - 300.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_update_prices_provider_failure_with_fallback() {
        let provider =
            MockPriceProvider::failure(PriceFetchError::ProviderError("test error".to_string()));

        let config = GasPriceFeedConfig {
            enabled: true,
            update_frequency: 3600,
            fallback_to_cache: true,
            ..GasPriceFeedConfig::default()
        };

        let manager =
            GasPriceManager::with_providers(config, vec![Box::new(provider)], default_mappings());

        let result = manager.update_prices().await;
        assert!(result.is_ok(), "Should succeed with fallback_to_cache=true");
    }

    #[tokio::test]
    async fn test_update_prices_provider_failure_no_fallback() {
        let provider =
            MockPriceProvider::failure(PriceFetchError::ProviderError("test error".to_string()));

        let config = GasPriceFeedConfig {
            enabled: true,
            update_frequency: 3600,
            fallback_to_cache: false,
            ..GasPriceFeedConfig::default()
        };

        let manager =
            GasPriceManager::with_providers(config, vec![Box::new(provider)], default_mappings());

        let result = manager.update_prices().await;
        assert!(result.is_err(), "Should fail with fallback_to_cache=false");
    }

    #[tokio::test]
    async fn test_update_prices_empty_tokens() {
        let provider = MockPriceProvider::success(vec![]);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            HashMap::new(), // empty mappings
        );

        let result = manager.update_prices().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_price_cached() {
        let prices = vec![make_price("ethereum", "ETH", 2500.0)];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        // Populate cache
        manager.update_prices().await.unwrap();

        // Retrieve by network name
        let price = manager.get_price("ethereum").await;
        assert!(price.is_some());
        assert!((price.unwrap().price_usd - 2500.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_get_price_not_found() {
        let provider = MockPriceProvider::success(vec![]);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        let price = manager.get_price("ethereum").await;
        assert!(price.is_none());
    }

    #[tokio::test]
    async fn test_get_price_unknown_network() {
        let provider = MockPriceProvider::success(vec![]);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        let price = manager.get_price("unknown-network").await;
        assert!(price.is_none());
    }

    #[tokio::test]
    async fn test_get_prices_multiple() {
        let prices = vec![
            make_price("ethereum", "ETH", 2500.0),
            make_price("binancecoin", "BNB", 300.0),
        ];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        manager.update_prices().await.unwrap();

        let networks = vec!["ethereum".to_string(), "bsc".to_string()];
        let result = manager.get_prices(&networks).await;

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("ethereum"));
        assert!(result.contains_key("bsc"));
    }

    #[tokio::test]
    async fn test_calculate_usd_cost() {
        let prices = vec![make_price("ethereum", "ETH", 2500.0)];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        manager.update_prices().await.unwrap();

        // 100,000 gas * 30 gwei = 0.003 ETH * $2500 = $7.50
        let cost = manager
            .calculate_usd_cost(
                "ethereum",
                "eth_usd",
                "0xabc123",
                100_000,
                30_000_000_000, // 30 gwei
            )
            .await;

        assert!(cost.is_some());
        let cost = cost.unwrap();
        assert!((cost.total_cost_usd - 7.5).abs() < 0.01);
        assert_eq!(cost.network, "ethereum");
        assert_eq!(cost.feed_name, "eth_usd");
        assert_eq!(cost.transaction_hash, "0xabc123");
        assert_eq!(cost.gas_used, 100_000);
        assert_eq!(cost.gas_price_wei, 30_000_000_000);
        assert!((cost.gas_token_price_usd - 2500.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_calculate_usd_cost_no_price() {
        let provider = MockPriceProvider::success(vec![]);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        let cost = manager
            .calculate_usd_cost("ethereum", "eth_usd", "0xabc", 100_000, 30_000_000_000)
            .await;

        assert!(cost.is_none());
    }

    #[tokio::test]
    async fn test_is_enabled() {
        let provider = MockPriceProvider::success(vec![]);

        let enabled_mgr = GasPriceManager::with_providers(
            GasPriceFeedConfig {
                enabled: true,
                ..GasPriceFeedConfig::default()
            },
            vec![Box::new(provider)],
            HashMap::new(),
        );
        assert!(enabled_mgr.is_enabled());

        let provider2 = MockPriceProvider::success(vec![]);
        let disabled_mgr = GasPriceManager::with_providers(
            GasPriceFeedConfig {
                enabled: false,
                ..GasPriceFeedConfig::default()
            },
            vec![Box::new(provider2)],
            HashMap::new(),
        );
        assert!(!disabled_mgr.is_enabled());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let prices = vec![make_price("ethereum", "ETH", 2500.0)];
        let provider = MockPriceProvider::success(prices);

        let config = GasPriceFeedConfig {
            enabled: true,
            update_frequency: 7200,
            ..GasPriceFeedConfig::default()
        };

        let manager =
            GasPriceManager::with_providers(config, vec![Box::new(provider)], default_mappings());

        manager.update_prices().await.unwrap();

        let (size, freq) = manager.cache_stats().await;
        assert!(size >= 1);
        assert_eq!(freq, 7200);
    }

    #[tokio::test]
    async fn test_start_disabled() {
        let provider = MockPriceProvider::success(vec![]);

        let config = GasPriceFeedConfig {
            enabled: false,
            ..GasPriceFeedConfig::default()
        };

        let manager = Arc::new(GasPriceManager::with_providers(
            config,
            vec![Box::new(provider)],
            HashMap::new(),
        ));

        // start() should return immediately when disabled
        manager.start().await;
        // If we get here, it didn't hang — test passes
    }

    #[tokio::test]
    async fn test_update_metrics() {
        let prices = vec![
            make_price("ethereum", "ETH", 2500.0),
            make_price("binancecoin", "BNB", 300.0),
        ];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        // update_prices internally calls update_metrics
        let result = manager.update_prices().await;
        assert!(result.is_ok());

        // Verify Prometheus gauges were set (read them back)
        use crate::metrics::gas_metrics::GAS_TOKEN_PRICE_USD;
        let eth_metric = GAS_TOKEN_PRICE_USD
            .with_label_values(&["ethereum", "ETH"])
            .get();
        assert!((eth_metric - 2500.0).abs() < f64::EPSILON);

        let bsc_metric = GAS_TOKEN_PRICE_USD.with_label_values(&["bsc", "BNB"]).get();
        assert!((bsc_metric - 300.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_update_staleness_metrics() {
        let prices = vec![make_price("ethereum", "ETH", 2500.0)];
        let provider = MockPriceProvider::success(prices);

        let manager = GasPriceManager::with_providers(
            enabled_config(),
            vec![Box::new(provider)],
            default_mappings(),
        );

        manager.update_prices().await.unwrap();

        // Call staleness metrics update
        manager.update_staleness_metrics().await;

        use crate::metrics::gas_metrics::GAS_PRICE_STALENESS_SECONDS;
        let staleness = GAS_PRICE_STALENESS_SECONDS
            .with_label_values(&["ethereum", "ETH"])
            .get();
        // Staleness should be very small (just fetched)
        assert!(staleness < 5.0);
    }
}
