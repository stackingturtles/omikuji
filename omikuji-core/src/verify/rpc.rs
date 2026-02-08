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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{Network, NetworkNode};
    use std::time::Duration;

    const TEST_TIMEOUT: Duration = Duration::from_secs(3);

    fn test_network(name: &str, nodes: Vec<NetworkNode>) -> Network {
        Network {
            name: name.to_string(),
            nodes,
            ..Network::default()
        }
    }

    fn test_node(name: &str, rpc_url: &str) -> NetworkNode {
        NetworkNode {
            name: name.to_string(),
            rpc_url: rpc_url.to_string(),
            ws_url: None,
        }
    }

    #[tokio::test]
    async fn test_check_rpc_empty_networks() {
        let results = check_rpc(&[], TEST_TIMEOUT).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_check_rpc_invalid_url() {
        let node = test_node("bad-node", "not a url at all");
        let network = test_network("testnet", vec![node]);
        let results = check_rpc(&[network], TEST_TIMEOUT).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert_eq!(results[0].category, CheckCategory::Rpc);
        assert!(results[0].message.contains("Invalid URL"));
        assert!(results[0].name.contains("testnet"));
        assert!(results[0].name.contains("bad-node"));
    }

    #[tokio::test]
    async fn test_check_rpc_unreachable_url() {
        let node = test_node("offline-node", "http://127.0.0.1:19877");
        let network = test_network("testnet", vec![node]);
        let results = check_rpc(&[network], TEST_TIMEOUT).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert_eq!(results[0].category, CheckCategory::Rpc);
        // Should fail on get_chain_id — either timeout or connection error
        let msg = &results[0].message;
        assert!(
            msg.contains("get_chain_id failed") || msg.contains("Timed out"),
            "Unexpected message: {msg}"
        );
    }

    #[tokio::test]
    async fn test_check_rpc_multiple_nodes() {
        let node1 = test_node("node-a", "not-a-url");
        let node2 = test_node("node-b", "http://127.0.0.1:19878");
        let network = test_network("multi", vec![node1, node2]);
        let results = check_rpc(&[network], TEST_TIMEOUT).await;

        // Both nodes should produce results
        assert_eq!(results.len(), 2);
        for result in &results {
            assert_eq!(result.status, CheckStatus::Fail);
            assert_eq!(result.category, CheckCategory::Rpc);
        }
    }
}
