//! Error types for event monitoring

use thiserror::Error;

/// Errors that can occur during event monitoring operations
#[derive(Error, Debug)]
pub enum EventMonitorError {
    /// Network not found or unavailable
    #[error("Network '{0}' not found or unavailable")]
    NetworkNotFound(String),

    /// Failed to subscribe to events
    #[error("Failed to subscribe to events for monitor '{monitor}': {reason}")]
    SubscriptionError { monitor: String, reason: String },

    /// Webhook call failed
    #[error("Webhook call failed for monitor '{monitor}' after {attempts} attempts: {reason}")]
    WebhookError {
        monitor: String,
        attempts: u8,
        reason: String,
    },

    /// Response validation failed
    #[error("Response validation failed for monitor '{monitor}': {reason}")]
    ValidationError { monitor: String, reason: String },

    /// Configuration error
    #[error("Invalid configuration for monitor '{monitor}': {reason}")]
    ConfigError { monitor: String, reason: String },

    /// Event decoding error
    #[error("Failed to decode event for monitor '{monitor}': {reason}")]
    DecodingError { monitor: String, reason: String },

    /// Response handler error
    #[error("Response handler failed for monitor '{monitor}': {reason}")]
    HandlerError { monitor: String, reason: String },

    /// Provider error
    #[error("Provider error for network '{network}': {reason}")]
    ProviderError { network: String, reason: String },

    /// HTTP client error
    #[error("HTTP client error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Other errors
    #[error("{0}")]
    Other(String),
}

/// Result type for event monitor operations
pub type Result<T> = std::result::Result<T, EventMonitorError>;

impl From<String> for EventMonitorError {
    fn from(msg: String) -> Self {
        EventMonitorError::Other(msg)
    }
}

/// Extension trait for adding context to errors
pub trait EventMonitorErrorContext<T> {
    /// Add monitor name context to the error
    fn monitor_context(self, monitor: &str) -> Result<T>;

    /// Add network context to the error
    fn network_context(self, network: &str) -> Result<T>;
}

impl<T, E> EventMonitorErrorContext<T> for std::result::Result<T, E>
where
    E: Into<EventMonitorError>,
{
    fn monitor_context(self, monitor: &str) -> Result<T> {
        self.map_err(|e| {
            let err = e.into();
            match err {
                EventMonitorError::Other(msg) => {
                    EventMonitorError::Other(format!("Error in monitor '{monitor}': {msg}"))
                }
                _ => err,
            }
        })
    }

    fn network_context(self, network: &str) -> Result<T> {
        self.map_err(|e| {
            let err = e.into();
            match err {
                EventMonitorError::Other(msg) => {
                    EventMonitorError::Other(format!("Error on network '{network}': {msg}"))
                }
                _ => err,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = EventMonitorError::NetworkNotFound("ethereum-mainnet".to_string());
        assert_eq!(
            err.to_string(),
            "Network 'ethereum-mainnet' not found or unavailable"
        );

        let err = EventMonitorError::WebhookError {
            monitor: "test_monitor".to_string(),
            attempts: 3,
            reason: "Connection timeout".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Webhook call failed for monitor 'test_monitor' after 3 attempts: Connection timeout"
        );
    }

    #[test]
    fn test_error_context() {
        fn failing_operation() -> Result<()> {
            Err(EventMonitorError::Other("Something went wrong".to_string()))
        }

        let result = failing_operation().monitor_context("my_monitor");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error in monitor 'my_monitor': Something went wrong"
        );
    }
}
