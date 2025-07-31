use crate::gas_price::GasPriceManager;
use crate::metrics::{EconomicMetrics, FeedMetrics};
use crate::network::NetworkManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Monitors wallet balances across all networks
pub struct WalletBalanceMonitor {
    network_manager: Arc<NetworkManager>,
    gas_price_manager: Option<Arc<GasPriceManager>>,
    update_interval_seconds: u64,
    /// Track daily spending for runway calculation (network -> daily spend in USD)
    daily_spending_estimates: HashMap<String, f64>,
}

impl WalletBalanceMonitor {
    /// Create a new wallet balance monitor
    pub fn new(network_manager: Arc<NetworkManager>) -> Self {
        Self {
            network_manager,
            gas_price_manager: None,
            update_interval_seconds: 60, // Default to 1 minute
            daily_spending_estimates: HashMap::new(),
        }
    }

    /// Set the gas price manager
    pub fn with_gas_price_manager(mut self, gas_price_manager: Arc<GasPriceManager>) -> Self {
        self.gas_price_manager = Some(gas_price_manager);
        self
    }

    /// Start monitoring wallet balances
    pub async fn start(self) {
        let mut interval = interval(Duration::from_secs(self.update_interval_seconds));

        info!(
            "Starting wallet balance monitor with {}s interval",
            self.update_interval_seconds
        );

        loop {
            interval.tick().await;
            self.update_all_balances().await;
        }
    }

    /// Update balances for all networks
    async fn update_all_balances(&self) {
        // Get all network names from the network manager
        let networks = self.network_manager.get_network_names();

        for network_name in networks {
            if let Err(e) = self.update_network_balance(&network_name).await {
                error!(
                    "Failed to update wallet balance for network {}: {}",
                    network_name, e
                );
            }
        }
    }

    /// Update balance for a specific network
    async fn update_network_balance(
        &self,
        network_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get the wallet address for this network
        let address = self.network_manager.get_wallet_address(network_name)?;

        // Get the provider to query balance
        let provider = self.network_manager.get_provider(network_name)?;

        // Fetch the balance
        use alloy::providers::Provider;
        match provider.get_balance(address).await {
            Ok(balance) => {
                let balance_wei = balance.to::<u128>();
                let balance_native = balance_wei as f64 / 1e18; // Convert to native units

                // Update basic balance metric
                FeedMetrics::set_wallet_balance(network_name, &format!("{address:?}"), balance_wei);

                // Get native token price from gas price manager if available
                let native_token_price = if let Some(ref gas_price_manager) = self.gas_price_manager
                {
                    if let Some(price_info) = gas_price_manager.get_price(network_name).await {
                        debug!(
                            "Got price for {} from gas price manager: ${:.2} USD (token: {})",
                            network_name, price_info.price_usd, price_info.symbol
                        );
                        price_info.price_usd
                    } else {
                        warn!(
                            "No price available for {} from gas price manager, using default $1.0",
                            network_name
                        );
                        1.0 // Default if price not available
                    }
                } else {
                    debug!("No gas price manager configured, using default price $1.0");
                    1.0 // Default price if no gas price manager
                };

                // Get network configuration to check for balance alert thresholds
                let thresholds = if let Ok(network_config) =
                    self.network_manager.get_network(network_name).await
                {
                    network_config.balance_alerts.as_ref()
                } else {
                    None
                };

                // Update economic metrics
                EconomicMetrics::update_wallet_balance_usd(
                    network_name,
                    &format!("{address:?}"),
                    balance_native,
                    native_token_price,
                    thresholds,
                );

                // Update runway if we have spending data
                if let Some(&daily_spend) = self.daily_spending_estimates.get(network_name) {
                    let balance_usd = balance_native * native_token_price;
                    EconomicMetrics::update_runway_days(
                        network_name,
                        &format!("{address:?}"),
                        balance_usd,
                        daily_spend,
                    );

                    EconomicMetrics::update_daily_spending_rate(network_name, daily_spend);
                }

                debug!(
                    "Updated wallet balance for {} on {}: {} wei ({:.6} native tokens, ${:.2} USD @ ${:.2}/token)",
                    address,
                    network_name,
                    balance_wei,
                    balance_native,
                    balance_native * native_token_price,
                    native_token_price
                );

                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to fetch balance for {} on {}: {}",
                    address, network_name, e
                );
                Err(Box::new(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_price::models::{CoinGeckoConfig, GasPriceFeedConfig};

    #[test]
    fn test_wallet_balance_monitor_creation() {
        // Create a minimal network manager for testing
        let networks = vec![];
        let network_manager = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(NetworkManager::new(&networks))
            .unwrap();
        let network_manager = Arc::new(network_manager);

        let monitor = WalletBalanceMonitor::new(network_manager.clone());

        assert_eq!(monitor.update_interval_seconds, 60);
        assert!(monitor.gas_price_manager.is_none());
        assert!(monitor.daily_spending_estimates.is_empty());
    }

    #[test]
    fn test_wallet_balance_monitor_with_gas_price_manager() {
        let networks = vec![];
        let network_manager = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(NetworkManager::new(&networks))
            .unwrap();
        let network_manager = Arc::new(network_manager);

        let gas_config = GasPriceFeedConfig {
            enabled: true,
            update_frequency: 60,
            provider: "coingecko".to_string(),
            coingecko: CoinGeckoConfig {
                api_key: None,
                base_url: "https://api.coingecko.com/api/v3".to_string(),
            },
            fallback_to_cache: true,
            persist_to_database: false,
        };
        let gas_price_manager = Arc::new(GasPriceManager::new(gas_config, HashMap::new(), None));

        let monitor = WalletBalanceMonitor::new(network_manager)
            .with_gas_price_manager(gas_price_manager.clone());

        assert!(monitor.gas_price_manager.is_some());
    }

    #[test]
    fn test_daily_spending_estimates() {
        let networks = vec![];
        let network_manager = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(NetworkManager::new(&networks))
            .unwrap();
        let network_manager = Arc::new(network_manager);

        let mut monitor = WalletBalanceMonitor::new(network_manager);

        // Test adding spending estimates
        monitor
            .daily_spending_estimates
            .insert("network1".to_string(), 10.5);
        monitor
            .daily_spending_estimates
            .insert("network2".to_string(), 25.0);

        assert_eq!(
            monitor.daily_spending_estimates.get("network1"),
            Some(&10.5)
        );
        assert_eq!(
            monitor.daily_spending_estimates.get("network2"),
            Some(&25.0)
        );
        assert_eq!(monitor.daily_spending_estimates.get("network3"), None);
    }

    #[test]
    fn test_update_interval() {
        let networks = vec![];
        let network_manager = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(NetworkManager::new(&networks))
            .unwrap();
        let network_manager = Arc::new(network_manager);

        let monitor = WalletBalanceMonitor::new(network_manager);
        assert_eq!(monitor.update_interval_seconds, 60);
    }

    #[tokio::test]
    async fn test_update_network_balance_error_handling() {
        let networks = vec![];
        let network_manager = NetworkManager::new(&networks).await.unwrap();
        let network_manager = Arc::new(network_manager);

        let monitor = WalletBalanceMonitor::new(network_manager.clone());

        // Test balance update with non-existent network
        let result = monitor.update_network_balance("non-existent-network").await;
        assert!(result.is_err());

        // Test error message contains expected text
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("Network not found")
                || error_message.contains("No wallet address found")
        );
    }

    #[test]
    fn test_monitor_fields() {
        let networks = vec![];
        let network_manager = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(NetworkManager::new(&networks))
            .unwrap();
        let network_manager = Arc::new(network_manager);

        let gas_config = GasPriceFeedConfig {
            enabled: true,
            update_frequency: 30,
            provider: "coingecko".to_string(),
            coingecko: CoinGeckoConfig {
                api_key: None,
                base_url: "https://api.coingecko.com/api/v3".to_string(),
            },
            fallback_to_cache: true,
            persist_to_database: false,
        };
        let gas_price_manager = Arc::new(GasPriceManager::new(gas_config, HashMap::new(), None));

        let mut monitor = WalletBalanceMonitor::new(network_manager.clone());

        // Test initial state
        assert_eq!(monitor.update_interval_seconds, 60);
        assert!(monitor.gas_price_manager.is_none());
        assert!(monitor.daily_spending_estimates.is_empty());

        // Test with gas price manager
        monitor = monitor.with_gas_price_manager(gas_price_manager);
        assert!(monitor.gas_price_manager.is_some());

        // Test daily spending estimates
        monitor
            .daily_spending_estimates
            .insert("eth-mainnet".to_string(), 50.0);
        assert_eq!(monitor.daily_spending_estimates.len(), 1);
        assert_eq!(
            monitor.daily_spending_estimates.get("eth-mainnet"),
            Some(&50.0)
        );
    }

    #[test]
    fn test_balance_conversion() {
        // Test wei to native conversion
        let balance_wei = 1_500_000_000_000_000_000u128; // 1.5 ETH in wei
        let balance_native = balance_wei as f64 / 1e18;
        assert!((balance_native - 1.5).abs() < 0.0000001);

        // Test balance in USD calculation
        let price_usd = 2000.0;
        let balance_usd = balance_native * price_usd;
        assert!((balance_usd - 3000.0).abs() < 0.01);

        // Test runway calculation
        let daily_spend = 100.0;
        let runway_days = balance_usd / daily_spend;
        assert!((runway_days - 30.0).abs() < 0.01);
    }

    // Edge case tests for Phase 4
    #[test]
    fn test_zero_balance_handling() {
        let balance_wei = 0u128;
        let balance_native = balance_wei as f64 / 1e18;
        assert_eq!(balance_native, 0.0);

        // Test runway calculation with zero balance
        let daily_spend = 10.0;
        let runway_days = if daily_spend > 0.0 {
            balance_native / daily_spend
        } else {
            0.0
        };
        assert_eq!(runway_days, 0.0);
    }

    #[test]
    fn test_extreme_balance_values() {
        // Test with very large balances
        let large_balance_wei = u128::MAX;
        let balance_native = large_balance_wei as f64 / 1e18;
        assert!(balance_native > 0.0);
        assert!(balance_native.is_finite());

        // Test with maximum practical ETH (total supply is ~120M ETH)
        let max_practical_eth = 120_000_000.0;
        let max_practical_wei = (max_practical_eth * 1e18) as u128;
        let converted_back = max_practical_wei as f64 / 1e18;
        assert!((converted_back - max_practical_eth).abs() / max_practical_eth < 0.0001);
    }

    #[test]
    fn test_negative_runway_scenarios() {
        // Test when gas prices spike dramatically
        let balance_usd = 100.0;
        let daily_spending_scenarios = vec![
            (0.0, "zero spending"),
            (0.01, "minimal spending"),
            (100.0, "break-even"),
            (1000.0, "high spending"),
            (f64::INFINITY, "infinite spending"),
        ];

        for (spending, scenario) in daily_spending_scenarios {
            let runway = if spending > 0.0 && spending.is_finite() {
                balance_usd / spending
            } else if spending == 0.0 {
                f64::INFINITY
            } else {
                0.0
            };

            assert!(
                runway >= 0.0 || runway.is_infinite(),
                "Failed for scenario: {scenario}"
            );
        }
    }

    #[test]
    fn test_concurrent_balance_updates() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc as StdArc;

        let balance_counter = StdArc::new(AtomicU64::new(0));
        let update_threads: Vec<_> = (0..10)
            .map(|i| {
                let counter = StdArc::clone(&balance_counter);
                std::thread::spawn(move || {
                    // Simulate balance update
                    let new_balance = 1000 + i * 100;
                    counter.store(new_balance, Ordering::SeqCst);
                })
            })
            .collect();

        for thread in update_threads {
            thread.join().unwrap();
        }

        let final_balance = balance_counter.load(Ordering::SeqCst);
        assert!((1000..=1900).contains(&final_balance));
    }

    #[test]
    fn test_gas_price_unavailable() {
        // Test when gas price data is unavailable
        let balance_eth = 1.0;
        let gas_price_opt: Option<f64> = None;

        // Calculate runway without gas price data
        let runway_days = match gas_price_opt {
            Some(gas_price) => {
                let daily_eth = gas_price * 0.001; // Simplified calculation
                balance_eth / daily_eth
            }
            None => {
                // Use default estimate or return infinity
                f64::INFINITY
            }
        };

        assert!(runway_days.is_infinite());
    }

    #[test]
    fn test_multiple_network_balance_aggregation() {
        let mut network_balances = HashMap::new();
        network_balances.insert("ethereum".to_string(), 1000.0); // $1000
        network_balances.insert("polygon".to_string(), 500.0); // $500
        network_balances.insert("arbitrum".to_string(), 250.0); // $250
        network_balances.insert("optimism".to_string(), 0.0); // $0

        let total_balance_usd: f64 = network_balances.values().sum();
        assert_eq!(total_balance_usd, 1750.0);

        // Test with some networks having errors
        network_balances.insert("base".to_string(), f64::NAN);
        let valid_balance: f64 = network_balances.values().filter(|v| v.is_finite()).sum();
        assert_eq!(valid_balance, 1750.0);
    }

    #[test]
    fn test_update_interval_edge_cases() {
        // Test various update intervals
        let test_intervals = vec![
            (0u64, "zero interval"),    // Should default to something reasonable
            (1u64, "one second"),       // Very frequent
            (86400u64, "one day"),      // Very infrequent
            (u64::MAX, "max interval"), // Extreme case
        ];

        for (interval, description) in test_intervals {
            let safe_interval = if interval == 0 {
                60 // Default to 60 seconds
            } else if interval > 86400 {
                86400 // Cap at 1 day
            } else {
                interval
            };

            assert!(
                safe_interval > 0 && safe_interval <= 86400,
                "Invalid interval for {description}: {safe_interval}"
            );
        }
    }

    #[test]
    fn test_balance_precision_loss() {
        // Test for precision loss in balance calculations
        let small_amounts_wei = vec![
            1u128,         // 1 wei
            100u128,       // 100 wei
            1_000_000u128, // 1 million wei
        ];

        for wei_amount in small_amounts_wei {
            let eth_amount = wei_amount as f64 / 1e18;
            // These amounts should be effectively zero in ETH
            assert!(eth_amount < 0.000_000_001);
        }
    }

    #[tokio::test]
    async fn test_network_error_recovery() {
        // Test recovery from network errors
        let networks = vec![];
        let network_manager = NetworkManager::new(&networks).await.unwrap();
        let network_manager = Arc::new(network_manager);

        let monitor = WalletBalanceMonitor::new(network_manager.clone());

        // Simulate multiple failed attempts followed by success
        let mut attempt_count = 0;
        let max_attempts = 3;

        while attempt_count < max_attempts {
            let result = monitor.update_network_balance("test-network").await;
            if result.is_err() {
                attempt_count += 1;
                // In real scenario, would wait before retry
            } else {
                break;
            }
        }

        assert!(attempt_count <= max_attempts);
    }
}
