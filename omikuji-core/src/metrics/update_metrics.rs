use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use tracing::{debug, info};

lazy_static! {
    /// Update decision counter
    static ref UPDATE_DECISION_COUNT: CounterVec = register_counter_vec!(
        "omikuji_update_decisions_total",
        "Total number of update decisions made",
        &["feed_name", "network", "decision", "reason"]
    ).expect("Failed to create update_decision_count metric");

    /// Time since last update in seconds
    static ref TIME_SINCE_UPDATE_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_time_since_update_seconds",
        "Seconds since last successful update",
        &["feed_name", "network", "update_type"]
    ).expect("Failed to create time_since_update metric");

    /// Deviation threshold breaches counter
    static ref DEVIATION_BREACH_COUNT: CounterVec = register_counter_vec!(
        "omikuji_deviation_breaches_total",
        "Total number of deviation threshold breaches",
        &["feed_name", "network", "severity"]
    ).expect("Failed to create deviation_breach_count metric");

    /// Update frequency violations counter
    static ref UPDATE_FREQUENCY_VIOLATION_COUNT: CounterVec = register_counter_vec!(
        "omikuji_update_frequency_violations_total",
        "Total number of minimum update frequency violations",
        &["feed_name", "network", "violation_type"]
    ).expect("Failed to create update_frequency_violation metric");

    /// Update check interval histogram
    static ref UPDATE_CHECK_INTERVAL_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_update_check_interval_seconds",
        "Time between update checks",
        &["feed_name", "network"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    ).expect("Failed to create update_check_interval metric");

    /// Skipped updates gauge
    static ref CONSECUTIVE_SKIPPED_UPDATES: GaugeVec = register_gauge_vec!(
        "omikuji_consecutive_skipped_updates",
        "Number of consecutive skipped updates",
        &["feed_name", "network", "skip_reason"]
    ).expect("Failed to create consecutive_skipped_updates metric");

    /// Update lag histogram (time between feed value change and contract update)
    static ref UPDATE_LAG_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_update_lag_seconds",
        "Time between feed value change and contract update",
        &["feed_name", "network"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    ).expect("Failed to create update_lag metric");

    /// Deviation at update time
    static ref UPDATE_DEVIATION_PERCENT: HistogramVec = register_histogram_vec!(
        "omikuji_update_deviation_percent",
        "Deviation percentage at time of update",
        &["feed_name", "network"],
        vec![0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0]
    ).expect("Failed to create update_deviation metric");

    /// Update attempts counter
    static ref UPDATE_ATTEMPT_COUNT: CounterVec = register_counter_vec!(
        "omikuji_update_attempts_total",
        "Total number of update attempts",
        &["feed_name", "network", "result"]
    ).expect("Failed to create update_attempt_count metric");
}

/// Update decision reasons
#[derive(Debug, Clone, Copy)]
pub enum UpdateReason {
    DeviationThreshold,
    TimeThreshold,
    Both,
    ForceUpdate,
    InitialUpdate,
}

impl UpdateReason {
    fn as_str(&self) -> &'static str {
        match self {
            UpdateReason::DeviationThreshold => "deviation_threshold",
            UpdateReason::TimeThreshold => "time_threshold",
            UpdateReason::Both => "both_thresholds",
            UpdateReason::ForceUpdate => "force_update",
            UpdateReason::InitialUpdate => "initial_update",
        }
    }
}

/// Skip reasons
#[derive(Debug, Clone, Copy)]
pub enum SkipReason {
    NoDeviation,
    TooSoon,
    NoChange,
    BelowThreshold,
    Error,
}

impl SkipReason {
    fn as_str(&self) -> &'static str {
        match self {
            SkipReason::NoDeviation => "no_deviation",
            SkipReason::TooSoon => "too_soon",
            SkipReason::NoChange => "no_change",
            SkipReason::BelowThreshold => "below_threshold",
            SkipReason::Error => "error",
        }
    }
}

/// Update metrics collector
pub struct UpdateMetrics;

impl UpdateMetrics {
    /// Record an update decision
    pub fn record_update_decision(
        feed_name: &str,
        network: &str,
        should_update: bool,
        reason: Option<UpdateReason>,
        skip_reason: Option<SkipReason>,
    ) {
        let decision = if should_update { "update" } else { "skip" };

        let reason_str = if should_update {
            reason.map(|r| r.as_str()).unwrap_or("unknown")
        } else {
            skip_reason.map(|r| r.as_str()).unwrap_or("unknown")
        };

        UPDATE_DECISION_COUNT
            .with_label_values(&[feed_name, network, decision, reason_str])
            .inc();

        // Update consecutive skipped counter
        if should_update {
            // Reset all skip counters
            for skip in [
                "no_deviation",
                "too_soon",
                "no_change",
                "below_threshold",
                "error",
            ] {
                CONSECUTIVE_SKIPPED_UPDATES
                    .with_label_values(&[feed_name, network, skip])
                    .set(0.0);
            }
        } else if let Some(skip) = skip_reason {
            let gauge =
                CONSECUTIVE_SKIPPED_UPDATES.with_label_values(&[feed_name, network, skip.as_str()]);
            gauge.set(gauge.get() + 1.0);
        }

        debug!(
            "Update decision for {}/{}: {} (reason: {})",
            feed_name, network, decision, reason_str
        );
    }

    /// Record time since last update
    pub fn update_time_since_last(
        feed_name: &str,
        network: &str,
        seconds_since_feed_update: f64,
        seconds_since_contract_update: f64,
    ) {
        TIME_SINCE_UPDATE_SECONDS
            .with_label_values(&[feed_name, network, "feed"])
            .set(seconds_since_feed_update);

        TIME_SINCE_UPDATE_SECONDS
            .with_label_values(&[feed_name, network, "contract"])
            .set(seconds_since_contract_update);
    }

    /// Record a deviation threshold breach
    pub fn record_deviation_breach(
        feed_name: &str,
        network: &str,
        deviation_percent: f64,
        threshold_percent: f64,
    ) {
        let severity = if deviation_percent >= threshold_percent * 3.0 {
            "critical"
        } else if deviation_percent >= threshold_percent * 2.0 {
            "high"
        } else {
            "normal"
        };

        DEVIATION_BREACH_COUNT
            .with_label_values(&[feed_name, network, severity])
            .inc();

        info!(
            "Deviation breach for {}/{}: {:.2}% (threshold: {:.2}%, severity: {})",
            feed_name, network, deviation_percent, threshold_percent, severity
        );
    }

    /// Record update frequency violation
    pub fn record_frequency_violation(
        feed_name: &str,
        network: &str,
        time_since_update: f64,
        minimum_frequency: f64,
    ) {
        let violation_type = if time_since_update >= minimum_frequency * 2.0 {
            "critical"
        } else {
            "warning"
        };

        UPDATE_FREQUENCY_VIOLATION_COUNT
            .with_label_values(&[feed_name, network, violation_type])
            .inc();

        info!(
            "Update frequency violation for {}/{}: {:.0}s since last update (minimum: {:.0}s)",
            feed_name, network, time_since_update, minimum_frequency
        );
    }

    /// Record update check interval
    pub fn record_check_interval(feed_name: &str, network: &str, interval_seconds: f64) {
        UPDATE_CHECK_INTERVAL_SECONDS
            .with_label_values(&[feed_name, network])
            .observe(interval_seconds);
    }

    /// Record update lag
    pub fn record_update_lag(
        feed_name: &str,
        network: &str,
        feed_timestamp: u64,
        update_timestamp: u64,
    ) {
        if update_timestamp > feed_timestamp {
            let lag_seconds = (update_timestamp - feed_timestamp) as f64;
            UPDATE_LAG_SECONDS
                .with_label_values(&[feed_name, network])
                .observe(lag_seconds);

            debug!(
                "Update lag for {}/{}: {:.0}s",
                feed_name, network, lag_seconds
            );
        }
    }

    /// Record deviation at update time
    pub fn record_update_deviation(feed_name: &str, network: &str, deviation_percent: f64) {
        UPDATE_DEVIATION_PERCENT
            .with_label_values(&[feed_name, network])
            .observe(deviation_percent);
    }

    /// Record update attempt
    pub fn record_update_attempt(feed_name: &str, network: &str, success: bool) {
        let result = if success { "success" } else { "failure" };

        UPDATE_ATTEMPT_COUNT
            .with_label_values(&[feed_name, network, result])
            .inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_update_decision_deviation_threshold() {
        UpdateMetrics::record_update_decision(
            "feed_dev_threshold",
            "network_dev",
            true,
            Some(UpdateReason::DeviationThreshold),
            None,
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_dev_threshold",
            "network_dev",
            "update",
            "deviation_threshold",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_time_threshold() {
        UpdateMetrics::record_update_decision(
            "feed_time_threshold",
            "network_time",
            true,
            Some(UpdateReason::TimeThreshold),
            None,
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_time_threshold",
            "network_time",
            "update",
            "time_threshold",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_both_thresholds() {
        UpdateMetrics::record_update_decision(
            "feed_both",
            "network_both",
            true,
            Some(UpdateReason::Both),
            None,
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_both",
            "network_both",
            "update",
            "both_thresholds",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_force_update() {
        UpdateMetrics::record_update_decision(
            "feed_force",
            "network_force",
            true,
            Some(UpdateReason::ForceUpdate),
            None,
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_force",
            "network_force",
            "update",
            "force_update",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_initial_update() {
        UpdateMetrics::record_update_decision(
            "feed_initial",
            "network_initial",
            true,
            Some(UpdateReason::InitialUpdate),
            None,
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_initial",
            "network_initial",
            "update",
            "initial_update",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_skip_no_deviation() {
        UpdateMetrics::record_update_decision(
            "feed_skip_no_dev",
            "network_skip_no_dev",
            false,
            None,
            Some(SkipReason::NoDeviation),
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_skip_no_dev",
            "network_skip_no_dev",
            "skip",
            "no_deviation",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_skip_too_soon() {
        UpdateMetrics::record_update_decision(
            "feed_skip_soon",
            "network_skip_soon",
            false,
            None,
            Some(SkipReason::TooSoon),
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_skip_soon",
            "network_skip_soon",
            "skip",
            "too_soon",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_skip_no_change() {
        UpdateMetrics::record_update_decision(
            "feed_skip_no_chg",
            "network_skip_no_chg",
            false,
            None,
            Some(SkipReason::NoChange),
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_skip_no_chg",
            "network_skip_no_chg",
            "skip",
            "no_change",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_skip_below_threshold() {
        UpdateMetrics::record_update_decision(
            "feed_skip_below",
            "network_skip_below",
            false,
            None,
            Some(SkipReason::BelowThreshold),
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_skip_below",
            "network_skip_below",
            "skip",
            "below_threshold",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_decision_skip_error() {
        UpdateMetrics::record_update_decision(
            "feed_skip_err",
            "network_skip_err",
            false,
            None,
            Some(SkipReason::Error),
        );

        let counter = UPDATE_DECISION_COUNT.with_label_values(&[
            "feed_skip_err",
            "network_skip_err",
            "skip",
            "error",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_skip_counter_increments_on_skip() {
        let feed = "feed_skip_inc";
        let network = "network_skip_inc";

        // Record a skip
        UpdateMetrics::record_update_decision(
            feed,
            network,
            false,
            None,
            Some(SkipReason::NoDeviation),
        );

        let gauge = CONSECUTIVE_SKIPPED_UPDATES.with_label_values(&[feed, network, "no_deviation"]);
        let count1 = gauge.get();
        assert!(count1 > 0.0);

        // Record another skip
        UpdateMetrics::record_update_decision(
            feed,
            network,
            false,
            None,
            Some(SkipReason::NoDeviation),
        );

        let count2 = gauge.get();
        assert!(count2 > count1);
    }

    #[test]
    fn test_skip_counter_resets_on_update() {
        let feed = "feed_reset_skip";
        let network = "network_reset_skip";

        // First skip to increment counter
        UpdateMetrics::record_update_decision(
            feed,
            network,
            false,
            None,
            Some(SkipReason::TooSoon),
        );

        let gauge = CONSECUTIVE_SKIPPED_UPDATES.with_label_values(&[feed, network, "too_soon"]);
        assert!(gauge.get() > 0.0);

        // Now perform an update
        UpdateMetrics::record_update_decision(
            feed,
            network,
            true,
            Some(UpdateReason::DeviationThreshold),
            None,
        );

        // All skip counters should be reset to 0
        for skip in [
            "no_deviation",
            "too_soon",
            "no_change",
            "below_threshold",
            "error",
        ] {
            let skip_gauge = CONSECUTIVE_SKIPPED_UPDATES.with_label_values(&[feed, network, skip]);
            assert_eq!(skip_gauge.get(), 0.0);
        }
    }

    #[test]
    fn test_update_time_since_last() {
        UpdateMetrics::update_time_since_last(
            "feed_time_since",
            "network_time_since",
            123.45,
            67.89,
        );

        let feed_gauge = TIME_SINCE_UPDATE_SECONDS.with_label_values(&[
            "feed_time_since",
            "network_time_since",
            "feed",
        ]);
        assert_eq!(feed_gauge.get(), 123.45);

        let contract_gauge = TIME_SINCE_UPDATE_SECONDS.with_label_values(&[
            "feed_time_since",
            "network_time_since",
            "contract",
        ]);
        assert_eq!(contract_gauge.get(), 67.89);
    }

    #[test]
    fn test_deviation_breach_normal() {
        UpdateMetrics::record_deviation_breach("feed_dev_normal", "network_dev_normal", 1.5, 1.0);

        let counter = DEVIATION_BREACH_COUNT.with_label_values(&[
            "feed_dev_normal",
            "network_dev_normal",
            "normal",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_deviation_breach_high_boundary() {
        // Exactly 2x threshold should be "high"
        UpdateMetrics::record_deviation_breach("feed_dev_high", "network_dev_high", 2.0, 1.0);

        let counter = DEVIATION_BREACH_COUNT.with_label_values(&[
            "feed_dev_high",
            "network_dev_high",
            "high",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_deviation_breach_high() {
        // > 2x but < 3x threshold should be "high"
        UpdateMetrics::record_deviation_breach("feed_dev_high2", "network_dev_high2", 2.5, 1.0);

        let counter = DEVIATION_BREACH_COUNT.with_label_values(&[
            "feed_dev_high2",
            "network_dev_high2",
            "high",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_deviation_breach_critical_boundary() {
        // Exactly 3x threshold should be "critical"
        UpdateMetrics::record_deviation_breach("feed_dev_crit", "network_dev_crit", 3.0, 1.0);

        let counter = DEVIATION_BREACH_COUNT.with_label_values(&[
            "feed_dev_crit",
            "network_dev_crit",
            "critical",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_deviation_breach_critical() {
        // > 3x threshold should be "critical"
        UpdateMetrics::record_deviation_breach("feed_dev_crit2", "network_dev_crit2", 5.0, 1.0);

        let counter = DEVIATION_BREACH_COUNT.with_label_values(&[
            "feed_dev_crit2",
            "network_dev_crit2",
            "critical",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_frequency_violation_warning() {
        // < 2x minimum should be "warning"
        UpdateMetrics::record_frequency_violation(
            "feed_freq_warn",
            "network_freq_warn",
            150.0,
            100.0,
        );

        let counter = UPDATE_FREQUENCY_VIOLATION_COUNT.with_label_values(&[
            "feed_freq_warn",
            "network_freq_warn",
            "warning",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_frequency_violation_critical_boundary() {
        // Exactly 2x minimum should be "critical"
        UpdateMetrics::record_frequency_violation(
            "feed_freq_crit",
            "network_freq_crit",
            200.0,
            100.0,
        );

        let counter = UPDATE_FREQUENCY_VIOLATION_COUNT.with_label_values(&[
            "feed_freq_crit",
            "network_freq_crit",
            "critical",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_frequency_violation_critical() {
        // > 2x minimum should be "critical"
        UpdateMetrics::record_frequency_violation(
            "feed_freq_crit2",
            "network_freq_crit2",
            300.0,
            100.0,
        );

        let counter = UPDATE_FREQUENCY_VIOLATION_COUNT.with_label_values(&[
            "feed_freq_crit2",
            "network_freq_crit2",
            "critical",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_check_interval() {
        UpdateMetrics::record_check_interval("feed_check_int", "network_check_int", 42.5);

        let histogram = UPDATE_CHECK_INTERVAL_SECONDS
            .with_label_values(&["feed_check_int", "network_check_int"]);
        assert!(histogram.get_sample_count() > 0);
    }

    #[test]
    fn test_record_update_lag_valid() {
        // update_timestamp > feed_timestamp should record lag
        UpdateMetrics::record_update_lag("feed_lag_valid", "network_lag_valid", 1000, 1050);

        let histogram =
            UPDATE_LAG_SECONDS.with_label_values(&["feed_lag_valid", "network_lag_valid"]);
        assert!(histogram.get_sample_count() > 0);
        assert_eq!(histogram.get_sample_sum(), 50.0);
    }

    #[test]
    fn test_record_update_lag_equal_timestamps() {
        // update_timestamp == feed_timestamp should not record lag
        let histogram_before =
            UPDATE_LAG_SECONDS.with_label_values(&["feed_lag_equal", "network_lag_equal"]);
        let count_before = histogram_before.get_sample_count();

        UpdateMetrics::record_update_lag("feed_lag_equal", "network_lag_equal", 2000, 2000);

        let histogram_after =
            UPDATE_LAG_SECONDS.with_label_values(&["feed_lag_equal", "network_lag_equal"]);
        let count_after = histogram_after.get_sample_count();

        // Count should not increase
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn test_record_update_lag_update_before_feed() {
        // update_timestamp < feed_timestamp should not record lag
        let histogram_before =
            UPDATE_LAG_SECONDS.with_label_values(&["feed_lag_before", "network_lag_before"]);
        let count_before = histogram_before.get_sample_count();

        UpdateMetrics::record_update_lag("feed_lag_before", "network_lag_before", 3000, 2500);

        let histogram_after =
            UPDATE_LAG_SECONDS.with_label_values(&["feed_lag_before", "network_lag_before"]);
        let count_after = histogram_after.get_sample_count();

        // Count should not increase
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn test_record_update_deviation() {
        UpdateMetrics::record_update_deviation("feed_upd_dev", "network_upd_dev", 3.75);

        let histogram =
            UPDATE_DEVIATION_PERCENT.with_label_values(&["feed_upd_dev", "network_upd_dev"]);
        assert!(histogram.get_sample_count() > 0);
    }

    #[test]
    fn test_record_update_attempt_success() {
        UpdateMetrics::record_update_attempt("feed_attempt_ok", "network_attempt_ok", true);

        let counter = UPDATE_ATTEMPT_COUNT.with_label_values(&[
            "feed_attempt_ok",
            "network_attempt_ok",
            "success",
        ]);
        assert!(counter.get() > 0.0);
    }

    #[test]
    fn test_record_update_attempt_failure() {
        UpdateMetrics::record_update_attempt("feed_attempt_fail", "network_attempt_fail", false);

        let counter = UPDATE_ATTEMPT_COUNT.with_label_values(&[
            "feed_attempt_fail",
            "network_attempt_fail",
            "failure",
        ]);
        assert!(counter.get() > 0.0);
    }
}
