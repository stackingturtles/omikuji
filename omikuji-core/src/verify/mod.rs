pub mod database;
pub mod feeds;
pub mod output;
pub mod rpc;
pub mod secrets;
pub mod types;

pub use output::*;
pub use types::*;

use std::time::{Duration, Instant};

use crate::config::models::OmikujiConfig;

pub struct VerifyOptions {
    pub database: bool,
    pub feeds: bool,
    pub rpc: bool,
    pub secrets: bool,
    pub json: bool,
    pub timeout: Duration,
}

pub async fn run_verification(
    config: &OmikujiConfig,
    options: &VerifyOptions,
) -> VerificationReport {
    let start = Instant::now();
    let mut all_checks = Vec::new();

    // When no specific category is selected, run all
    let run_all = !options.database && !options.feeds && !options.rpc && !options.secrets;

    let run_database = run_all || options.database;
    let run_feeds = run_all || options.feeds;
    let run_rpc = run_all || options.rpc;
    let run_secrets = run_all || options.secrets;

    // Run all selected categories concurrently
    let (db_results, feed_results, rpc_results, secret_results) = tokio::join!(
        async {
            if run_database {
                database::check_database(options.timeout).await
            } else {
                Vec::new()
            }
        },
        async {
            if run_feeds {
                feeds::check_feeds(&config.datafeeds, options.timeout).await
            } else {
                Vec::new()
            }
        },
        async {
            if run_rpc {
                rpc::check_rpc(&config.networks, options.timeout).await
            } else {
                Vec::new()
            }
        },
        async {
            if run_secrets {
                secrets::check_secrets(&config.key_storage, &config.networks, options.timeout).await
            } else {
                Vec::new()
            }
        },
    );

    all_checks.extend(db_results);
    all_checks.extend(feed_results);
    all_checks.extend(rpc_results);
    all_checks.extend(secret_results);

    VerificationReport {
        checks: all_checks,
        total_duration: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::metrics_config::MetricsConfig;
    use crate::config::models::*;
    use crate::gas_price::models::GasPriceFeedConfig;

    fn test_config() -> OmikujiConfig {
        OmikujiConfig {
            networks: vec![],
            datafeeds: vec![],
            database_cleanup: DatabaseCleanupConfig::default(),
            key_storage: KeyStorageConfig::default(),
            metrics: MetricsConfig::default(),
            gas_price_feeds: GasPriceFeedConfig::default(),
            scheduled_tasks: vec![],
            event_monitors: vec![],
            default_execution_limits: ExecutionLimits::default(),
        }
    }

    fn options_all_false() -> VerifyOptions {
        VerifyOptions {
            database: false,
            feeds: false,
            rpc: false,
            secrets: false,
            json: false,
            timeout: Duration::from_secs(3),
        }
    }

    #[tokio::test]
    async fn test_run_verification_all_flags_false_runs_all_categories() {
        // When all flags are false, run_all is true and all categories execute.
        // With empty config, feeds and rpc return empty. Database and secrets
        // will produce results depending on env state.
        let config = test_config();
        let options = options_all_false();

        let report = run_verification(&config, &options).await;

        // The report should be a valid VerificationReport
        assert!(report.total_duration.as_nanos() > 0);
        // Database checks always produce at least one result (DATABASE_URL check)
        let has_db_check = report
            .checks
            .iter()
            .any(|c| c.category == CheckCategory::Database);
        assert!(has_db_check, "Expected database checks to be present");

        // Secrets checks always produce at least one result (provider factory)
        let has_secrets_check = report
            .checks
            .iter()
            .any(|c| c.category == CheckCategory::Secrets);
        assert!(has_secrets_check, "Expected secrets checks to be present");
    }

    #[tokio::test]
    async fn test_run_verification_only_database_flag() {
        let config = test_config();
        let options = VerifyOptions {
            database: true,
            feeds: false,
            rpc: false,
            secrets: false,
            json: false,
            timeout: Duration::from_secs(3),
        };

        let report = run_verification(&config, &options).await;

        // Should only have database checks, no feed/rpc/secrets
        for check in &report.checks {
            assert_eq!(
                check.category,
                CheckCategory::Database,
                "Expected only Database checks, found {:?}",
                check.category
            );
        }
        assert!(
            !report.checks.is_empty(),
            "Expected at least one database check"
        );
    }

    #[tokio::test]
    async fn test_run_verification_only_feeds_flag() {
        let config = test_config();
        let options = VerifyOptions {
            database: false,
            feeds: true,
            rpc: false,
            secrets: false,
            json: false,
            timeout: Duration::from_secs(3),
        };

        let report = run_verification(&config, &options).await;

        // With empty datafeeds, feeds returns empty. No other categories should run.
        assert!(
            report.checks.is_empty(),
            "Expected no checks with empty datafeeds and only feeds flag"
        );
    }

    #[tokio::test]
    async fn test_run_verification_only_rpc_flag() {
        let config = test_config();
        let options = VerifyOptions {
            database: false,
            feeds: false,
            rpc: true,
            secrets: false,
            json: false,
            timeout: Duration::from_secs(3),
        };

        let report = run_verification(&config, &options).await;

        // With empty networks, rpc returns empty. No other categories should run.
        assert!(
            report.checks.is_empty(),
            "Expected no checks with empty networks and only rpc flag"
        );
    }

    #[tokio::test]
    async fn test_run_verification_returns_verification_report() {
        let config = test_config();
        let options = options_all_false();

        let report = run_verification(&config, &options).await;

        // Verify the report structure
        let (pass, warn, fail) = report.count_by_status();
        assert_eq!(
            pass + warn + fail,
            report.checks.len(),
            "count_by_status should sum to total checks"
        );
    }
}
