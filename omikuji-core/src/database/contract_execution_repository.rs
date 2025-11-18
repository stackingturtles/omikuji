//! Repository for managing contract executions in the database
//!
//! This module provides database operations for storing and retrieving
//! contract executions triggered by event monitors.

use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info};

use super::models::{ContractExecution, NewContractExecution};

/// Repository for contract execution operations
pub struct ContractExecutionRepository {
    pool: PgPool,
}

impl ContractExecutionRepository {
    /// Create a new ContractExecutionRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Save a new contract execution record
    pub async fn save(&self, execution: &NewContractExecution) -> Result<ContractExecution> {
        debug!(
            "Saving contract execution for monitor '{}' on network '{}'",
            execution.monitor_name, execution.network
        );

        let record = sqlx::query_as!(
            ContractExecution,
            r#"
            INSERT INTO omikuji.contract_executions (
                monitor_name,
                network,
                transaction_hash,
                contract_address,
                function_selector,
                call_data,
                value_wei,
                gas_limit,
                gas_price_wei,
                status,
                trigger_event_tx_hash,
                trigger_event_log_index
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $11)
            RETURNING *
            "#,
            execution.monitor_name,
            execution.network,
            execution.transaction_hash,
            execution.contract_address,
            execution.function_selector,
            execution.call_data,
            execution.value_wei,
            execution.gas_limit,
            execution.gas_price_wei,
            execution.trigger_event_tx_hash,
            execution.trigger_event_log_index
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to save contract execution")?;

        info!(
            "Saved contract execution {} for monitor '{}' with tx hash {}",
            record.id, record.monitor_name, record.transaction_hash
        );

        Ok(record)
    }

    /// Update the status of a contract execution
    pub async fn update_status(
        &self,
        id: i32,
        status: &str,
        gas_used: Option<i64>,
        error_message: Option<&str>,
    ) -> Result<()> {
        debug!("Updating contract execution {} status to '{}'", id, status);

        sqlx::query!(
            r#"
            UPDATE omikuji.contract_executions
            SET status = $2,
                gas_used = $3,
                error_message = $4,
                updated_at = NOW()
            WHERE id = $1
            "#,
            id,
            status,
            gas_used,
            error_message
        )
        .execute(&self.pool)
        .await
        .context("Failed to update contract execution status")?;

        info!("Updated contract execution {} status to '{}'", id, status);
        Ok(())
    }

    /// Get a contract execution by ID
    pub async fn get_by_id(&self, id: i32) -> Result<Option<ContractExecution>> {
        debug!("Fetching contract execution with ID {}", id);

        let record = sqlx::query_as!(
            ContractExecution,
            r#"
            SELECT * FROM omikuji.contract_executions
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch contract execution")?;

        Ok(record)
    }

    /// Get a contract execution by transaction hash
    pub async fn get_by_tx_hash(&self, tx_hash: &str) -> Result<Option<ContractExecution>> {
        debug!("Fetching contract execution with tx hash {}", tx_hash);

        let record = sqlx::query_as!(
            ContractExecution,
            r#"
            SELECT * FROM omikuji.contract_executions
            WHERE transaction_hash = $1
            "#,
            tx_hash
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch contract execution by tx hash")?;

        Ok(record)
    }

    /// Get all executions for a specific event monitor
    pub async fn get_by_monitor(
        &self,
        monitor_name: &str,
        limit: i64,
    ) -> Result<Vec<ContractExecution>> {
        debug!(
            "Fetching contract executions for monitor '{}' with limit {}",
            monitor_name, limit
        );

        let records = sqlx::query_as!(
            ContractExecution,
            r#"
            SELECT * FROM omikuji.contract_executions
            WHERE monitor_name = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            monitor_name,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch contract executions by monitor")?;

        debug!(
            "Found {} contract executions for monitor '{}'",
            records.len(),
            monitor_name
        );

        Ok(records)
    }

    /// Get all executions triggered by a specific event
    pub async fn get_by_trigger_event(
        &self,
        tx_hash: &str,
        log_index: i32,
    ) -> Result<Vec<ContractExecution>> {
        debug!(
            "Fetching contract executions triggered by event {}:{}",
            tx_hash, log_index
        );

        let records = sqlx::query_as!(
            ContractExecution,
            r#"
            SELECT * FROM omikuji.contract_executions
            WHERE trigger_event_tx_hash = $1 AND trigger_event_log_index = $2
            ORDER BY created_at DESC
            "#,
            tx_hash,
            log_index
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch contract executions by trigger event")?;

        debug!(
            "Found {} contract executions triggered by event {}:{}",
            records.len(),
            tx_hash,
            log_index
        );

        Ok(records)
    }

    /// Get execution statistics for a monitor
    pub async fn get_monitor_stats(&self, monitor_name: &str) -> Result<MonitorExecutionStats> {
        debug!("Fetching execution stats for monitor '{}'", monitor_name);

        let stats = sqlx::query_as!(
            MonitorExecutionStats,
            r#"
            SELECT 
                COUNT(*)::BIGINT as total_executions,
                COUNT(CASE WHEN status = 'success' THEN 1 END)::BIGINT as successful_executions,
                COUNT(CASE WHEN status = 'failed' THEN 1 END)::BIGINT as failed_executions,
                COUNT(CASE WHEN status = 'pending' THEN 1 END)::BIGINT as pending_executions,
                COALESCE(SUM(CAST(value_wei AS NUMERIC)), 0)::TEXT as total_value_wei,
                COALESCE(SUM(gas_used), 0)::BIGINT as total_gas_used
            FROM omikuji.contract_executions
            WHERE monitor_name = $1
            "#,
            monitor_name
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch monitor execution stats")?;

        Ok(stats)
    }

    /// Get pending executions that might need status updates
    pub async fn get_pending_executions(&self, limit: i64) -> Result<Vec<ContractExecution>> {
        debug!("Fetching pending contract executions with limit {}", limit);

        let records = sqlx::query_as!(
            ContractExecution,
            r#"
            SELECT * FROM omikuji.contract_executions
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch pending contract executions")?;

        debug!("Found {} pending contract executions", records.len());

        Ok(records)
    }

    /// Clean up old execution records
    pub async fn cleanup_old_executions(&self, days_to_keep: i32) -> Result<i64> {
        debug!(
            "Cleaning up contract executions older than {} days",
            days_to_keep
        );

        let result = sqlx::query!(
            r#"
            DELETE FROM omikuji.contract_executions
            WHERE created_at < NOW() - INTERVAL '1 day' * $1
            "#,
            days_to_keep as i64
        )
        .execute(&self.pool)
        .await
        .context("Failed to cleanup old contract executions")?;

        let deleted_count = result.rows_affected() as i64;
        info!("Deleted {} old contract execution records", deleted_count);

        Ok(deleted_count)
    }
}

/// Statistics for monitor executions
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MonitorExecutionStats {
    pub total_executions: Option<i64>,
    pub successful_executions: Option<i64>,
    pub failed_executions: Option<i64>,
    pub pending_executions: Option<i64>,
    pub total_value_wei: Option<String>,
    pub total_gas_used: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_contract_execution_creation() {
        let execution = NewContractExecution {
            monitor_name: "test_monitor".to_string(),
            network: "ethereum".to_string(),
            transaction_hash: "0x123...".to_string(),
            contract_address: "0xabc...".to_string(),
            function_selector: "0x12345678".to_string(),
            call_data: "0xabcdef".to_string(),
            value_wei: "1000000000000000000".to_string(),
            gas_limit: 200000,
            gas_price_wei: "20000000000".to_string(),
            trigger_event_tx_hash: "0xdef...".to_string(),
            trigger_event_log_index: 5,
        };

        assert_eq!(execution.monitor_name, "test_monitor");
        assert_eq!(execution.value_wei, "1000000000000000000");
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_save_and_retrieve_execution() {
        // This test requires a real database connection
        // It's marked as ignored and can be run with: cargo test -- --ignored
    }
}
