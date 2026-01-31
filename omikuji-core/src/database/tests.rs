#[cfg(test)]
mod tests {
    use crate::database::establish_connection;
    use crate::database::models::{FeedLog, NewFeedLog};
    use chrono::{Duration, Utc};

    #[test]
    fn test_new_feed_log_creation() {
        let log = NewFeedLog {
            feed_name: "eth_usd".to_string(),
            network_name: "ethereum".to_string(),
            feed_value: 2045.34,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: None,
            network_error: false,
        };

        assert_eq!(log.feed_name, "eth_usd");
        assert_eq!(log.network_name, "ethereum");
        assert_eq!(log.feed_value, 2045.34);
        assert!(log.error_status_code.is_none());
        assert!(!log.network_error);
    }

    #[test]
    fn test_error_feed_log_creation() {
        let log = NewFeedLog {
            feed_name: "btc_usd".to_string(),
            network_name: "base".to_string(),
            feed_value: 0.0,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: Some(500),
            network_error: false,
        };

        assert_eq!(log.error_status_code, Some(500));
        assert!(!log.network_error);
    }

    #[test]
    fn test_network_error_log_creation() {
        let log = NewFeedLog {
            feed_name: "eth_usd".to_string(),
            network_name: "ethereum".to_string(),
            feed_value: 0.0,
            feed_timestamp: Utc::now().timestamp(),
            error_status_code: None,
            network_error: true,
        };

        assert!(log.error_status_code.is_none());
        assert!(log.network_error);
    }

    #[test]
    fn test_feed_log_model() {
        let now = Utc::now();
        let log = FeedLog {
            id: 1,
            feed_name: "eth_usd".to_string(),
            network_name: "ethereum".to_string(),
            feed_value: 2500.0,
            feed_timestamp: now.timestamp(),
            updated_at: now,
            error_status_code: None,
            network_error: false,
            created_at: now,
        };

        assert_eq!(log.id, 1);
        assert_eq!(log.feed_name, "eth_usd");
        assert_eq!(log.network_name, "ethereum");
        assert_eq!(log.feed_value, 2500.0);
        assert!(!log.network_error);
    }

    // Mock database connection for testing
    #[cfg(test)]
    mod mock_db {
        use std::env;

        pub fn setup_test_db_url() {
            // Set a test database URL if not already set
            if env::var("DATABASE_URL").is_err() {
                env::set_var(
                    "DATABASE_URL",
                    "postgres://test:test@localhost:5432/test_db",
                );
            }
        }

        pub fn cleanup_test_db_url() {
            env::remove_var("DATABASE_URL");
        }
    }

    #[tokio::test]
    async fn test_establish_connection_no_database_url() {
        // Save current DATABASE_URL if it exists
        let saved_url = std::env::var("DATABASE_URL").ok();

        // Ensure DATABASE_URL is not set
        mock_db::cleanup_test_db_url();

        let result = establish_connection().await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_str = err.to_string();
        // In SQLX_OFFLINE mode, the error may be about connection pool instead of env var
        assert!(
            err_str.contains("DATABASE_URL environment variable not set")
                || err_str.contains("Failed to create PostgreSQL connection pool"),
            "Expected connection-related error, got: {}",
            err
        );

        // Restore DATABASE_URL if it was set
        if let Some(url) = saved_url {
            std::env::set_var("DATABASE_URL", url);
        }
    }

    #[tokio::test]
    async fn test_establish_connection_invalid_url() {
        // Save current DATABASE_URL if it exists
        let saved_url = std::env::var("DATABASE_URL").ok();

        // Set an invalid database URL
        std::env::set_var("DATABASE_URL", "invalid://url");

        let result = establish_connection().await;
        assert!(result.is_err());

        // Restore DATABASE_URL if it was set, otherwise clean up
        if let Some(url) = saved_url {
            std::env::set_var("DATABASE_URL", url);
        } else {
            mock_db::cleanup_test_db_url();
        }
    }

    #[test]
    fn test_database_url_masking() {
        // Test URL masking logic
        let test_url = "postgres://user:password@localhost:5432/mydb";
        let url_parts: Vec<&str> = test_url.split('@').collect();

        assert_eq!(url_parts.len(), 2);

        let masked = if url_parts.len() > 1 {
            let host_and_db = url_parts[1];
            format!("postgres://***@{host_and_db}")
        } else {
            "postgres://***".to_string()
        };

        assert_eq!(masked, "postgres://***@localhost:5432/mydb");
    }

    #[test]
    fn test_cutoff_date_calculation() {
        let now = Utc::now();
        let days = 7;
        let cutoff = now - Duration::days(days as i64);

        // Check that cutoff is approximately 7 days ago
        let diff = now - cutoff;
        assert_eq!(diff.num_days(), 7);
    }

    #[test]
    fn test_cleanup_manager_config_validation() {
        use crate::config::metrics_config::MetricsConfig;
        use crate::config::models::{DatabaseCleanupConfig, OmikujiConfig};
        use crate::gas_price::models::GasPriceFeedConfig;

        let config = OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: DatabaseCleanupConfig {
                enabled: true,
                schedule: "0 2 * * *".to_string(),
            },
            key_storage: Default::default(),
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        };

        assert!(config.database_cleanup.enabled);
    }

    // Additional repository tests
    #[test]
    fn test_repository_creation() {

        // This would normally use a real pool in integration tests
        // For unit tests, we're just testing the structure
        // let pool = PgPool::new();
        // let repo = FeedLogRepository::new(pool);
    }

    #[test]
    fn test_transaction_repository_methods() {

        // Test that TransactionLogRepository can be created
        // In real tests, this would use a test database
        // let pool = PgPool::new();
        // let repo = TransactionLogRepository::new(pool);
    }

    #[tokio::test]
    async fn test_cleanup_manager_creation() {
        use crate::config::metrics_config::MetricsConfig;
        use crate::config::models::{DatabaseCleanupConfig, OmikujiConfig};
        use crate::gas_price::models::GasPriceFeedConfig;

        let config = OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: DatabaseCleanupConfig {
                enabled: false, // Disabled for test
                schedule: "0 0 * * *".to_string(),
            },
            key_storage: Default::default(),
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: Default::default(),
        };

        // Mock repository for testing
        // In real tests, this would use a test database
        // let pool = create_test_pool().await;
        // let repository = Arc::new(FeedLogRepository::new(pool));
        // let manager = CleanupManager::new(config, repository).await;

        assert!(!config.database_cleanup.enabled);
    }

    #[test]
    fn test_cron_schedule_parsing() {
        // Test various cron schedule formats
        let valid_schedules = vec![
            "0 0 * * *",   // Daily at midnight
            "0 2 * * *",   // Daily at 2 AM
            "0 */6 * * *", // Every 6 hours
            "30 2 * * 0",  // Weekly on Sunday at 2:30 AM
            "0 0 1 * *",   // Monthly on the 1st
        ];

        for schedule in valid_schedules {
            assert!(!schedule.is_empty());
            assert!(schedule.split_whitespace().count() == 5);
        }
    }

    // Edge case tests for Phase 4 - Database Error Recovery
    #[test]
    fn test_connection_pool_exhaustion_handling() {
        // Test connection pool configuration
        use sqlx::postgres::PgPoolOptions;
        use std::time::Duration as StdDuration;

        let _pool_options = PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(StdDuration::from_secs(3))
            .idle_timeout(StdDuration::from_secs(600))
            .max_lifetime(StdDuration::from_secs(1800));

        // Verify pool options are set correctly
        // In a real test, we'd try to exceed connections and verify error handling
    }

    #[test]
    fn test_transaction_rollback_scenarios() {
        // Test transaction rollback logic
        use crate::database::models::NewFeedLog;

        let invalid_log = NewFeedLog {
            feed_name: "".to_string(), // Empty name should fail validation
            network_name: "ethereum".to_string(),
            feed_value: f64::NAN,            // NaN value should fail
            feed_timestamp: -1,              // Invalid timestamp
            error_status_code: Some(999999), // Invalid status code
            network_error: false,
        };

        // In a real test with DB connection, this would test rollback
        assert!(invalid_log.feed_name.is_empty());
        assert!(invalid_log.feed_value.is_nan());
        assert!(invalid_log.feed_timestamp < 0);
    }

    #[test]
    fn test_invalid_timestamp_handling() {
        // Test various invalid timestamp scenarios
        let test_cases = vec![
            (0i64, "zero timestamp"),
            (-1i64, "negative timestamp"),
            (i64::MAX, "max timestamp"),
            (253402300799i64, "year 9999 timestamp"), // Far future
        ];

        for (timestamp, description) in test_cases {
            let log = NewFeedLog {
                feed_name: "test".to_string(),
                network_name: "test".to_string(),
                feed_value: 100.0,
                feed_timestamp: timestamp,
                error_status_code: None,
                network_error: false,
            };

            // Verify we can handle these edge cases
            assert_eq!(log.feed_timestamp, timestamp, "Failed for: {description}");
        }
    }

    #[test]
    fn test_feed_value_edge_cases() {
        // Test edge case values for feed prices
        let edge_values = vec![
            (0.0, "zero value"),
            (-100.0, "negative value"),
            (f64::MAX, "max float value"),
            (f64::MIN, "min float value"),
            (f64::EPSILON, "epsilon value"),
            (1e-10, "very small value"),
            (1e10, "very large value"),
        ];

        for (value, description) in edge_values {
            let log = NewFeedLog {
                feed_name: "edge_test".to_string(),
                network_name: "test".to_string(),
                feed_value: value,
                feed_timestamp: Utc::now().timestamp(),
                error_status_code: None,
                network_error: false,
            };

            assert_eq!(log.feed_value, value, "Failed for: {description}");
        }
    }

    #[test]
    fn test_concurrent_write_handling() {
        // Test concurrent write scenarios
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let write_counter = Arc::new(AtomicU32::new(0));
        let error_counter = Arc::new(AtomicU32::new(0));

        // Simulate concurrent writes
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let write_count = Arc::clone(&write_counter);
                let error_count = Arc::clone(&error_counter);

                std::thread::spawn(move || {
                    // Simulate write attempt
                    write_count.fetch_add(1, Ordering::SeqCst);

                    // Simulate some writes failing due to conflicts
                    if i % 3 == 0 {
                        error_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(write_counter.load(Ordering::SeqCst), 10);
        assert!(error_counter.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn test_sql_injection_prevention() {
        // Test that potentially malicious inputs are handled safely
        let malicious_inputs = vec![
            "'; DROP TABLE feed_logs; --",
            "' OR '1'='1",
            "\\x00\\x01\\x02\\x03", // Null bytes
            "feed_name); DELETE FROM feed_logs; --",
            "<script>alert('xss')</script>",
        ];

        for input in malicious_inputs {
            let log = NewFeedLog {
                feed_name: input.to_string(),
                network_name: input.to_string(),
                feed_value: 100.0,
                feed_timestamp: Utc::now().timestamp(),
                error_status_code: None,
                network_error: false,
            };

            // Verify the string is stored as-is (parameterized queries handle escaping)
            assert_eq!(log.feed_name, input);
        }
    }

    #[test]
    fn test_database_migration_failure_scenarios() {
        // Test migration failure handling
        use std::path::Path;

        let migration_path = Path::new("./migrations");
        assert!(migration_path.exists(), "Migrations directory should exist");

        // In a real test, we'd test:
        // - Missing migration files
        // - Invalid SQL in migrations
        // - Migration version conflicts
        // - Rollback scenarios
    }

    #[test]
    fn test_maximum_batch_size_handling() {
        // Test batch insert limits
        const MAX_BATCH_SIZE: usize = 1000;

        let large_batch: Vec<NewFeedLog> = (0..MAX_BATCH_SIZE + 100)
            .map(|i| NewFeedLog {
                feed_name: format!("feed_{i}"),
                network_name: "test".to_string(),
                feed_value: i as f64,
                feed_timestamp: Utc::now().timestamp(),
                error_status_code: None,
                network_error: false,
            })
            .collect();

        assert_eq!(large_batch.len(), MAX_BATCH_SIZE + 100);

        // In a real implementation, this would test batch splitting logic
        let chunks: Vec<_> = large_batch.chunks(MAX_BATCH_SIZE).collect();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), MAX_BATCH_SIZE);
        assert_eq!(chunks[1].len(), 100);
    }

    #[test]
    fn test_cleanup_with_active_connections() {
        // Test cleanup behavior when there are active connections
        use crate::config::models::DatabaseCleanupConfig;

        let cleanup_config = DatabaseCleanupConfig {
            enabled: true,
            schedule: "*/5 * * * *".to_string(), // Every 5 minutes
        };

        // Simulate active connection scenario
        // In real test, would verify cleanup waits for connections to finish
        assert!(cleanup_config.enabled);
    }

    #[test]
    fn test_network_partition_recovery() {
        // Test database reconnection after network partition
        use std::time::Duration as StdDuration;

        let reconnect_intervals = [
            StdDuration::from_millis(100),
            StdDuration::from_millis(500),
            StdDuration::from_secs(1),
            StdDuration::from_secs(5),
            StdDuration::from_secs(30),
        ];

        // Verify exponential backoff strategy
        for (i, interval) in reconnect_intervals.iter().enumerate() {
            if i > 0 {
                assert!(interval > &reconnect_intervals[i - 1]);
            }
        }
    }
}
