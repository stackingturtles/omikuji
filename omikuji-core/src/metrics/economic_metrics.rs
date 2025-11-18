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
