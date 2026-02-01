//! Plugin registry for managing and accessing plugins
//!
//! The registry provides a centralized location for registering
//! and retrieving plugin implementations.

use crate::traits::{
    ClusterProvider, ClusterProviderFactory, ClusterProviderType, MonitoringProvider,
    MonitoringProviderFactory, MonitoringProviderType, SecretProvider, SecretProviderFactory,
    SecretProviderType,
};
use anyhow::{bail, Context, Result};
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Central plugin registry
pub struct PluginRegistry {
    /// Secret provider factories
    secret_factories: DashMap<SecretProviderType, Arc<dyn SecretProviderFactory>>,
    /// Cluster provider factories
    cluster_factories: DashMap<ClusterProviderType, Arc<dyn ClusterProviderFactory>>,
    /// Monitoring provider factories
    monitoring_factories: DashMap<MonitoringProviderType, Arc<dyn MonitoringProviderFactory>>,

    /// Active secret provider
    active_secret_provider: ArcSwap<Option<Arc<dyn SecretProvider>>>,
    /// Active cluster provider
    active_cluster_provider: ArcSwap<Option<Arc<dyn ClusterProvider>>>,
    /// Active monitoring providers (can have multiple)
    active_monitoring_providers: Arc<DashMap<String, Arc<dyn MonitoringProvider>>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            secret_factories: DashMap::new(),
            cluster_factories: DashMap::new(),
            monitoring_factories: DashMap::new(),
            active_secret_provider: ArcSwap::from(Arc::new(None)),
            active_cluster_provider: ArcSwap::from(Arc::new(None)),
            active_monitoring_providers: Arc::new(DashMap::new()),
        }
    }

    /// Register a secret provider factory
    pub fn register_secret_factory(&self, factory: Arc<dyn SecretProviderFactory>) -> Result<()> {
        let provider_type = factory.provider_type();

        if self.secret_factories.contains_key(&provider_type) {
            bail!("Secret provider factory for {provider_type:?} already registered");
        }

        self.secret_factories.insert(provider_type, factory);
        info!("Registered secret provider factory: {:?}", provider_type);
        Ok(())
    }

    /// Register a cluster provider factory
    pub fn register_cluster_factory(&self, factory: Arc<dyn ClusterProviderFactory>) -> Result<()> {
        let provider_type = factory.provider_type();

        if self.cluster_factories.contains_key(&provider_type) {
            bail!("Cluster provider factory for {provider_type:?} already registered");
        }

        self.cluster_factories.insert(provider_type, factory);
        info!("Registered cluster provider factory: {:?}", provider_type);
        Ok(())
    }

    /// Register a monitoring provider factory
    pub fn register_monitoring_factory(
        &self,
        factory: Arc<dyn MonitoringProviderFactory>,
    ) -> Result<()> {
        let provider_type = factory.provider_type();

        if self.monitoring_factories.contains_key(&provider_type) {
            bail!("Monitoring provider factory for {provider_type:?} already registered");
        }

        self.monitoring_factories.insert(provider_type, factory);
        info!(
            "Registered monitoring provider factory: {:?}",
            provider_type
        );
        Ok(())
    }

    /// Create a secret provider from a registered factory without activating it
    pub async fn create_secret_provider(
        &self,
        config: crate::traits::SecretProviderConfig,
    ) -> Result<Box<dyn SecretProvider>> {
        let provider_type = match &config {
            crate::traits::SecretProviderConfig::Env { .. } => SecretProviderType::Env,
            crate::traits::SecretProviderConfig::File { .. } => SecretProviderType::File,
            crate::traits::SecretProviderConfig::Keyring { .. } => SecretProviderType::Keyring,
            crate::traits::SecretProviderConfig::AwsSecrets { .. } => {
                SecretProviderType::AwsSecrets
            }
            crate::traits::SecretProviderConfig::Vault { .. } => SecretProviderType::Vault,
        };

        let factory = self
            .secret_factories
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("No factory registered for {provider_type:?}"))?;

        factory
            .create(config)
            .await
            .context("Failed to create secret provider")
    }

    /// Check if a secret provider factory is registered for the given type
    pub fn has_secret_factory(&self, provider_type: &SecretProviderType) -> bool {
        self.secret_factories.contains_key(provider_type)
    }

    /// Set the active secret provider
    pub async fn set_active_secret_provider(
        &self,
        config: crate::traits::SecretProviderConfig,
    ) -> Result<()> {
        let provider_type = match &config {
            crate::traits::SecretProviderConfig::Env { .. } => SecretProviderType::Env,
            crate::traits::SecretProviderConfig::File { .. } => SecretProviderType::File,
            crate::traits::SecretProviderConfig::Keyring { .. } => SecretProviderType::Keyring,
            crate::traits::SecretProviderConfig::AwsSecrets { .. } => {
                SecretProviderType::AwsSecrets
            }
            crate::traits::SecretProviderConfig::Vault { .. } => SecretProviderType::Vault,
        };

        let factory = self
            .secret_factories
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("No factory registered for {provider_type:?}"))?;

        let provider = factory
            .create(config)
            .await
            .context("Failed to create secret provider")?;

        // Health check before activating
        provider
            .health_check()
            .await
            .context("Secret provider health check failed")?;

        self.active_secret_provider
            .store(Arc::new(Some(Arc::from(provider))));
        info!("Activated secret provider: {:?}", provider_type);

        Ok(())
    }

    /// Set the active cluster provider
    pub async fn set_active_cluster_provider(
        &self,
        config: crate::traits::ClusterProviderConfig,
    ) -> Result<()> {
        let provider_type = match &config {
            crate::traits::ClusterProviderConfig::SingleNode => ClusterProviderType::SingleNode,
            #[cfg(feature = "pro")]
            crate::traits::ClusterProviderConfig::Raft { .. } => ClusterProviderType::Raft,
            #[cfg(feature = "pro")]
            crate::traits::ClusterProviderConfig::Etcd { .. } => ClusterProviderType::Etcd,
        };

        let factory = self
            .cluster_factories
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("No factory registered for {provider_type:?}"))?;

        let provider = factory
            .create(config)
            .await
            .context("Failed to create cluster provider")?;

        // Initialize the cluster provider
        provider
            .initialize()
            .await
            .context("Failed to initialize cluster provider")?;

        self.active_cluster_provider
            .store(Arc::new(Some(Arc::from(provider))));
        info!("Activated cluster provider: {:?}", provider_type);

        Ok(())
    }

    /// Add a monitoring provider
    pub async fn add_monitoring_provider(
        &self,
        name: String,
        config: crate::traits::MonitoringProviderConfig,
    ) -> Result<()> {
        let provider_type = match &config {
            crate::traits::MonitoringProviderConfig::Prometheus { .. } => {
                MonitoringProviderType::Prometheus
            }
            #[cfg(feature = "pro")]
            crate::traits::MonitoringProviderConfig::DataDog { .. } => {
                MonitoringProviderType::DataDog
            }
            #[cfg(feature = "pro")]
            crate::traits::MonitoringProviderConfig::NewRelic { .. } => {
                MonitoringProviderType::NewRelic
            }
            #[cfg(feature = "pro")]
            crate::traits::MonitoringProviderConfig::CloudWatch { .. } => {
                MonitoringProviderType::CloudWatch
            }
        };

        let factory = self
            .monitoring_factories
            .get(&provider_type)
            .ok_or_else(|| anyhow::anyhow!("No factory registered for {provider_type:?}"))?;

        let provider = factory
            .create(config)
            .await
            .context("Failed to create monitoring provider")?;

        // Initialize the monitoring provider
        provider
            .initialize()
            .await
            .context("Failed to initialize monitoring provider")?;

        self.active_monitoring_providers
            .insert(name.clone(), Arc::from(provider));
        info!("Added monitoring provider '{}': {:?}", name, provider_type);

        Ok(())
    }

    /// Get the active secret provider
    pub fn get_secret_provider(&self) -> Option<Arc<dyn SecretProvider>> {
        self.active_secret_provider.load().as_ref().clone()
    }

    /// Get the active cluster provider
    pub fn get_cluster_provider(&self) -> Option<Arc<dyn ClusterProvider>> {
        self.active_cluster_provider.load().as_ref().clone()
    }

    /// Get a monitoring provider by name
    pub fn get_monitoring_provider(&self, name: &str) -> Option<Arc<dyn MonitoringProvider>> {
        self.active_monitoring_providers
            .get(name)
            .map(|p| p.clone())
    }

    /// Get all monitoring providers
    pub fn get_all_monitoring_providers(&self) -> Vec<(String, Arc<dyn MonitoringProvider>)> {
        self.active_monitoring_providers
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Shutdown all active providers
    pub async fn shutdown(&self) -> Result<()> {
        // Shutdown cluster provider
        if let Some(provider) = self.get_cluster_provider() {
            debug!("Shutting down cluster provider");
            if let Err(e) = provider.shutdown().await {
                warn!("Error shutting down cluster provider: {}", e);
            }
        }

        // Shutdown monitoring providers
        for (name, provider) in self.get_all_monitoring_providers() {
            debug!("Shutting down monitoring provider: {}", name);
            if let Err(e) = provider.shutdown().await {
                warn!("Error shutting down monitoring provider '{}': {}", name, e);
            }
        }

        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global plugin registry instance
static GLOBAL_REGISTRY: once_cell::sync::Lazy<PluginRegistry> =
    once_cell::sync::Lazy::new(PluginRegistry::new);

/// Get the global plugin registry
pub fn global_registry() -> &'static PluginRegistry {
    &GLOBAL_REGISTRY
}
