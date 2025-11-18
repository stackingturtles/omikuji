use crate::config::metrics_config::MetricCategory;
use crate::metrics::config_manager::is_metric_enabled;
use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use std::time::Duration;
use tracing::{debug, warn};

lazy_static! {
    /// HTTP request count by feed, network and status
    static ref HTTP_REQUEST_COUNT: CounterVec = register_counter_vec!(
        "omikuji_datasource_http_requests_total",
        "Total number of HTTP requests to data sources",
        &["feed_name", "network", "status", "method"]
    ).expect("Failed to create http_request_count metric");

    /// HTTP request duration histogram
    static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_datasource_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["feed_name", "network", "status"],
        // Buckets: 10ms to 30s
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]
    ).expect("Failed to create http_request_duration metric");

    /// Data source availability (1 = available, 0 = unavailable)
    static ref DATASOURCE_AVAILABILITY: GaugeVec = register_gauge_vec!(
        "omikuji_datasource_availability",
        "Data source availability status (1 = available, 0 = unavailable)",
        &["feed_name", "network", "url"]
    ).expect("Failed to create datasource_availability metric");

    /// Consecutive error count for data sources
    static ref DATASOURCE_ERROR_COUNT: GaugeVec = register_gauge_vec!(
        "omikuji_datasource_consecutive_errors",
        "Number of consecutive errors from data source",
        &["feed_name", "network", "error_type"]
    ).expect("Failed to create datasource_error_count metric");

    /// Response size histogram in bytes
    static ref HTTP_RESPONSE_SIZE_BYTES: HistogramVec = register_histogram_vec!(
        "omikuji_datasource_http_response_size_bytes",
        "HTTP response size in bytes",
        &["feed_name", "network"],
        // Buckets: 100B to 10MB
        vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 10000000.0]
    ).expect("Failed to create http_response_size metric");

    /// Data source latency percentiles
    static ref DATASOURCE_LATENCY_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_datasource_latency_seconds",
        "Data source latency including parsing",
        &["feed_name", "network", "operation"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]
    ).expect("Failed to create datasource_latency metric");

    /// Parse errors counter
    static ref PARSE_ERROR_COUNT: CounterVec = register_counter_vec!(
        "omikuji_datasource_parse_errors_total",
        "Total number of parsing errors",
        &["feed_name", "network", "error_type"]
    ).expect("Failed to create parse_error_count metric");

    /// Rate limit hits counter
    static ref RATE_LIMIT_COUNT: CounterVec = register_counter_vec!(
        "omikuji_datasource_rate_limits_total",
        "Total number of rate limit responses",
        &["feed_name", "network", "url"]
    ).expect("Failed to create rate_limit_count metric");
}

/// Data source metrics collector
pub struct DatasourceMetrics;

impl DatasourceMetrics {
    /// Record an HTTP request
    pub fn record_http_request(
        feed_name: &str,
        network: &str,
        method: &str,
        url: &str,
        status_code: u16,
        duration: Duration,
        response_size: Option<usize>,
    ) {
        if !is_metric_enabled(MetricCategory::Datasource) {
            return;
        }

        let status = match status_code {
            200..=299 => "success",
            400..=499 => "client_error",
            500..=599 => "server_error",
            _ => "other",
        };

        // Increment request counter
        HTTP_REQUEST_COUNT
            .with_label_values(&[feed_name, network, status, method])
            .inc();

        // Record duration
        HTTP_REQUEST_DURATION_SECONDS
            .with_label_values(&[feed_name, network, status])
            .observe(duration.as_secs_f64());

        // Record response size if available
        if let Some(size) = response_size {
            HTTP_RESPONSE_SIZE_BYTES
                .with_label_values(&[feed_name, network])
                .observe(size as f64);
        }

        // Update availability
        let is_available = matches!(status_code, 200..=299);
        DATASOURCE_AVAILABILITY
            .with_label_values(&[feed_name, network, url])
            .set(if is_available { 1.0 } else { 0.0 });

        // Reset error count on success
        if is_available {
            DATASOURCE_ERROR_COUNT
                .with_label_values(&[feed_name, network, "http"])
                .set(0.0);
        }

        // Check for rate limiting
        if status_code == 429 {
            RATE_LIMIT_COUNT
                .with_label_values(&[feed_name, network, url])
                .inc();
            warn!(
                "Rate limit hit for {} on {} from {}",
                feed_name, network, url
            );
        }

        debug!(
            "HTTP {} request to {} for {}/{}: {} in {:.3}s ({}B)",
            method,
            url,
            feed_name,
            network,
            status_code,
            duration.as_secs_f64(),
            response_size.unwrap_or(0)
        );
    }

    /// Record a failed HTTP request
    pub fn record_http_error(
        feed_name: &str,
        network: &str,
        url: &str,
        error: &str,
        duration: Option<Duration>,
    ) {
        if !is_metric_enabled(MetricCategory::Datasource) {
            return;
        }

        // Increment error counter
        HTTP_REQUEST_COUNT
            .with_label_values(&[feed_name, network, "error", "GET"])
            .inc();

        // Record duration if available
        if let Some(dur) = duration {
            HTTP_REQUEST_DURATION_SECONDS
                .with_label_values(&[feed_name, network, "error"])
                .observe(dur.as_secs_f64());
        }

        // Update availability
        DATASOURCE_AVAILABILITY
            .with_label_values(&[feed_name, network, url])
            .set(0.0);

        // Increment consecutive error count
        let error_type = if error.contains("timeout") {
            "timeout"
        } else if error.contains("connection") {
            "connection"
        } else if error.contains("dns") {
            "dns"
        } else {
            "other"
        };

        let gauge = DATASOURCE_ERROR_COUNT.with_label_values(&[feed_name, network, error_type]);

        // Increment the current value
        let current = gauge.get();
        gauge.set(current + 1.0);

        warn!(
            "HTTP request failed for {} on {} from {}: {} (consecutive errors: {})",
            feed_name,
            network,
            url,
            error,
            current + 1.0
        );
    }

    /// Record data parsing metrics
    pub fn record_parse_operation(
        feed_name: &str,
        network: &str,
        success: bool,
        duration: Duration,
        error_type: Option<&str>,
    ) {
        if !is_metric_enabled(MetricCategory::Datasource) {
            return;
        }

        DATASOURCE_LATENCY_SECONDS
            .with_label_values(&[feed_name, network, "parse"])
            .observe(duration.as_secs_f64());

        if !success {
            let error = error_type.unwrap_or("unknown");
            PARSE_ERROR_COUNT
                .with_label_values(&[feed_name, network, error])
                .inc();

            warn!(
                "Failed to parse data for {} on {}: {}",
                feed_name, network, error
            );
        }
    }

    /// Record total datasource operation (fetch + parse)
    pub fn record_datasource_operation(
        feed_name: &str,
        network: &str,
        success: bool,
        total_duration: Duration,
    ) {
        if !is_metric_enabled(MetricCategory::Datasource) {
            return;
        }

        let operation = if success { "success" } else { "failure" };

        DATASOURCE_LATENCY_SECONDS
            .with_label_values(&[feed_name, network, operation])
            .observe(total_duration.as_secs_f64());

        debug!(
            "Data source operation for {}/{} completed in {:.3}s: {}",
            feed_name,
            network,
            total_duration.as_secs_f64(),
            operation
        );
    }
}
