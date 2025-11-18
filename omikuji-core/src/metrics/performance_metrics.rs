use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, GaugeVec,
    HistogramVec,
};
use std::time::Duration;
use tracing::{debug, warn};

lazy_static! {
    /// Concurrent feed updates gauge
    static ref CONCURRENT_FEED_UPDATES: GaugeVec = register_gauge_vec!(
        "omikuji_concurrent_feed_updates",
        "Number of concurrent feed update tasks",
        &["network"]
    ).expect("Failed to create concurrent_feed_updates metric");

    /// Memory usage in bytes
    static ref MEMORY_USAGE_BYTES: GaugeVec = register_gauge_vec!(
        "omikuji_memory_usage_bytes",
        "Memory usage in bytes",
        &["memory_type"]
    ).expect("Failed to create memory_usage_bytes metric");

    /// Open connections count
    static ref OPEN_CONNECTIONS: GaugeVec = register_gauge_vec!(
        "omikuji_open_connections",
        "Number of open connections",
        &["connection_type", "network"]
    ).expect("Failed to create open_connections metric");

    /// Task execution time histogram
    static ref TASK_EXECUTION_TIME_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_task_execution_time_seconds",
        "Task execution time in seconds",
        &["task_type", "network"],
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0]
    ).expect("Failed to create task_execution_time metric");

    /// CPU usage percentage
    static ref CPU_USAGE_PERCENT: GaugeVec = register_gauge_vec!(
        "omikuji_cpu_usage_percent",
        "CPU usage percentage",
        &["cpu_type"]
    ).expect("Failed to create cpu_usage_percent metric");

    /// Thread pool utilization
    static ref THREAD_POOL_UTILIZATION: GaugeVec = register_gauge_vec!(
        "omikuji_thread_pool_utilization",
        "Thread pool utilization",
        &["pool_name", "state"]
    ).expect("Failed to create thread_pool_utilization metric");

    /// Event loop lag
    static ref EVENT_LOOP_LAG_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_event_loop_lag_seconds",
        "Event loop lag in seconds",
        &["runtime"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
    ).expect("Failed to create event_loop_lag metric");

    /// Database connection pool stats
    static ref DB_CONNECTION_POOL: GaugeVec = register_gauge_vec!(
        "omikuji_db_connection_pool",
        "Database connection pool statistics",
        &["pool_state"]
    ).expect("Failed to create db_connection_pool metric");

    /// Cache hit rate
    static ref CACHE_HIT_RATE: CounterVec = register_counter_vec!(
        "omikuji_cache_operations_total",
        "Cache operation counts",
        &["cache_name", "operation", "result"]
    ).expect("Failed to create cache_hit_rate metric");

    /// Startup time histogram
    static ref STARTUP_TIME_SECONDS: HistogramVec = register_histogram_vec!(
        "omikuji_startup_time_seconds",
        "Application startup time by component",
        &["component"],
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
    ).expect("Failed to create startup_time metric");
}

/// Performance metrics collector
pub struct PerformanceMetrics;

impl PerformanceMetrics {
    /// Update concurrent feed updates count
    pub fn update_concurrent_feeds(network: &str, count: usize) {
        CONCURRENT_FEED_UPDATES
            .with_label_values(&[network])
            .set(count as f64);

        if count > 50 {
            warn!(
                "High number of concurrent feed updates on {}: {}",
                network, count
            );
        }
    }

    /// Update memory usage
    pub fn update_memory_usage(heap_bytes: usize, stack_bytes: Option<usize>, total_bytes: usize) {
        MEMORY_USAGE_BYTES
            .with_label_values(&["heap"])
            .set(heap_bytes as f64);

        if let Some(stack) = stack_bytes {
            MEMORY_USAGE_BYTES
                .with_label_values(&["stack"])
                .set(stack as f64);
        }

        MEMORY_USAGE_BYTES
            .with_label_values(&["total"])
            .set(total_bytes as f64);

        let total_mb = total_bytes as f64 / (1024.0 * 1024.0);
        if total_mb > 1024.0 {
            warn!("High memory usage: {:.1} MB", total_mb);
        }
    }

    /// Update open connections
    pub fn update_open_connections(connection_type: &str, network: &str, count: usize) {
        OPEN_CONNECTIONS
            .with_label_values(&[connection_type, network])
            .set(count as f64);

        if count > 100 {
            warn!(
                "High number of {} connections for {}: {}",
                connection_type, network, count
            );
        }
    }

    /// Record task execution time
    pub fn record_task_execution(task_type: &str, network: &str, duration: Duration) {
        TASK_EXECUTION_TIME_SECONDS
            .with_label_values(&[task_type, network])
            .observe(duration.as_secs_f64());

        if duration.as_secs() > 30 {
            warn!(
                "Long task execution time for {} on {}: {:.1}s",
                task_type,
                network,
                duration.as_secs_f64()
            );
        }
    }

    /// Update CPU usage
    pub fn update_cpu_usage(user_percent: f64, system_percent: f64, total_percent: f64) {
        CPU_USAGE_PERCENT
            .with_label_values(&["user"])
            .set(user_percent);

        CPU_USAGE_PERCENT
            .with_label_values(&["system"])
            .set(system_percent);

        CPU_USAGE_PERCENT
            .with_label_values(&["total"])
            .set(total_percent);

        if total_percent > 80.0 {
            warn!("High CPU usage: {:.1}%", total_percent);
        }
    }

    /// Update thread pool stats
    pub fn update_thread_pool(
        pool_name: &str,
        active_threads: usize,
        idle_threads: usize,
        total_threads: usize,
    ) {
        THREAD_POOL_UTILIZATION
            .with_label_values(&[pool_name, "active"])
            .set(active_threads as f64);

        THREAD_POOL_UTILIZATION
            .with_label_values(&[pool_name, "idle"])
            .set(idle_threads as f64);

        THREAD_POOL_UTILIZATION
            .with_label_values(&[pool_name, "total"])
            .set(total_threads as f64);

        let utilization = if total_threads > 0 {
            (active_threads as f64 / total_threads as f64) * 100.0
        } else {
            0.0
        };

        if utilization > 90.0 {
            warn!(
                "High thread pool utilization for {}: {:.1}%",
                pool_name, utilization
            );
        }
    }

    /// Record event loop lag
    pub fn record_event_loop_lag(runtime: &str, lag: Duration) {
        EVENT_LOOP_LAG_SECONDS
            .with_label_values(&[runtime])
            .observe(lag.as_secs_f64());

        if lag.as_millis() > 100 {
            warn!(
                "High event loop lag in {}: {:.0}ms",
                runtime,
                lag.as_millis()
            );
        }
    }

    /// Update database connection pool
    pub fn update_db_pool(active: usize, idle: usize, waiting: usize, max_size: usize) {
        DB_CONNECTION_POOL
            .with_label_values(&["active"])
            .set(active as f64);

        DB_CONNECTION_POOL
            .with_label_values(&["idle"])
            .set(idle as f64);

        DB_CONNECTION_POOL
            .with_label_values(&["waiting"])
            .set(waiting as f64);

        DB_CONNECTION_POOL
            .with_label_values(&["max_size"])
            .set(max_size as f64);

        if waiting > 0 {
            warn!("Database connections waiting: {}", waiting);
        }
    }

    /// Record cache operation
    pub fn record_cache_operation(cache_name: &str, operation: &str, hit: bool) {
        let result = if hit { "hit" } else { "miss" };

        CACHE_HIT_RATE
            .with_label_values(&[cache_name, operation, result])
            .inc();
    }

    /// Record component startup time
    pub fn record_startup_time(component: &str, duration: Duration) {
        STARTUP_TIME_SECONDS
            .with_label_values(&[component])
            .observe(duration.as_secs_f64());

        debug!(
            "Component {} started in {:.3}s",
            component,
            duration.as_secs_f64()
        );
    }

    /// Calculate cache hit rate
    pub fn get_cache_hit_rate(cache_name: &str) -> Option<f64> {
        // This is a helper method to calculate hit rate
        // In practice, you'd use Prometheus queries for this
        debug!(
            "Cache hit rate calculation for {} would be done via PromQL",
            cache_name
        );
        None
    }
}
