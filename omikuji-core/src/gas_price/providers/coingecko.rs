use crate::gas_price::models::{CoinGeckoConfig, GasTokenPrice, PriceFetchError, PriceProvider};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// CoinGecko price provider implementation
pub struct CoinGeckoProvider {
    config: CoinGeckoConfig,
    client: Client,
}

/// CoinGecko API response structure
#[derive(Debug, Deserialize)]
struct CoinGeckoResponse {
    #[serde(flatten)]
    prices: HashMap<String, PriceData>,
}

#[derive(Debug, Deserialize)]
struct PriceData {
    usd: f64,
    #[serde(default)]
    symbol: Option<String>,
}

impl CoinGeckoProvider {
    /// Create a new CoinGecko provider
    pub fn new(config: CoinGeckoConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Build the API URL for fetching prices
    fn build_url(&self, token_ids: &[String]) -> String {
        let ids = token_ids.join(",");
        format!(
            "{}/simple/price?ids={}&vs_currencies=usd",
            self.config.base_url, ids
        )
    }

    /// Get default symbol for a token ID
    fn get_default_symbol(token_id: &str) -> &'static str {
        match token_id {
            "ethereum" => "ETH",
            "binancecoin" => "BNB",
            "matic-network" => "MATIC",
            "avalanche-2" => "AVAX",
            "fantom" => "FTM",
            _ => "UNKNOWN",
        }
    }
}

#[async_trait::async_trait]
impl PriceProvider for CoinGeckoProvider {
    async fn fetch_prices(
        &self,
        token_ids: &[String],
    ) -> Result<Vec<GasTokenPrice>, PriceFetchError> {
        if token_ids.is_empty() {
            return Ok(vec![]);
        }

        let url = self.build_url(token_ids);
        debug!("Fetching prices from CoinGecko: {}", url);

        let mut request = self.client.get(&url);

        // Add API key header if configured
        if let Some(api_key) = &self.config.api_key {
            request = request.header("x-cg-pro-api-key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| PriceFetchError::HttpError(e.to_string()))?;

        // Handle rate limiting
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            warn!("CoinGecko rate limit exceeded");
            return Err(PriceFetchError::RateLimitExceeded);
        }

        // Handle authentication errors
        if response.status() == StatusCode::UNAUTHORIZED {
            error!("CoinGecko API key is invalid");
            return Err(PriceFetchError::InvalidApiKey);
        }

        // Handle other errors
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            error!("CoinGecko API error: {} - {}", status, body);
            return Err(PriceFetchError::ProviderError(format!(
                "HTTP {status}: {body}"
            )));
        }

        // Parse response
        let data: CoinGeckoResponse = response
            .json()
            .await
            .map_err(|e| PriceFetchError::ParseError(e.to_string()))?;

        // Log the raw response for debugging
        info!("CoinGecko API response: {:?}", data);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut prices = Vec::new();

        for token_id in token_ids {
            if let Some(price_data) = data.prices.get(token_id) {
                let symbol = price_data
                    .symbol
                    .as_deref()
                    .unwrap_or_else(|| Self::get_default_symbol(token_id))
                    .to_uppercase();

                prices.push(GasTokenPrice {
                    token_id: token_id.clone(),
                    symbol: symbol.clone(),
                    price_usd: price_data.usd,
                    timestamp,
                    source: self.name().to_string(),
                });

                info!(
                    "Fetched price for {} ({}): ${:.2} USD",
                    token_id, symbol, price_data.usd
                );
            } else {
                warn!("No price data found for token: {}", token_id);
            }
        }

        Ok(prices)
    }

    fn name(&self) -> &str {
        "coingecko"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_price::models::{CoinGeckoConfig, PriceProvider};
    use mockito;

    fn create_provider(base_url: &str, api_key: Option<String>) -> CoinGeckoProvider {
        CoinGeckoProvider::new(CoinGeckoConfig {
            base_url: base_url.to_string(),
            api_key,
        })
    }

    #[tokio::test]
    async fn test_fetch_prices_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/simple/price?ids=ethereum&vs_currencies=usd")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ethereum":{"usd":2500.50}}"#)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider
            .fetch_prices(&["ethereum".to_string()])
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].token_id, "ethereum");
        assert_eq!(result[0].symbol, "ETH");
        assert!((result[0].price_usd - 2500.50).abs() < f64::EPSILON);
        assert_eq!(result[0].source, "coingecko");
        assert!(result[0].timestamp > 0);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_multiple_tokens() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
                    "vs_currencies".into(),
                    "usd".into(),
                )]),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ethereum":{"usd":2500.0},"binancecoin":{"usd":300.0}}"#)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider
            .fetch_prices(&["ethereum".to_string(), "binancecoin".to_string()])
            .await
            .unwrap();

        assert_eq!(result.len(), 2);

        let eth = result.iter().find(|p| p.token_id == "ethereum").unwrap();
        assert_eq!(eth.symbol, "ETH");
        assert!((eth.price_usd - 2500.0).abs() < f64::EPSILON);

        let bnb = result.iter().find(|p| p.token_id == "binancecoin").unwrap();
        assert_eq!(bnb.symbol, "BNB");
        assert!((bnb.price_usd - 300.0).abs() < f64::EPSILON);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_empty_tokens() {
        let server = mockito::Server::new_async().await;
        let provider = create_provider(&server.url(), None);

        let result = provider.fetch_prices(&[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_prices_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider.fetch_prices(&["ethereum".to_string()]).await;

        assert!(matches!(result, Err(PriceFetchError::RateLimitExceeded)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_invalid_api_key() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(401)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), Some("bad-key".to_string()));
        let result = provider.fetch_prices(&["ethereum".to_string()]).await;

        assert!(matches!(result, Err(PriceFetchError::InvalidApiKey)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider.fetch_prices(&["ethereum".to_string()]).await;

        match result {
            Err(PriceFetchError::ProviderError(msg)) => {
                assert!(msg.contains("500"));
            }
            other => panic!("Expected ProviderError, got {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_malformed_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not valid json {{{")
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider.fetch_prices(&["ethereum".to_string()]).await;

        assert!(matches!(result, Err(PriceFetchError::ParseError(_))));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_missing_token() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ethereum":{"usd":2500.0}}"#)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), None);
        let result = provider
            .fetch_prices(&["ethereum".to_string(), "unknown-token".to_string()])
            .await
            .unwrap();

        // Only ethereum should be in results, unknown-token is silently skipped
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].token_id, "ethereum");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_prices_with_api_key() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("x-cg-pro-api-key", "my-secret-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ethereum":{"usd":2500.0}}"#)
            .create_async()
            .await;

        let provider = create_provider(&server.url(), Some("my-secret-key".to_string()));
        let result = provider
            .fetch_prices(&["ethereum".to_string()])
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        mock.assert_async().await;
    }

    #[test]
    fn test_build_url() {
        let provider = CoinGeckoProvider::new(CoinGeckoConfig {
            base_url: "https://api.coingecko.com/api/v3".to_string(),
            api_key: None,
        });

        let url = provider.build_url(&["ethereum".to_string()]);
        assert_eq!(
            url,
            "https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd"
        );

        let url = provider.build_url(&["ethereum".to_string(), "binancecoin".to_string()]);
        assert_eq!(
            url,
            "https://api.coingecko.com/api/v3/simple/price?ids=ethereum,binancecoin&vs_currencies=usd"
        );
    }

    #[test]
    fn test_get_default_symbol() {
        assert_eq!(CoinGeckoProvider::get_default_symbol("ethereum"), "ETH");
        assert_eq!(CoinGeckoProvider::get_default_symbol("binancecoin"), "BNB");
        assert_eq!(
            CoinGeckoProvider::get_default_symbol("matic-network"),
            "MATIC"
        );
        assert_eq!(CoinGeckoProvider::get_default_symbol("avalanche-2"), "AVAX");
        assert_eq!(CoinGeckoProvider::get_default_symbol("fantom"), "FTM");
        assert_eq!(
            CoinGeckoProvider::get_default_symbol("some-unknown-token"),
            "UNKNOWN"
        );
    }

    #[test]
    fn test_name() {
        let provider = CoinGeckoProvider::new(CoinGeckoConfig::default());
        assert_eq!(provider.name(), "coingecko");
    }
}
