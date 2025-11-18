//! Error context utilities for consistent error handling
//!
//! This module provides helpers for creating consistent error contexts
//! throughout the codebase, reducing duplication and improving maintainability.

use anyhow::{Context, Result};
use std::fmt::Display;

/// Standard error context messages for common operations
pub mod messages {
    /// Create a "Failed to {action} {object}" message
    pub fn failed_to(action: &str, object: &str) -> String {
        format!("Failed to {action} {object}")
    }

    /// Create a "Failed to parse {type}: {value}" message
    pub fn failed_to_parse(type_name: &str, value: impl std::fmt::Display) -> String {
        format!("Failed to parse {type_name}: {value}")
    }

    /// Create a "Failed to get {property} from {source}" message
    pub fn failed_to_get(property: &str, source: &str) -> String {
        format!("Failed to get {property} from {source}")
    }

    /// Create a "Failed to create {component}" message
    pub fn failed_to_create(component: &str) -> String {
        format!("Failed to create {component}")
    }

    /// Create a "{operation} failed for {target}" message
    pub fn operation_failed(operation: &str, target: &str) -> String {
        format!("{operation} failed for {target}")
    }
}

/// Database-specific error contexts
pub mod database {
    use super::messages;

    pub fn insert(table: &str) -> String {
        messages::failed_to("insert", table)
    }

    pub fn get(table: &str) -> String {
        messages::failed_to("get", table)
    }

    pub fn update(table: &str) -> String {
        messages::failed_to("update", table)
    }

    pub fn delete(table: &str) -> String {
        messages::failed_to("delete", table)
    }

    pub fn query(operation: &str) -> String {
        format!("Database query failed: {operation}")
    }

    pub fn connection() -> &'static str {
        "Failed to establish database connection"
    }

    pub fn migration() -> &'static str {
        "Failed to run database migrations"
    }
}

/// Network-specific error contexts
pub mod network {
    use super::messages;

    pub fn provider_creation(network_name: &str) -> String {
        format!("Failed to create provider for network '{network_name}'")
    }

    pub fn rpc_url_parse(url: &str) -> String {
        messages::failed_to_parse("RPC URL", url)
    }

    pub fn key_retrieval(network_name: &str) -> String {
        format!("Failed to retrieve key for network '{network_name}'")
    }

    pub fn block_number(network_name: &str) -> String {
        format!("Failed to get block number for network '{network_name}'")
    }

    pub fn chain_id(network_name: &str) -> String {
        format!("Failed to get chain ID for network '{network_name}'")
    }

    pub fn connection(network_name: &str) -> String {
        format!("Failed to connect to network '{network_name}'")
    }
}

/// Contract-specific error contexts
pub mod contract {
    use super::messages;

    pub fn decode(method_name: &str) -> String {
        format!("Failed to decode {method_name} response")
    }

    pub fn call(method_name: &str) -> String {
        format!("Contract call {method_name} failed")
    }

    pub fn transaction_send() -> &'static str {
        "Failed to send transaction"
    }

    pub fn transaction_confirmation() -> &'static str {
        "Failed to confirm transaction"
    }

    pub fn address_parse(address: &str) -> String {
        messages::failed_to_parse("contract address", address)
    }

    pub fn abi_encode(function: &str) -> String {
        format!("Failed to encode ABI for function '{function}'")
    }

    pub fn abi_decode(function: &str) -> String {
        format!("Failed to decode ABI for function '{function}'")
    }
}

/// Configuration-specific error contexts
pub mod config {

    pub fn load(path: &str) -> String {
        format!("Failed to load configuration from '{path}'")
    }

    pub fn parse(field: &str) -> String {
        format!("Failed to parse configuration field '{field}'")
    }

    pub fn validate(reason: &str) -> String {
        format!("Configuration validation failed: {reason}")
    }

    pub fn missing_field(field: &str) -> String {
        format!("Missing required configuration field '{field}'")
    }
}

/// Key storage error contexts
pub mod key_storage {

    pub fn store(network: &str) -> String {
        format!("Failed to store key for network '{network}'")
    }

    pub fn retrieve(network: &str) -> String {
        format!("Failed to retrieve key for network '{network}'")
    }

    pub fn remove(network: &str) -> String {
        format!("Failed to remove key for network '{network}'")
    }

    pub fn parse() -> &'static str {
        "Failed to parse private key"
    }

    pub fn list() -> &'static str {
        "Failed to list stored keys"
    }
}

/// Extension trait for Result types to add common error contexts
pub trait ErrorContextExt<T> {
    /// Add context for a database operation
    fn context_db(self, operation: &str, table: &str) -> Result<T>;

    /// Add context for a network operation
    fn context_network(self, operation: &str, network: &str) -> Result<T>;

    /// Add context for a contract operation
    fn context_contract(self, operation: &str, contract: &str) -> Result<T>;

    /// Add context with dynamic formatting
    fn context_fmt<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ErrorContextExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context_db(self, operation: &str, table: &str) -> Result<T> {
        self.with_context(|| format!("Database {operation} operation failed for {table}"))
    }

    fn context_network(self, operation: &str, network: &str) -> Result<T> {
        self.with_context(|| format!("Network {operation} operation failed for '{network}'"))
    }

    fn context_contract(self, operation: &str, contract: &str) -> Result<T> {
        self.with_context(|| format!("Contract {operation} operation failed for '{contract}'"))
    }

    fn context_fmt<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.with_context(f)
    }
}

/// Helper for creating validation error contexts
pub fn validation_error(field: &str, value: impl Display, reason: &str) -> String {
    format!("Invalid {field} '{value}': {reason}")
}

/// Helper for creating not found error contexts
pub fn not_found(resource: &str, identifier: impl Display) -> String {
    format!("{resource} '{identifier}' not found")
}

/// Helper for creating permission error contexts
pub fn permission_denied(action: &str, resource: &str) -> String {
    format!("Permission denied to {action} {resource}")
}

/// Helper for creating timeout error contexts
pub fn timeout(operation: &str, duration: impl Display) -> String {
    format!("{operation} timed out after {duration}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_helpers() {
        assert_eq!(
            messages::failed_to("parse", "config"),
            "Failed to parse config"
        );
        assert_eq!(
            messages::failed_to_parse("address", "0x123"),
            "Failed to parse address: 0x123"
        );
        assert_eq!(
            messages::failed_to_get("balance", "wallet"),
            "Failed to get balance from wallet"
        );
    }

    #[test]
    fn test_database_contexts() {
        assert_eq!(database::insert("feed_log"), "Failed to insert feed_log");
        assert_eq!(database::get("transaction"), "Failed to get transaction");
        assert_eq!(
            database::query("SELECT * FROM users"),
            "Database query failed: SELECT * FROM users"
        );
    }

    #[test]
    fn test_network_contexts() {
        assert_eq!(
            network::provider_creation("mainnet"),
            "Failed to create provider for network 'mainnet'"
        );
        assert_eq!(
            network::rpc_url_parse("http://invalid"),
            "Failed to parse RPC URL: http://invalid"
        );
    }

    #[test]
    fn test_contract_contexts() {
        assert_eq!(
            contract::decode("latestAnswer"),
            "Failed to decode latestAnswer response"
        );
        assert_eq!(contract::call("submit"), "Contract call submit failed");
    }

    #[test]
    fn test_validation_helpers() {
        assert_eq!(
            validation_error("gas_price", "100", "must be positive"),
            "Invalid gas_price '100': must be positive"
        );
        assert_eq!(
            not_found("Network", "testnet"),
            "Network 'testnet' not found"
        );
        assert_eq!(
            permission_denied("write", "database"),
            "Permission denied to write database"
        );
        assert_eq!(
            timeout("Transaction", "30s"),
            "Transaction timed out after 30s"
        );
    }

    #[test]
    fn test_error_context_ext() {
        let result: std::result::Result<i32, std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));

        let with_db_context = result.context_db("insert", "users");
        assert!(with_db_context.is_err());
        let err = with_db_context.unwrap_err();
        assert!(err
            .to_string()
            .contains("Database insert operation failed for users"));
    }
}
