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
