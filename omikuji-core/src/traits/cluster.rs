//! Clustering trait for multi-node coordination
//!
//! This trait enables different clustering implementations,
//! from single-node operation to distributed consensus systems.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Configuration for cluster providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClusterProviderConfig {
    /// Single-node mode (no clustering)
    SingleNode,
    /// Raft consensus (Pro Edition)
    #[cfg(feature = "pro")]
    Raft {
        /// Node ID
        node_id: String,
        /// Listen address
        listen_addr: SocketAddr,
        /// Peer addresses
        peers: Vec<SocketAddr>,
        /// Data directory
        data_dir: String,
    },
    /// Etcd-based coordination (Pro Edition)
    #[cfg(feature = "pro")]
    Etcd {
        /// Etcd endpoints
        endpoints: Vec<String>,
        /// Optional TLS configuration
        tls_config: Option<TlsConfig>,
    },
}

/// TLS configuration for secure cluster communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to CA certificate
    pub ca_cert: String,
    /// Path to client certificate
    pub client_cert: String,
    /// Path to client key
    pub client_key: String,
}

/// Information about a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: String,
    /// Node address
    pub address: SocketAddr,
    /// Node status
    pub status: NodeStatus,
    /// Last seen timestamp
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// Node metadata
    pub metadata: HashMap<String, String>,
}

/// Node status in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeStatus {
    /// Node is healthy and active
    Active,
    /// Node is starting up
    Starting,
    /// Node is shutting down
    Stopping,
    /// Node is unreachable
    Unreachable,
    /// Node has failed
    Failed,
}

/// Leadership status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeadershipInfo {
    /// Current leader node ID
    pub leader_id: Option<String>,
    /// Term number (for Raft-like systems)
    pub term: Option<u64>,
    /// Whether this node is the leader
    pub is_leader: bool,
}

/// Distributed lock handle
#[async_trait]
pub trait DistributedLock: Send + Sync {
    /// Release the lock
    async fn release(&self) -> Result<()>;

    /// Extend the lock lease
    async fn extend(&self, duration: std::time::Duration) -> Result<()>;

    /// Check if the lock is still held
    async fn is_held(&self) -> bool;
}

/// Core trait for cluster coordination providers
#[async_trait]
pub trait ClusterProvider: Send + Sync {
    /// Initialize and join the cluster
    ///
    /// # Returns
    /// Ok(()) on successful initialization
    async fn initialize(&self) -> Result<()>;

    /// Shutdown and leave the cluster gracefully
    ///
    /// # Returns
    /// Ok(()) on clean shutdown
    async fn shutdown(&self) -> Result<()>;

    /// Get information about all nodes in the cluster
    ///
    /// # Returns
    /// A vector of node information
    async fn get_nodes(&self) -> Result<Vec<NodeInfo>>;

    /// Get information about this node
    ///
    /// # Returns
    /// Information about the current node
    async fn get_self_info(&self) -> Result<NodeInfo>;

    /// Get current leadership information
    ///
    /// # Returns
    /// Leadership status and information
    async fn get_leadership(&self) -> Result<LeadershipInfo>;

    /// Acquire a distributed lock
    ///
    /// # Arguments
    /// * `key` - The lock key/name
    /// * `timeout` - Maximum time to wait for the lock
    ///
    /// # Returns
    /// A lock handle if acquired, or an error
    async fn acquire_lock(
        &self,
        key: &str,
        timeout: std::time::Duration,
    ) -> Result<Box<dyn DistributedLock>>;

    /// Store a value in distributed storage
    ///
    /// # Arguments
    /// * `key` - The storage key
    /// * `value` - The value to store
    ///
    /// # Returns
    /// Ok(()) on success
    async fn put(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Retrieve a value from distributed storage
    ///
    /// # Arguments
    /// * `key` - The storage key
    ///
    /// # Returns
    /// The stored value, or None if not found
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a value from distributed storage
    ///
    /// # Arguments
    /// * `key` - The storage key
    ///
    /// # Returns
    /// Ok(()) on success (even if key didn't exist)
    async fn delete(&self, key: &str) -> Result<()>;

    /// Watch for changes to a key
    ///
    /// # Arguments
    /// * `key` - The key to watch
    /// * `callback` - Function to call when the key changes
    ///
    /// # Returns
    /// A watch handle that can be dropped to stop watching
    async fn watch(
        &self,
        key: &str,
        callback: Box<dyn Fn(Option<Vec<u8>>) + Send + Sync + 'static>,
    ) -> Result<Box<dyn Send + Sync>>;

    /// Broadcast a message to all nodes
    ///
    /// # Arguments
    /// * `topic` - The message topic
    /// * `message` - The message payload
    ///
    /// # Returns
    /// Ok(()) if message was sent (delivery not guaranteed)
    async fn broadcast(&self, topic: &str, message: &[u8]) -> Result<()>;

    /// Subscribe to broadcast messages
    ///
    /// # Arguments
    /// * `topic` - The topic to subscribe to
    /// * `callback` - Function to call when messages are received
    ///
    /// # Returns
    /// A subscription handle that can be dropped to unsubscribe
    async fn subscribe(
        &self,
        topic: &str,
        callback: Box<dyn Fn(&[u8]) + Send + Sync + 'static>,
    ) -> Result<Box<dyn Send + Sync>>;

    /// Check cluster health
    ///
    /// # Returns
    /// Ok(()) if cluster is healthy, or an error describing issues
    async fn health_check(&self) -> Result<()>;

    /// Get the name of this provider
    ///
    /// # Returns
    /// A human-readable name for this provider
    fn provider_name(&self) -> &str;

    /// Get the provider type
    ///
    /// # Returns
    /// The provider type identifier
    fn provider_type(&self) -> ClusterProviderType;
}

/// Types of cluster providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterProviderType {
    /// Single node (no clustering)
    SingleNode,
    /// Raft consensus
    Raft,
    /// Etcd coordination
    Etcd,
    /// Custom provider
    Custom,
}

/// Factory trait for creating cluster providers
#[async_trait]
pub trait ClusterProviderFactory: Send + Sync {
    /// Create a cluster provider from configuration
    async fn create(&self, config: ClusterProviderConfig) -> Result<Box<dyn ClusterProvider>>;

    /// Get the provider type this factory creates
    fn provider_type(&self) -> ClusterProviderType;
}
