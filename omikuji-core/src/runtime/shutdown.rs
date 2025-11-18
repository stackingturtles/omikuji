//! Graceful shutdown coordination
//!
//! This module provides infrastructure for coordinating graceful shutdown
//! of all daemon components. It ensures that:
//!
//! - In-flight transactions are completed or safely aborted
//! - Database connections are properly closed
//! - Network connections are cleanly terminated
//! - Resources are properly released
//! - Cleanup happens in the correct order
//!
//! # Signal Handling
//!
//! The shutdown system responds to standard Unix signals:
//!
//! - **SIGINT** (Ctrl+C): Initiates graceful shutdown
//! - **SIGTERM**: Initiates graceful shutdown
//!
//! # Shutdown Sequence
//!
//! When shutdown is initiated:
//!
//! 1. Stop accepting new work
//! 2. Signal all monitors to stop
//! 3. Wait for in-flight operations to complete (with timeout)
//! 4. Close database connections
//! 5. Close network connections
//! 6. Flush metrics
//! 7. Release all resources

use tokio::sync::watch;
use tracing::{debug, info, warn};

/// Handle for coordinating graceful shutdown
///
/// This handle is shared across all daemon components and provides
/// a way to signal and wait for shutdown.
///
/// # Example
///
/// ```rust
/// use omikuji_core::runtime::ShutdownHandle;
///
/// # async fn example() {
/// let shutdown = ShutdownHandle::new();
/// let shutdown_clone = shutdown.clone();
///
/// // In a component
/// tokio::spawn(async move {
///     loop {
///         tokio::select! {
///             _ = shutdown_clone.wait_for_shutdown() => {
///                 println!("Shutdown signal received");
///                 break;
///             }
///             _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
///                 println!("Still running...");
///             }
///         }
///     }
/// });
///
/// // Trigger shutdown after some time
/// tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
/// shutdown.trigger_shutdown();
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct ShutdownHandle {
    /// Watch channel for shutdown signaling
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle
    ///
    /// Initially, no shutdown is signaled.
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        debug!("Created new ShutdownHandle");
        Self { sender, receiver }
    }

    /// Trigger a shutdown
    ///
    /// Signals all components waiting on this handle that shutdown
    /// has been initiated.
    pub fn trigger_shutdown(&self) {
        info!("Triggering shutdown");
        if self.sender.send(true).is_err() {
            warn!("Failed to send shutdown signal (no receivers)");
        }
    }

    /// Wait for shutdown signal
    ///
    /// This async function will complete when shutdown is triggered.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use omikuji_core::runtime::ShutdownHandle;
    /// # async fn example() {
    /// let shutdown = ShutdownHandle::new();
    ///
    /// tokio::select! {
    ///     _ = shutdown.wait_for_shutdown() => {
    ///         println!("Shutting down");
    ///     }
    ///     _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
    ///         println!("Still running");
    ///     }
    /// }
    /// # }
    /// ```
    pub async fn wait_for_shutdown(&self) {
        let mut receiver = self.receiver.clone();
        let _ = receiver.wait_for(|shutdown| *shutdown).await;
        debug!("Shutdown signal received");
    }

    /// Check if shutdown has been triggered
    ///
    /// Non-blocking check for shutdown state.
    pub fn is_shutdown(&self) -> bool {
        *self.receiver.borrow()
    }
}

impl Default for ShutdownHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Install signal handlers for graceful shutdown
///
/// Sets up handlers for SIGINT (Ctrl+C) that will trigger the
/// provided shutdown handle.
///
/// # Arguments
///
/// * `shutdown` - The shutdown handle to trigger on signal
///
/// # Example
///
/// ```rust,no_run
/// # use omikuji_core::runtime::shutdown::{ShutdownHandle, install_signal_handlers};
/// # async fn example() -> anyhow::Result<()> {
/// let shutdown = ShutdownHandle::new();
/// install_signal_handlers(shutdown.clone())?;
///
/// // Now SIGINT will trigger shutdown
/// shutdown.wait_for_shutdown().await;
/// # Ok(())
/// # }
/// ```
pub fn install_signal_handlers(shutdown: ShutdownHandle) -> anyhow::Result<()> {
    info!("Installing signal handlers for graceful shutdown");

    tokio::spawn(async move {
        // Wait for SIGINT (Ctrl+C)
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received shutdown signal (SIGINT)");
                shutdown.trigger_shutdown();
            }
            Err(e) => {
                warn!("Failed to listen for ctrl_c signal: {}", e);
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_shutdown_handle_creation() {
        let shutdown = ShutdownHandle::new();
        assert!(!shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_trigger() {
        let shutdown = ShutdownHandle::new();
        assert!(!shutdown.is_shutdown());

        shutdown.trigger_shutdown();
        assert!(shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_wait() {
        let shutdown = ShutdownHandle::new();
        let shutdown_clone = shutdown.clone();

        let task = tokio::spawn(async move {
            shutdown_clone.wait_for_shutdown().await;
            "shutdown complete"
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        shutdown.trigger_shutdown();

        let result = task.await.unwrap();
        assert_eq!(result, "shutdown complete");
    }

    #[tokio::test]
    async fn test_multiple_waiters() {
        let shutdown = ShutdownHandle::new();

        let mut tasks = vec![];
        for _ in 0..5 {
            let shutdown_clone = shutdown.clone();
            tasks.push(tokio::spawn(async move {
                shutdown_clone.wait_for_shutdown().await;
            }));
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
        shutdown.trigger_shutdown();

        for task in tasks {
            task.await.unwrap();
        }
    }
}
