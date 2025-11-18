//! Runtime module for Omikuji daemon orchestration
//!
//! This module contains the core runtime infrastructure that will be shared
//! between the Community and Pro editions. It provides:
//!
//! - **Daemon**: Main orchestrator that coordinates all monitoring and task execution
//! - **Builder**: Fluent API for constructing and configuring the daemon
//! - **Startup**: Initialization context and startup sequencing
//! - **Shutdown**: Graceful shutdown handling and cleanup
//!
//! # Architecture
//!
//! The runtime module follows the builder pattern to construct a fully configured
//! daemon instance that can run autonomously:
//!
//! ```rust,no_run
//! # use omikuji_core::runtime::DaemonBuilder;
//! # async fn example() -> anyhow::Result<()> {
//! let daemon = DaemonBuilder::new()
//!     // Configuration will be added in Phase 2
//!     .build()
//!     .await?;
//!
//! daemon.run().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Phase 2 Implementation
//!
//! In Phase 2, this module will be populated with:
//! - Network manager integration
//! - Database connection pooling
//! - Datafeed monitor lifecycle
//! - Event monitor lifecycle
//! - Scheduled task execution
//! - Metrics collection
//! - Transaction queue management

pub mod builder;
pub mod daemon;
pub mod shutdown;
pub mod startup;

// Re-export main types for convenience
pub use builder::DaemonBuilder;
pub use daemon::Daemon;
pub use shutdown::ShutdownHandle;
pub use startup::StartupContext;
