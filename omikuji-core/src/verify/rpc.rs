use std::time::{Duration, Instant};

use alloy::providers::{Provider, ProviderBuilder};
use futures::stream::{self, StreamExt};
use url::Url;

use crate::config::models::Network;

use super::types::{CheckCategory, CheckResult, CheckStatus};

pub async fn check_rpc(networks: &[Network], timeout: Duration) -> Vec<CheckResult> {
    let checks: Vec<_> = networks
        .iter()
        .flat_map(|network| {
            network
                .nodes
                .iter()
                .map(move |node| (network.name.clone(), node.clone()))
        })
        .collect();

    stream::iter(checks)
        .map(|(network_name, node)| check_one_node(network_name, node, timeout))
        .buffer_unordered(5)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
}

async fn check_one_node(
    network_name: String,
    node: crate::config::models::NetworkNode,
    timeout: Duration,
) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let name = format!("RPC: {}/{}", network_name, node.name);

    // 1. Parse URL
    let start = Instant::now();
    let url = match Url::parse(&node.rpc_url) {
        Ok(u) => u,
        Err(e) => {
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status: CheckStatus::Fail,
                message: format!("Invalid URL: {e}"),
                hint: Some(format!("URL: {}", node.rpc_url)),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    // 2. Create provider
    let provider = ProviderBuilder::new().on_http(url);

    // 3. Get chain ID
    let chain_id = match tokio::time::timeout(timeout, provider.get_chain_id()).await {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => {
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status: CheckStatus::Fail,
                message: format!("get_chain_id failed: {e}"),
                hint: Some(format!("Check RPC endpoint: {}", node.rpc_url)),
                duration: start.elapsed(),
            });
            return results;
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status: CheckStatus::Fail,
                message: format!("Timed out after {} s", timeout.as_secs()),
                hint: Some(format!("Check RPC endpoint: {}", node.rpc_url)),
                duration: start.elapsed(),
            });
            return results;
        }
    };

    // 4. Get block number
    match tokio::time::timeout(timeout, provider.get_block_number()).await {
        Ok(Ok(block_number)) => {
            let status = if block_number < 100 {
                CheckStatus::Warn
            } else {
                CheckStatus::Pass
            };
            let hint = if block_number < 100 {
                Some("Node may still be syncing".to_string())
            } else {
                None
            };
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status,
                message: format!("Chain ID: {chain_id}, Block: {block_number}"),
                hint,
                duration: start.elapsed(),
            });
        }
        Ok(Err(e)) => {
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status: CheckStatus::Warn,
                message: format!("Chain ID: {chain_id}, but get_block_number failed: {e}"),
                hint: None,
                duration: start.elapsed(),
            });
        }
        Err(_) => {
            results.push(CheckResult {
                category: CheckCategory::Rpc,
                name,
                status: CheckStatus::Warn,
                message: format!("Chain ID: {chain_id}, but get_block_number timed out"),
                hint: None,
                duration: start.elapsed(),
            });
        }
    }

    results
}
