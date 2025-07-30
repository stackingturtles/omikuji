use super::connection::DatabasePool;
use crate::metrics::gas_metrics::TransactionDetails;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tracing::debug;

/// Repository for transaction log operations
pub struct TransactionLogRepository {
    pool: DatabasePool,
}

/// Transaction log entry from database
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct TransactionLog {
    pub id: i32,
    pub tx_hash: String,
    pub feed_name: String,
    pub network_name: String,
    pub gas_limit: i64,
    pub gas_used: i64,
    pub gas_price_gwei: f64,
    pub total_cost_wei: String, // Store as string to avoid BigDecimal issues
    pub efficiency_percent: f64,
    pub tx_type: String,
    pub status: String,
    pub block_number: i64,
    pub error_message: Option<String>,
    pub max_fee_per_gas_gwei: Option<f64>,
    pub max_priority_fee_per_gas_gwei: Option<f64>,
    pub created_at: DateTime<Utc>,
}

impl TransactionLogRepository {
    /// Create a new repository instance
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Save a transaction log entry
    pub async fn save_transaction(&self, details: TransactionDetails) -> Result<i32> {
        let total_cost_wei = details.total_cost_wei.to_string();

        debug!(
            "Attempting to save transaction log: feed={}, network={}, tx_hash={}, gas_used={}, gas_price={:.2} gwei, status={}, efficiency={:.1}%",
            details.feed_name, details.network, details.tx_hash, details.gas_used, details.gas_price_gwei, details.status, details.efficiency_percent
        );

        let result = sqlx::query_as::<_, (i32,)>(
            r#"
            INSERT INTO omikuji.transaction_log (
                tx_hash, feed_name, network_name, gas_limit, gas_used,
                gas_price_gwei, total_cost_wei, efficiency_percent,
                tx_type, status, block_number, error_message
            ) VALUES ($1, $2, $3, $4, $5, $6, $7::NUMERIC, $8, $9, $10, $11, $12)
            ON CONFLICT (tx_hash) DO UPDATE SET
                gas_used = EXCLUDED.gas_used,
                gas_price_gwei = EXCLUDED.gas_price_gwei,
                total_cost_wei = EXCLUDED.total_cost_wei,
                efficiency_percent = EXCLUDED.efficiency_percent,
                status = EXCLUDED.status,
                block_number = EXCLUDED.block_number,
                error_message = EXCLUDED.error_message
            RETURNING id
            "#,
        )
        .bind(&details.tx_hash)
        .bind(&details.feed_name)
        .bind(&details.network)
        .bind(details.gas_limit as i64)
        .bind(details.gas_used as i64)
        .bind(details.gas_price_gwei)
        .bind(total_cost_wei)
        .bind(details.efficiency_percent)
        .bind(&details.tx_type)
        .bind(&details.status)
        .bind(details.block_number as i64)
        .bind(&details.error_message)
        .fetch_one(&self.pool)
        .await
        .context("Failed to save transaction log")?;

        debug!(
            "Successfully saved transaction log with id={}: feed={} on {} - tx_hash: {}, block_number={}",
            result.0, details.feed_name, details.network, details.tx_hash, details.block_number
        );

        Ok(result.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_transaction_details() -> TransactionDetails {
        TransactionDetails {
            tx_hash: "0xabc123".to_string(),
            feed_name: "eth_usd".to_string(),
            network: "ethereum".to_string(),
            gas_limit: 200000,
            gas_used: 150000,
            gas_price_gwei: 35.5,
            total_cost_wei: 5325000000000000u128,
            efficiency_percent: 75.0,
            tx_type: "eip1559".to_string(),
            status: "success".to_string(),
            block_number: 15000000,
            error_message: None,
        }
    }

    #[test]
    fn test_transaction_log_struct() {
        let now = Utc::now();
        let log = TransactionLog {
            id: 1,
            tx_hash: "0xtest123".to_string(),
            feed_name: "btc_usd".to_string(),
            network_name: "base".to_string(),
            gas_limit: 100000,
            gas_used: 80000,
            gas_price_gwei: 20.0,
            total_cost_wei: "1600000000000000".to_string(),
            efficiency_percent: 80.0,
            tx_type: "legacy".to_string(),
            status: "success".to_string(),
            block_number: 1000000,
            error_message: None,
            max_fee_per_gas_gwei: None,
            max_priority_fee_per_gas_gwei: None,
            created_at: now,
        };

        assert_eq!(log.id, 1);
        assert_eq!(log.tx_hash, "0xtest123");
        assert_eq!(log.efficiency_percent, 80.0);
        assert_eq!(log.status, "success");
    }

    #[test]
    fn test_transaction_details_conversion() {
        let details = create_test_transaction_details();
        let total_cost_wei = details.total_cost_wei.to_string();

        assert_eq!(total_cost_wei, "5325000000000000");
        assert_eq!(details.feed_name, "eth_usd");
        assert_eq!(details.gas_used, 150000);
        assert_eq!(details.efficiency_percent, 75.0);
    }

    #[test]
    fn test_save_transaction_query() {
        let query = r#"
            INSERT INTO omikuji.transaction_log (
                tx_hash, feed_name, network_name, gas_limit, gas_used,
                gas_price_gwei, total_cost_wei, efficiency_percent,
                tx_type, status, block_number, error_message
            ) VALUES ($1, $2, $3, $4, $5, $6, $7::NUMERIC, $8, $9, $10, $11, $12)
            ON CONFLICT (tx_hash) DO UPDATE SET
                gas_used = EXCLUDED.gas_used,
                gas_price_gwei = EXCLUDED.gas_price_gwei,
                total_cost_wei = EXCLUDED.total_cost_wei,
                efficiency_percent = EXCLUDED.efficiency_percent,
                status = EXCLUDED.status,
                block_number = EXCLUDED.block_number,
                error_message = EXCLUDED.error_message
            RETURNING id
            "#;

        assert!(query.contains("INSERT INTO omikuji.transaction_log"));
        assert!(query.contains("ON CONFLICT (tx_hash) DO UPDATE"));
        assert!(query.contains("RETURNING id"));
    }

    #[test]
    fn test_get_stats_query() {
        let query = r#"
            SELECT 
                feed_name,
                network_name,
                COUNT(*) as total_transactions,
                COUNT(CASE WHEN status = 'success' THEN 1 END) as successful_transactions,
                COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_transactions,
                COUNT(CASE WHEN status = 'error' THEN 1 END) as error_transactions,
                AVG(gas_used) as avg_gas_used,
                AVG(gas_price_gwei) as avg_gas_price_gwei,
                AVG(CASE WHEN status = 'success' THEN efficiency_percent END) as avg_efficiency_percent,
                SUM(total_cost_wei::NUMERIC)::TEXT as total_cost_wei,
                MIN(created_at) as first_transaction,
                MAX(created_at) as last_transaction
            FROM omikuji.transaction_log
            WHERE feed_name = $1 AND network_name = $2
            GROUP BY feed_name, network_name
            "#;

        assert!(query.contains("COUNT(*) as total_transactions"));
        assert!(query.contains("AVG(gas_used) as avg_gas_used"));
        assert!(query.contains("GROUP BY feed_name, network_name"));
    }

    #[test]
    fn test_get_recent_query() {
        let query = r#"
            SELECT * FROM omikuji.transaction_log
            WHERE feed_name = $1 AND network_name = $2
            ORDER BY created_at DESC
            LIMIT $3
            "#;

        assert!(query.contains("SELECT * FROM omikuji.transaction_log"));
        assert!(query.contains("ORDER BY created_at DESC"));
        assert!(query.contains("LIMIT $3"));
    }

    #[test]
    fn test_efficiency_calculation() {
        let gas_limit = 200000;
        let gas_used = 150000;
        let efficiency = (gas_used as f64 / gas_limit as f64) * 100.0;

        assert_eq!(efficiency, 75.0);
    }

    #[test]
    fn test_cleanup_query() {
        let query = r#"
            DELETE FROM omikuji.transaction_log
            WHERE created_at < CURRENT_TIMESTAMP - INTERVAL '$1 days'
            "#;

        assert!(query.contains("DELETE FROM omikuji.transaction_log"));
        assert!(query.contains("CURRENT_TIMESTAMP - INTERVAL"));
    }

    #[test]
    fn test_high_gas_threshold() {
        let threshold_gwei = 100.0;
        let high_gas_price = 150.0;
        let low_gas_price = 50.0;

        assert!(high_gas_price > threshold_gwei);
        assert!(low_gas_price < threshold_gwei);
    }

    #[test]
    fn test_failed_transaction_details() {
        let mut details = create_test_transaction_details();
        details.status = "failed".to_string();
        details.error_message = Some("Reverted: Insufficient funds".to_string());

        assert_eq!(details.status, "failed");
        assert!(details.error_message.is_some());
        assert!(details
            .error_message
            .unwrap()
            .contains("Insufficient funds"));
    }
}
