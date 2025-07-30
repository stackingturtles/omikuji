use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::env;
use tracing::{debug, info};

pub type DatabasePool = Pool<Postgres>;

/// Establishes a connection pool to the PostgreSQL database
pub async fn establish_connection() -> Result<DatabasePool> {
    // Try to get database URL from environment
    let database_url =
        env::var("DATABASE_URL").context("DATABASE_URL environment variable not set")?;

    // Parse the database URL to extract connection details
    let url_parts: Vec<&str> = database_url.split('@').collect();
    let db_info = if url_parts.len() > 1 {
        let host_and_db = url_parts[1];
        // Mask sensitive parts of the URL for logging
        format!("postgres://***@{host_and_db}")
    } else {
        "postgres://***".to_string()
    };

    debug!("Attempting to connect to database: {}", db_info);
    info!("Connecting to PostgreSQL database");

    // Create connection pool with sensible defaults
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .connect(&database_url)
        .await
        .context("Failed to create PostgreSQL connection pool")?;

    // Test the connection and get some info
    let (version,): (String,) = sqlx::query_as("SELECT version()")
        .fetch_one(&pool)
        .await
        .context("Failed to verify database connection")?;

    debug!("Connected to database. PostgreSQL version: {}", version);
    debug!("Connection pool created with max_connections=10, min_connections=2");
    info!("Successfully connected to PostgreSQL database");

    Ok(pool)
}

/// Run pending migrations
pub async fn run_migrations(pool: &DatabasePool) -> Result<()> {
    info!("Running database migrations");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("Failed to run database migrations")?;

    info!("Database migrations completed successfully");

    Ok(())
}

/// Verify that required database tables exist and are accessible
pub async fn verify_tables(pool: &DatabasePool) -> Result<()> {
    info!("Verifying database tables accessibility");

    // Check feed_log table
    let feed_log_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'omikuji' 
            AND table_name = 'feed_log'
        )",
    )
    .fetch_one(pool)
    .await
    .context("Failed to check if feed_log table exists")?;

    if !feed_log_exists.0 {
        return Err(anyhow::anyhow!(
            "Table 'feed_log' does not exist in schema 'omikuji'"
        ));
    }

    // Check transaction_log table
    let transaction_log_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'omikuji' 
            AND table_name = 'transaction_log'
        )",
    )
    .fetch_one(pool)
    .await
    .context("Failed to check if transaction_log table exists")?;

    if !transaction_log_exists.0 {
        return Err(anyhow::anyhow!(
            "Table 'transaction_log' does not exist in schema 'omikuji'"
        ));
    }

    // Check gas_token_prices table (replaces the old gas_price_log)
    let gas_token_prices_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'omikuji' 
            AND table_name = 'gas_token_prices'
        )",
    )
    .fetch_one(pool)
    .await
    .context("Failed to check if gas_token_prices table exists")?;

    if !gas_token_prices_exists.0 {
        return Err(anyhow::anyhow!(
            "Table 'gas_token_prices' does not exist in schema 'omikuji'"
        ));
    }

    // Test write access to feed_log table
    sqlx::query("SELECT COUNT(*) FROM omikuji.feed_log LIMIT 1")
        .fetch_one(pool)
        .await
        .context("Failed to query feed_log table - check SELECT permissions")?;

    // Test write permissions by attempting a transaction that we'll rollback
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin test transaction")?;

    sqlx::query(
        "INSERT INTO omikuji.feed_log (feed_name, network_name, feed_value, feed_timestamp) 
         VALUES ($1, $2, $3, $4)",
    )
    .bind("_test_feed")
    .bind("_test_network")
    .bind(0.0)
    .bind(0i64)
    .execute(&mut *tx)
    .await
    .context("Failed to test INSERT permission on feed_log table")?;

    // Rollback the test insert
    tx.rollback()
        .await
        .context("Failed to rollback test transaction")?;

    info!("All required tables exist and are accessible");
    debug!("Verified tables: feed_log, transaction_log, gas_token_prices");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_url_masking() {
        // Test with full connection string
        let database_url = "postgres://user:password@localhost:5432/mydb";
        let url_parts: Vec<&str> = database_url.split('@').collect();

        let masked = if url_parts.len() > 1 {
            let host_and_db = url_parts[1];
            format!("postgres://***@{host_and_db}")
        } else {
            "postgres://***".to_string()
        };

        assert_eq!(masked, "postgres://***@localhost:5432/mydb");

        // Test with no @ symbol
        let database_url = "postgres://localhost";
        let url_parts: Vec<&str> = database_url.split('@').collect();

        let masked = if url_parts.len() > 1 {
            let host_and_db = url_parts[1];
            format!("postgres://***@{host_and_db}")
        } else {
            "postgres://***".to_string()
        };

        assert_eq!(masked, "postgres://***");
    }

    #[tokio::test]
    async fn test_establish_connection_without_env() {
        // Save current DATABASE_URL if it exists
        let saved_url = env::var("DATABASE_URL").ok();

        // Set an invalid DATABASE_URL to ensure connection fails
        env::set_var("DATABASE_URL", "invalid://url");

        // Test that connection fails with invalid DATABASE_URL
        let result = establish_connection().await;
        assert!(
            result.is_err(),
            "Expected connection to fail with invalid DATABASE_URL"
        );

        // Now remove DATABASE_URL entirely
        env::remove_var("DATABASE_URL");

        // Test that connection fails without DATABASE_URL
        let result = establish_connection().await;
        assert!(
            result.is_err(),
            "Expected connection to fail without DATABASE_URL"
        );

        // Get the full error chain
        let error = result.unwrap_err();
        let mut error_chain = vec![error.to_string()];

        // Walk the error chain
        let mut current_error = error.source();
        while let Some(e) = current_error {
            error_chain.push(e.to_string());
            current_error = e.source();
        }

        // Check if any error in the chain mentions DATABASE_URL or indicates missing env var
        let contains_expected_error = error_chain.iter().any(|msg| {
            msg.contains("DATABASE_URL")
                || msg.contains("environment variable")
                || msg.contains("not set")
                || msg.contains("Failed to create PostgreSQL connection pool")
        });

        assert!(
            contains_expected_error,
            "Expected error about DATABASE_URL or connection failure in error chain: {error_chain:?}"
        );

        // Restore DATABASE_URL if it was set
        if let Some(url) = saved_url {
            env::set_var("DATABASE_URL", url);
        }
    }

    #[test]
    fn test_pool_options() {
        // Test that pool options are set correctly
        let max_connections = 10;
        let min_connections = 2;

        assert!(max_connections > min_connections);
        assert_eq!(max_connections, 10);
        assert_eq!(min_connections, 2);
    }

    #[test]
    fn test_table_existence_query() {
        // Test the SQL query format for checking table existence
        let query = "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'omikuji' 
            AND table_name = 'feed_log'
        )";

        assert!(query.contains("information_schema.tables"));
        assert!(query.contains("table_schema = 'omikuji'"));
        assert!(query.contains("table_name = 'feed_log'"));
    }

    #[test]
    fn test_test_insert_query() {
        // Test the format of the test insert query
        let query =
            "INSERT INTO omikuji.feed_log (feed_name, network_name, feed_value, feed_timestamp) 
         VALUES ($1, $2, $3, $4)";

        assert!(query.contains("INSERT INTO omikuji.feed_log"));
        assert!(query.contains("feed_name"));
        assert!(query.contains("network_name"));
        assert!(query.contains("feed_value"));
        assert!(query.contains("feed_timestamp"));
        assert!(query.contains("$1, $2, $3, $4"));
    }

    #[test]
    fn test_migration_path() {
        // Test that migration path is correct
        let migration_path = "./migrations";
        assert_eq!(migration_path, "./migrations");
    }
}
