//! Single-node cluster provider (no-op clustering for community edition)

use anyhow::Result;
use async_trait::async_trait;
use omikuji_core::traits::cluster::{
    ClusterProvider, ClusterProviderConfig, ClusterProviderFactory, ClusterProviderType,
    DistributedLock, LeadershipInfo, NodeInfo, NodeStatus,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tracing::debug;

/// No-op distributed lock for single-node mode
struct NoOpLock;

#[async_trait]
impl DistributedLock for NoOpLock {
    async fn release(&self) -> Result<()> {
        Ok(())
    }

    async fn extend(&self, _duration: Duration) -> Result<()> {
        Ok(())
    }

    async fn is_held(&self) -> bool {
        true
    }
}

/// Single-node cluster provider (always leader, no coordination needed)
pub struct SingleNodeCluster {
    node_id: String,
}

impl SingleNodeCluster {
    pub fn new() -> Self {
        Self {
            node_id: "single-node-0".to_string(),
        }
    }
}

impl Default for SingleNodeCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClusterProvider for SingleNodeCluster {
    async fn initialize(&self) -> Result<()> {
        debug!("Initializing single-node cluster (no-op)");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        debug!("Shutting down single-node cluster (no-op)");
        Ok(())
    }

    async fn get_nodes(&self) -> Result<Vec<NodeInfo>> {
        Ok(vec![NodeInfo {
            id: self.node_id.clone(),
            address: "127.0.0.1:8080".parse::<SocketAddr>().unwrap(),
            status: NodeStatus::Active,
            last_seen: chrono::Utc::now(),
            metadata: HashMap::new(),
        }])
    }

    async fn get_self_info(&self) -> Result<NodeInfo> {
        Ok(NodeInfo {
            id: self.node_id.clone(),
            address: "127.0.0.1:8080".parse::<SocketAddr>().unwrap(),
            status: NodeStatus::Active,
            last_seen: chrono::Utc::now(),
            metadata: HashMap::new(),
        })
    }

    async fn get_leadership(&self) -> Result<LeadershipInfo> {
        Ok(LeadershipInfo {
            leader_id: Some(self.node_id.clone()),
            term: Some(1),
            is_leader: true,
        })
    }

    async fn acquire_lock(
        &self,
        _key: &str,
        _timeout: Duration,
    ) -> Result<Box<dyn DistributedLock>> {
        // Always succeeds immediately in single-node mode
        Ok(Box::new(NoOpLock))
    }

    async fn put(&self, _key: &str, _value: &[u8]) -> Result<()> {
        // No-op: no distributed storage in single-node mode
        debug!("Single-node cluster: put operation (no-op)");
        Ok(())
    }

    async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>> {
        // No distributed storage in single-node mode
        Ok(None)
    }

    async fn delete(&self, _key: &str) -> Result<()> {
        // No-op: no distributed storage in single-node mode
        debug!("Single-node cluster: delete operation (no-op)");
        Ok(())
    }

    async fn watch(
        &self,
        _key: &str,
        _callback: Box<dyn Fn(Option<Vec<u8>>) + Send + Sync + 'static>,
    ) -> Result<Box<dyn Send + Sync>> {
        // No-op: no distributed storage in single-node mode
        Ok(Box::new(()))
    }

    async fn broadcast(&self, _topic: &str, _message: &[u8]) -> Result<()> {
        // No-op: no other nodes to broadcast to
        debug!("Single-node cluster: broadcast operation (no-op)");
        Ok(())
    }

    async fn subscribe(
        &self,
        _topic: &str,
        _callback: Box<dyn Fn(&[u8]) + Send + Sync + 'static>,
    ) -> Result<Box<dyn Send + Sync>> {
        // No-op: no other nodes to receive messages from
        Ok(Box::new(()))
    }

    async fn health_check(&self) -> Result<()> {
        // Always healthy in single-node mode
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "single-node"
    }

    fn provider_type(&self) -> ClusterProviderType {
        ClusterProviderType::SingleNode
    }
}

/// Factory for creating single-node cluster providers
pub struct SingleNodeFactory;

impl SingleNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SingleNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClusterProviderFactory for SingleNodeFactory {
    async fn create(&self, _config: ClusterProviderConfig) -> Result<Box<dyn ClusterProvider>> {
        Ok(Box::new(SingleNodeCluster::new()))
    }

    fn provider_type(&self) -> ClusterProviderType {
        ClusterProviderType::SingleNode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_node_is_leader() {
        let cluster = SingleNodeCluster::new();
        let leadership = cluster.get_leadership().await.unwrap();
        assert!(leadership.is_leader);
        assert_eq!(leadership.leader_id, Some("single-node-0".to_string()));
    }

    #[tokio::test]
    async fn test_single_node_lock() {
        let cluster = SingleNodeCluster::new();
        let lock = cluster
            .acquire_lock("test-key", Duration::from_secs(1))
            .await
            .unwrap();
        assert!(lock.is_held().await);
    }

    #[tokio::test]
    async fn test_single_node_health() {
        let cluster = SingleNodeCluster::new();
        assert!(cluster.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_factory() {
        let factory = SingleNodeFactory::new();
        let cluster = factory
            .create(ClusterProviderConfig::SingleNode)
            .await
            .unwrap();
        assert_eq!(cluster.provider_name(), "single-node");
    }
}
