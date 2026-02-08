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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_value_change_rate_normal() {
        QualityMetrics::record_value_change_rate("test_feed_1", "ethereum", 100.0, 105.0, 60.0);
        // Should calculate: ((105 - 100).abs() / 100) * 100 / 60 * 60 = 5% per minute
    }

    #[test]
    fn test_record_value_change_rate_high_rate() {
        QualityMetrics::record_value_change_rate("test_feed_2", "ethereum", 100.0, 120.0, 10.0);
        // Should calculate: ((120 - 100).abs() / 100) * 100 / 10 * 60 = 120% per minute (triggers warning)
    }

    #[test]
    fn test_record_value_change_rate_zero_previous() {
        // Should be guarded and do nothing
        QualityMetrics::record_value_change_rate("test_feed_3", "ethereum", 0.0, 100.0, 60.0);
    }

    #[test]
    fn test_record_value_change_rate_zero_time_delta() {
        // Should be guarded and do nothing
        QualityMetrics::record_value_change_rate("test_feed_4", "ethereum", 100.0, 105.0, 0.0);
    }

    #[test]
    fn test_record_value_change_rate_at_threshold() {
        QualityMetrics::record_value_change_rate("test_feed_5", "ethereum", 100.0, 110.0, 60.0);
        // Should calculate: 10% per minute (at threshold)
    }

    #[test]
    fn test_record_outlier_too_low() {
        QualityMetrics::record_outlier("test_feed_6", "ethereum", 50.0, (100.0, 200.0), "rejected");
        // Should classify as "too_low"
    }

    #[test]
    fn test_record_outlier_too_high() {
        QualityMetrics::record_outlier("test_feed_7", "ethereum", 250.0, (100.0, 200.0), "capped");
        // Should classify as "too_high"
    }

    #[test]
    fn test_record_outlier_anomaly() {
        QualityMetrics::record_outlier("test_feed_8", "ethereum", 150.0, (100.0, 200.0), "flagged");
        // Should classify as "anomaly" (within range but still flagged)
    }

    #[test]
    fn test_record_outlier_at_boundaries() {
        QualityMetrics::record_outlier("test_feed_9", "ethereum", 100.0, (100.0, 200.0), "none");
        // At lower boundary, should be "anomaly"

        QualityMetrics::record_outlier("test_feed_10", "ethereum", 200.0, (100.0, 200.0), "none");
        // At upper boundary, should be "anomaly"
    }

    #[test]
    fn test_update_consistency_score_normal() {
        QualityMetrics::update_consistency_score("test_feed_11", "ethereum", 75.0);
        // Normal score, no warning
    }

    #[test]
    fn test_update_consistency_score_low() {
        QualityMetrics::update_consistency_score("test_feed_12", "ethereum", 45.0);
        // Below 50, should trigger warning
    }

    #[test]
    fn test_update_consistency_score_clamping_high() {
        QualityMetrics::update_consistency_score("test_feed_13", "ethereum", 150.0);
        // Should be clamped to 100
    }

    #[test]
    fn test_update_consistency_score_clamping_low() {
        QualityMetrics::update_consistency_score("test_feed_14", "ethereum", -50.0);
        // Should be clamped to 0
    }

    #[test]
    fn test_update_consistency_score_at_threshold() {
        QualityMetrics::update_consistency_score("test_feed_15", "ethereum", 50.0);
        // At threshold (50), should not trigger warning
    }

    #[test]
    fn test_update_stale_data_duration_normal() {
        QualityMetrics::update_stale_data_duration(
            "test_feed_16",
            "ethereum",
            "no_updates",
            1800.0,
        );
        // 30 minutes, should not trigger error
    }

    #[test]
    fn test_update_stale_data_duration_critical() {
        QualityMetrics::update_stale_data_duration(
            "test_feed_17",
            "ethereum",
            "no_updates",
            7200.0,
        );
        // 2 hours, should trigger error
    }

    #[test]
    fn test_update_stale_data_duration_at_threshold() {
        QualityMetrics::update_stale_data_duration(
            "test_feed_18",
            "ethereum",
            "no_updates",
            3600.0,
        );
        // Exactly 1 hour, should not trigger error (boundary case)
    }

    #[test]
    fn test_update_stale_data_duration_just_over_threshold() {
        QualityMetrics::update_stale_data_duration(
            "test_feed_19",
            "ethereum",
            "no_updates",
            3601.0,
        );
        // Just over 1 hour, should trigger error
    }

    #[test]
    fn test_record_ma_deviation_normal() {
        QualityMetrics::record_ma_deviation("test_feed_20", "ethereum", 105.0, 100.0, "5m");
        // 5% deviation, should not trigger warning
    }

    #[test]
    fn test_record_ma_deviation_high() {
        QualityMetrics::record_ma_deviation("test_feed_21", "ethereum", 130.0, 100.0, "5m");
        // 30% deviation, should trigger warning
    }

    #[test]
    fn test_record_ma_deviation_zero_ma() {
        // Should be guarded and do nothing
        QualityMetrics::record_ma_deviation("test_feed_22", "ethereum", 100.0, 0.0, "5m");
    }

    #[test]
    fn test_record_ma_deviation_at_threshold() {
        QualityMetrics::record_ma_deviation("test_feed_23", "ethereum", 120.0, 100.0, "5m");
        // 20% deviation, at threshold
    }

    #[test]
    fn test_record_ma_deviation_negative() {
        QualityMetrics::record_ma_deviation("test_feed_24", "ethereum", 75.0, 100.0, "5m");
        // 25% negative deviation, should trigger warning
    }

    #[test]
    fn test_update_source_agreement_high() {
        QualityMetrics::update_source_agreement("test_feed_25", "ethereum", 95.0, 3);
        // High agreement, no warning
    }

    #[test]
    fn test_update_source_agreement_low_multiple_sources() {
        QualityMetrics::update_source_agreement("test_feed_26", "ethereum", 70.0, 3);
        // Low agreement with multiple sources, should trigger warning
    }

    #[test]
    fn test_update_source_agreement_low_single_source() {
        QualityMetrics::update_source_agreement("test_feed_27", "ethereum", 70.0, 1);
        // Low agreement but only 1 source, should not trigger warning
    }

    #[test]
    fn test_update_source_agreement_at_threshold() {
        QualityMetrics::update_source_agreement("test_feed_28", "ethereum", 80.0, 2);
        // At 80% threshold, should not trigger warning
    }

    #[test]
    fn test_update_source_agreement_clamping() {
        QualityMetrics::update_source_agreement("test_feed_29", "ethereum", 150.0, 3);
        // Should be clamped to 100

        QualityMetrics::update_source_agreement("test_feed_30", "ethereum", -50.0, 3);
        // Should be clamped to 0
    }

    #[test]
    fn test_record_invalid_value() {
        QualityMetrics::record_invalid_value(
            "test_feed_31",
            "ethereum",
            "range_check",
            -100.0,
            "negative value",
        );
        // Should increment counter
    }

    #[test]
    fn test_record_invalid_value_different_types() {
        QualityMetrics::record_invalid_value(
            "test_feed_32",
            "ethereum",
            "type_check",
            0.0,
            "null value",
        );

        QualityMetrics::record_invalid_value(
            "test_feed_33",
            "ethereum",
            "format_check",
            999.999,
            "invalid format",
        );
    }

    #[test]
    fn test_record_data_gap_short() {
        QualityMetrics::record_data_gap("test_feed_34", "ethereum", 120.0);
        // 2 minutes, should be "short"
    }

    #[test]
    fn test_record_data_gap_medium() {
        QualityMetrics::record_data_gap("test_feed_35", "ethereum", 900.0);
        // 15 minutes, should be "medium"
    }

    #[test]
    fn test_record_data_gap_long() {
        QualityMetrics::record_data_gap("test_feed_36", "ethereum", 2400.0);
        // 40 minutes, should be "long"
    }

    #[test]
    fn test_record_data_gap_critical() {
        QualityMetrics::record_data_gap("test_feed_37", "ethereum", 7200.0);
        // 2 hours, should be "critical"
    }

    #[test]
    fn test_record_data_gap_boundaries() {
        QualityMetrics::record_data_gap("test_feed_38", "ethereum", 299.9);
        // Just under 300, should be "short"

        QualityMetrics::record_data_gap("test_feed_39", "ethereum", 300.0);
        // Exactly 300, should be "medium"

        QualityMetrics::record_data_gap("test_feed_40", "ethereum", 1799.9);
        // Just under 1800, should be "medium"

        QualityMetrics::record_data_gap("test_feed_41", "ethereum", 1800.0);
        // Exactly 1800, should be "long"

        QualityMetrics::record_data_gap("test_feed_42", "ethereum", 3599.9);
        // Just under 3600, should be "long"

        QualityMetrics::record_data_gap("test_feed_43", "ethereum", 3600.0);
        // Exactly 3600, should be "critical"
    }

    #[test]
    fn test_update_reliability_score_normal() {
        QualityMetrics::update_reliability_score("test_feed_44", "ethereum", 95.0, 90.0, 85.0);
        // Weighted: 95*0.4 + 90*0.4 + 85*0.2 = 38 + 36 + 17 = 91
    }

    #[test]
    fn test_update_reliability_score_perfect() {
        QualityMetrics::update_reliability_score("test_feed_45", "ethereum", 100.0, 100.0, 100.0);
        // Should be 100
    }

    #[test]
    fn test_update_reliability_score_poor() {
        QualityMetrics::update_reliability_score("test_feed_46", "ethereum", 50.0, 50.0, 50.0);
        // Should be 50
    }

    #[test]
    fn test_update_reliability_score_clamping() {
        QualityMetrics::update_reliability_score("test_feed_47", "ethereum", 150.0, 150.0, 150.0);
        // Should be clamped to 100

        QualityMetrics::update_reliability_score("test_feed_48", "ethereum", -50.0, -50.0, -50.0);
        // Should be clamped to 0
    }

    #[test]
    fn test_update_reliability_score_weighted_components() {
        QualityMetrics::update_reliability_score("test_feed_49", "ethereum", 100.0, 0.0, 0.0);
        // Uptime only: 100*0.4 = 40

        QualityMetrics::update_reliability_score("test_feed_50", "ethereum", 0.0, 100.0, 0.0);
        // Accuracy only: 100*0.4 = 40

        QualityMetrics::update_reliability_score("test_feed_51", "ethereum", 0.0, 0.0, 100.0);
        // Consistency only: 100*0.2 = 20
    }

    #[test]
    fn test_record_timestamp_drift_small() {
        QualityMetrics::record_timestamp_drift("test_feed_52", "ethereum", 1000, 1050);
        // 50 seconds drift, should not trigger warning
    }

    #[test]
    fn test_record_timestamp_drift_large() {
        QualityMetrics::record_timestamp_drift("test_feed_53", "ethereum", 1000, 1600);
        // 600 seconds drift, should trigger warning
    }

    #[test]
    fn test_record_timestamp_drift_negative() {
        QualityMetrics::record_timestamp_drift("test_feed_54", "ethereum", 1600, 1000);
        // -600 seconds drift (absolute value 600), should trigger warning
    }

    #[test]
    fn test_record_timestamp_drift_at_threshold() {
        QualityMetrics::record_timestamp_drift("test_feed_55", "ethereum", 1000, 1300);
        // 300 seconds drift, at threshold (should not trigger warning)
    }

    #[test]
    fn test_record_timestamp_drift_over_threshold() {
        QualityMetrics::record_timestamp_drift("test_feed_56", "ethereum", 1000, 1301);
        // 301 seconds drift, should trigger warning
    }

    #[test]
    fn test_record_timestamp_drift_zero() {
        QualityMetrics::record_timestamp_drift("test_feed_57", "ethereum", 1000, 1000);
        // Zero drift
    }
}
