//! Core traits for the Omikuji plugin system
//!
//! This module defines the fundamental traits that enable
//! extensibility between the community and pro editions.

pub mod cluster;
pub mod monitoring;
pub mod secrets;

pub use cluster::{
    ClusterProvider, ClusterProviderConfig, ClusterProviderFactory, ClusterProviderType,
};
pub use monitoring::{
    MonitoringProvider, MonitoringProviderConfig, MonitoringProviderFactory, MonitoringProviderType,
};
pub use secrets::{
    SecretInfo, SecretProvider, SecretProviderConfig, SecretProviderFactory, SecretProviderType,
};
