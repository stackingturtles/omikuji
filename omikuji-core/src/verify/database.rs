use std::time::{Duration, Instant};

use sqlx::postgres::PgPoolOptions;

use super::types::{CheckCategory, CheckResult, CheckStatus};

pub async fn check_database(timeout: Duration) -> Vec<CheckResult> {
    let mut results = Vec::new();

    // 1. Check DATABASE_URL is set
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => {
            results.push(CheckResult {
                category: CheckCategory::Database,
                name: "DATABASE_URL set".to_string(),
                status: CheckStatus::Pass,
                message: "Environment variable is set".to_string(),
                hint: None,
                duration: Duration::ZERO,
            });
            url
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Database,
                name: "DATABASE_URL set".to_string(),
                status: CheckStatus::Fail,
                message: "Environment variable not set".to_string(),
                hint: Some("Set DATABASE_URL=postgres://user:pass@host:5432/dbname".to_string()),
                duration: Duration::ZERO,
            });
            return results;
        }
    };

    // 2. Check URL format
    let start = Instant::now();
    if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
        results.push(CheckResult {
            category: CheckCategory::Database,
            name: "URL format".to_string(),
            status: CheckStatus::Fail,
            message: "DATABASE_URL must start with postgres:// or postgresql://".to_string(),
            hint: Some("Example: postgres://user:pass@host:5432/dbname".to_string()),
            duration: start.elapsed(),
        });
        return results;
    }

    if url::Url::parse(&database_url).is_err() {
        results.push(CheckResult {
            category: CheckCategory::Database,
            name: "URL format".to_string(),
            status: CheckStatus::Fail,
            message: "DATABASE_URL is not a valid URL".to_string(),
            hint: Some("Example: postgres://user:pass@host:5432/dbname".to_string()),
            duration: start.elapsed(),
        });
        return results;
    }

    results.push(CheckResult {
        category: CheckCategory::Database,
        name: "URL format".to_string(),
        status: CheckStatus::Pass,
        message: "Valid PostgreSQL URL".to_string(),
        hint: None,
        duration: start.elapsed(),
    });

    // 3. Connectivity
    let start = Instant::now();
    let connect_result = tokio::time::timeout(
        timeout,
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url),
    )
    .await;

    let pool = match connect_result {
        Ok(Ok(pool)) => {
            // Get PG version
            let version_msg = match sqlx::query_as::<_, (String,)>("SELECT version()")
                .fetch_one(&pool)
                .await
            {
                Ok((version,)) => format!("Connected — {version}"),
                Err(_) => "Connected".to_string(),
            };
            results.push(CheckResult {
                category: CheckCategory::Database,
                name: "Connectivity".to_string(),
                status: CheckStatus::Pass,
                message: version_msg,
                hint: None,
                duration: start.elapsed(),
            });
            pool
        }
        Ok(Err(e)) => {
            results.push(CheckResult {
                category: CheckCategory::Database,
                name: "Connectivity".to_string(),
                status: CheckStatus::Fail,
                message: format!("Connection failed: {e}"),
                hint: Some(
                    "Check that PostgreSQL is running and credentials are correct".to_string(),
                ),
                duration: start.elapsed(),
            });
            return results;
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Database,
                name: "Connectivity".to_string(),
                status: CheckStatus::Fail,
                message: format!("Connection timed out after {} s", timeout.as_secs()),
                hint: Some("Check network connectivity to the database host".to_string()),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    // 4. Check required tables
    let tables = [
        ("omikuji.feed_log", "feed_log"),
        ("omikuji.transaction_log", "transaction_log"),
        ("omikuji.gas_token_prices", "gas_token_prices"),
    ];

    for (full_name, table_name) in &tables {
        let start = Instant::now();
        let query = format!(
            "SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'omikuji'
                AND table_name = '{table_name}'
            )"
        );
        match sqlx::query_as::<_, (bool,)>(&query).fetch_one(&pool).await {
            Ok((true,)) => {
                results.push(CheckResult {
                    category: CheckCategory::Database,
                    name: format!("Table {full_name}"),
                    status: CheckStatus::Pass,
                    message: "Table exists".to_string(),
                    hint: None,
                    duration: start.elapsed(),
                });
            }
            Ok((false,)) => {
                results.push(CheckResult {
                    category: CheckCategory::Database,
                    name: format!("Table {full_name}"),
                    status: CheckStatus::Fail,
                    message: "Table does not exist".to_string(),
                    hint: Some("Run database migrations first".to_string()),
                    duration: start.elapsed(),
                });
            }
            Err(e) => {
                results.push(CheckResult {
                    category: CheckCategory::Database,
                    name: format!("Table {full_name}"),
                    status: CheckStatus::Fail,
                    message: format!("Query failed: {e}"),
                    hint: None,
                    duration: start.elapsed(),
                });
            }
        }
    }

    pool.close().await;

    results
}
