//! Omikuji Core - Plugin traits and interfaces
//!
//! This crate defines the core traits that enable extensibility
//! between the Omikuji community edition and pro edition.
//!
//! # Architecture
//!
//! The plugin system is built around three main trait categories:
//!
//! - **Secret Management**: Different ways to store and retrieve private keys
//! - **Clustering**: Coordination between multiple Omikuji instances
//! - **Monitoring**: Metrics collection and alerting
//!
//! # Usage
//!
//! ## Community Edition
//!
//! ```rust,no_run
//! use omikuji_core::plugin_registry;
//! use omikuji_core::traits::SecretProviderConfig;
//!
//! async fn setup_community() {
//!     let registry = plugin_registry::global_registry();
//!
//!     // Register community providers
//!     // registry.register_secret_factory(env_provider_factory);
//!
//!     // Activate providers
//!     let secret_config = SecretProviderConfig::Env {
//!         prefix: Some("OMIKUJI_".to_string()),
//!     };
//!     registry.set_active_secret_provider(secret_config).await.unwrap();
//! }
//! ```
//!
//! ## Pro Edition
//!
//! ```rust,no_run
//! #[cfg(feature = "pro")]
//! async fn setup_pro() {
//!     use omikuji_core::plugin_registry;
//!     use omikuji_core::traits::{SecretProviderConfig, ClusterProviderConfig};
//!
//!     let registry = plugin_registry::global_registry();
//!
//!     // Register pro providers
//!     // registry.register_secret_factory(aws_secrets_factory);
//!     // registry.register_cluster_factory(raft_factory);
//!
//!     // Activate providers
//!     let secret_config = SecretProviderConfig::AwsSecrets {
//!         region: Some("us-east-1".to_string()),
//!         prefix: Some("omikuji/".to_string()),
//!     };
//!     registry.set_active_secret_provider(secret_config).await.unwrap();
//! }
//! ```

pub mod config;
pub mod constants;
pub mod contracts;
pub mod database;
pub mod datafeed;
pub mod error;
pub mod error_context;
pub mod error_handlers;
pub mod event_monitors;
pub mod gas;
pub mod gas_price;
pub mod metrics;
pub mod network;
pub mod plugin_registry;
pub mod runtime;
pub mod scheduled_tasks;
#[cfg(test)]
pub mod test_utils;
pub mod traits;
pub mod transaction;
pub mod ui;
pub mod utils;
pub mod wallet;

// Re-export commonly used items
pub use plugin_registry::{global_registry, PluginRegistry};
pub use runtime::{Daemon, DaemonBuilder};

// Add version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Add feature flags for conditional compilation
#[cfg(feature = "pro")]
pub const EDITION: &str = "pro";

#[cfg(not(feature = "pro"))]
pub const EDITION: &str = "community";

/// Initialize the plugin system
///
/// This should be called once at application startup
pub fn initialize() {
    tracing::info!(
        "Initializing Omikuji Core v{} ({} edition)",
        VERSION,
        EDITION
    );

    // The global registry is initialized lazily, but we can
    // trigger it here to ensure it's ready
    let _ = global_registry();
}
