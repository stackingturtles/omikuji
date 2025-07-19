//! Repository for managing processed events in the database

use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::debug;

use crate::database::models::{NewProcessedEvent, ProcessedEvent};

/// Repository for managing processed events
pub struct EventRepository;

impl EventRepository {
    /// Check if an event has already been processed
    pub async fn is_event_processed(
        pool: &PgPool,
        transaction_hash: &str,
        log_index: i32,
    ) -> Result<bool> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM processed_events 
                WHERE transaction_hash = $1 AND log_index = $2
            ) as "exists!"
            "#,
            transaction_hash,
            log_index
        )
        .fetch_one(pool)
        .await?;

        Ok(exists)
    }

    /// Mark an event as processed
    pub async fn mark_event_processed(
        pool: &PgPool,
        event: NewProcessedEvent,
    ) -> Result<ProcessedEvent> {
        let record = sqlx::query_as!(
            ProcessedEvent,
            r#"
            INSERT INTO processed_events (
                transaction_hash, log_index, contract_address, 
                event_type, event_data
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (transaction_hash, log_index) DO NOTHING
            RETURNING 
                id,
                transaction_hash,
                log_index,
                contract_address,
                event_type,
                processed_at,
                event_data
            "#,
            event.transaction_hash,
            event.log_index,
            event.contract_address,
            event.event_type,
            event.event_data
        )
        .fetch_one(pool)
        .await?;

        debug!(
            "Marked event as processed: tx={}, log_index={}, type={}",
            event.transaction_hash, event.log_index, event.event_type
        );

        Ok(record)
    }

    /// Mark an event as processed within a transaction
    pub async fn mark_event_processed_tx(
        tx: &mut Transaction<'_, Postgres>,
        event: NewProcessedEvent,
    ) -> Result<ProcessedEvent> {
        let record = sqlx::query_as!(
            ProcessedEvent,
            r#"
            INSERT INTO processed_events (
                transaction_hash, log_index, contract_address, 
                event_type, event_data
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (transaction_hash, log_index) DO NOTHING
            RETURNING 
                id,
                transaction_hash,
                log_index,
                contract_address,
                event_type,
                processed_at,
                event_data
            "#,
            event.transaction_hash,
            event.log_index,
            event.contract_address,
            event.event_type,
            event.event_data
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(record)
    }

    /// Get all processed events for a contract
    pub async fn get_processed_events_for_contract(
        pool: &PgPool,
        contract_address: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ProcessedEvent>> {
        let limit = limit.unwrap_or(100);
        
        let records = sqlx::query_as!(
            ProcessedEvent,
            r#"
            SELECT 
                id,
                transaction_hash,
                log_index,
                contract_address,
                event_type,
                processed_at,
                event_data
            FROM processed_events
            WHERE contract_address = $1
            ORDER BY processed_at DESC
            LIMIT $2
            "#,
            contract_address,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Clean up old processed events
    pub async fn cleanup_old_events(
        pool: &PgPool,
        retention_days: i32,
    ) -> Result<i64> {
        let result = sqlx::query!(
            r#"
            DELETE FROM processed_events
            WHERE processed_at < NOW() - INTERVAL '1 day' * $1
            "#,
            retention_days
        )
        .execute(pool)
        .await?;

        let deleted_count = result.rows_affected() as i64;
        
        if deleted_count > 0 {
            debug!(
                "Cleaned up {} old processed events (retention: {} days)",
                deleted_count, retention_days
            );
        }

        Ok(deleted_count)
    }

    /// Get the count of processed events by type
    pub async fn get_event_counts_by_type(
        pool: &PgPool,
    ) -> Result<Vec<(String, i64)>> {
        let records = sqlx::query!(
            r#"
            SELECT 
                event_type,
                COUNT(*) as count
            FROM processed_events
            GROUP BY event_type
            ORDER BY count DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|r| (r.event_type, r.count.unwrap_or(0)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[tokio::test]
    async fn test_event_processing_flow() {
        let pool = create_test_db_pool().await;

        // Create a test event
        let event = NewProcessedEvent {
            transaction_hash: "0x123abc".to_string(),
            log_index: 0,
            contract_address: "0xcontract".to_string(),
            event_type: "TestEvent".to_string(),
            event_data: Some(serde_json::json!({"test": "data"})),
        };

        // Check that event is not processed initially
        let is_processed = EventRepository::is_event_processed(
            &pool,
            &event.transaction_hash,
            event.log_index,
        )
        .await
        .unwrap();
        assert!(!is_processed);

        // Mark event as processed
        let processed = EventRepository::mark_event_processed(&pool, event.clone())
            .await
            .unwrap();
        
        assert_eq!(processed.transaction_hash, event.transaction_hash);
        assert_eq!(processed.log_index, event.log_index);

        // Check that event is now marked as processed
        let is_processed = EventRepository::is_event_processed(
            &pool,
            &event.transaction_hash,
            event.log_index,
        )
        .await
        .unwrap();
        assert!(is_processed);

        // Try to mark the same event again (should not error due to ON CONFLICT)
        let result = EventRepository::mark_event_processed(&pool, event)
            .await;
        assert!(result.is_ok());
    }
}