use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error};

use crate::metrics::DatasourceMetrics;

/// Errors that can occur when fetching data
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("HTTP error with status code: {0}")]
    Http(u16),

    #[error("Network error: {0}")]
    Network(String),

    #[error("JSON parsing error: {0}")]
    Json(String),
}

/// Fetches JSON data from a given URL
pub struct Fetcher {
    client: Client,
}

impl Fetcher {
    /// Creates a new Fetcher with a reusable HTTP client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Fetches JSON data from the specified URL
    /// Returns the parsed JSON value on success
    pub async fn fetch_json(&self, url: &str, feed_name: &str, network: &str) -> Result<Value> {
        debug!(
            "Fetching data from: {} for feed {}/{}",
            url, feed_name, network
        );

        let start_time = Instant::now();

        let response = match self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let duration = start_time.elapsed();
                error!("Network error fetching from {}: {}", url, e);

                // Record HTTP error metric
                DatasourceMetrics::record_http_error(
                    feed_name,
                    network,
                    url,
                    &e.to_string(),
                    Some(duration),
                );

                return Err(FetchError::Network(e.to_string()).into());
            }
        };

        let status = response.status();
        let content_length = response.content_length().map(|len| len as usize);

        if !status.is_success() {
            let duration = start_time.elapsed();
            error!(
                "HTTP error {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            );

            // Record HTTP request metric with error status
            DatasourceMetrics::record_http_request(
                feed_name,
                network,
                "GET",
                url,
                status.as_u16(),
                duration,
                content_length,
            );

            return Err(FetchError::Http(status.as_u16()).into());
        }

        // Parse JSON and measure parsing time
        let parse_start = Instant::now();
        let json: Value = match response.json().await {
            Ok(json) => {
                let parse_duration = parse_start.elapsed();
                let total_duration = start_time.elapsed();

                // Record successful metrics
                DatasourceMetrics::record_http_request(
                    feed_name,
                    network,
                    "GET",
                    url,
                    status.as_u16(),
                    total_duration,
                    content_length,
                );

                DatasourceMetrics::record_parse_operation(
                    feed_name,
                    network,
                    true,
                    parse_duration,
                    None,
                );

                DatasourceMetrics::record_datasource_operation(
                    feed_name,
                    network,
                    true,
                    total_duration,
                );

                json
            }
            Err(e) => {
                let parse_duration = parse_start.elapsed();
                let total_duration = start_time.elapsed();

                error!("JSON parsing error: {}", e);

                // Record parsing error
                DatasourceMetrics::record_parse_operation(
                    feed_name,
                    network,
                    false,
                    parse_duration,
                    Some("json_parse_error"),
                );

                DatasourceMetrics::record_datasource_operation(
                    feed_name,
                    network,
                    false,
                    total_duration,
                );

                return Err(FetchError::Json(e.to_string()).into());
            }
        };

        debug!("Successfully fetched and parsed JSON data");
        Ok(json)
    }
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}
