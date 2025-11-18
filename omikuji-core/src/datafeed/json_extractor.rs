use anyhow::{Context, Result};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Extracts values from JSON using dot-notation paths
pub struct JsonExtractor;

impl JsonExtractor {
    /// Extracts a float value from JSON using a dot-notation path
    ///
    /// # Arguments
    /// * `json` - The JSON value to extract from
    /// * `path` - Dot-notation path (e.g., "RAW.ETH.USD.PRICE")
    ///
    /// # Returns
    /// The extracted float value
    pub fn extract_float(json: &Value, path: &str) -> Result<f64> {
        let components: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for (index, component) in components.iter().enumerate() {
            current = current.get(component).with_context(|| {
                format!(
                    "Failed to extract path component '{component}' at position {index} in path '{path}'"
                )
            })?;
        }

        // Try to extract as float
        match current {
            Value::Number(n) => n
                .as_f64()
                .with_context(|| format!("Failed to convert number to f64 at path '{path}'")),
            Value::String(s) => s
                .parse::<f64>()
                .with_context(|| format!("Failed to parse string '{s}' as f64 at path '{path}'")),
            _ => {
                anyhow::bail!(
                    "Value at path '{}' is not a number or string, found: {:?}",
                    path,
                    current
                );
            }
        }
    }

    /// Extracts a timestamp from JSON using a dot-notation path
    ///
    /// # Arguments
    /// * `json` - The JSON value to extract from
    /// * `path` - Optional dot-notation path
    ///
    /// # Returns
    /// The extracted timestamp or current time if path is None
    pub fn extract_timestamp(json: &Value, path: Option<&str>) -> Result<u64> {
        match path {
            Some(p) => {
                let value = Self::extract_float(json, p)?;
                Ok(value as u64)
            }
            None => {
                // Generate current timestamp
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .context("Failed to get current timestamp")?;
                Ok(now.as_secs())
            }
        }
    }

    /// Extracts both value and timestamp from JSON
    ///
    /// # Arguments
    /// * `json` - The JSON value to extract from
    /// * `value_path` - Dot-notation path for the value
    /// * `timestamp_path` - Optional dot-notation path for the timestamp
    ///
    /// # Returns
    /// Tuple of (value, timestamp)
    pub fn extract_feed_data(
        json: &Value,
        value_path: &str,
        timestamp_path: Option<&str>,
    ) -> Result<(f64, u64)> {
        let value = Self::extract_float(json, value_path)?;

        // Try to extract timestamp, but fall back to current time if it fails
        let timestamp = match Self::extract_timestamp(json, timestamp_path) {
            Ok(ts) => ts,
            Err(e) => {
                if timestamp_path.is_some() {
                    // Log warning if a timestamp path was provided but extraction failed
                    warn!(
                        "Failed to extract timestamp from path {:?}: {}",
                        timestamp_path, e
                    );
                }
                // Fall back to current time
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .context("Failed to get current timestamp")?
                    .as_secs()
            }
        };

        debug!(
            "Extracted feed data: value={}, timestamp={}",
            value, timestamp
        );

        Ok((value, timestamp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_feed_data_with_timestamp() {
        let json_data = json!({
            "data": {
                "rates": {
                    "USD": "2500.50"
                },
                "epoch": 1700000000
            }
        });

        let result =
            JsonExtractor::extract_feed_data(&json_data, "data.rates.USD", Some("data.epoch"));
        assert!(result.is_ok());

        let (value, timestamp) = result.unwrap();
        assert_eq!(value, 2500.50);
        assert_eq!(timestamp, 1700000000);
    }

    #[test]
    fn test_extract_feed_data_without_timestamp() {
        let json_data = json!({
            "price": 1800.25
        });

        let result = JsonExtractor::extract_feed_data(&json_data, "price", None);
        assert!(result.is_ok());

        let (value, timestamp) = result.unwrap();
        assert_eq!(value, 1800.25);
        // Timestamp should be current time (approximately)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Allow for a few seconds difference
        assert!((timestamp as i64 - now as i64).abs() < 5);
    }

    #[test]
    fn test_extract_timestamp_with_string_value() {
        let json_data = json!({
            "data": {
                "timestamp": "1700000000"
            }
        });

        let result = JsonExtractor::extract_timestamp(&json_data, Some("data.timestamp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1700000000);
    }

    #[test]
    fn test_extract_timestamp_missing_path() {
        let json_data = json!({
            "data": {
                "value": 100
            }
        });

        let result = JsonExtractor::extract_timestamp(&json_data, Some("data.missing_timestamp"));
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to extract path component 'missing_timestamp'"));
    }

    #[test]
    fn test_extract_feed_data_missing_timestamp_path() {
        let json_data = json!({
            "price": 1500.75,
            "data": {
                "value": "unused"
            }
        });

        // Should succeed but warn about missing timestamp
        let result =
            JsonExtractor::extract_feed_data(&json_data, "price", Some("missing.timestamp"));
        assert!(result.is_ok());

        let (value, timestamp) = result.unwrap();
        assert_eq!(value, 1500.75);
        // Should get current timestamp since extraction failed
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!((timestamp as i64 - now as i64).abs() < 5);
    }

    #[test]
    fn test_extract_numeric_timestamp() {
        let json_data = json!({
            "time": 1650000000
        });

        let result = JsonExtractor::extract_timestamp(&json_data, Some("time"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1650000000);
    }

    #[test]
    fn test_extract_float_timestamp() {
        let json_data = json!({
            "timestamp": 1650000000.5
        });

        let result = JsonExtractor::extract_timestamp(&json_data, Some("timestamp"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1650000000); // Should truncate to u64
    }
}
