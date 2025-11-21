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
}
