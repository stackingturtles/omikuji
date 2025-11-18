use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use tracing::{debug, error, warn};

lazy_static! {
    /// Feed value change rate
    static ref FEED_VALUE_CHANGE_RATE: HistogramVec = register_histogram_vec!(
        "omikuji_feed_value_change_rate_percent",
        "Rate of change in feed values (percent per minute)",
        &["feed_name", "network"],
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0]
    ).expect("Failed to create feed_value_change_rate metric");

    /// Outlier detection counter
    static ref OUTLIER_DETECTION_COUNT: CounterVec = register_counter_vec!(
        "omikuji_outlier_detections_total",
        "Total number of outlier values detected",
        &["feed_name", "network", "outlier_type", "action"]
    ).expect("Failed to create outlier_detection_count metric");

    /// Data consistency score
    static ref DATA_CONSISTENCY_SCORE: GaugeVec = register_gauge_vec!(
        "omikuji_data_consistency_score",
        "Data consistency score (0-100)",
        &["feed_name", "network"]
    ).expect("Failed to create data_consistency_score metric");

    /// Stale data duration
    static ref STALE_DATA_DURATION_SECONDS: GaugeVec = register_gauge_vec!(
        "omikuji_stale_data_duration_seconds",
        "Duration of stale data condition",
        &["feed_name", "network", "staleness_type"]
    ).expect("Failed to create stale_data_duration metric");

    /// Value deviation from moving average
    static ref VALUE_DEVIATION_FROM_MA: HistogramVec = register_histogram_vec!(
        "omikuji_value_deviation_from_ma_percent",
        "Deviation from moving average in percent",
        &["feed_name", "network", "ma_period"],
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
    ).expect("Failed to create value_deviation_from_ma metric");

    /// Data source agreement gauge
    static ref DATA_SOURCE_AGREEMENT: GaugeVec = register_gauge_vec!(
        "omikuji_data_source_agreement",
        "Agreement between multiple data sources (0-100%)",
        &["feed_name", "network"]
    ).expect("Failed to create data_source_agreement metric");

    /// Invalid value counter
    static ref INVALID_VALUE_COUNT: CounterVec = register_counter_vec!(
        "omikuji_invalid_values_total",
        "Total number of invalid values encountered",
        &["feed_name", "network", "validation_type"]
    ).expect("Failed to create invalid_value_count metric");

    /// Data gap occurrences
    static ref DATA_GAP_COUNT: CounterVec = register_counter_vec!(
        "omikuji_data_gaps_total",
        "Total number of data gaps detected",
        &["feed_name", "network", "gap_duration_category"]
    ).expect("Failed to create data_gap_count metric");

    /// Feed reliability score
    static ref FEED_RELIABILITY_SCORE: GaugeVec = register_gauge_vec!(
        "omikuji_feed_reliability_score",
        "Overall feed reliability score (0-100)",
        &["feed_name", "network"]
    ).expect("Failed to create feed_reliability_score metric");

    /// Timestamp drift
    static ref TIMESTAMP_DRIFT_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_timestamp_drift_seconds",
        "Drift between feed timestamp and system time",
        &["feed_name", "network"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    ).expect("Failed to create timestamp_drift metric");
}

/// Quality metrics collector
pub struct QualityMetrics;

impl QualityMetrics {
    /// Record feed value change rate
    pub fn record_value_change_rate(
        feed_name: &str,
        network: &str,
        previous_value: f64,
        current_value: f64,
        time_delta_seconds: f64,
    ) {
        if previous_value != 0.0 && time_delta_seconds > 0.0 {
            let change_percent = ((current_value - previous_value).abs() / previous_value) * 100.0;
            let rate_per_minute = (change_percent / time_delta_seconds) * 60.0;

            FEED_VALUE_CHANGE_RATE
                .with_label_values(&[feed_name, network])
                .observe(rate_per_minute);

            if rate_per_minute > 10.0 {
                warn!(
                    "High value change rate for {}/{}: {:.2}% per minute",
                    feed_name, network, rate_per_minute
                );
            }
        }
    }

    /// Record outlier detection
    pub fn record_outlier(
        feed_name: &str,
        network: &str,
        value: f64,
        expected_range: (f64, f64),
        action_taken: &str,
    ) {
        let outlier_type = if value < expected_range.0 {
            "too_low"
        } else if value > expected_range.1 {
            "too_high"
        } else {
            "anomaly"
        };

        OUTLIER_DETECTION_COUNT
            .with_label_values(&[feed_name, network, outlier_type, action_taken])
            .inc();

        error!(
            "Outlier detected for {}/{}: value={}, expected_range={:?}, type={}, action={}",
            feed_name, network, value, expected_range, outlier_type, action_taken
        );
    }

    /// Update data consistency score
    pub fn update_consistency_score(feed_name: &str, network: &str, score: f64) {
        let clamped_score = score.clamp(0.0, 100.0);

        DATA_CONSISTENCY_SCORE
            .with_label_values(&[feed_name, network])
            .set(clamped_score);

        if clamped_score < 50.0 {
            warn!(
                "Low data consistency score for {}/{}: {:.1}",
                feed_name, network, clamped_score
            );
        }
    }

    /// Update stale data duration
    pub fn update_stale_data_duration(
        feed_name: &str,
        network: &str,
        staleness_type: &str,
        duration_seconds: f64,
    ) {
        STALE_DATA_DURATION_SECONDS
            .with_label_values(&[feed_name, network, staleness_type])
            .set(duration_seconds);

        if duration_seconds > 3600.0 {
            error!(
                "Critical: {} data stale for {}/{}: {:.0}s",
                staleness_type, feed_name, network, duration_seconds
            );
        }
    }

    /// Record deviation from moving average
    pub fn record_ma_deviation(
        feed_name: &str,
        network: &str,
        current_value: f64,
        ma_value: f64,
        ma_period: &str,
    ) {
        if ma_value != 0.0 {
            let deviation_percent = ((current_value - ma_value).abs() / ma_value) * 100.0;

            VALUE_DEVIATION_FROM_MA
                .with_label_values(&[feed_name, network, ma_period])
                .observe(deviation_percent);

            if deviation_percent > 20.0 {
                warn!(
                    "Large deviation from {} MA for {}/{}: {:.2}% (current: {}, MA: {})",
                    ma_period, feed_name, network, deviation_percent, current_value, ma_value
                );
            }
        }
    }

    /// Update data source agreement
    pub fn update_source_agreement(
        feed_name: &str,
        network: &str,
        agreement_percent: f64,
        sources_compared: usize,
    ) {
        let clamped_agreement = agreement_percent.clamp(0.0, 100.0);

        DATA_SOURCE_AGREEMENT
            .with_label_values(&[feed_name, network])
            .set(clamped_agreement);

        if clamped_agreement < 80.0 && sources_compared > 1 {
            warn!(
                "Low data source agreement for {}/{}: {:.1}% across {} sources",
                feed_name, network, clamped_agreement, sources_compared
            );
        }
    }

    /// Record invalid value
    pub fn record_invalid_value(
        feed_name: &str,
        network: &str,
        validation_type: &str,
        value: f64,
        reason: &str,
    ) {
        INVALID_VALUE_COUNT
            .with_label_values(&[feed_name, network, validation_type])
            .inc();

        error!(
            "Invalid value for {}/{}: {} (type: {}, reason: {})",
            feed_name, network, value, validation_type, reason
        );
    }

    /// Record data gap
    pub fn record_data_gap(feed_name: &str, network: &str, gap_duration_seconds: f64) {
        let duration_category = match gap_duration_seconds {
            s if s < 300.0 => "short",
            s if s < 1800.0 => "medium",
            s if s < 3600.0 => "long",
            _ => "critical",
        };

        DATA_GAP_COUNT
            .with_label_values(&[feed_name, network, duration_category])
            .inc();

        warn!(
            "Data gap detected for {}/{}: {:.0}s (category: {})",
            feed_name, network, gap_duration_seconds, duration_category
        );
    }

    /// Update feed reliability score
    pub fn update_reliability_score(
        feed_name: &str,
        network: &str,
        uptime_percent: f64,
        accuracy_percent: f64,
        consistency_percent: f64,
    ) {
        // Weighted average: uptime 40%, accuracy 40%, consistency 20%
        let score = (uptime_percent * 0.4 + accuracy_percent * 0.4 + consistency_percent * 0.2)
            .clamp(0.0, 100.0);

        FEED_RELIABILITY_SCORE
            .with_label_values(&[feed_name, network])
            .set(score);

        debug!(
            "Feed reliability for {}/{}: {:.1} (uptime: {:.1}%, accuracy: {:.1}%, consistency: {:.1}%)",
            feed_name, network, score, uptime_percent, accuracy_percent, consistency_percent
        );
    }

    /// Record timestamp drift
    pub fn record_timestamp_drift(
        feed_name: &str,
        network: &str,
        feed_timestamp: u64,
        system_timestamp: u64,
    ) {
        let drift_seconds = (feed_timestamp as i64 - system_timestamp as i64).abs() as f64;

        TIMESTAMP_DRIFT_SECONDS
            .with_label_values(&[feed_name, network])
            .observe(drift_seconds);

        if drift_seconds > 300.0 {
            warn!(
                "Large timestamp drift for {}/{}: {:.0}s",
                feed_name, network, drift_seconds
            );
        }
    }
}
