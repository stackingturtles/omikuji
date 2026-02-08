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
