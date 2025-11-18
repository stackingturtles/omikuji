use crate::config::metrics_config::{MetricCategory, MetricsConfig};
use once_cell::sync::OnceCell;
use std::sync::Arc;

/// Global metrics configuration
static METRICS_CONFIG: OnceCell<Arc<MetricsConfig>> = OnceCell::new();

/// Initialize the global metrics configuration
pub fn init_metrics_config(config: MetricsConfig) {
    METRICS_CONFIG
        .set(Arc::new(config))
        .expect("Metrics config already initialized");
}

/// Check if a metric category is enabled
pub fn is_metric_enabled(category: MetricCategory) -> bool {
    METRICS_CONFIG
        .get()
        .map(|config| config.is_category_enabled(category))
        .unwrap_or(true) // Default to enabled if not initialized
}

/// Get the metrics configuration
pub fn get_metrics_config() -> Option<Arc<MetricsConfig>> {
    METRICS_CONFIG.get().cloned()
}
