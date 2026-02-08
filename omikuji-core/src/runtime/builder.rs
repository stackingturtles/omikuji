//! Builder pattern for daemon construction
//!
//! This module provides a fluent API for constructing and configuring
//! the Omikuji daemon. It ensures that all required components are
//! properly initialized before the daemon can run.
//!
//! # Example
//!
//! ```rust,no_run
//! # use omikuji_core::runtime::DaemonBuilder;
//! # use std::path::PathBuf;
//! # async fn example() -> anyhow::Result<()> {
//! let daemon = DaemonBuilder::new()
//!     .config_path(PathBuf::from("config.yaml"))
//!     .private_key_env("OMIKUJI_PRIVATE_KEY")
//!     .build()
//!     .await?;
//!
//! daemon.run().await?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, info};

use super::shutdown::install_signal_handlers;
use super::{Daemon, ShutdownHandle, StartupContext};
use crate::config;

/// Builder for constructing a daemon instance
///
/// Uses the builder pattern to ensure all required components are
/// configured before the daemon is built.
pub struct DaemonBuilder {
    /// Path to configuration file
    config_path: Option<PathBuf>,
    /// Private key environment variable name (for backward compatibility)
    private_key_env: String,
}

impl DaemonBuilder {
    /// Create a new daemon builder with default settings
    ///
    /// # Example
    ///
    /// ```rust
    /// # use omikuji_core::runtime::DaemonBuilder;
    /// let builder = DaemonBuilder::new();
    /// ```
    pub fn new() -> Self {
        debug!("Creating new DaemonBuilder");
        Self {
            config_path: None,
            private_key_env: "OMIKUJI_PRIVATE_KEY".to_string(),
        }
    }

    /// Set the configuration file path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to YAML configuration file
    ///
    /// # Example
    ///
    /// ```rust
    /// # use omikuji_core::runtime::DaemonBuilder;
    /// # use std::path::PathBuf;
    /// let builder = DaemonBuilder::new()
    ///     .config_path(PathBuf::from("config.yaml"));
    /// ```
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Set the private key environment variable name
    ///
    /// Used for backward compatibility when keyring lookups fail.
    ///
    /// # Arguments
    ///
    /// * `env_var` - Environment variable name (default: "OMIKUJI_PRIVATE_KEY")
    ///
    /// # Example
    ///
    /// ```rust
    /// # use omikuji_core::runtime::DaemonBuilder;
    /// let builder = DaemonBuilder::new()
    ///     .private_key_env("MY_PRIVATE_KEY");
    /// ```
    pub fn private_key_env(mut self, env_var: impl Into<String>) -> Self {
        self.private_key_env = env_var.into();
        self
    }

    /// Build the daemon instance
    ///
    /// Performs all initialization and returns a ready-to-run daemon.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration is invalid or cannot be loaded
    /// - Required components fail to initialize
    /// - Database connection fails (if configured)
    /// - Network connections fail
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use omikuji_core::runtime::DaemonBuilder;
    /// # async fn example() -> anyhow::Result<()> {
    /// let daemon = DaemonBuilder::new()
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build(self) -> Result<Daemon> {
        info!("Building daemon");

        // Determine configuration path
        let config_path = self.config_path.unwrap_or_else(config::default_config_path);

        info!("Using configuration file: {}", config_path.display());

        // Create startup context
        debug!("Creating startup context");
        let mut startup_ctx = StartupContext::new(&config_path, self.private_key_env).await?;

        // Initialize all components
        debug!("Initializing components");
        startup_ctx.initialize_components().await?;

        // Create shutdown handle and install signal handlers
        debug!("Creating shutdown handle");
        let shutdown = ShutdownHandle::new();
        install_signal_handlers(shutdown.clone())?;

        // Create daemon instance
        debug!("Creating daemon instance");
        let daemon = Daemon::new(startup_ctx, shutdown);

        info!("Daemon built successfully");
        Ok(daemon)
    }
}

impl Default for DaemonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_builder_creation() {
        let _builder = DaemonBuilder::new();
    }

    #[test]
    fn test_builder_default() {
        let _builder = DaemonBuilder::default();
    }

    #[test]
    fn test_builder_configuration() {
        let builder = DaemonBuilder::new()
            .config_path(PathBuf::from("test_config.yaml"))
            .private_key_env("TEST_KEY");

        assert_eq!(builder.private_key_env, "TEST_KEY");
        assert_eq!(builder.config_path, Some(PathBuf::from("test_config.yaml")));
    }

    #[tokio::test]
    async fn test_build_with_invalid_config_path() {
        let result = DaemonBuilder::new()
            .config_path(PathBuf::from("/tmp/nonexistent_omikuji_test_config.yaml"))
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_with_valid_config() {
        let anvil = Anvil::new().try_spawn().expect("Anvil required");
        let url = anvil.endpoint();

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        let yaml = format!(
            r#"
networks:
  - name: anvil
    transaction_type: eip1559
    nodes:
      - name: Test Anvil
        rpc_url: {url}

datafeeds: []
scheduled_tasks: []
event_monitors: []

key_storage:
  storage_type: env
"#
        );
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        // Ensure DATABASE_URL is not set so DB init is skipped
        std::env::remove_var("DATABASE_URL");

        // Verify StartupContext::new succeeds with this config
        // (We avoid full build() which calls initialize_components → init_metrics_config
        // that uses a global OnceCell and panics if already initialized by another test)
        let ctx = super::super::StartupContext::new(&path, "TEST_KEY".to_string()).await;
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.config.networks.len(), 1);
        assert_eq!(ctx.config.networks[0].name, "anvil");
    }
}
