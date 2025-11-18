use prometheus::{CounterVec, GaugeVec, HistogramVec, Opts, Result};

pub struct MetricsFactory;

impl MetricsFactory {
    pub fn create_counter_vec(name: &str, help: &str, labels: &[&str]) -> Result<CounterVec> {
        let opts = Opts::new(name, help);
        CounterVec::new(opts, labels)
    }

    pub fn create_gauge_vec(name: &str, help: &str, labels: &[&str]) -> Result<GaugeVec> {
        let opts = Opts::new(name, help);
        GaugeVec::new(opts, labels)
    }

    pub fn create_histogram_vec(
        name: &str,
        help: &str,
        labels: &[&str],
        buckets: &[f64],
    ) -> Result<HistogramVec> {
        let opts = Opts::new(name, help).buckets(buckets.to_vec());
        HistogramVec::new(opts, labels)
    }
}
