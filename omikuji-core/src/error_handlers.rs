//! Domain-specific error handlers and wrappers
//!
//! This module provides specialized error types and handlers for different
//! domains within the application, reducing boilerplate and ensuring consistency.

use crate::error_context;
use anyhow::{Context, Result};
use std::fmt::Display;
use thiserror::Error;

/// Network operation errors with context
#[derive(Error, Debug)]
pub enum NetworkOperationError {
    #[error("Provider creation failed for network '{network}': {source}")]
    ProviderCreation {
        network: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("RPC connection failed for network '{network}': {source}")]
    RpcConnection {
        network: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Key not found for network '{network}'")]
    KeyNotFound { network: String },

    #[error("Invalid RPC URL '{url}': {source}")]
    InvalidRpcUrl {
        url: String,
        #[source]
        source: anyhow::Error,
    },
}

impl NetworkOperationError {
    pub fn provider_creation(network: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self::ProviderCreation {
            network: network.into(),
            source: source.into(),
        }
    }

    pub fn rpc_connection(network: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self::RpcConnection {
            network: network.into(),
            source: source.into(),
        }
    }

    pub fn key_not_found(network: impl Into<String>) -> Self {
        Self::KeyNotFound {
            network: network.into(),
        }
    }

    pub fn invalid_rpc_url(url: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self::InvalidRpcUrl {
            url: url.into(),
            source: source.into(),
        }
    }
}

/// Database operation errors with context
#[derive(Error, Debug)]
pub enum DatabaseOperationError {
    #[error("Failed to {operation} {entity}: {source}")]
    Operation {
        operation: String,
        entity: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Database connection failed: {source}")]
    Connection {
        #[source]
        source: anyhow::Error,
    },

    #[error("Migration failed: {source}")]
    Migration {
        #[source]
        source: anyhow::Error,
    },

    #[error("{entity} not found with {identifier}")]
    NotFound { entity: String, identifier: String },
}

impl DatabaseOperationError {
    pub fn operation(
        operation: impl Into<String>,
        entity: impl Into<String>,
        source: impl Into<anyhow::Error>,
    ) -> Self {
        Self::Operation {
            operation: operation.into(),
            entity: entity.into(),
            source: source.into(),
        }
    }

    pub fn not_found(entity: impl Into<String>, identifier: impl Display) -> Self {
        Self::NotFound {
            entity: entity.into(),
            identifier: identifier.to_string(),
        }
    }
}

/// Contract operation errors with context
#[derive(Error, Debug)]
pub enum ContractOperationError {
    #[error("Contract call '{method}' failed: {source}")]
    CallFailed {
        method: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Failed to decode {method} response: {source}")]
    DecodeFailed {
        method: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Transaction failed: {reason}")]
    TransactionFailed { reason: String },

    #[error("Invalid contract address '{address}': {source}")]
    InvalidAddress {
        address: String,
        #[source]
        source: anyhow::Error,
    },
}

impl ContractOperationError {
    pub fn call_failed(method: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self::CallFailed {
            method: method.into(),
            source: source.into(),
        }
    }

    pub fn decode_failed(method: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self::DecodeFailed {
            method: method.into(),
            source: source.into(),
        }
    }

    pub fn transaction_failed(reason: impl Into<String>) -> Self {
        Self::TransactionFailed {
            reason: reason.into(),
        }
    }
}

/// Helper trait for wrapping errors with domain context
pub trait ErrorWrapper<T> {
    /// Wrap a network operation error
    fn wrap_network_error(self, network: &str, operation: &str) -> Result<T>;

    /// Wrap a database operation error
    fn wrap_db_error(self, entity: &str, operation: &str) -> Result<T>;

    /// Wrap a contract operation error
    fn wrap_contract_error(self, method: &str) -> Result<T>;
}

impl<T, E> ErrorWrapper<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn wrap_network_error(self, network: &str, operation: &str) -> Result<T> {
        self.with_context(|| error_context::network::connection(network))
            .with_context(|| format!("During {operation} operation"))
    }

    fn wrap_db_error(self, entity: &str, operation: &str) -> Result<T> {
        self.map_err(|e| {
            DatabaseOperationError::operation(operation, entity, anyhow::Error::new(e)).into()
        })
    }

    fn wrap_contract_error(self, method: &str) -> Result<T> {
        self.map_err(|e| ContractOperationError::call_failed(method, anyhow::Error::new(e)).into())
    }
}

/// Common validation helpers that return appropriate errors
pub mod validation {
    use super::*;

    /// Validate that a value is not empty
    pub fn require_not_empty(value: &str, field_name: &str) -> Result<()> {
        if value.trim().is_empty() {
            anyhow::bail!(error_context::validation_error(
                field_name,
                value,
                "cannot be empty"
            ));
        }
        Ok(())
    }

    /// Validate that a network exists in the configuration
    pub fn require_network_exists(network_name: &str, available_networks: &[String]) -> Result<()> {
        if !available_networks.contains(&network_name.to_string()) {
            anyhow::bail!(error_context::not_found("Network", network_name));
        }
        Ok(())
    }

    /// Validate a numeric value is within range
    pub fn require_in_range<T: PartialOrd + Display>(
        value: T,
        min: T,
        max: T,
        field_name: &str,
    ) -> Result<()> {
        if value < min || value > max {
            anyhow::bail!(error_context::validation_error(
                field_name,
                &value,
                &format!("must be between {min} and {max}")
            ));
        }
        Ok(())
    }

    /// Validate that a value is positive
    pub fn require_positive<T: PartialOrd + Default + Display>(
        value: T,
        field_name: &str,
    ) -> Result<()> {
        if value <= T::default() {
            anyhow::bail!(error_context::validation_error(
                field_name,
                &value,
                "must be positive"
            ));
        }
        Ok(())
    }
}

/// Retry helper with context-aware error handling
pub async fn retry_with_context<F, Fut, T>(
    operation_name: &str,
    max_attempts: u32,
    mut f: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt < max_attempts {
                    tracing::warn!(
                        "{} failed (attempt {}/{}): {}",
                        operation_name,
                        attempt,
                        max_attempts,
                        e
                    );
                    last_error = Some(e);
                    // Could add exponential backoff here
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
                } else {
                    return Err(e).with_context(|| {
                        format!("{operation_name} failed after {max_attempts} attempts")
                    });
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Retry failed with no error")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_network_operation_errors() {
        let err = NetworkOperationError::provider_creation(
            "mainnet",
            anyhow::anyhow!("Connection refused"),
        );
        assert!(err
            .to_string()
            .contains("Provider creation failed for network 'mainnet'"));

        let err = NetworkOperationError::key_not_found("testnet");
        assert_eq!(err.to_string(), "Key not found for network 'testnet'");
    }

    #[test]
    fn test_database_operation_errors() {
        let err = DatabaseOperationError::operation(
            "insert",
            "feed_log",
            anyhow::anyhow!("Constraint violation"),
        );
        assert!(err.to_string().contains("Failed to insert feed_log"));

        let err = DatabaseOperationError::not_found("User", "123");
        assert_eq!(err.to_string(), "User not found with 123");
    }

    #[test]
    fn test_validation_helpers() {
        assert!(validation::require_not_empty("test", "field").is_ok());
        assert!(validation::require_not_empty("", "field").is_err());

        assert!(validation::require_positive(10, "value").is_ok());
        assert!(validation::require_positive(-5, "value").is_err());

        assert!(validation::require_in_range(5, 1, 10, "value").is_ok());
        assert!(validation::require_in_range(15, 1, 10, "value").is_err());
    }

    #[tokio::test]
    async fn test_retry_with_context() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();

        let result = retry_with_context("test operation", 3, || {
            let count = count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                if count.load(Ordering::SeqCst) < 3 {
                    Err(anyhow::anyhow!("Temporary failure"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }
}
