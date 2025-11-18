#[cfg(test)]
mod tests {
    use crate::gas_price::cache::PriceCache;
    use crate::gas_price::models::{CoinGeckoConfig, GasPriceFeedConfig, GasTokenPrice};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_price_cache_basic_operations() {
        let cache = PriceCache::new(60); // 60 second TTL

        // Test inserting and retrieving a price
        let price = GasTokenPrice {
            token_id: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            price_usd: 2500.0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            source: "test".to_string(),
        };

        cache.insert(price.clone()).await;

        // Should retrieve the price
        let retrieved = cache.get("ethereum").await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.price_usd, 2500.0);
        assert_eq!(retrieved.symbol, "ETH");
    }

    #[tokio::test]
    async fn test_price_cache_expiration() {
        let cache = PriceCache::new(1); // 1 second TTL

        let price = GasTokenPrice {
            token_id: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            price_usd: 2500.0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            source: "test".to_string(),
        };

        cache.insert(price).await;

        // Should retrieve immediately
        assert!(cache.get("ethereum").await.is_some());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should not retrieve after expiration
        assert!(cache.get("ethereum").await.is_none());
    }

    #[tokio::test]
    async fn test_price_cache_multiple_tokens() {
        let cache = PriceCache::new(60);

        let prices = vec![
            GasTokenPrice {
                token_id: "ethereum".to_string(),
                symbol: "ETH".to_string(),
                price_usd: 2500.0,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                source: "test".to_string(),
            },
            GasTokenPrice {
                token_id: "binancecoin".to_string(),
                symbol: "BNB".to_string(),
                price_usd: 300.0,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                source: "test".to_string(),
            },
        ];

        cache.insert_many(prices).await;

        // Test get_many
        let token_ids = vec!["ethereum".to_string(), "binancecoin".to_string()];
        let retrieved = cache.get_many(&token_ids).await;

        assert_eq!(retrieved.len(), 2);
        assert!(retrieved.contains_key("ethereum"));
        assert!(retrieved.contains_key("binancecoin"));
        assert_eq!(retrieved.get("ethereum").unwrap().price_usd, 2500.0);
        assert_eq!(retrieved.get("binancecoin").unwrap().price_usd, 300.0);
    }

    #[test]
    fn test_gas_cost_calculation() {
        // Test USD cost calculation
        let gas_used: u64 = 100_000;
        let gas_price_wei: u128 = 30_000_000_000; // 30 gwei
        let token_price_usd = 2500.0;

        // Calculate cost: gas_used * gas_price_wei / 1e18 * token_price_usd
        let gas_cost_native = (gas_used as f64 * gas_price_wei as f64) / 1e18;
        let total_cost_usd = gas_cost_native * token_price_usd;

        // 100,000 * 30 gwei = 3,000,000 gwei = 0.003 ETH
        // 0.003 ETH * $2500 = $7.50
        assert!((total_cost_usd - 7.5).abs() < 0.01);
    }

    #[test]
    fn test_config_defaults() {
        let config = GasPriceFeedConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.update_frequency, 3600);
        assert_eq!(config.provider, "coingecko");
        assert!(config.fallback_to_cache);
        assert!(!config.persist_to_database);
    }

    #[test]
    fn test_coingecko_config_defaults() {
        let config = CoinGeckoConfig::default();

        assert!(config.api_key.is_none());
        assert_eq!(config.base_url, "https://api.coingecko.com/api/v3");
    }
}
