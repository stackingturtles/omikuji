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
