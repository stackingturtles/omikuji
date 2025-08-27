pub mod queue;

use alloy::primitives::{Address, I256, U256};
use anyhow::Result;
use tokio::sync::oneshot;

/// Types of transactions that can be submitted through the queue
#[derive(Debug, Clone)]
pub enum TransactionType {
    /// Datafeed price submission
    DatafeedSubmission {
        feed_name: String,
        contract_address: Address,
        round_id: U256,
        value: I256,
    },
    /// Scheduled task execution
    ScheduledTask {
        task_name: String,
        contract_address: Address,
        calldata: Vec<u8>,
    },
    /// Event-triggered transaction
    EventTriggered {
        event_name: String,
        contract_address: Address,
        calldata: Vec<u8>,
    },
}

/// A transaction submission request
#[derive(Debug)]
pub struct TransactionRequest {
    /// Network to submit the transaction on
    pub network: String,
    /// Type and details of the transaction
    pub transaction_type: TransactionType,
    /// Channel to send the result back
    pub response_tx: oneshot::Sender<Result<TransactionResponse>>,
    /// Optional gas limit override
    pub gas_limit: Option<U256>,
    /// Optional max retries override
    pub max_retries: Option<u32>,
}

/// Response from a transaction submission
#[derive(Debug, Clone)]
pub struct TransactionResponse {
    /// Transaction hash
    pub tx_hash: String,
    /// Block number where transaction was mined
    pub block_number: u64,
    /// Gas used
    pub gas_used: u64,
    /// Whether the transaction succeeded
    pub success: bool,
}