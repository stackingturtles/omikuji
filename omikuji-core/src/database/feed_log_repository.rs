use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use tracing::{debug, info};

use super::models::{FeedLog, NewFeedLog};
use super::repository::Repository;

/// Repository for feed log operations
pub struct FeedLogRepository {
    pool: PgPool,
}

impl FeedLogRepository {
    /// Creates a new repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Repository<NewFeedLog> for FeedLogRepository {
    async fn save(&self, log: NewFeedLog) -> Result<NewFeedLog> {
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

        Ok(log)
    }

    async fn get(&self, id: &str) -> Result<Option<NewFeedLog>> {
        let record = sqlx::query_as::<_, FeedLog>(
            r#"
            SELECT 
                id,
                feed_name,
                network_name,
                feed_value,
                feed_timestamp,
                updated_at,
                error_status_code,
                network_error,
                created_at
            FROM omikuji.feed_log
            WHERE feed_name = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get latest feed log")?;

        Ok(record.map(|r| NewFeedLog {
            feed_name: r.feed_name,
            network_name: r.network_name,
            feed_value: r.feed_value,
            feed_timestamp: r.feed_timestamp,
            error_status_code: r.error_status_code,
            network_error: r.network_error,
        }))
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM omikuji.feed_log
            WHERE feed_name = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to delete old feed logs")?;

        let deleted_count = result.rows_affected();

        if deleted_count > 0 {
            info!(
                "Deleted {} old feed logs for feed '{}'",
                deleted_count, id
            );
        }

        Ok(())
    }
}
