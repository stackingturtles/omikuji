//! Monitoring trait for metrics and alerting
//!
//! This trait enables different monitoring implementations,
//! from basic Prometheus metrics to enterprise monitoring systems.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for monitoring providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MonitoringProviderConfig {
    /// Standard Prometheus metrics
    Prometheus {
        /// Port to expose metrics on
        port: u16,
        /// Path for metrics endpoint
        path: String,
    },
    /// DataDog integration (Pro Edition)
    #[cfg(feature = "pro")]
    DataDog {
        /// DataDog API key
        api_key: String,
        /// DataDog site (e.g., "datadoghq.com", "datadoghq.eu")
        site: String,
        /// Custom tags
        tags: HashMap<String, String>,
    },
    /// New Relic integration (Pro Edition)
    #[cfg(feature = "pro")]
    NewRelic {
        /// New Relic license key
        license_key: String,
        /// Application name
        app_name: String,
    },
    /// CloudWatch integration (Pro Edition)
    #[cfg(feature = "pro")]
    CloudWatch {
        /// AWS region
        region: String,
        /// Namespace for metrics
        namespace: String,
    },
}

/// Types of metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricType {
    /// Counter - monotonically increasing value
    Counter,
    /// Gauge - value that can go up or down
    Gauge,
    /// Histogram - distribution of values
    Histogram,
    /// Summary - similar to histogram with quantiles
    Summary,
}

/// A metric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// Integer value
    Integer(i64),
    /// Floating point value
    Float(f64),
}

impl From<i64> for MetricValue {
    fn from(v: i64) -> Self {
        MetricValue::Integer(v)
    }
}

impl From<f64> for MetricValue {
    fn from(v: f64) -> Self {
        MetricValue::Float(v)
    }
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertSeverity {
    /// Informational alert
    Info,
    /// Warning alert
    Warning,
    /// Critical alert requiring immediate attention
    Critical,
    /// Emergency alert - system unusable
    Emergency,
}

/// An alert to be sent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Alert identifier
    pub id: String,
    /// Alert title
    pub title: String,
    /// Detailed alert description
    pub description: String,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Source component
    pub source: String,
    /// Additional context
    pub tags: HashMap<String, String>,
    /// When the alert was triggered
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Core trait for monitoring providers
#[async_trait]
pub trait MonitoringProvider: Send + Sync {
    /// Initialize the monitoring provider
    ///
    /// # Returns
    /// Ok(()) on successful initialization
    async fn initialize(&self) -> Result<()>;

    /// Shutdown the monitoring provider
    ///
    /// # Returns
    /// Ok(()) on clean shutdown
    async fn shutdown(&self) -> Result<()>;

    /// Record a metric value
    ///
    /// # Arguments
    /// * `name` - Metric name (e.g., "omikuji_feed_updates_total")
    /// * `value` - The metric value
    /// * `metric_type` - Type of metric
    /// * `labels` - Optional labels/tags for the metric
    ///
    /// # Returns
    /// Ok(()) on success
    async fn record_metric(
        &self,
        name: &str,
        value: MetricValue,
        metric_type: MetricType,
        labels: Option<HashMap<String, String>>,
    ) -> Result<()>;

    /// Increment a counter metric
    ///
    /// # Arguments
    /// * `name` - Counter name
    /// * `labels` - Optional labels/tags
    ///
    /// # Returns
    /// Ok(()) on success
    async fn increment_counter(
        &self,
        name: &str,
        labels: Option<HashMap<String, String>>,
    ) -> Result<()> {
        self.record_metric(name, MetricValue::Integer(1), MetricType::Counter, labels)
            .await
    }

    /// Set a gauge value
    ///
    /// # Arguments
    /// * `name` - Gauge name
    /// * `value` - The gauge value
    /// * `labels` - Optional labels/tags
    ///
    /// # Returns
    /// Ok(()) on success
    async fn set_gauge(
        &self,
        name: &str,
        value: MetricValue,
        labels: Option<HashMap<String, String>>,
    ) -> Result<()> {
        self.record_metric(name, value, MetricType::Gauge, labels)
            .await
    }

    /// Record a histogram observation
    ///
    /// # Arguments
    /// * `name` - Histogram name
    /// * `value` - The observed value
    /// * `labels` - Optional labels/tags
    ///
    /// # Returns
    /// Ok(()) on success
    async fn observe_histogram(
        &self,
        name: &str,
        value: f64,
        labels: Option<HashMap<String, String>>,
    ) -> Result<()> {
        self.record_metric(
            name,
            MetricValue::Float(value),
            MetricType::Histogram,
            labels,
        )
        .await
    }

    /// Send an alert
    ///
    /// # Arguments
    /// * `alert` - The alert to send
    ///
    /// # Returns
    /// Ok(()) if alert was sent
    async fn send_alert(&self, alert: Alert) -> Result<()>;

    /// Start a timer for measuring duration
    ///
    /// # Arguments
    /// * `name` - Metric name for the duration
    /// * `labels` - Optional labels/tags
    ///
    /// # Returns
    /// A timer handle that records duration when dropped
    fn start_timer(
        &self,
        name: String,
        labels: Option<HashMap<String, String>>,
    ) -> Box<dyn Send + Sync>;

    /// Create a child span for tracing
    ///
    /// # Arguments
    /// * `name` - Span name
    /// * `attributes` - Span attributes
    ///
    /// # Returns
    /// A span handle
    fn create_span(&self, name: &str, attributes: HashMap<String, String>) -> Box<dyn Send + Sync>;

    /// Check monitoring system health
    ///
    /// # Returns
    /// Ok(()) if monitoring is healthy
    async fn health_check(&self) -> Result<()>;

    /// Get the name of this provider
    ///
    /// # Returns
    /// A human-readable name for this provider
    fn provider_name(&self) -> &str;

    /// Get the provider type
    ///
    /// # Returns
    /// The provider type identifier
    fn provider_type(&self) -> MonitoringProviderType;

    /// Check if this provider supports alerting
    ///
    /// # Returns
    /// true if the provider can send alerts
    fn supports_alerting(&self) -> bool {
        false
    }

    /// Check if this provider supports distributed tracing
    ///
    /// # Returns
    /// true if the provider supports tracing
    fn supports_tracing(&self) -> bool {
        false
    }
}

/// Types of monitoring providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MonitoringProviderType {
    /// Prometheus metrics
    Prometheus,
    /// DataDog
    DataDog,
    /// New Relic
    NewRelic,
    /// AWS CloudWatch
    CloudWatch,
    /// Custom provider
    Custom,
}

/// Factory trait for creating monitoring providers
#[async_trait]
pub trait MonitoringProviderFactory: Send + Sync {
    /// Create a monitoring provider from configuration
    async fn create(&self, config: MonitoringProviderConfig)
        -> Result<Box<dyn MonitoringProvider>>;

    /// Get the provider type this factory creates
    fn provider_type(&self) -> MonitoringProviderType;
}
