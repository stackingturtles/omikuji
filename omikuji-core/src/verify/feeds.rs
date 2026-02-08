use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};

use crate::config::models::Datafeed;
use crate::datafeed::json_extractor::JsonExtractor;

use super::types::{CheckCategory, CheckResult, CheckStatus};

pub async fn check_feeds(datafeeds: &[Datafeed], timeout: Duration) -> Vec<CheckResult> {
    stream::iter(datafeeds)
        .map(|df| check_one_feed(df, timeout))
        .buffer_unordered(5)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
}

async fn check_one_feed(datafeed: &Datafeed, timeout: Duration) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let name = format!("Feed: {}", datafeed.name);
    let start = Instant::now();

    // 1. HTTP GET the feed URL
    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Fail,
                message: format!("Failed to create HTTP client: {e}"),
                hint: None,
                duration: start.elapsed(),
            });
            return results;
        }
    };

    let response = match client.get(&datafeed.feed_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            let hint = if e.is_timeout() {
                Some(format!(
                    "Request timed out after {} s — check URL: {}",
                    timeout.as_secs(),
                    datafeed.feed_url
                ))
            } else if e.is_connect() {
                Some(format!("Cannot connect to {}", datafeed.feed_url))
            } else {
                Some(format!("URL: {}", datafeed.feed_url))
            };
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Fail,
                message: format!("HTTP request failed: {e}"),
                hint,
                duration: start.elapsed(),
            });
            return results;
        }
    };

    // 2. Check HTTP status
    if !response.status().is_success() {
        results.push(CheckResult {
            category: CheckCategory::Feed,
            name,
            status: CheckStatus::Fail,
            message: format!("HTTP {}", response.status()),
            hint: Some(format!("URL: {}", datafeed.feed_url)),
            duration: start.elapsed(),
        });
        return results;
    }

    // 3. Parse JSON
    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Fail,
                message: format!("Failed to read response body: {e}"),
                hint: None,
                duration: start.elapsed(),
            });
            return results;
        }
    };

    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Fail,
                message: format!("Invalid JSON response: {e}"),
                hint: Some(format!("URL: {}", datafeed.feed_url)),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    // 4. Extract value using JsonExtractor
    match JsonExtractor::extract_float(&json, &datafeed.feed_json_path) {
        Ok(value) => {
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Pass,
                message: format!("Value: {value:.2} via path {}", datafeed.feed_json_path),
                hint: None,
                duration: start.elapsed(),
            });
        }
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Feed,
                name,
                status: CheckStatus::Fail,
                message: format!("JSON path extraction failed: {e}"),
                hint: Some(format!(
                    "Check feed_json_path '{}' against response from {}",
                    datafeed.feed_json_path, datafeed.feed_url
                )),
                duration: start.elapsed(),
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::Datafeed;
    use alloy::primitives::I256;
    use mockito;
    use std::time::Duration;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn test_datafeed(url: &str, json_path: &str) -> Datafeed {
        Datafeed {
            name: "test-feed".to_string(),
            networks: "test-network".to_string(),
            check_frequency: 60,
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            contract_type: "fluxmon".to_string(),
            read_contract_config: false,
            minimum_update_frequency: 60,
            deviation_threshold_pct: 1.0,
            feed_url: url.to_string(),
            feed_json_path: json_path.to_string(),
            feed_json_path_timestamp: None,
            enable_timestamp_safety_check: false,
            decimals: Some(8),
            min_value: Some(I256::ZERO),
            max_value: Some(I256::MAX),
            data_retention_days: 7,
        }
    }

    #[tokio::test]
    async fn test_check_feeds_empty_slice() {
        let results = check_feeds(&[], TEST_TIMEOUT).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_check_feeds_valid_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/price")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"price": 42.5}}"#)
            .create_async()
            .await;

        let url = format!("{}/price", server.url());
        let df = test_datafeed(&url, "data.price");
        let results = check_feeds(&[df], TEST_TIMEOUT).await;

        mock.assert_async().await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Pass);
        assert_eq!(results[0].category, CheckCategory::Feed);
        assert!(results[0].message.contains("42.50"));
        assert!(results[0].message.contains("data.price"));
    }

    #[tokio::test]
    async fn test_check_feeds_http_404() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let url = format!("{}/missing", server.url());
        let df = test_datafeed(&url, "price");
        let results = check_feeds(&[df], TEST_TIMEOUT).await;

        mock.assert_async().await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert!(results[0].message.contains("404"));
    }

    #[tokio::test]
    async fn test_check_feeds_invalid_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/bad")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("this is not json")
            .create_async()
            .await;

        let url = format!("{}/bad", server.url());
        let df = test_datafeed(&url, "price");
        let results = check_feeds(&[df], TEST_TIMEOUT).await;

        mock.assert_async().await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert!(results[0].message.contains("Invalid JSON"));
    }

    #[tokio::test]
    async fn test_check_feeds_path_extraction_fails() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/data")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"value": 100}}"#)
            .create_async()
            .await;

        let url = format!("{}/data", server.url());
        // Use a path that does not exist in the JSON
        let df = test_datafeed(&url, "data.nonexistent.deep");
        let results = check_feeds(&[df], TEST_TIMEOUT).await;

        mock.assert_async().await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert!(results[0].message.contains("JSON path extraction failed"));
    }

    #[tokio::test]
    async fn test_check_one_feed_unreachable_url() {
        // Use a URL that will not connect
        let df = test_datafeed("http://127.0.0.1:19876/unreachable", "price");
        let results = check_one_feed(&df, Duration::from_secs(2)).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert_eq!(results[0].category, CheckCategory::Feed);
        assert!(results[0].message.contains("HTTP request failed"));
    }
}
