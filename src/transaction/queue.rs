use super::{TransactionRequest, TransactionResponse, TransactionType};
use crate::config::models::OmikujiConfig;
use crate::contracts::flux_aggregator::IFluxAggregator;
use crate::database::TransactionLogRepository;
use crate::gas::GasEstimator;
use crate::gas_price::GasPriceManager;
use crate::metrics::{ContractMetrics, UpdateMetrics};
use crate::network::NetworkManager;
use crate::utils::transaction_handler::{TransactionContext, TransactionHandler};
use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, I256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest as AlloyTransactionRequest,
    signers::local::PrivateKeySigner,
    sol_types::SolCall,
    transports::Transport,
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use url::Url;

/// Manages transaction submission with proper nonce coordination
pub struct TransactionQueue {
    /// Sender for submitting transactions
    tx_sender: mpsc::Sender<TransactionRequest>,
    /// Handle to the processing task
    processor_handle: Option<JoinHandle<()>>,
    /// Shared state for the queue
    state: Arc<QueueState>,
}

/// Internal state shared between queue and processor
struct QueueState {
    /// Configuration
    config: OmikujiConfig,
    /// Network manager for provider access
    network_manager: Arc<NetworkManager>,
    /// Transaction log repository
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
    /// Gas price manager for cost tracking
    gas_price_manager: Option<Arc<GasPriceManager>>,
    /// Nonce tracking per network (network -> (wallet_address, current_nonce))
    nonces: RwLock<HashMap<String, Mutex<(Address, u64)>>>,
}

impl TransactionQueue {
    /// Create a new transaction queue
    pub fn new(
        config: OmikujiConfig,
        network_manager: Arc<NetworkManager>,
        tx_log_repo: Option<Arc<TransactionLogRepository>>,
        gas_price_manager: Option<Arc<GasPriceManager>>,
    ) -> Self {
        let (tx_sender, rx) = mpsc::channel::<TransactionRequest>(100);
        
        let state = Arc::new(QueueState {
            config,
            network_manager,
            tx_log_repo,
            gas_price_manager,
            nonces: RwLock::new(HashMap::new()),
        });

        let processor_handle = Some(tokio::spawn(Self::process_queue(rx, state.clone())));

        Self {
            tx_sender,
            processor_handle,
            state,
        }
    }

    /// Submit a transaction to the queue
    pub async fn submit(&self, request: TransactionRequest) -> Result<()> {
        self.tx_sender
            .send(request)
            .await
            .map_err(|_| anyhow::anyhow!("Transaction queue is closed"))
    }

    /// Process the queue (runs in background task)
    async fn process_queue(mut rx: mpsc::Receiver<TransactionRequest>, state: Arc<QueueState>) {
        info!("Transaction queue processor started");

        while let Some(request) = rx.recv().await {
            // Process immediately to maintain order per network
            if let Err(e) = Self::process_transaction(request, &state).await {
                error!("Failed to process transaction: {}", e);
            }
        }

        info!("Transaction queue processor stopped");
    }

    /// Process a single transaction
    async fn process_transaction(request: TransactionRequest, state: &Arc<QueueState>) -> Result<()> {
        let network = &request.network;
        
        // Process based on transaction type
        match &request.transaction_type {
            TransactionType::DatafeedSubmission {
                feed_name,
                contract_address,
                round_id,
                value,
            } => {
                let result = Self::submit_datafeed_value(
                    state,
                    feed_name,
                    *contract_address,
                    *round_id,
                    *value,
                    network,
                    request.gas_limit,
                    request.max_retries,
                )
                .await;

                // Send response
                let _ = request.response_tx.send(result);
            }
            TransactionType::ScheduledTask { .. } | TransactionType::EventTriggered { .. } => {
                // TODO: Implement other transaction types
                let _ = request
                    .response_tx
                    .send(Err(anyhow::anyhow!("Transaction type not yet implemented")));
            }
        }

        Ok(())
    }

    /// Get or create nonce tracker for a network
    async fn get_nonce_for_network(
        state: &Arc<QueueState>,
        network: &str,
    ) -> Result<Arc<Mutex<(Address, u64)>>> {
        let nonces = state.nonces.read().await;
        if let Some(nonce_mutex) = nonces.get(network) {
            return Ok(Arc::new(Mutex::new(nonce_mutex.lock().await.clone())));
        }
        drop(nonces);

        // Initialize nonce for this network
        let mut nonces = state.nonces.write().await;
        
        // Double-check after acquiring write lock
        if let Some(nonce_mutex) = nonces.get(network) {
            return Ok(Arc::new(Mutex::new(nonce_mutex.lock().await.clone())));
        }

        // Get wallet address and initial nonce
        let wallet_address = state.network_manager.get_wallet_address(network)?;
        let provider = state.network_manager.get_provider(network)?;
        let initial_nonce = provider.get_transaction_count(wallet_address).await?;
        
        debug!("Initialized nonce for network {}: {} (address: {})", network, initial_nonce, wallet_address);
        
        let nonce_data = Mutex::new((wallet_address, initial_nonce));
        nonces.insert(network.to_string(), nonce_data);
        
        Ok(Arc::new(Mutex::new((wallet_address, initial_nonce))))
    }


    /// Submit a datafeed value
    async fn submit_datafeed_value(
        state: &Arc<QueueState>,
        feed_name: &str,
        contract_address: Address,
        round_id: U256,
        value: I256,
        network: &str,
        gas_limit: Option<U256>,
        _max_retries: Option<u32>,
    ) -> Result<TransactionResponse> {
        // Get network config
        let network_config = state
            .config
            .networks
            .iter()
            .find(|n| n.name == network)
            .ok_or_else(|| anyhow::anyhow!("Network {} not found in config", network))?;

        // Get nonce tracker for this network
        let nonce_tracker = Self::get_nonce_for_network(state, network).await?;
        let mut nonce_guard = nonce_tracker.lock().await;
        let (wallet_address, current_nonce) = *nonce_guard;

        // Create provider with signer inline
        let rpc_url = state.network_manager.get_rpc_url(network)?;
        let private_key = state.network_manager.get_private_key(network)?;
        
        let signer = private_key
            .parse::<PrivateKeySigner>()
            .with_context(|| "Failed to parse private key as signer")?;

        let wallet = EthereumWallet::from(signer);
        let url = Url::parse(rpc_url)
            .with_context(|| format!("Failed to parse RPC URL: {}", rpc_url))?;

        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(url);

        // Build transaction with explicit nonce
        let call = IFluxAggregator::submitCall {
            _roundId: round_id,
            _submission: value,
        };

        let mut tx = AlloyTransactionRequest::default()
            .to(contract_address)
            .input(call.abi_encode().into())
            .from(wallet_address)
            .nonce(current_nonce); // Set explicit nonce

        // Estimate gas
        let gas_estimator = GasEstimator::new(
            Arc::new(state.network_manager.get_provider(network)?.as_ref().clone()),
            network_config.clone(),
        );
        
        let gas_estimate = gas_estimator.estimate_gas(&tx).await?;
        tx = tx.with_gas_limit(gas_limit.unwrap_or(gas_estimate.gas_limit).to::<u64>());

        // Apply gas settings based on transaction type
        match network_config.transaction_type.to_lowercase().as_str() {
            "legacy" => {
                if let Some(gas_price) = gas_estimate.gas_price {
                    tx = tx.with_gas_price(gas_price.to::<u128>());
                }
            }
            "eip1559" => {
                if let Some(max_fee) = gas_estimate.max_fee_per_gas {
                    tx = tx.with_max_fee_per_gas(max_fee.to::<u128>());
                }
                if let Some(priority_fee) = gas_estimate.max_priority_fee_per_gas {
                    tx = tx.with_max_priority_fee_per_gas(priority_fee.to::<u128>());
                }
            }
            _ => {
                warn!("Unknown transaction type, defaulting to EIP-1559");
                if let Some(max_fee) = gas_estimate.max_fee_per_gas {
                    tx = tx.with_max_fee_per_gas(max_fee.to::<u128>());
                }
                if let Some(priority_fee) = gas_estimate.max_priority_fee_per_gas {
                    tx = tx.with_max_priority_fee_per_gas(priority_fee.to::<u128>());
                }
            }
        }

        info!(
            "Submitting transaction for {} on {} with nonce {}",
            feed_name, network, current_nonce
        );

        // Send transaction
        let pending_tx = provider.send_transaction(tx).await?;
        let tx_hash = *pending_tx.tx_hash();
        
        info!("Transaction sent: 0x{:x} with nonce {}", tx_hash, current_nonce);

        // Increment nonce for next transaction
        nonce_guard.1 += 1;
        drop(nonce_guard);

        // Wait for confirmation
        let receipt = pending_tx.get_receipt().await?;

        // Record metrics
        UpdateMetrics::record_update_attempt(feed_name, network, receipt.status());
        
        if receipt.status() {
            ContractMetrics::record_contract_write(
                feed_name,
                network,
                true,
                std::time::Duration::from_secs(1), // TODO: Track actual duration
                Some(&format!("0x{:x}", tx_hash)),
            );
        }

        // Use transaction handler for processing
        if let Some(tx_log_repo) = &state.tx_log_repo {
            let context = TransactionContext::Datafeed {
                feed_name: feed_name.to_string(),
            };

            TransactionHandler::new(receipt.clone(), context, network.to_string())
                .with_gas_price_manager(state.gas_price_manager.as_ref())
                .with_tx_log_repo(Some(tx_log_repo))
                .process()
                .await?;
        }

        Ok(TransactionResponse {
            tx_hash: format!("0x{:x}", tx_hash),
            block_number: receipt.block_number.unwrap_or(0),
            gas_used: receipt.gas_used as u64,
            success: receipt.status(),
        })
    }

    /// Shutdown the queue gracefully
    pub async fn shutdown(mut self) {
        drop(self.tx_sender);
        
        if let Some(handle) = self.processor_handle.take() {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_creation() {
        // This is a placeholder test
        // Real tests would require mock providers and config
        assert!(true);
    }
}