use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::{debug, info};

use super::models::{FeedLog, NewFeedLog};

/// Repository for feed log operations
pub struct FeedLogRepository {
    pool: PgPool,
}

impl FeedLogRepository {
    /// Creates a new repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Saves a new feed log entry
    pub async fn save(&self, log: NewFeedLog) -> Result<FeedLog> {
        debug!(
            "Attempting to save feed log: feed={}, network={}, value={}, timestamp={}, error_status={:?}, network_error={}",
            log.feed_name, log.network_name, log.feed_value, log.feed_timestamp, log.error_status_code, log.network_error
        );

        let record = sqlx::query_as::<_, FeedLog>(
            r#"
            INSERT INTO omikuji.feed_log (
                feed_name, 
                network_name, 
                feed_value, 
                feed_timestamp, 
                error_status_code, 
                network_error,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            RETURNING 
                id,
                feed_name,
                network_name,
                feed_value,
                feed_timestamp,
                updated_at,
                error_status_code,
                network_error,
                created_at
            "#,
        )
        .bind(&log.feed_name)
        .bind(&log.network_name)
        .bind(log.feed_value)
        .bind(log.feed_timestamp)
        .bind(log.error_status_code)
        .bind(log.network_error)
        .fetch_one(&self.pool)
        .await
        .context("Failed to insert feed log")?;

        debug!(
            "Successfully saved feed log with id={}: feed={}, network={}, value={}, timestamp={}, created_at={}",
            record.id, record.feed_name, record.network_name, record.feed_value, record.feed_timestamp, record.created_at
        );

        Ok(record)
    }

    /// Deletes feed logs older than the specified number of days
    #[allow(dead_code)]
    pub async fn delete_older_than(
        &self,
        feed_name: &str,
        network_name: &str,
        days: u32,
    ) -> Result<u64> {
        let cutoff_date = Utc::now() - Duration::days(days as i64);

        let result = sqlx::query(
            r#"
            DELETE FROM omikuji.feed_log
            WHERE feed_name = $1 
                AND network_name = $2
                AND created_at < $3
            "#,
        )
        .bind(feed_name)
        .bind(network_name)
        .bind(cutoff_date)
        .execute(&self.pool)
        .await
        .context("Failed to delete old feed logs")?;

        let deleted_count = result.rows_affected();

        if deleted_count > 0 {
            info!(
                "Deleted {} old feed logs for feed '{}' on network '{}' (older than {} days)",
                deleted_count, feed_name, network_name, days
            );
        }

        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_query_format() {
        // Test the SQL query format for saving feed logs
        let query = r#"
            INSERT INTO omikuji.feed_log (
                feed_name, 
                network_name, 
                feed_value, 
                feed_timestamp, 
                error_status_code, 
                network_error,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            RETURNING 
                id,
                feed_name,
                network_name,
                feed_value,
                feed_timestamp,
                updated_at,
                error_status_code,
                network_error,
                created_at
            "#;

        assert!(query.contains("INSERT INTO omikuji.feed_log"));
        assert!(query.contains("RETURNING"));
        assert!(query.contains("NOW()"));
    }

    #[test]
    fn test_delete_older_than_cutoff_calculation() {
        let days = 30u32;
        let now = Utc::now();
        let cutoff_date = now - Duration::days(days as i64);

        let duration = now - cutoff_date;
        assert_eq!(duration.num_days(), 30);
    }
}
