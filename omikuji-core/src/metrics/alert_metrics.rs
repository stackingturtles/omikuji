use lazy_static::lazy_static;
use prometheus::{register_counter_vec, register_gauge_vec, CounterVec, GaugeVec};
use tracing::{error, warn};

lazy_static! {
    /// Critical errors counter
    static ref CRITICAL_ERRORS: CounterVec = register_counter_vec!(
        "omikuji_critical_errors_total",
        "Total number of critical errors",
        &["error_type", "component", "network"]
    ).expect("Failed to create critical_errors metric");

    /// Feed update lag alert
    static ref FEED_UPDATE_LAG_ALERT: GaugeVec = register_gauge_vec!(
        "omikuji_feed_update_lag_alert",
        "Feed update lag alert status (1 = alert, 0 = ok)",
        &["feed_name", "network", "severity"]
    ).expect("Failed to create feed_update_lag_alert metric");

    /// Transaction retry exhaustion
    static ref TRANSACTION_RETRY_EXHAUSTED: CounterVec = register_counter_vec!(
        "omikuji_transaction_retry_exhausted_total",
        "Total number of exhausted transaction retries",
        &["feed_name", "network", "final_error"]
    ).expect("Failed to create transaction_retry_exhausted metric");

    /// System health score
    static ref SYSTEM_HEALTH_SCORE: GaugeVec = register_gauge_vec!(
        "omikuji_system_health_score",
        "Overall system health score (0-100)",
        &["component"]
    ).expect("Failed to create system_health_score metric");

    /// Alert fatigue prevention
    static ref ALERT_SUPPRESSION_ACTIVE: GaugeVec = register_gauge_vec!(
        "omikuji_alert_suppression_active",
        "Alert suppression status (1 = suppressed, 0 = normal)",
        &["alert_type", "reason"]
    ).expect("Failed to create alert_suppression_active metric");

    /// Cascading failure detection
    static ref CASCADING_FAILURE_RISK: GaugeVec = register_gauge_vec!(
        "omikuji_cascading_failure_risk",
        "Risk of cascading failure (0-100)",
        &["network", "risk_factor"]
    ).expect("Failed to create cascading_failure_risk metric");

    /// Emergency shutdown triggers
    static ref EMERGENCY_SHUTDOWN_TRIGGERED: CounterVec = register_counter_vec!(
        "omikuji_emergency_shutdown_triggered_total",
        "Total number of emergency shutdown triggers",
        &["component", "reason"]
    ).expect("Failed to create emergency_shutdown_triggered metric");

    /// Degraded mode active
    static ref DEGRADED_MODE_ACTIVE: GaugeVec = register_gauge_vec!(
        "omikuji_degraded_mode_active",
        "Degraded mode status (1 = degraded, 0 = normal)",
        &["component", "degradation_type"]
    ).expect("Failed to create degraded_mode_active metric");

    /// SLA violation
    static ref SLA_VIOLATION: CounterVec = register_counter_vec!(
        "omikuji_sla_violations_total",
        "Total number of SLA violations",
        &["feed_name", "network", "sla_type"]
    ).expect("Failed to create sla_violation metric");

    /// Alert queue depth
    static ref ALERT_QUEUE_DEPTH: GaugeVec = register_gauge_vec!(
        "omikuji_alert_queue_depth",
        "Number of pending alerts in queue",
        &["severity", "destination"]
    ).expect("Failed to create alert_queue_depth metric");
}

/// Alert metrics collector
pub struct AlertMetrics;

impl AlertMetrics {
    /// Record a critical error
    pub fn record_critical_error(
        error_type: &str,
        component: &str,
        network: &str,
        error_message: &str,
    ) {
        CRITICAL_ERRORS
            .with_label_values(&[error_type, component, network])
            .inc();

        error!(
            "CRITICAL ERROR in {} on {}: {} - {}",
            component, network, error_type, error_message
        );
    }

    /// Update feed update lag alert
    pub fn update_feed_lag_alert(
        feed_name: &str,
        network: &str,
        lag_seconds: f64,
        threshold_seconds: f64,
    ) {
        let (alert_status, severity) = if lag_seconds > threshold_seconds * 3.0 {
            (1.0, "critical")
        } else if lag_seconds > threshold_seconds * 2.0 {
            (1.0, "high")
        } else if lag_seconds > threshold_seconds {
            (1.0, "medium")
        } else {
            (0.0, "ok")
        };

        FEED_UPDATE_LAG_ALERT
            .with_label_values(&[feed_name, network, severity])
            .set(alert_status);

        if alert_status > 0.0 {
            error!(
                "Feed update lag alert for {}/{}: {:.0}s lag (threshold: {:.0}s, severity: {})",
                feed_name, network, lag_seconds, threshold_seconds, severity
            );
        }
    }

    /// Record transaction retry exhaustion
    pub fn record_retry_exhaustion(
        feed_name: &str,
        network: &str,
        final_error: &str,
        attempts: u32,
    ) {
        TRANSACTION_RETRY_EXHAUSTED
            .with_label_values(&[feed_name, network, final_error])
            .inc();

        error!(
            "Transaction retry exhausted for {}/{} after {} attempts: {}",
            feed_name, network, attempts, final_error
        );
    }

    /// Update system health score
    pub fn update_system_health(
        component: &str,
        availability: f64,
        performance: f64,
        error_rate: f64,
    ) {
        // Calculate health score (higher is better)
        // availability: 0-100%, performance: 0-100%, error_rate: 0-100% (inverted)
        let health_score =
            (availability * 0.4 + performance * 0.3 + (100.0 - error_rate) * 0.3).clamp(0.0, 100.0);

        SYSTEM_HEALTH_SCORE
            .with_label_values(&[component])
            .set(health_score);

        if health_score < 50.0 {
            error!(
                "Low system health for {}: {:.1} (availability: {:.1}%, performance: {:.1}%, error_rate: {:.1}%)",
                component, health_score, availability, performance, error_rate
            );
        }
    }

    /// Update alert suppression status
    pub fn update_alert_suppression(alert_type: &str, is_suppressed: bool, reason: &str) {
        ALERT_SUPPRESSION_ACTIVE
            .with_label_values(&[alert_type, reason])
            .set(if is_suppressed { 1.0 } else { 0.0 });

        if is_suppressed {
            warn!("Alert suppression active for {}: {}", alert_type, reason);
        }
    }

    /// Update cascading failure risk
    pub fn update_cascading_failure_risk(
        network: &str,
        error_rate: f64,
        dependency_failures: f64,
        resource_exhaustion: f64,
    ) {
        // Calculate risk based on multiple factors
        let risk_score = (error_rate * 0.4 + dependency_failures * 0.4 + resource_exhaustion * 0.2)
            .clamp(0.0, 100.0);

        CASCADING_FAILURE_RISK
            .with_label_values(&[network, "overall"])
            .set(risk_score);

        CASCADING_FAILURE_RISK
            .with_label_values(&[network, "error_rate"])
            .set(error_rate);

        CASCADING_FAILURE_RISK
            .with_label_values(&[network, "dependencies"])
            .set(dependency_failures);

        CASCADING_FAILURE_RISK
            .with_label_values(&[network, "resources"])
            .set(resource_exhaustion);

        if risk_score > 70.0 {
            error!(
                "High cascading failure risk for {}: {:.1}% (errors: {:.1}%, deps: {:.1}%, resources: {:.1}%)",
                network, risk_score, error_rate, dependency_failures, resource_exhaustion
            );
        }
    }

    /// Record emergency shutdown
    pub fn record_emergency_shutdown(component: &str, reason: &str) {
        EMERGENCY_SHUTDOWN_TRIGGERED
            .with_label_values(&[component, reason])
            .inc();

        error!("EMERGENCY SHUTDOWN triggered for {}: {}", component, reason);
    }

    /// Update degraded mode status
    pub fn update_degraded_mode(component: &str, is_degraded: bool, degradation_type: &str) {
        DEGRADED_MODE_ACTIVE
            .with_label_values(&[component, degradation_type])
            .set(if is_degraded { 1.0 } else { 0.0 });

        if is_degraded {
            warn!(
                "Component {} operating in degraded mode: {}",
                component, degradation_type
            );
        }
    }

    /// Record SLA violation
    pub fn record_sla_violation(
        feed_name: &str,
        network: &str,
        sla_type: &str,
        actual_value: f64,
        sla_target: f64,
    ) {
        SLA_VIOLATION
            .with_label_values(&[feed_name, network, sla_type])
            .inc();

        error!(
            "SLA violation for {}/{} - {}: actual={:.2}, target={:.2}",
            feed_name, network, sla_type, actual_value, sla_target
        );
    }

    /// Update alert queue depth
    pub fn update_alert_queue(
        critical: usize,
        high: usize,
        medium: usize,
        low: usize,
        destination: &str,
    ) {
        ALERT_QUEUE_DEPTH
            .with_label_values(&["critical", destination])
            .set(critical as f64);

        ALERT_QUEUE_DEPTH
            .with_label_values(&["high", destination])
            .set(high as f64);

        ALERT_QUEUE_DEPTH
            .with_label_values(&["medium", destination])
            .set(medium as f64);

        ALERT_QUEUE_DEPTH
            .with_label_values(&["low", destination])
            .set(low as f64);

        let total = critical + high + medium + low;
        if total > 100 {
            warn!(
                "Large alert queue for {}: {} total (critical: {}, high: {}, medium: {}, low: {})",
                destination, total, critical, high, medium, low
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_critical_error() {
        AlertMetrics::record_critical_error(
            "connection_timeout",
            "feed_fetcher_1",
            "ethereum_mainnet_1",
            "Connection to upstream failed",
        );

        let metric = CRITICAL_ERRORS
            .get_metric_with_label_values(&[
                "connection_timeout",
                "feed_fetcher_1",
                "ethereum_mainnet_1",
            ])
            .expect("Metric should exist");
        assert!(metric.get() >= 1.0, "Counter should be incremented");
    }

    #[test]
    fn test_feed_lag_alert_ok() {
        AlertMetrics::update_feed_lag_alert("btc_usd_feed_1", "ethereum_mainnet_2", 50.0, 100.0);

        let metric = FEED_UPDATE_LAG_ALERT
            .get_metric_with_label_values(&["btc_usd_feed_1", "ethereum_mainnet_2", "ok"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 0.0, "Alert status should be 0.0 for ok");
    }

    #[test]
    fn test_feed_lag_alert_medium_boundary() {
        // Exactly at threshold boundary
        AlertMetrics::update_feed_lag_alert("eth_usd_feed_1", "polygon_mainnet_1", 100.1, 100.0);

        let metric = FEED_UPDATE_LAG_ALERT
            .get_metric_with_label_values(&["eth_usd_feed_1", "polygon_mainnet_1", "medium"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 1.0, "Alert status should be 1.0 for medium");
    }

    #[test]
    fn test_feed_lag_alert_high_boundary() {
        // Just above 2x threshold
        AlertMetrics::update_feed_lag_alert("sol_usd_feed_1", "solana_mainnet_1", 200.1, 100.0);

        let metric = FEED_UPDATE_LAG_ALERT
            .get_metric_with_label_values(&["sol_usd_feed_1", "solana_mainnet_1", "high"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 1.0, "Alert status should be 1.0 for high");
    }

    #[test]
    fn test_feed_lag_alert_critical_boundary() {
        // Just above 3x threshold
        AlertMetrics::update_feed_lag_alert("avax_usd_feed_1", "avalanche_mainnet_1", 300.1, 100.0);

        let metric = FEED_UPDATE_LAG_ALERT
            .get_metric_with_label_values(&["avax_usd_feed_1", "avalanche_mainnet_1", "critical"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 1.0, "Alert status should be 1.0 for critical");
    }

    #[test]
    fn test_record_retry_exhaustion() {
        AlertMetrics::record_retry_exhaustion(
            "link_usd_feed_1",
            "arbitrum_mainnet_1",
            "nonce_too_low",
            5,
        );

        let metric = TRANSACTION_RETRY_EXHAUSTED
            .get_metric_with_label_values(&[
                "link_usd_feed_1",
                "arbitrum_mainnet_1",
                "nonce_too_low",
            ])
            .expect("Metric should exist");
        assert!(metric.get() >= 1.0, "Counter should be incremented");
    }

    #[test]
    fn test_system_health_perfect_score() {
        AlertMetrics::update_system_health("oracle_service_1", 100.0, 100.0, 0.0);

        let metric = SYSTEM_HEALTH_SCORE
            .get_metric_with_label_values(&["oracle_service_1"])
            .expect("Metric should exist");

        // 100*0.4 + 100*0.3 + (100-0)*0.3 = 40 + 30 + 30 = 100
        assert_eq!(metric.get(), 100.0, "Perfect scores should yield 100.0");
    }

    #[test]
    fn test_system_health_worst_score() {
        AlertMetrics::update_system_health("oracle_service_2", 0.0, 0.0, 100.0);

        let metric = SYSTEM_HEALTH_SCORE
            .get_metric_with_label_values(&["oracle_service_2"])
            .expect("Metric should exist");

        // 0*0.4 + 0*0.3 + (100-100)*0.3 = 0 + 0 + 0 = 0
        assert_eq!(metric.get(), 0.0, "Worst scores should yield 0.0");
    }

    #[test]
    fn test_system_health_mixed_score() {
        AlertMetrics::update_system_health("oracle_service_3", 80.0, 60.0, 10.0);

        let metric = SYSTEM_HEALTH_SCORE
            .get_metric_with_label_values(&["oracle_service_3"])
            .expect("Metric should exist");

        // 80*0.4 + 60*0.3 + (100-10)*0.3 = 32 + 18 + 27 = 77
        assert_eq!(
            metric.get(),
            77.0,
            "Mixed scores should calculate correctly"
        );
    }

    #[test]
    fn test_system_health_low_score_threshold() {
        AlertMetrics::update_system_health("oracle_service_4", 50.0, 40.0, 50.0);

        let metric = SYSTEM_HEALTH_SCORE
            .get_metric_with_label_values(&["oracle_service_4"])
            .expect("Metric should exist");

        // 50*0.4 + 40*0.3 + (100-50)*0.3 = 20 + 12 + 15 = 47
        assert_eq!(
            metric.get(),
            47.0,
            "Low health score should trigger error log"
        );
    }

    #[test]
    fn test_alert_suppression_enabled() {
        AlertMetrics::update_alert_suppression("feed_lag_alert_1", true, "maintenance_window");

        let metric = ALERT_SUPPRESSION_ACTIVE
            .get_metric_with_label_values(&["feed_lag_alert_1", "maintenance_window"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 1.0, "Suppression should be 1.0 when enabled");
    }

    #[test]
    fn test_alert_suppression_disabled() {
        AlertMetrics::update_alert_suppression("feed_lag_alert_2", false, "normal_operation");

        let metric = ALERT_SUPPRESSION_ACTIVE
            .get_metric_with_label_values(&["feed_lag_alert_2", "normal_operation"])
            .expect("Metric should exist");
        assert_eq!(metric.get(), 0.0, "Suppression should be 0.0 when disabled");
    }

    #[test]
    fn test_cascading_failure_risk_low() {
        AlertMetrics::update_cascading_failure_risk("optimism_mainnet_1", 10.0, 5.0, 2.0);

        let overall = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["optimism_mainnet_1", "overall"])
            .expect("Overall metric should exist");

        // 10*0.4 + 5*0.4 + 2*0.2 = 4 + 2 + 0.4 = 6.4
        assert_eq!(overall.get(), 6.4, "Low risk should calculate correctly");
    }

    #[test]
    fn test_cascading_failure_risk_high() {
        AlertMetrics::update_cascading_failure_risk("base_mainnet_1", 80.0, 90.0, 60.0);

        let overall = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["base_mainnet_1", "overall"])
            .expect("Overall metric should exist");

        // 80*0.4 + 90*0.4 + 60*0.2 = 32 + 36 + 12 = 80
        assert_eq!(overall.get(), 80.0, "High risk should calculate correctly");

        let error_rate = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["base_mainnet_1", "error_rate"])
            .expect("Error rate metric should exist");
        assert_eq!(error_rate.get(), 80.0);

        let dependencies = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["base_mainnet_1", "dependencies"])
            .expect("Dependencies metric should exist");
        assert_eq!(dependencies.get(), 90.0);

        let resources = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["base_mainnet_1", "resources"])
            .expect("Resources metric should exist");
        assert_eq!(resources.get(), 60.0);
    }

    #[test]
    fn test_cascading_failure_risk_boundary_70() {
        AlertMetrics::update_cascading_failure_risk("zksync_mainnet_1", 75.0, 75.0, 50.0);

        let overall = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["zksync_mainnet_1", "overall"])
            .expect("Overall metric should exist");

        // 75*0.4 + 75*0.4 + 50*0.2 = 30 + 30 + 10 = 70
        assert_eq!(
            overall.get(),
            70.0,
            "Boundary at 70.0 should calculate correctly"
        );
    }

    #[test]
    fn test_cascading_failure_risk_above_threshold() {
        AlertMetrics::update_cascading_failure_risk("scroll_mainnet_1", 85.0, 80.0, 75.0);

        let overall = CASCADING_FAILURE_RISK
            .get_metric_with_label_values(&["scroll_mainnet_1", "overall"])
            .expect("Overall metric should exist");

        // 85*0.4 + 80*0.4 + 75*0.2 = 34 + 32 + 15 = 81
        assert_eq!(
            overall.get(),
            81.0,
            "Risk above 70 should trigger error log"
        );
    }

    #[test]
    fn test_record_emergency_shutdown() {
        AlertMetrics::record_emergency_shutdown("price_updater_1", "memory_exhaustion");

        let metric = EMERGENCY_SHUTDOWN_TRIGGERED
            .get_metric_with_label_values(&["price_updater_1", "memory_exhaustion"])
            .expect("Metric should exist");
        assert!(metric.get() >= 1.0, "Counter should be incremented");
    }

    #[test]
    fn test_degraded_mode_enabled() {
        AlertMetrics::update_degraded_mode("tx_submitter_1", true, "rate_limited");

        let metric = DEGRADED_MODE_ACTIVE
            .get_metric_with_label_values(&["tx_submitter_1", "rate_limited"])
            .expect("Metric should exist");
        assert_eq!(
            metric.get(),
            1.0,
            "Degraded mode should be 1.0 when enabled"
        );
    }

    #[test]
    fn test_degraded_mode_disabled() {
        AlertMetrics::update_degraded_mode("tx_submitter_2", false, "normal");

        let metric = DEGRADED_MODE_ACTIVE
            .get_metric_with_label_values(&["tx_submitter_2", "normal"])
            .expect("Metric should exist");
        assert_eq!(
            metric.get(),
            0.0,
            "Degraded mode should be 0.0 when disabled"
        );
    }

    #[test]
    fn test_record_sla_violation() {
        AlertMetrics::record_sla_violation(
            "matic_usd_feed_1",
            "polygon_mainnet_2",
            "latency",
            150.0,
            100.0,
        );

        let metric = SLA_VIOLATION
            .get_metric_with_label_values(&["matic_usd_feed_1", "polygon_mainnet_2", "latency"])
            .expect("Metric should exist");
        assert!(metric.get() >= 1.0, "Counter should be incremented");
    }

    #[test]
    fn test_alert_queue_normal() {
        AlertMetrics::update_alert_queue(5, 10, 20, 30, "pagerduty_1");

        let critical = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["critical", "pagerduty_1"])
            .expect("Critical metric should exist");
        assert_eq!(critical.get(), 5.0);

        let high = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["high", "pagerduty_1"])
            .expect("High metric should exist");
        assert_eq!(high.get(), 10.0);

        let medium = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["medium", "pagerduty_1"])
            .expect("Medium metric should exist");
        assert_eq!(medium.get(), 20.0);

        let low = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["low", "pagerduty_1"])
            .expect("Low metric should exist");
        assert_eq!(low.get(), 30.0);
    }

    #[test]
    fn test_alert_queue_large() {
        AlertMetrics::update_alert_queue(30, 40, 50, 60, "slack_channel_1");

        let critical = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["critical", "slack_channel_1"])
            .expect("Critical metric should exist");
        assert_eq!(critical.get(), 30.0);

        let high = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["high", "slack_channel_1"])
            .expect("High metric should exist");
        assert_eq!(high.get(), 40.0);

        let medium = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["medium", "slack_channel_1"])
            .expect("Medium metric should exist");
        assert_eq!(medium.get(), 50.0);

        let low = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["low", "slack_channel_1"])
            .expect("Low metric should exist");
        assert_eq!(low.get(), 60.0);

        // Total is 180, which should trigger warning
    }

    #[test]
    fn test_alert_queue_boundary_100() {
        AlertMetrics::update_alert_queue(25, 25, 25, 25, "email_destination_1");

        let critical = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["critical", "email_destination_1"])
            .expect("Critical metric should exist");
        assert_eq!(critical.get(), 25.0);

        // Total is exactly 100, should not trigger warning
    }

    #[test]
    fn test_alert_queue_boundary_101() {
        AlertMetrics::update_alert_queue(26, 25, 25, 25, "webhook_destination_1");

        let critical = ALERT_QUEUE_DEPTH
            .get_metric_with_label_values(&["critical", "webhook_destination_1"])
            .expect("Critical metric should exist");
        assert_eq!(critical.get(), 26.0);

        // Total is 101, should trigger warning
    }
}
