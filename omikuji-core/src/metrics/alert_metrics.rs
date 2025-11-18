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
