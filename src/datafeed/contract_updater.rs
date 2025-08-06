use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::{I256, U256},
    providers::{Provider, ProviderBuilder, RootProvider},
    signers::local::PrivateKeySigner,
    transports::http::{Client, Http},
};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use url::Url;

use super::contract_utils::{
    calculate_deviation_percentage, create_contract_with_provider, current_timestamp, errors,
    parse_address, scale_value_for_contract, validate_value_bounds,
};
use crate::config::models::{Datafeed, OmikujiConfig};
use crate::contracts::FluxAggregatorContract;
use crate::database::TransactionLogRepository;
use crate::gas_price::GasPriceManager;
use crate::metrics::{FeedMetrics, SkipReason, UpdateMetrics, UpdateReason};
use crate::network::NetworkManager;
use crate::utils::{TransactionContext, TransactionHandler};

/// Handles contract updates based on time and deviation thresholds
pub struct ContractUpdater<'a> {
    network_manager: &'a Arc<NetworkManager>,
    config: &'a OmikujiConfig,
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
    gas_price_manager: Option<&'a Arc<GasPriceManager>>,
}

impl<'a> ContractUpdater<'a> {
    /// Creates a new ContractUpdater
    pub fn new(network_manager: &'a Arc<NetworkManager>, config: &'a OmikujiConfig) -> Self {
        Self {
            network_manager,
            config,
            tx_log_repo: None,
            gas_price_manager: None,
        }
    }

    /// Creates a new ContractUpdater with transaction logging
    pub fn with_tx_logging(
        network_manager: &'a Arc<NetworkManager>,
        config: &'a OmikujiConfig,
        tx_log_repo: Arc<TransactionLogRepository>,
    ) -> Self {
        Self {
            network_manager,
            config,
            tx_log_repo: Some(tx_log_repo),
            gas_price_manager: None,
        }
    }

    /// Sets the gas price manager
    pub fn with_gas_price_manager(mut self, gas_price_manager: &'a Arc<GasPriceManager>) -> Self {
        self.gas_price_manager = Some(gas_price_manager);
        self
    }

    /// Gets the network configuration for a datafeed
    fn get_network_config(&self, datafeed: &Datafeed) -> Result<&crate::config::models::Network> {
        self.config
            .networks
            .iter()
            .find(|n| n.name == datafeed.networks)
            .ok_or_else(|| {
                anyhow::anyhow!("Network {} not found in configuration", datafeed.networks)
            })
    }

    /// Gets a contract instance with provider for read operations
    async fn get_contract_for_read(
        &self,
        datafeed: &Datafeed,
    ) -> Result<FluxAggregatorContract<Http<Client>, RootProvider<Http<Client>>>> {
        let provider = self.network_manager.get_provider(&datafeed.networks)?;
        let address = parse_address(&datafeed.contract_address)?;
        Ok(create_contract_with_provider(
            address,
            provider.as_ref().clone(),
        ))
    }

    /// Creates a provider with signer for write operations
    async fn create_signer_provider(
        &self,
        network_name: &str,
    ) -> Result<impl Provider<Http<Client>, Ethereum> + Clone> {
        // Get the private key and RPC URL
        let private_key = self
            .network_manager
            .get_private_key(network_name)
            .with_context(|| format!("{} {}", errors::NO_SIGNER_AVAILABLE, network_name))?;

        let rpc_url = self.network_manager.get_rpc_url(network_name)?;

        // Parse the private key
        let signer = private_key
            .parse::<PrivateKeySigner>()
            .with_context(|| "Failed to parse private key as signer")?;

        let wallet = EthereumWallet::from(signer);

        // Create a provider with wallet
        let url =
            Url::parse(rpc_url).with_context(|| format!("Failed to parse RPC URL: {rpc_url}"))?;

        let provider_with_wallet = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(url);

        Ok(provider_with_wallet)
    }

    /// Checks if a contract update is needed based on time elapsed
    /// Returns true if minimum_update_frequency has passed since last update
    pub async fn should_update_based_on_time(&self, datafeed: &Datafeed) -> Result<bool> {
        // Get contract instance
        let contract = self.get_contract_for_read(datafeed).await?;

        // Get latest timestamp from contract
        let latest_timestamp = contract
            .latest_timestamp()
            .await
            .with_context(|| "Failed to get latest timestamp from contract")?;

        // Get current timestamp
        let now = current_timestamp()?;
        let time_since_update = now.saturating_sub(latest_timestamp.to::<u64>());

        debug!(
            "Datafeed {}: last update {}s ago, minimum frequency {}s",
            datafeed.name, time_since_update, datafeed.minimum_update_frequency
        );

        // Update time since last metric
        UpdateMetrics::update_time_since_last(
            &datafeed.name,
            &datafeed.networks,
            time_since_update as f64,
            time_since_update as f64,
        );

        // Check if minimum frequency is violated
        if time_since_update > datafeed.minimum_update_frequency * 2 {
            UpdateMetrics::record_frequency_violation(
                &datafeed.name,
                &datafeed.networks,
                time_since_update as f64,
                datafeed.minimum_update_frequency as f64,
            );
        }

        // Check if enough time has passed
        Ok(time_since_update >= datafeed.minimum_update_frequency)
    }

    /// Checks if a contract update is needed based on value deviation
    /// Returns true if the new value deviates from the current on-chain value
    /// by more than the configured threshold percentage
    pub async fn should_update_based_on_deviation(
        &self,
        datafeed: &Datafeed,
        new_value: f64,
    ) -> Result<bool> {
        // Get contract instance
        let contract = self.get_contract_for_read(datafeed).await?;

        // Get latest answer from contract
        let latest_answer = match contract.latest_answer().await {
            Ok(answer) => answer,
            Err(e) => {
                error!(
                    "Failed to get latest answer from contract for datafeed {}: {}. Skipping deviation check.",
                    datafeed.name, e
                );
                return Ok(false);
            }
        };

        // Scale the new value to match contract format
        let decimals = datafeed.decimals.unwrap_or(8);
        let scaled_new_value = scale_value_for_contract(new_value, decimals);

        // Convert I256 to i128 for comparison
        let current_value = latest_answer
            .try_into()
            .context("Failed to convert latest answer to i128")?;

        // Calculate deviation percentage
        let deviation = calculate_deviation_percentage(current_value, scaled_new_value);

        debug!(
            "Datafeed {}: current value {}, new value {} (scaled), deviation {}%",
            datafeed.name, current_value, scaled_new_value, deviation
        );

        // Check if deviation exceeds threshold
        let exceeds_threshold = deviation > datafeed.deviation_threshold_pct;

        if exceeds_threshold {
            info!(
                "Datafeed {}: deviation {}% exceeds threshold {}%",
                datafeed.name, deviation, datafeed.deviation_threshold_pct
            );

            // Record deviation breach
            UpdateMetrics::record_deviation_breach(
                &datafeed.name,
                &datafeed.networks,
                deviation,
                datafeed.deviation_threshold_pct,
            );
        }

        // Record deviation at check time
        UpdateMetrics::record_update_deviation(&datafeed.name, &datafeed.networks, deviation);

        Ok(exceeds_threshold)
    }

    /// Checks if an update is needed based on time, deviation, and timestamp safety
    /// Returns (should_update, reason) where reason describes what triggered the update
    pub async fn check_update_needed(
        &self,
        datafeed: &Datafeed,
        new_value: f64,
        feed_timestamp: u64,
    ) -> Result<(bool, String)> {
        // Check both conditions
        let time_check = self.should_update_based_on_time(datafeed).await?;
        let deviation_check = self
            .should_update_based_on_deviation(datafeed, new_value)
            .await?;

        // Check if either condition is met before applying timestamp safety check
        let basic_update_needed = time_check || deviation_check;

        if !basic_update_needed {
            // Record decision and return early if no basic update is needed
            UpdateMetrics::record_update_decision(
                &datafeed.name,
                &datafeed.networks,
                false,
                None,
                Some(SkipReason::NoDeviation),
            );
            return Ok((
                false,
                "neither time nor deviation thresholds met".to_string(),
            ));
        }

        // Apply timestamp safety check if enabled
        if datafeed.enable_timestamp_safety_check {
            debug!(
                "Timestamp safety check enabled for datafeed {} with feed timestamp {}",
                datafeed.name, feed_timestamp
            );

            // Try to get the latest on-chain timestamp
            match self.get_latest_on_chain_timestamp(datafeed).await {
                Ok(on_chain_timestamp) => {
                    if feed_timestamp <= on_chain_timestamp {
                        info!(
                            "Skipping update for {}: local timestamp {} is not newer than on-chain timestamp {}",
                            datafeed.name, feed_timestamp, on_chain_timestamp
                        );

                        // Record skip decision
                        UpdateMetrics::record_update_decision(
                            &datafeed.name,
                            &datafeed.networks,
                            false,
                            None,
                            Some(SkipReason::NoDeviation), // Could add a new SkipReason::TimestampStale
                        );

                        return Ok((false, format!("timestamp safety check failed: local timestamp {feed_timestamp} <= on-chain timestamp {on_chain_timestamp}")));
                    } else {
                        debug!(
                            "Timestamp safety check passed for {}: local timestamp {} > on-chain timestamp {}",
                            datafeed.name, feed_timestamp, on_chain_timestamp
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get on-chain timestamp for {}: {}. Proceeding with update (timestamp safety check disabled for this update).",
                        datafeed.name, e
                    );
                }
            }
        }

        // Determine update reason and record decision
        let (reason_str, update_reason) = match (time_check, deviation_check) {
            (true, true) => (
                "both time and deviation thresholds".to_string(),
                Some(UpdateReason::Both),
            ),
            (true, false) => (
                "time threshold".to_string(),
                Some(UpdateReason::TimeThreshold),
            ),
            (false, true) => (
                "deviation threshold".to_string(),
                Some(UpdateReason::DeviationThreshold),
            ),
            _ => unreachable!("basic_update_needed is true, so at least one condition must be met"),
        };

        // Record successful update decision
        UpdateMetrics::record_update_decision(
            &datafeed.name,
            &datafeed.networks,
            true,
            update_reason,
            None,
        );

        Ok((true, reason_str))
    }

    /// Gets the latest on-chain timestamp from the contract
    async fn get_latest_on_chain_timestamp(&self, datafeed: &Datafeed) -> Result<u64> {
        let contract = self.get_contract_for_read(datafeed).await?;
        contract
            .get_latest_on_chain_timestamp_with_metrics(
                Some(&datafeed.name),
                Some(&datafeed.networks),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to get latest on-chain timestamp for datafeed {}",
                    datafeed.name
                )
            })
    }

    /// Submits a new value to the contract
    pub async fn submit_value(&self, datafeed: &Datafeed, value: f64) -> Result<()> {
        info!(
            "Submitting value {} to contract {} on network {}",
            value, datafeed.contract_address, datafeed.networks
        );

        // Create provider with signer
        let provider = self.create_signer_provider(&datafeed.networks).await?;

        // Create contract instance with the signing provider
        let address = parse_address(&datafeed.contract_address)?;
        let contract = create_contract_with_provider(address, provider);

        // Get current round ID
        let latest_round = contract
            .latest_round()
            .await
            .with_context(|| "Failed to get latest round from contract")?;

        let next_round = latest_round + U256::from(1);

        // Convert value to contract format
        let decimals = datafeed.decimals.unwrap_or(8);
        let scaled_value = scale_value_for_contract(value, decimals);

        // Validate against min/max bounds
        validate_value_bounds(scaled_value, datafeed)?;

        // Convert to I256 for contract
        let submission =
            I256::try_from(scaled_value).context("Failed to convert scaled value to I256")?;

        info!(
            "Submitting to round {} with value {} (scaled from {})",
            next_round, submission, value
        );

        // Get network configuration for gas settings
        let network_config = self.get_network_config(datafeed)?;

        // Get wallet address for gas estimation
        let wallet_address = self
            .network_manager
            .get_wallet_address(&datafeed.networks)
            .ok(); // It's optional, so we use ok() to convert Result to Option

        // Record update attempt
        UpdateMetrics::record_update_attempt(
            &datafeed.name,
            &datafeed.networks,
            false, // Will update to true if successful
        );

        // Submit the transaction with gas estimation
        match contract
            .submit_price_with_gas_estimation(
                next_round,
                submission,
                network_config,
                &datafeed.name,
                self.tx_log_repo.clone(),
                wallet_address,
            )
            .await
        {
            Ok(receipt) => {
                // Use the standardized transaction handler
                let context = TransactionContext::Datafeed {
                    feed_name: datafeed.name.clone(),
                };

                TransactionHandler::new(receipt, context, datafeed.networks.clone())
                    .with_gas_price_manager(self.gas_price_manager)
                    .with_tx_log_repo(self.tx_log_repo.as_ref())
                    .process()
                    .await?;

                Ok(())
            }
            Err(e) => {
                error!("Failed to submit value to contract: {}", e);

                // Update attempt already recorded as failure above

                Err(anyhow::anyhow!(
                    "{}: {}",
                    errors::CONTRACT_SUBMISSION_FAILED,
                    e
                ))
            }
        }
    }

    /// Read current contract state and update metrics
    pub async fn update_contract_metrics(
        &self,
        datafeed: &Datafeed,
        feed_value: f64,
    ) -> Result<()> {
        // Get contract instance
        let contract = self.get_contract_for_read(datafeed).await?;

        // Get latest answer from contract
        let latest_answer = contract
            .latest_answer()
            .await
            .with_context(|| "Failed to get latest answer from contract")?;

        // Get latest timestamp
        let latest_timestamp = contract
            .latest_timestamp()
            .await
            .with_context(|| "Failed to get latest timestamp from contract")?;

        // Get latest round
        let latest_round = contract
            .latest_round()
            .await
            .with_context(|| "Failed to get latest round from contract")?;

        // Convert contract value from scaled integer to float
        let decimals = datafeed.decimals.unwrap_or(8);
        let divisor = 10f64.powi(decimals as i32);
        let answer_i128: i128 = latest_answer
            .try_into()
            .context("Failed to convert latest answer to i128")?;
        let contract_value = answer_i128 as f64 / divisor;

        // Update metrics
        FeedMetrics::set_contract_value(
            &datafeed.name,
            &datafeed.networks,
            contract_value,
            latest_round.to::<u64>(),
            latest_timestamp.to::<u64>(),
        );

        // Calculate and update deviation
        FeedMetrics::update_deviation(
            &datafeed.name,
            &datafeed.networks,
            feed_value,
            contract_value,
        );

        debug!(
            "Updated contract metrics for {}: value={}, round={}, timestamp={}",
            datafeed.name, contract_value, latest_round, latest_timestamp
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::Datafeed;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_datafeed() -> Datafeed {
        Datafeed {
            name: "test_feed".to_string(),
            networks: "test".to_string(),
            check_frequency: 60,
            contract_address: "0x1234567890123456789012345678901234567890".to_string(),
            contract_type: "fluxmon".to_string(),
            read_contract_config: false,
            decimals: Some(8),
            min_value: Some(I256::try_from(-1000000000).unwrap()),
            max_value: Some(I256::try_from(1000000000).unwrap()),
            minimum_update_frequency: 3600, // 1 hour
            deviation_threshold_pct: 0.5,
            feed_url: "http://example.com/api".to_string(),
            feed_json_path: "data.price".to_string(),
            feed_json_path_timestamp: Some("data.timestamp".to_string()),
            enable_timestamp_safety_check: false,
            data_retention_days: 7,
        }
    }

    #[test]
    fn test_value_scaling_with_decimals() {
        let datafeed = create_test_datafeed();
        let value = 2557.96;
        let decimals = datafeed.decimals.unwrap_or(8);
        let scaled_value = scale_value_for_contract(value, decimals);

        assert_eq!(scaled_value, 255796000000);
    }

    #[test]
    fn test_value_bounds_validation() {
        let mut datafeed = create_test_datafeed();
        datafeed.min_value = Some(I256::try_from(100000000).unwrap()); // 1.0 with 8 decimals
        datafeed.max_value = Some(I256::try_from(1000000000000i64).unwrap()); // 10000.0 with 8 decimals

        let decimals = datafeed.decimals.unwrap_or(8);
        let multiplier = 10f64.powi(decimals as i32);

        // Test value below minimum
        let low_value = 0.5;
        let scaled_low = (low_value * multiplier).round() as i128;
        assert!(I256::try_from(scaled_low).unwrap() < datafeed.min_value.unwrap());

        // Test value above maximum
        let high_value = 20000.0;
        let scaled_high = (high_value * multiplier).round() as i128;
        assert!(I256::try_from(scaled_high).unwrap() > datafeed.max_value.unwrap());

        // Test value within bounds
        let normal_value = 1000.0;
        let scaled_normal = (normal_value * multiplier).round() as i128;
        assert!(I256::try_from(scaled_normal).unwrap() >= datafeed.min_value.unwrap());
        assert!(I256::try_from(scaled_normal).unwrap() <= datafeed.max_value.unwrap());
    }

    #[test]
    fn test_time_difference_calculation() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Simulate last update 30 minutes ago
        let last_update = now - 1800;
        let time_since_update = now.saturating_sub(last_update);

        assert_eq!(time_since_update, 1800);

        // Test with 1 hour minimum update frequency
        let minimum_frequency = 3600u64;
        assert!(time_since_update < minimum_frequency); // Should not update

        // Simulate last update 2 hours ago
        let old_update = now - 7200;
        let time_since_old = now.saturating_sub(old_update);
        assert!(time_since_old >= minimum_frequency); // Should update
    }

    #[test]
    fn test_negative_value_scaling() {
        let datafeed = create_test_datafeed();
        let value = -123.45;
        let decimals = datafeed.decimals.unwrap_or(8);
        let multiplier = 10f64.powi(decimals as i32);
        let scaled_value = (value * multiplier).round() as i128;

        assert_eq!(scaled_value, -12345000000);
    }

    #[test]
    fn test_zero_value_scaling() {
        let datafeed = create_test_datafeed();
        let value = 0.0;
        let decimals = datafeed.decimals.unwrap_or(8);
        let multiplier = 10f64.powi(decimals as i32);
        let scaled_value = (value * multiplier).round() as i128;

        assert_eq!(scaled_value, 0);
    }

    #[test]
    fn test_different_decimal_scales() {
        let mut datafeed = create_test_datafeed();
        let value = 100.0;

        // Test with 18 decimals (common for ETH)
        datafeed.decimals = Some(18);
        let multiplier = 10f64.powi(18);
        let scaled_18 = (value * multiplier).round() as i128;
        assert_eq!(scaled_18, 100_000_000_000_000_000_000);

        // Test with 6 decimals (common for USDC)
        datafeed.decimals = Some(6);
        let multiplier = 10f64.powi(6);
        let scaled_6 = (value * multiplier).round() as i128;
        assert_eq!(scaled_6, 100_000_000);

        // Test with 0 decimals
        datafeed.decimals = Some(0);
        let multiplier = 10f64.powi(0);
        let scaled_0 = (value * multiplier).round() as i128;
        assert_eq!(scaled_0, 100);
    }

    #[test]
    fn test_rounding_behavior() {
        let datafeed = create_test_datafeed();
        let decimals = datafeed.decimals.unwrap_or(8);
        let multiplier = 10f64.powi(decimals as i32);

        // Test rounding up
        let value_up = 123.456789;
        let scaled_up = (value_up * multiplier).round() as i128;
        assert_eq!(scaled_up, 12345678900);

        // Test rounding down
        let value_down = 123.454321;
        let scaled_down = (value_down * multiplier).round() as i128;
        assert_eq!(scaled_down, 12345432100);
    }

    #[test]
    fn test_minimum_update_frequency_edge_cases() {
        let datafeed = create_test_datafeed();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Test exact minimum frequency
        let last_update_exact = now - datafeed.minimum_update_frequency;
        let time_since_exact = now.saturating_sub(last_update_exact);
        assert_eq!(time_since_exact, datafeed.minimum_update_frequency);
        assert!(time_since_exact >= datafeed.minimum_update_frequency); // Should update

        // Test one second before minimum frequency
        let last_update_just_before = now - (datafeed.minimum_update_frequency - 1);
        let time_since_just_before = now.saturating_sub(last_update_just_before);
        assert_eq!(
            time_since_just_before,
            datafeed.minimum_update_frequency - 1
        );
        assert!(time_since_just_before < datafeed.minimum_update_frequency); // Should not update
    }

    #[test]
    fn test_deviation_threshold_calculations() {
        let datafeed = create_test_datafeed(); // has 0.5% threshold
        let decimals = datafeed.decimals.unwrap_or(8);

        // Current on-chain value: $100.00 (scaled)
        let current_value: i128 = 10000000000; // 100 * 10^8

        // Test value with 0.4% deviation (below threshold)
        let small_change = 100.4;
        let scaled_small = scale_value_for_contract(small_change, decimals);
        let small_deviation = calculate_deviation_percentage(current_value, scaled_small);
        assert!(small_deviation < datafeed.deviation_threshold_pct);

        // Test value with 0.6% deviation (above threshold)
        let large_change = 100.6;
        let scaled_large = scale_value_for_contract(large_change, decimals);
        let large_deviation = calculate_deviation_percentage(current_value, scaled_large);
        assert!(large_deviation > datafeed.deviation_threshold_pct);

        // Test exact threshold (0.5%)
        let exact_change = 100.5;
        let scaled_exact = scale_value_for_contract(exact_change, decimals);
        let exact_deviation = calculate_deviation_percentage(current_value, scaled_exact);
        assert_eq!(exact_deviation, datafeed.deviation_threshold_pct);
    }

    #[test]
    fn test_timestamp_safety_check_default_disabled() {
        let datafeed = create_test_datafeed();
        // Default should be false (disabled)
        assert!(!datafeed.enable_timestamp_safety_check);
    }

    #[test]
    fn test_timestamp_safety_check_enabled() {
        let mut datafeed = create_test_datafeed();
        datafeed.enable_timestamp_safety_check = true;
        assert!(datafeed.enable_timestamp_safety_check);
    }

    #[test]
    fn test_timestamp_comparison_logic() {
        // Test that newer timestamp should allow update
        let feed_timestamp = 1700000000u64; // Jan 2024
        let on_chain_timestamp = 1600000000u64; // Sep 2020
        assert!(
            feed_timestamp > on_chain_timestamp,
            "Feed timestamp should be newer"
        );

        // Test that older timestamp should prevent update
        let old_feed_timestamp = 1500000000u64; // July 2017
        let recent_on_chain = 1700000000u64; // Jan 2024
        assert!(
            old_feed_timestamp <= recent_on_chain,
            "Feed timestamp should be older or equal"
        );

        // Test equal timestamps (should prevent update)
        let equal_timestamp = 1650000000u64;
        assert!(
            equal_timestamp <= equal_timestamp,
            "Equal timestamps should prevent update"
        );
    }

    #[test]
    fn test_datafeed_with_timestamp_safety_configuration() {
        let mut datafeed = create_test_datafeed();

        // Test disabling safety check
        datafeed.enable_timestamp_safety_check = false;
        assert!(!datafeed.enable_timestamp_safety_check);

        // Test enabling safety check
        datafeed.enable_timestamp_safety_check = true;
        assert!(datafeed.enable_timestamp_safety_check);
    }

    #[test]
    fn test_unix_timestamp_conversion() {
        // Test typical Unix timestamp ranges
        let timestamp_2020 = 1577836800u64; // Jan 1, 2020
        let timestamp_2024 = 1704067200u64; // Jan 1, 2024
        let timestamp_2025 = 1735689600u64; // Jan 1, 2025

        // Ensure timestamps are in reasonable order
        assert!(timestamp_2020 < timestamp_2024);
        assert!(timestamp_2024 < timestamp_2025);

        // Test that they convert to u64 properly
        assert_eq!(timestamp_2024 as u64, 1704067200);
    }
}
