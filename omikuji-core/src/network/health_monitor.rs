//! Network health monitoring module
//!
//! This module provides continuous monitoring of network health metrics including
//! block heights, gas prices, sync status, and endpoint health.

use crate::metrics::NetworkMetrics;
use crate::network::{EthProvider, NetworkManager};
use alloy::providers::Provider;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for the network health monitor
#[derive(Debug, Clone)]
pub struct HealthMonitorConfig {
    /// Polling interval in seconds
    pub polling_interval_secs: u64,
    /// Number of blocks to keep for averaging
    pub block_history_size: usize,
    /// Sync threshold in seconds (if block is older than this, network is not synced)
    pub sync_threshold_secs: u64,
}

impl Default for HealthMonitorConfig {
    fn default() -> Self {
        Self {
            polling_interval_secs: 30,
            block_history_size: 10,
            sync_threshold_secs: 120, // 2 minutes
        }
    }
}

/// Block information for tracking
#[derive(Debug, Clone)]
struct BlockInfo {
    number: u64,
    timestamp: u64,
}

/// Network state tracking
struct NetworkState {
    /// Last N blocks for calculating averages
    block_history: Vec<BlockInfo>,
    /// Last recorded block number
    last_block_number: Option<u64>,
}

impl NetworkState {
    fn new() -> Self {
        Self {
            block_history: Vec::new(),
            last_block_number: None,
        }
    }

    /// Add a new block and maintain history size
    fn add_block(&mut self, block: BlockInfo, max_size: usize) {
        self.block_history.push(block);
        if self.block_history.len() > max_size {
            self.block_history.remove(0);
        }
    }

    /// Calculate average block time in seconds
    fn calculate_average_block_time(&self) -> Option<f64> {
        if self.block_history.len() < 2 {
            return None;
        }

        let mut total_time = 0u64;
        let mut count = 0;

        for i in 1..self.block_history.len() {
            let time_diff = self.block_history[i]
                .timestamp
                .saturating_sub(self.block_history[i - 1].timestamp);
            let block_diff = self.block_history[i]
                .number
                .saturating_sub(self.block_history[i - 1].number);

            if block_diff > 0 {
                total_time += time_diff / block_diff;
                count += 1;
            }
        }

        if count > 0 {
            Some(total_time as f64 / count as f64)
        } else {
            None
        }
    }
}

/// Network health monitor that continuously updates network metrics
pub struct NetworkHealthMonitor {
    network_manager: Arc<NetworkManager>,
    config: HealthMonitorConfig,
    network_states: HashMap<String, NetworkState>,
}

impl NetworkHealthMonitor {
    /// Create a new network health monitor
    pub fn new(network_manager: Arc<NetworkManager>, config: HealthMonitorConfig) -> Self {
        Self {
            network_manager,
            config,
            network_states: HashMap::new(),
        }
    }

    /// Start monitoring network health
    pub async fn start(mut self) {
        info!(
            "Starting network health monitor with {}s interval",
            self.config.polling_interval_secs
        );

        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));

        loop {
            interval.tick().await;
            self.monitor_all_networks().await;
        }
    }

    /// Monitor all configured networks
    async fn monitor_all_networks(&mut self) {
        let networks = self.network_manager.get_network_names();

        for network_name in networks {
            if let Err(e) = self.monitor_network(&network_name).await {
                error!("Failed to monitor network {}: {}", network_name, e);
                NetworkMetrics::update_endpoint_health(&network_name, "primary", false);
            }
        }
    }

    /// Monitor a single network
    async fn monitor_network(&mut self, network_name: &str) -> anyhow::Result<()> {
        debug!("Monitoring network: {}", network_name);

        // Get provider for this network
        let provider = self.network_manager.get_provider(network_name)?;

        // Update chain head and detect reorgs
        let block_number = self.update_chain_head(network_name, &provider).await?;

        // Get block details for more metrics
        if let Ok(Some(block)) = provider.get_block_by_number(block_number.into()).await {
            // Update block timestamp
            let timestamp = block.header.timestamp;
            NetworkMetrics::update_last_block_timestamp(network_name, timestamp);

            // Check sync status
            let is_synced = self.check_sync_status(timestamp);
            NetworkMetrics::update_sync_status(network_name, is_synced);

            // Update block history and calculate average block time
            let state = self
                .network_states
                .entry(network_name.to_string())
                .or_insert_with(NetworkState::new);

            state.add_block(
                BlockInfo {
                    number: block_number,
                    timestamp,
                },
                self.config.block_history_size,
            );

            if let Some(avg_block_time) = state.calculate_average_block_time() {
                NetworkMetrics::update_block_time(network_name, avg_block_time);
            }

            // Update gas metrics if available
            if let Some(base_fee) = block.header.base_fee_per_gas {
                let base_fee_gwei = base_fee as f64 / 1_000_000_000.0;
                NetworkMetrics::update_base_fee(network_name, base_fee_gwei);

                // Calculate congestion level based on base fee
                // This is a simple heuristic: higher base fee = more congestion
                let congestion = self.calculate_congestion_level(base_fee_gwei);
                NetworkMetrics::update_congestion_level(network_name, congestion);
            }
        }

        // Update gas price and priority fee percentiles
        self.update_gas_prices(network_name, &provider).await?;

        // Update endpoint health
        NetworkMetrics::update_endpoint_health(network_name, "primary", true);

        Ok(())
    }

    /// Update chain head and detect reorganizations
    async fn update_chain_head(
        &mut self,
        network_name: &str,
        provider: &Arc<EthProvider>,
    ) -> anyhow::Result<u64> {
        let block_number = provider.get_block_number().await?;

        // Check for reorg
        let state = self
            .network_states
            .entry(network_name.to_string())
            .or_insert_with(NetworkState::new);

        if let Some(last_block) = state.last_block_number {
            if block_number < last_block {
                let depth = last_block - block_number;
                warn!(
                    "Chain reorganization detected on {}: {} -> {} (depth: {})",
                    network_name, last_block, block_number, depth
                );
            }
        }

        state.last_block_number = Some(block_number);

        // Update metric
        NetworkMetrics::update_chain_head(network_name, block_number);

        Ok(block_number)
    }

    /// Check if network is synced based on last block timestamp
    fn check_sync_status(&self, block_timestamp: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let age = now.saturating_sub(block_timestamp);
        age < self.config.sync_threshold_secs
    }

    /// Update gas prices and percentiles
    async fn update_gas_prices(
        &self,
        network_name: &str,
        provider: &Arc<EthProvider>,
    ) -> anyhow::Result<()> {
        // Get current gas price
        if let Ok(gas_price) = provider.get_gas_price().await {
            let gas_price_gwei = gas_price as f64 / 1_000_000_000.0;
            NetworkMetrics::update_gas_price(network_name, "current", gas_price_gwei);
        }

        // Try to get fee history for percentiles (if supported)
        // Note: This is a simplified version. In production, you'd want to use
        // the fee_history RPC call to get actual percentiles
        if let Ok(gas_price) = provider.get_gas_price().await {
            let base_gwei = gas_price as f64 / 1_000_000_000.0;

            // Simulate percentiles (in production, use actual fee_history data)
            NetworkMetrics::update_priority_fee_percentile(network_name, "p25", base_gwei * 0.8);
            NetworkMetrics::update_priority_fee_percentile(network_name, "p50", base_gwei);
            NetworkMetrics::update_priority_fee_percentile(network_name, "p75", base_gwei * 1.2);
            NetworkMetrics::update_priority_fee_percentile(network_name, "p90", base_gwei * 1.5);
        }

        Ok(())
    }

    /// Calculate congestion level based on base fee
    fn calculate_congestion_level(&self, base_fee_gwei: f64) -> f64 {
        // Simple heuristic: map base fee to congestion level
        // These thresholds can be adjusted based on network characteristics
        match base_fee_gwei {
            fee if fee < 10.0 => fee * 2.0, // 0-20% for fees under 10 gwei
            fee if fee < 30.0 => 20.0 + (fee - 10.0), // 20-40% for 10-30 gwei
            fee if fee < 50.0 => 40.0 + (fee - 30.0) * 0.5, // 40-50% for 30-50 gwei
            fee if fee < 100.0 => 50.0 + (fee - 50.0) * 0.4, // 50-70% for 50-100 gwei
            fee if fee < 200.0 => 70.0 + (fee - 100.0) * 0.2, // 70-90% for 100-200 gwei
            _ => 90.0 + (base_fee_gwei - 200.0).min(10.0) * 0.1, // 90-100% for 200+ gwei
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_state_block_time_calculation() {
        let mut state = NetworkState::new();

        // Add blocks with known timestamps
        state.add_block(
            BlockInfo {
                number: 100,
                timestamp: 1000,
            },
            10,
        );
        state.add_block(
            BlockInfo {
                number: 101,
                timestamp: 1012,
            },
            10,
        );
        state.add_block(
            BlockInfo {
                number: 102,
                timestamp: 1024,
            },
            10,
        );

        // Average block time should be 12 seconds
        let avg = state.calculate_average_block_time();
        assert!(avg.is_some());
        assert_eq!(avg.unwrap(), 12.0);
    }

    #[tokio::test]
    async fn test_congestion_level_calculation() {
        let config = HealthMonitorConfig::default();
        let network_manager = Arc::new(NetworkManager::new(&[]).await.unwrap());
        let monitor = NetworkHealthMonitor::new(network_manager, config);

        // Test various base fees
        assert!(monitor.calculate_congestion_level(5.0) < 20.0);
        assert!(monitor.calculate_congestion_level(25.0) > 20.0);
        assert!(monitor.calculate_congestion_level(75.0) > 50.0);
        assert!(monitor.calculate_congestion_level(150.0) > 70.0);
        assert!(monitor.calculate_congestion_level(300.0) > 90.0);
        assert!(monitor.calculate_congestion_level(1000.0) <= 100.0);
    }

    #[tokio::test]
    async fn test_sync_status_check() {
        let config = HealthMonitorConfig::default();
        let network_manager = Arc::new(NetworkManager::new(&[]).await.unwrap());
        let monitor = NetworkHealthMonitor::new(network_manager, config);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Recent block should be synced
        assert!(monitor.check_sync_status(now - 30));

        // Old block should not be synced
        assert!(!monitor.check_sync_status(now - 300));
    }

    #[test]
    fn test_block_history_maintenance() {
        let mut state = NetworkState::new();
        let max_size = 3;

        // Add more blocks than max size
        for i in 0..5 {
            state.add_block(
                BlockInfo {
                    number: i,
                    timestamp: i * 10,
                },
                max_size,
            );
        }

        // Should only keep last 3 blocks
        assert_eq!(state.block_history.len(), 3);
        assert_eq!(state.block_history[0].number, 2);
        assert_eq!(state.block_history[2].number, 4);
    }
}
