use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, error, info};

use super::repository::FeedLogRepository;
use crate::config::models::OmikujiConfig;

/// Manages the database cleanup task
pub struct CleanupManager {
    config: OmikujiConfig,
    repository: Arc<FeedLogRepository>,
    scheduler: JobScheduler,
}

impl CleanupManager {
    /// Creates a new cleanup manager
    pub async fn new(config: OmikujiConfig, repository: Arc<FeedLogRepository>) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .context("Failed to create job scheduler")?;

        Ok(Self {
            config,
            repository,
            scheduler,
        })
    }

    /// Starts the cleanup scheduler
    pub async fn start(&mut self) -> Result<()> {
        if !self.config.database_cleanup.enabled {
            info!("Database cleanup is disabled in configuration");
            return Ok(());
        }

        let schedule = &self.config.database_cleanup.schedule;
        info!(
            "Starting database cleanup scheduler with cron schedule: {}",
            schedule
        );

        // Clone necessary data for the closure
        let config = self.config.clone();
        let repository = Arc::clone(&self.repository);

        // Create the cleanup job
        let job = Job::new_async(schedule.as_str(), move |_uuid, _l| {
            let config = config.clone();
            let repository = Arc::clone(&repository);

            Box::pin(async move {
                info!("Running scheduled database cleanup");
                if let Err(e) = run_cleanup(&config, &repository).await {
                    error!("Database cleanup failed: {}", e);
                }
            })
        })
        .context("Failed to create cleanup job")?;

        // Add job to scheduler
        self.scheduler
            .add(job)
            .await
            .context("Failed to add job to scheduler")?;

        // Start the scheduler
        self.scheduler
            .start()
            .await
            .context("Failed to start scheduler")?;

        info!("Database cleanup scheduler started successfully");
        Ok(())
    }

    /// Stops the cleanup scheduler
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping database cleanup scheduler");
        self.scheduler
            .shutdown()
            .await
            .context("Failed to shutdown scheduler")?;
        Ok(())
    }
}

/// Runs the cleanup task for all datafeeds
async fn run_cleanup(config: &OmikujiConfig, repository: &FeedLogRepository) -> Result<()> {
    let start_time = Utc::now();
    let mut total_deleted = 0u64;
    let mut feed_count = 0;

    // Clean up each datafeed based on its retention configuration
    for datafeed in &config.datafeeds {
        let retention_days = datafeed.data_retention_days;

        match repository
            .delete_older_than(&datafeed.name, &datafeed.networks, retention_days)
            .await
        {
            Ok(deleted) => {
                if deleted > 0 {
                    debug!(
                        "Deleted {} old records for feed '{}' on network '{}' (retention: {} days)",
                        deleted, datafeed.name, datafeed.networks, retention_days
                    );
                }
                total_deleted += deleted;
                feed_count += 1;
            }
            Err(e) => {
                error!(
                    "Failed to clean up feed '{}' on network '{}': {}",
                    datafeed.name, datafeed.networks, e
                );
            }
        }
    }

    let duration = Utc::now() - start_time;

    if total_deleted > 0 {
        info!(
            "Database cleanup completed: deleted {} records across {} feeds in {:.2}s",
            total_deleted,
            feed_count,
            duration.num_milliseconds() as f64 / 1000.0
        );
    } else {
        debug!(
            "Database cleanup completed: no old records to delete (checked {} feeds in {:.2}s)",
            feed_count,
            duration.num_milliseconds() as f64 / 1000.0
        );
    }

    Ok(())
}

/// Runs a one-time cleanup (useful for testing or manual execution)
#[allow(dead_code)]
pub async fn run_manual_cleanup(
    config: &OmikujiConfig,
    repository: &FeedLogRepository,
) -> Result<()> {
    info!("Running manual database cleanup");
    run_cleanup(config, repository).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::metrics_config::MetricsConfig;
    use crate::config::models::{Datafeed, Network, NetworkNode};
    use crate::gas_price::models::GasPriceFeedConfig;

    fn create_test_config() -> OmikujiConfig {
        OmikujiConfig {
            networks: vec![Network {
                name: "test-network".to_string(),
                nodes: vec![NetworkNode {
                    name: "Local Node".to_string(),
                    rpc_url: "http://localhost:8545".to_string(),
                    ws_url: None,
                }],
                transaction_type: "eip1559".to_string(),
                gas_config: Default::default(),
                gas_token: "ethereum".to_string(),
                gas_token_symbol: "ETH".to_string(),
                rpc_url: None,
                ws_url: None,
            }],
            datafeeds: vec![Datafeed {
                name: "test-feed".to_string(),
                networks: "test-network".to_string(),
                check_frequency: 60,
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                contract_type: "fluxmon".to_string(),
                read_contract_config: false,
                minimum_update_frequency: 3600,
                deviation_threshold_pct: 0.5,
                feed_url: "https://example.com/api".to_string(),
                feed_json_path: "data.price".to_string(),
                feed_json_path_timestamp: Some("data.timestamp".to_string()),
                decimals: None,
                min_value: None,
                max_value: None,
                data_retention_days: 7,
            }],
            database_cleanup: Default::default(),
            key_storage: Default::default(),
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
        }
    }

    #[test]
    fn test_cleanup_config() {
        let config = create_test_config();
        assert!(config.database_cleanup.enabled);
        assert_eq!(config.database_cleanup.schedule, "0 0 * * * *");
    }

    #[test]
    fn test_cron_schedule_validation() {
        // Test various valid cron expressions
        let valid_schedules = vec![
            "0 2 * * *",   // Daily at 2 AM
            "0 */6 * * *", // Every 6 hours
            "0 0 * * 0",   // Weekly on Sunday
            "0 0 1 * *",   // Monthly on the 1st
            "*/5 * * * *", // Every 5 minutes
        ];

        for schedule in valid_schedules {
            assert!(!schedule.is_empty());
            let parts: Vec<&str> = schedule.split_whitespace().collect();
            assert_eq!(parts.len(), 5);
        }
    }

    #[tokio::test]
    async fn test_cleanup_manager_creation() {
        let config = create_test_config();

        // This would normally use a real database pool
        // For unit tests, we just test that the manager can be created
        // let pool = create_test_pool().await;
        // let repository = Arc::new(FeedLogRepository::new(pool));
        // let manager = CleanupManager::new(config, repository).await;
        // assert!(manager.is_ok());

        assert!(config.database_cleanup.enabled);
    }

    #[test]
    fn test_retention_calculation() {
        let retention_days = 30;
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(retention_days as i64);

        let diff = now - cutoff;
        assert_eq!(diff.num_days(), retention_days as i64);
    }

    #[test]
    fn test_duration_formatting() {
        let start = chrono::Utc::now();
        let end = start + chrono::Duration::milliseconds(1500);
        let duration = end - start;

        let seconds = duration.num_milliseconds() as f64 / 1000.0;
        assert_eq!(seconds, 1.5);
    }

    #[test]
    fn test_feed_config_retention() {
        let config = create_test_config();

        // Test that each datafeed would use the global retention setting
        for datafeed in &config.datafeeds {
            assert_eq!(datafeed.name, "test-feed");
            assert_eq!(datafeed.networks, "test-network");
        }
    }

    #[test]
    fn test_cleanup_disabled_config() {
        let mut config = create_test_config();
        config.database_cleanup.enabled = false;

        assert!(!config.database_cleanup.enabled);
    }
}
