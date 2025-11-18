use alloy::{primitives::U256, rpc::types::TransactionReceipt};
use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use tracing::{info, warn};

lazy_static! {
    /// Total gas used counter per feed and network
    static ref GAS_USED_TOTAL: CounterVec = register_counter_vec!(
        "omikuji_gas_used_total",
        "Total gas consumed by transactions",
        &["feed_name", "network", "status"]
    ).expect("Failed to create gas_used_total metric");

    /// Gas price histogram in gwei per network
    static ref GAS_PRICE_GWEI: HistogramVec = register_histogram_vec!(
        "omikuji_gas_price_gwei",
        "Gas price in gwei for transactions",
        &["network", "tx_type"],
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0]
    ).expect("Failed to create gas_price_gwei metric");

    /// Gas efficiency gauge (percentage of gas limit used)
    static ref GAS_EFFICIENCY_PERCENT: GaugeVec = register_gauge_vec!(
        "omikuji_gas_efficiency_percent",
        "Percentage of gas limit actually used",
        &["feed_name", "network"]
    ).expect("Failed to create gas_efficiency_percent metric");

    /// Transaction cost in native token (wei)
    static ref TRANSACTION_COST_WEI: HistogramVec = register_histogram_vec!(
        "omikuji_transaction_cost_wei",
        "Transaction cost in wei",
        &["feed_name", "network"],
        // Buckets in wei (0.0001 to 1 native token)
        vec![1e14, 5e14, 1e15, 5e15, 1e16, 5e16, 1e17, 5e17, 1e18]
    ).expect("Failed to create transaction_cost_wei metric");

    /// Number of transactions counter
    static ref TRANSACTION_COUNT: CounterVec = register_counter_vec!(
        "omikuji_transaction_count",
        "Number of transactions submitted",
        &["feed_name", "network", "status", "tx_type"]
    ).expect("Failed to create transaction_count metric");

    /// Gas limit vs gas used comparison
    static ref GAS_LIMIT_GAUGE: GaugeVec = register_gauge_vec!(
        "omikuji_gas_limit",
        "Gas limit set for transactions",
        &["feed_name", "network"]
    ).expect("Failed to create gas_limit gauge");

    /// Current gas token price in USD
    pub static ref GAS_TOKEN_PRICE_USD: GaugeVec = register_gauge_vec!(
        "omikuji_gas_token_price_usd",
        "Current price per gas token in USD",
        &["network", "token_symbol"]
    ).expect("Failed to create gas_token_price_usd metric");

    /// Cumulative gas cost in USD
    static ref CUMULATIVE_GAS_COST_USD: CounterVec = register_counter_vec!(
        "omikuji_cumulative_gas_cost_usd",
        "Running total cost in USD",
        &["network", "feed_name"]
    ).expect("Failed to create cumulative_gas_cost_usd metric");

    /// Hourly gas cost in USD
    static ref HOURLY_GAS_COST_USD: GaugeVec = register_gauge_vec!(
        "omikuji_hourly_gas_cost_usd",
        "Gas cost per hour in USD",
        &["network", "feed_name"]
    ).expect("Failed to create hourly_gas_cost_usd metric");

    /// Daily gas cost in USD
    static ref DAILY_GAS_COST_USD: GaugeVec = register_gauge_vec!(
        "omikuji_daily_gas_cost_usd",
        "Gas cost per day in USD",
        &["network", "feed_name"]
    ).expect("Failed to create daily_gas_cost_usd metric");

    /// Gas price feed updates counter
    pub static ref GAS_PRICE_FEED_UPDATES_TOTAL: CounterVec = register_counter_vec!(
        "omikuji_gas_price_feed_updates_total",
        "Total number of gas price feed updates",
        &["provider", "status"]
    ).expect("Failed to create gas_price_feed_updates_total metric");

    /// Gas price feed errors counter
    pub static ref GAS_PRICE_FEED_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "omikuji_gas_price_feed_errors_total",
        "Total number of gas price feed errors",
        &["provider", "error_type"]
    ).expect("Failed to create gas_price_feed_errors_total metric");

    /// Gas price staleness in seconds
    pub static ref GAS_PRICE_STALENESS_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_gas_price_staleness_seconds",
        "Time since last gas price update in seconds",
        &["network", "token_symbol"]
    ).expect("Failed to create gas_price_staleness_seconds metric");
}

/// Gas metrics collector
pub struct GasMetrics;

impl GasMetrics {
    /// Record gas metrics from a transaction receipt
    pub fn record_transaction(
        feed_name: &str,
        network: &str,
        receipt: &TransactionReceipt,
        gas_limit: U256,
        tx_type: &str,
    ) {
        let status = if receipt.status() {
            "success"
        } else {
            "failed"
        };

        // Get gas used and effective gas price
        let gas_used = receipt.gas_used;
        let effective_gas_price = U256::from(receipt.effective_gas_price);

        // Calculate metrics
        let gas_used_f64 = gas_used as f64;
        let gas_limit_f64 = gas_limit.to::<u64>() as f64;
        let gas_price_gwei = effective_gas_price.to::<u128>() as f64 / 1e9;
        let total_cost_wei = U256::from(gas_used).saturating_mul(effective_gas_price);
        let efficiency_percent = if gas_limit > U256::ZERO {
            (gas_used_f64 / gas_limit_f64) * 100.0
        } else {
            0.0
        };

        // Record metrics
        GAS_USED_TOTAL
            .with_label_values(&[feed_name, network, status])
            .inc_by(gas_used_f64);

        GAS_PRICE_GWEI
            .with_label_values(&[network, tx_type])
            .observe(gas_price_gwei);

        GAS_EFFICIENCY_PERCENT
            .with_label_values(&[feed_name, network])
            .set(efficiency_percent);

        TRANSACTION_COST_WEI
            .with_label_values(&[feed_name, network])
            .observe(total_cost_wei.to::<u128>() as f64);

        TRANSACTION_COUNT
            .with_label_values(&[feed_name, network, status, tx_type])
            .inc();

        GAS_LIMIT_GAUGE
            .with_label_values(&[feed_name, network])
            .set(gas_limit_f64);

        // Log the transaction
        info!(
            "Transaction gas metrics - Feed: {}, Network: {}, Status: {}, \
            Gas Used: {}, Gas Limit: {}, Efficiency: {:.1}%, \
            Gas Price: {:.2} gwei, Total Cost: {} wei ({:.6} native tokens), \
            Tx Hash: {:?}",
            feed_name,
            network,
            status,
            gas_used,
            gas_limit,
            efficiency_percent,
            gas_price_gwei,
            total_cost_wei,
            total_cost_wei.to::<u128>() as f64 / 1e18,
            receipt.transaction_hash
        );

        // Warn if efficiency is poor
        if efficiency_percent < 50.0 && gas_limit > U256::ZERO {
            warn!(
                "Low gas efficiency for {} on {}: {:.1}% of limit used. \
                Consider reducing gas limit.",
                feed_name, network, efficiency_percent
            );
        } else if efficiency_percent > 90.0 {
            warn!(
                "High gas usage for {} on {}: {:.1}% of limit used. \
                Consider increasing gas limit for safety.",
                feed_name, network, efficiency_percent
            );
        }
    }

    /// Record a failed transaction (one that didn't get a receipt)
    pub fn record_failed_transaction(
        feed_name: &str,
        network: &str,
        gas_limit: U256,
        estimated_gas_price: Option<U256>,
        tx_type: &str,
        error: &str,
    ) {
        TRANSACTION_COUNT
            .with_label_values(&[feed_name, network, "error", tx_type])
            .inc();

        if let Some(gas_price) = estimated_gas_price {
            let gas_price_gwei = gas_price.to::<u128>() as f64 / 1e9;
            GAS_PRICE_GWEI
                .with_label_values(&[network, tx_type])
                .observe(gas_price_gwei);
        }

        warn!(
            "Transaction failed before receipt - Feed: {}, Network: {}, \
            Gas Limit: {}, Error: {}",
            feed_name, network, gas_limit, error
        );
    }

    /// Record gas cost in USD
    pub fn record_usd_cost(
        feed_name: &str,
        network: &str,
        gas_used: u64,
        gas_price_wei: u128,
        gas_token_price_usd: f64,
    ) {
        // Calculate cost in USD
        let gas_cost_native = (gas_used as f64 * gas_price_wei as f64) / 1e18;
        let total_cost_usd = gas_cost_native * gas_token_price_usd;

        // Update cumulative cost
        CUMULATIVE_GAS_COST_USD
            .with_label_values(&[network, feed_name])
            .inc_by(total_cost_usd);

        info!(
            "Transaction cost in USD - Feed: {}, Network: {}, \
            Gas Used: {}, Gas Price: {} wei, Token Price: ${:.2}, \
            Total Cost: ${:.6}",
            feed_name, network, gas_used, gas_price_wei, gas_token_price_usd, total_cost_usd
        );
    }

    /// Update hourly and daily cost gauges (to be called periodically)
    pub fn update_cost_gauges(
        feed_name: &str,
        network: &str,
        hourly_cost_usd: f64,
        daily_cost_usd: f64,
    ) {
        HOURLY_GAS_COST_USD
            .with_label_values(&[network, feed_name])
            .set(hourly_cost_usd);

        DAILY_GAS_COST_USD
            .with_label_values(&[network, feed_name])
            .set(daily_cost_usd);
    }
}

/// Transaction details for logging
#[derive(Debug, Clone)]
pub struct TransactionDetails {
    pub feed_name: String,
    pub network: String,
    pub tx_hash: String,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub gas_price_gwei: f64,
    pub total_cost_wei: u128,
    pub efficiency_percent: f64,
    pub status: String,
    pub tx_type: String,
    pub block_number: u64,
    pub error_message: Option<String>,
}
