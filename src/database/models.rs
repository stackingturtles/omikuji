use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Represents a single feed value log entry in the database
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FeedLog {
    /// Auto-incrementing internal feed ID
    pub id: i32,

    /// Feed name as defined in config.yaml
    pub feed_name: String,

    /// Network name for the feed
    pub network_name: String,

    /// The value retrieved from the feed
    pub feed_value: f64,

    /// Timestamp as reported by the feed
    pub feed_timestamp: i64,

    /// Timestamp when the system recorded the value
    pub updated_at: DateTime<Utc>,

    /// HTTP status code if different from 200
    pub error_status_code: Option<i32>,

    /// Whether there was a network error (no HTTP response)
    pub network_error: bool,

    /// Timestamp when the record was created
    pub created_at: DateTime<Utc>,
}

/// Parameters for creating a new feed log entry
#[derive(Debug, Clone)]
pub struct NewFeedLog {
    pub feed_name: String,
    pub network_name: String,
    pub feed_value: f64,
    pub feed_timestamp: i64,
    pub error_status_code: Option<i32>,
    pub network_error: bool,
}

/// Represents a processed event entry in the database
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProcessedEvent {
    /// Auto-incrementing ID
    pub id: i32,

    /// Transaction hash containing the event (with 0x prefix)
    pub transaction_hash: String,

    /// Log index of the event within the transaction
    pub log_index: i32,

    /// Address of the contract that emitted the event
    pub contract_address: String,

    /// Type of event (e.g., mint, burn, deposit, HasItRainedSince)
    pub event_type: String,

    /// Timestamp when the event was processed by Omikuji
    pub processed_at: DateTime<Utc>,

    /// Optional JSON data about the event for debugging/auditing
    pub event_data: Option<serde_json::Value>,
}

/// Parameters for creating a new processed event entry
#[derive(Debug, Clone)]
pub struct NewProcessedEvent {
    pub transaction_hash: String,
    pub log_index: i32,
    pub contract_address: String,
    pub event_type: String,
    pub event_data: Option<serde_json::Value>,
}

/// Represents a contract execution triggered by an event monitor
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ContractExecution {
    /// Auto-incrementing ID
    pub id: i32,

    /// Name of the event monitor that triggered this execution
    pub monitor_name: String,

    /// Network where the execution occurred
    pub network: String,

    /// Transaction hash of the execution (with 0x prefix)
    pub transaction_hash: String,

    /// Contract address that was called
    pub contract_address: String,

    /// Function selector that was called (4 bytes hex)
    pub function_selector: String,

    /// Encoded function call data (hex)
    pub call_data: String,

    /// Value sent with the transaction in wei
    pub value_wei: String,

    /// Gas limit used for the transaction
    pub gas_limit: i64,

    /// Gas price in wei
    pub gas_price_wei: String,

    /// Actual gas used (filled after transaction is mined)
    pub gas_used: Option<i64>,

    /// Transaction status: pending, success, failed
    pub status: String,

    /// Error message if the transaction failed
    pub error_message: Option<String>,

    /// The event that triggered this execution (transaction hash + log index)
    pub trigger_event_tx_hash: String,
    pub trigger_event_log_index: i32,

    /// Timestamp when the execution was initiated
    pub created_at: DateTime<Utc>,

    /// Timestamp when the execution status was last updated
    pub updated_at: DateTime<Utc>,
}

/// Parameters for creating a new contract execution entry
#[derive(Debug, Clone)]
pub struct NewContractExecution {
    pub monitor_name: String,
    pub network: String,
    pub transaction_hash: String,
    pub contract_address: String,
    pub function_selector: String,
    pub call_data: String,
    pub value_wei: String,
    pub gas_limit: i64,
    pub gas_price_wei: String,
    pub trigger_event_tx_hash: String,
    pub trigger_event_log_index: i32,
}
