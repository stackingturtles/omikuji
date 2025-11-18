use serde::{Deserialize, Serialize};

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Metrics server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Enable detailed/high-cardinality metrics
    #[serde(default = "default_detailed_metrics")]
    pub detailed_metrics: bool,

    /// Category-specific configuration
    #[serde(default)]
    pub categories: MetricCategories,
}

/// Individual metric category toggles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricCategories {
    /// Data source health metrics
    #[serde(default = "default_true")]
    pub datasource: bool,

    /// Update decision metrics
    #[serde(default = "default_true")]
    pub update_decisions: bool,

    /// Network/RPC metrics
    #[serde(default = "default_true")]
    pub network: bool,

    /// Contract interaction metrics
    #[serde(default = "default_true")]
    pub contract: bool,

    /// Data quality metrics
    #[serde(default = "default_true")]
    pub quality: bool,

    /// Economic/cost metrics
    #[serde(default = "default_true")]
    pub economic: bool,

    /// Performance metrics
    #[serde(default = "default_true")]
    pub performance: bool,

    /// Configuration info metrics
    #[serde(default = "default_true")]
    pub config: bool,

    /// Alert-worthy metrics
    #[serde(default = "default_true")]
    pub alerts: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            port: default_port(),
            detailed_metrics: default_detailed_metrics(),
            categories: MetricCategories::default(),
        }
    }
}

impl Default for MetricCategories {
    fn default() -> Self {
        Self {
            datasource: true,
            update_decisions: true,
            network: true,
            contract: true,
            quality: true,
            economic: true,
            performance: true,
            config: true,
            alerts: true,
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_port() -> u16 {
    crate::constants::metrics::METRICS_SERVER_PORT
}

fn default_detailed_metrics() -> bool {
    false
}

fn default_true() -> bool {
    true
}

impl MetricsConfig {
    /// Check if a specific category is enabled
    pub fn is_category_enabled(&self, category: MetricCategory) -> bool {
        if !self.enabled {
            return false;
        }

        match category {
            MetricCategory::Datasource => self.categories.datasource,
            MetricCategory::UpdateDecisions => self.categories.update_decisions,
            MetricCategory::Network => self.categories.network,
            MetricCategory::Contract => self.categories.contract,
            MetricCategory::Quality => self.categories.quality && self.detailed_metrics,
            MetricCategory::Economic => self.categories.economic,
            MetricCategory::Performance => self.categories.performance && self.detailed_metrics,
            MetricCategory::Config => self.categories.config,
            MetricCategory::Alerts => self.categories.alerts,
        }
    }
}

/// Metric category enum for runtime checks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MetricCategory {
    Datasource,
    UpdateDecisions,
    Network,
    Contract,
    Quality,
    Economic,
    Performance,
    Config,
    Alerts,
}
