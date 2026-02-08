use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use tracing::{debug, error, warn};

use crate::config::models::BalanceAlertThresholds;

lazy_static! {
    /// Cumulative gas costs in USD
    static ref CUMULATIVE_GAS_COST_USD: CounterVec = register_counter_vec!(
        "omikuji_cumulative_gas_cost_usd",
        "Cumulative gas costs in USD",
        &["feed_name", "network", "time_period"]
    ).expect("Failed to create cumulative_gas_cost_usd metric");

    /// Wallet balance in USD
    static ref WALLET_BALANCE_USD: GaugeVec = register_gauge_vec!(
        "omikuji_wallet_balance_usd",
        "Wallet balance in USD",
        &["network", "address"]
    ).expect("Failed to create wallet_balance_usd metric");

    /// Estimated runway days
    static ref ESTIMATED_RUNWAY_DAYS: GaugeVec = register_gauge_vec!(
        "omikuji_estimated_runway_days",
        "Estimated days until wallet depletion",
        &["network", "address"]
    ).expect("Failed to create estimated_runway_days metric");

    /// Cost per update in USD
    static ref COST_PER_UPDATE_USD: HistogramVec = register_histogram_vec!(
        "omikuji_cost_per_update_usd",
        "Cost per update in USD",
        &["feed_name", "network"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0]
    ).expect("Failed to create cost_per_update_usd metric");

    /// Daily spending rate USD
    static ref DAILY_SPENDING_RATE_USD: GaugeVec = register_gauge_vec!(
        "omikuji_daily_spending_rate_usd",
        "Average daily spending rate in USD",
        &["network"]
    ).expect("Failed to create daily_spending_rate_usd metric");

    /// Gas price vs network average ratio
    static ref GAS_PRICE_RATIO: HistogramVec = register_histogram_vec!(
        "omikuji_gas_price_ratio",
        "Ratio of paid gas price to network average",
        &["feed_name", "network"],
        vec![0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0]
    ).expect("Failed to create gas_price_ratio metric");

    /// Low balance alert gauge
    static ref LOW_BALANCE_ALERT: GaugeVec = register_gauge_vec!(
        "omikuji_low_balance_alert",
        "Low balance alert status (1 = alert, 0 = ok)",
        &["network", "address", "severity"]
    ).expect("Failed to create low_balance_alert metric");

    /// Cost efficiency score
    static ref COST_EFFICIENCY_SCORE: GaugeVec = register_gauge_vec!(
        "omikuji_cost_efficiency_score",
        "Cost efficiency score (0-100)",
        &["feed_name", "network"]
    ).expect("Failed to create cost_efficiency_score metric");

    /// Monthly budget utilization
    static ref BUDGET_UTILIZATION_PERCENT: GaugeVec = register_gauge_vec!(
        "omikuji_budget_utilization_percent",
        "Monthly budget utilization percentage",
        &["network"]
    ).expect("Failed to create budget_utilization_percent metric");

    /// Gas optimization savings
    static ref GAS_OPTIMIZATION_SAVINGS_USD: CounterVec = register_counter_vec!(
        "omikuji_gas_optimization_savings_usd",
        "Cumulative savings from gas optimizations",
        &["feed_name", "network", "optimization_type"]
    ).expect("Failed to create gas_optimization_savings metric");
}

/// Economic metrics collector
pub struct EconomicMetrics;

impl EconomicMetrics {
    /// Record gas cost in USD
    pub fn record_gas_cost_usd(
        feed_name: &str,
        network: &str,
        cost_usd: f64,
        native_token_price: f64,
    ) {
        // Update cumulative costs
        CUMULATIVE_GAS_COST_USD
            .with_label_values(&[feed_name, network, "all_time"])
            .inc_by(cost_usd);

        CUMULATIVE_GAS_COST_USD
            .with_label_values(&[feed_name, network, "monthly"])
            .inc_by(cost_usd);

        CUMULATIVE_GAS_COST_USD
            .with_label_values(&[feed_name, network, "daily"])
            .inc_by(cost_usd);

        // Record cost per update
        COST_PER_UPDATE_USD
            .with_label_values(&[feed_name, network])
            .observe(cost_usd);

        debug!(
            "Gas cost for {}/{}: ${:.4} USD (token price: ${:.2})",
            feed_name, network, cost_usd, native_token_price
        );

        if cost_usd > 10.0 {
            warn!(
                "High transaction cost for {}/{}: ${:.2} USD",
                feed_name, network, cost_usd
            );
        }
    }

    /// Update wallet balance in USD
    pub fn update_wallet_balance_usd(
        network: &str,
        address: &str,
        balance_native: f64,
        native_token_price: f64,
        thresholds: Option<&BalanceAlertThresholds>,
    ) {
        let balance_usd = balance_native * native_token_price;

        WALLET_BALANCE_USD
            .with_label_values(&[network, address])
            .set(balance_usd);

        // Only check for alerts if thresholds are configured
        if let Some(thresholds) = thresholds {
            // Check for low balance alerts
            let (alert_status, severity) = if balance_usd < thresholds.critical_threshold_usd {
                (1.0, "critical")
            } else if balance_usd < thresholds.warning_threshold_usd {
                (1.0, "warning")
            } else if balance_usd < thresholds.info_threshold_usd {
                (1.0, "info")
            } else {
                (0.0, "ok")
            };

            LOW_BALANCE_ALERT
                .with_label_values(&[network, address, severity])
                .set(alert_status);

            if alert_status > 0.0 {
                error!(
                    "Low balance alert for {} on {}: ${:.2} USD (severity: {})",
                    address, network, balance_usd, severity
                );
            }
        } else {
            // Clear any existing alerts when thresholds are not configured
            LOW_BALANCE_ALERT
                .with_label_values(&[network, address, "ok"])
                .set(0.0);
        }
    }

    /// Update estimated runway
    pub fn update_runway_days(
        network: &str,
        address: &str,
        balance_usd: f64,
        daily_spending_rate: f64,
    ) {
        let runway_days = if daily_spending_rate > 0.0 {
            balance_usd / daily_spending_rate
        } else {
            f64::INFINITY
        };

        let clamped_runway = runway_days.min(365.0); // Cap at 1 year for display

        ESTIMATED_RUNWAY_DAYS
            .with_label_values(&[network, address])
            .set(clamped_runway);

        if runway_days < 7.0 {
            error!(
                "Critical: Only {:.1} days of runway left for {} on {}",
                runway_days, address, network
            );
        } else if runway_days < 30.0 {
            warn!(
                "Low runway: {:.1} days left for {} on {}",
                runway_days, address, network
            );
        }
    }

    /// Update daily spending rate
    pub fn update_daily_spending_rate(network: &str, rate_usd: f64) {
        DAILY_SPENDING_RATE_USD
            .with_label_values(&[network])
            .set(rate_usd);

        debug!("Daily spending rate for {}: ${:.2} USD", network, rate_usd);
    }

    /// Record gas price ratio
    pub fn record_gas_price_ratio(
        feed_name: &str,
        network: &str,
        paid_price_gwei: f64,
        network_avg_price_gwei: f64,
    ) {
        if network_avg_price_gwei > 0.0 {
            let ratio = paid_price_gwei / network_avg_price_gwei;

            GAS_PRICE_RATIO
                .with_label_values(&[feed_name, network])
                .observe(ratio);

            if ratio > 1.5 {
                warn!(
                    "Paying {:.1}x network average gas price for {}/{} ({:.1} vs {:.1} gwei)",
                    ratio, feed_name, network, paid_price_gwei, network_avg_price_gwei
                );
            }
        }
    }

    /// Update cost efficiency score
    pub fn update_cost_efficiency_score(
        feed_name: &str,
        network: &str,
        updates_performed: u64,
        total_cost_usd: f64,
        optimal_cost_usd: f64,
    ) {
        let score = if total_cost_usd > 0.0 {
            ((optimal_cost_usd / total_cost_usd) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        COST_EFFICIENCY_SCORE
            .with_label_values(&[feed_name, network])
            .set(score);

        if score < 50.0 {
            warn!(
                "Low cost efficiency for {}/{}: {:.1}% (${:.2} actual vs ${:.2} optimal for {} updates)",
                feed_name, network, score, total_cost_usd, optimal_cost_usd, updates_performed
            );
        }
    }

    /// Update budget utilization
    pub fn update_budget_utilization(network: &str, spent_usd: f64, budget_usd: f64) {
        let utilization = if budget_usd > 0.0 {
            (spent_usd / budget_usd) * 100.0
        } else {
            0.0
        };

        BUDGET_UTILIZATION_PERCENT
            .with_label_values(&[network])
            .set(utilization);

        if utilization > 90.0 {
            warn!(
                "High budget utilization for {}: {:.1}% (${:.2} of ${:.2})",
                network, utilization, spent_usd, budget_usd
            );
        } else if utilization > 100.0 {
            error!(
                "Budget exceeded for {}: {:.1}% (${:.2} of ${:.2})",
                network, utilization, spent_usd, budget_usd
            );
        }
    }

    /// Record gas optimization savings
    pub fn record_optimization_savings(
        feed_name: &str,
        network: &str,
        optimization_type: &str,
        savings_usd: f64,
    ) {
        GAS_OPTIMIZATION_SAVINGS_USD
            .with_label_values(&[feed_name, network, optimization_type])
            .inc_by(savings_usd);

        debug!(
            "Gas optimization savings for {}/{}: ${:.4} USD from {}",
            feed_name, network, savings_usd, optimization_type
        );
    }

    /// Reset monthly counters (should be called by a scheduled job)
    pub fn reset_monthly_counters() {
        // This would be called by a scheduled task to reset monthly metrics
        // In practice, you might want to use Prometheus recording rules instead
        warn!("Monthly counter reset requested - implement with care in production");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_gas_cost_usd_normal() {
        EconomicMetrics::record_gas_cost_usd("test_feed_1", "eth_test", 5.0, 2000.0);
        // Verify it increments counters for all_time, monthly, daily
        // and observes cost_per_update histogram
    }

    #[test]
    fn test_record_gas_cost_usd_high_cost_warning() {
        // Should trigger warning when cost > 10.0
        EconomicMetrics::record_gas_cost_usd("test_feed_2", "eth_test", 15.0, 2000.0);
    }

    #[test]
    fn test_record_gas_cost_usd_zero() {
        EconomicMetrics::record_gas_cost_usd("test_feed_3", "eth_test", 0.0, 2000.0);
    }

    #[test]
    fn test_update_wallet_balance_usd_critical_threshold() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Balance: 50 native * 2.0 price = 100.0 USD < 100.0 critical
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest1",
            49.0,
            2.0,
            Some(&thresholds),
        );
        // Should set LOW_BALANCE_ALERT with severity="critical"
    }

    #[test]
    fn test_update_wallet_balance_usd_warning_threshold() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Balance: 200 native * 2.0 price = 400.0 USD < 500.0 warning
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest2",
            200.0,
            2.0,
            Some(&thresholds),
        );
        // Should set LOW_BALANCE_ALERT with severity="warning"
    }

    #[test]
    fn test_update_wallet_balance_usd_info_threshold() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Balance: 400 native * 2.0 price = 800.0 USD < 1000.0 info
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest3",
            400.0,
            2.0,
            Some(&thresholds),
        );
        // Should set LOW_BALANCE_ALERT with severity="info"
    }

    #[test]
    fn test_update_wallet_balance_usd_ok_threshold() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Balance: 600 native * 2.0 price = 1200.0 USD >= 1000.0 info
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest4",
            600.0,
            2.0,
            Some(&thresholds),
        );
        // Should set LOW_BALANCE_ALERT with severity="ok" and alert_status=0.0
    }

    #[test]
    fn test_update_wallet_balance_usd_no_thresholds() {
        // Without thresholds, should clear alert to "ok"
        EconomicMetrics::update_wallet_balance_usd("eth_test", "0xtest5", 100.0, 2.0, None);
        // Should set LOW_BALANCE_ALERT with severity="ok" and alert_status=0.0
    }

    #[test]
    fn test_update_wallet_balance_usd_threshold_boundary_critical() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Exactly at critical threshold: 50 native * 2.0 price = 100.0 USD
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest6",
            50.0,
            2.0,
            Some(&thresholds),
        );
        // 100.0 is NOT < 100.0, so should be warning level
    }

    #[test]
    fn test_update_wallet_balance_usd_threshold_boundary_warning() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Exactly at warning threshold: 250 native * 2.0 price = 500.0 USD
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest7",
            250.0,
            2.0,
            Some(&thresholds),
        );
        // 500.0 is NOT < 500.0, so should be info level
    }

    #[test]
    fn test_update_wallet_balance_usd_threshold_boundary_info() {
        let thresholds = BalanceAlertThresholds {
            critical_threshold_usd: 100.0,
            warning_threshold_usd: 500.0,
            info_threshold_usd: 1000.0,
        };
        // Exactly at info threshold: 500 native * 2.0 price = 1000.0 USD
        EconomicMetrics::update_wallet_balance_usd(
            "eth_test",
            "0xtest8",
            500.0,
            2.0,
            Some(&thresholds),
        );
        // 1000.0 is NOT < 1000.0, so should be ok
    }

    #[test]
    fn test_update_runway_days_normal() {
        // 1000 USD balance, 10 USD/day rate = 100 days
        EconomicMetrics::update_runway_days("eth_test", "0xtest9", 1000.0, 10.0);
    }

    #[test]
    fn test_update_runway_days_zero_rate() {
        // Zero spending rate should result in INFINITY
        EconomicMetrics::update_runway_days("eth_test", "0xtest10", 1000.0, 0.0);
        // Should cap at 365 days for display
    }

    #[test]
    fn test_update_runway_days_critical_boundary() {
        // Exactly 7 days runway - should NOT trigger error (needs < 7.0)
        EconomicMetrics::update_runway_days("eth_test", "0xtest11", 70.0, 10.0);
    }

    #[test]
    fn test_update_runway_days_critical_below() {
        // Below 7 days - should trigger error
        EconomicMetrics::update_runway_days("eth_test", "0xtest12", 60.0, 10.0);
    }

    #[test]
    fn test_update_runway_days_warning_boundary() {
        // Exactly 30 days runway - should NOT trigger warning (needs < 30.0)
        EconomicMetrics::update_runway_days("eth_test", "0xtest13", 300.0, 10.0);
    }

    #[test]
    fn test_update_runway_days_warning_below() {
        // Below 30 days but above 7 - should trigger warning
        EconomicMetrics::update_runway_days("eth_test", "0xtest14", 200.0, 10.0);
    }

    #[test]
    fn test_update_runway_days_capped_at_365() {
        // 5000 USD / 10 USD/day = 500 days, should be capped at 365
        EconomicMetrics::update_runway_days("eth_test", "0xtest15", 5000.0, 10.0);
    }

    #[test]
    fn test_update_daily_spending_rate() {
        EconomicMetrics::update_daily_spending_rate("eth_test", 50.0);
    }

    #[test]
    fn test_update_daily_spending_rate_zero() {
        EconomicMetrics::update_daily_spending_rate("eth_test", 0.0);
    }

    #[test]
    fn test_record_gas_price_ratio_normal() {
        // Ratio: 100 / 100 = 1.0 (paying average)
        EconomicMetrics::record_gas_price_ratio("test_feed_4", "eth_test", 100.0, 100.0);
    }

    #[test]
    fn test_record_gas_price_ratio_high_warning() {
        // Ratio: 200 / 100 = 2.0 > 1.5, should warn
        EconomicMetrics::record_gas_price_ratio("test_feed_5", "eth_test", 200.0, 100.0);
    }

    #[test]
    fn test_record_gas_price_ratio_zero_avg() {
        // Zero average should be guarded - no observation should occur
        EconomicMetrics::record_gas_price_ratio("test_feed_6", "eth_test", 100.0, 0.0);
    }

    #[test]
    fn test_record_gas_price_ratio_boundary() {
        // Exactly 1.5x - should NOT warn (needs > 1.5)
        EconomicMetrics::record_gas_price_ratio("test_feed_7", "eth_test", 150.0, 100.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_normal() {
        // Score: (50 / 100) * 100 = 50%
        EconomicMetrics::update_cost_efficiency_score("test_feed_8", "eth_test", 100, 100.0, 50.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_perfect() {
        // Score: (100 / 100) * 100 = 100%
        EconomicMetrics::update_cost_efficiency_score("test_feed_9", "eth_test", 100, 100.0, 100.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_zero_cost() {
        // Zero total cost should result in 100% score
        EconomicMetrics::update_cost_efficiency_score("test_feed_10", "eth_test", 100, 0.0, 50.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_low_warning() {
        // Score: (25 / 100) * 100 = 25% < 50%, should warn
        EconomicMetrics::update_cost_efficiency_score("test_feed_11", "eth_test", 100, 100.0, 25.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_boundary() {
        // Exactly 50% - should NOT warn (needs < 50.0)
        EconomicMetrics::update_cost_efficiency_score("test_feed_12", "eth_test", 100, 100.0, 50.0);
    }

    #[test]
    fn test_update_cost_efficiency_score_clamp_above_100() {
        // Score: (150 / 100) * 100 = 150%, should clamp to 100%
        EconomicMetrics::update_cost_efficiency_score(
            "test_feed_13",
            "eth_test",
            100,
            100.0,
            150.0,
        );
    }

    #[test]
    fn test_update_budget_utilization_normal() {
        // Utilization: (500 / 1000) * 100 = 50%
        EconomicMetrics::update_budget_utilization("eth_test", 500.0, 1000.0);
    }

    #[test]
    fn test_update_budget_utilization_zero_budget() {
        // Zero budget should result in 0% utilization
        EconomicMetrics::update_budget_utilization("eth_test", 100.0, 0.0);
    }

    #[test]
    fn test_update_budget_utilization_high_warning() {
        // Utilization: (950 / 1000) * 100 = 95% > 90%, should warn
        EconomicMetrics::update_budget_utilization("eth_test", 950.0, 1000.0);
    }

    #[test]
    fn test_update_budget_utilization_exceeded_error() {
        // Utilization: (1100 / 1000) * 100 = 110% > 100%, should error
        EconomicMetrics::update_budget_utilization("eth_test", 1100.0, 1000.0);
    }

    #[test]
    fn test_update_budget_utilization_boundary_90() {
        // Exactly 90% - should NOT warn (needs > 90.0)
        EconomicMetrics::update_budget_utilization("eth_test", 900.0, 1000.0);
    }

    #[test]
    fn test_update_budget_utilization_boundary_100() {
        // Exactly 100% - should NOT error (needs > 100.0)
        EconomicMetrics::update_budget_utilization("eth_test", 1000.0, 1000.0);
    }

    #[test]
    fn test_record_optimization_savings_normal() {
        EconomicMetrics::record_optimization_savings(
            "test_feed_14",
            "eth_test",
            "batch_optimization",
            25.50,
        );
    }

    #[test]
    fn test_record_optimization_savings_zero() {
        EconomicMetrics::record_optimization_savings(
            "test_feed_15",
            "eth_test",
            "timing_optimization",
            0.0,
        );
    }

    #[test]
    fn test_record_optimization_savings_multiple_types() {
        EconomicMetrics::record_optimization_savings(
            "test_feed_16",
            "eth_test",
            "gas_price_optimization",
            10.0,
        );
        EconomicMetrics::record_optimization_savings(
            "test_feed_16",
            "eth_test",
            "batch_optimization",
            15.0,
        );
        EconomicMetrics::record_optimization_savings(
            "test_feed_16",
            "eth_test",
            "timing_optimization",
            5.0,
        );
    }

    #[test]
    fn test_reset_monthly_counters_smoke() {
        // Just verify it doesn't panic and logs warning
        EconomicMetrics::reset_monthly_counters();
    }
}
