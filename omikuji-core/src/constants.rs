//! Application-wide constants
//!
//! This module contains all the magic numbers and default values used throughout
//! the application, making them easy to find and modify.

/// Gas-related constants
pub mod gas {
    /// Default gas limit for transactions when not specified
    pub const DEFAULT_GAS_LIMIT: u64 = 300_000;

    /// Gas estimation multiplier to add safety margin
    pub const GAS_ESTIMATION_MULTIPLIER: f64 = 1.2;

    /// Maximum number of fee bump attempts for stuck transactions
    pub const MAX_FEE_BUMP_ATTEMPTS: u32 = 3;

    /// Fee bump multiplier for each retry attempt
    pub const FEE_BUMP_MULTIPLIER: f64 = 1.1;

    /// Default max gas price in gwei (for safety)
    pub const DEFAULT_MAX_GAS_PRICE_GWEI: u64 = 500;

    /// Default priority fee in gwei for EIP-1559 transactions
    pub const DEFAULT_PRIORITY_FEE_GWEI: u64 = 2;
}

/// Time-related constants
pub mod time {
    /// Approximation for feed timestamp when exact time is unknown (in seconds)
    pub const FEED_TIMESTAMP_APPROXIMATION_SECS: u64 = 60;

    /// Default timeout for RPC calls (in seconds)
    pub const RPC_TIMEOUT_SECS: u64 = 30;

    /// Default check frequency for datafeeds (in seconds)
    pub const DEFAULT_CHECK_FREQUENCY_SECS: u64 = 60;

    /// Minimum update frequency for datafeeds (in seconds)
    pub const DEFAULT_MIN_UPDATE_FREQUENCY_SECS: u64 = 3600;
}

/// Network-related constants
pub mod network {
    /// Default HTTP request timeout (in seconds)
    pub const HTTP_REQUEST_TIMEOUT_SECS: u64 = 30;

    /// Maximum number of retries for failed network requests
    pub const MAX_NETWORK_RETRIES: u32 = 3;

    /// Delay between network retry attempts (in milliseconds)
    pub const NETWORK_RETRY_DELAY_MS: u64 = 1000;
}

/// Contract-related constants
pub mod contract {
    /// Default number of decimals for contract values
    pub const DEFAULT_DECIMALS: u8 = 8;

    /// Maximum allowed decimals for contract values
    pub const MAX_DECIMALS: u8 = 18;
}

/// Metrics-related constants
pub mod metrics {
    /// Port for Prometheus metrics server
    pub const METRICS_SERVER_PORT: u16 = 9090;

    /// Metrics update interval (in seconds)
    pub const METRICS_UPDATE_INTERVAL_SECS: u64 = 10;
}

/// Database-related constants
pub mod database {
    /// Maximum number of records to process in a single batch
    pub const BATCH_SIZE: usize = 1000;

    /// Default data retention period (in days)
    pub const DEFAULT_RETENTION_DAYS: i64 = 30;

    /// Connection pool maximum size
    pub const MAX_POOL_SIZE: u32 = 10;

    /// Connection timeout (in seconds)
    pub const CONNECTION_TIMEOUT_SECS: u64 = 30;
}

/// Validation constants
pub mod validation {
    /// Maximum length for feed names
    pub const MAX_FEED_NAME_LENGTH: usize = 64;

    /// Maximum length for network names
    pub const MAX_NETWORK_NAME_LENGTH: usize = 32;

    /// Maximum deviation threshold percentage
    pub const MAX_DEVIATION_THRESHOLD_PCT: f64 = 100.0;

    /// Minimum deviation threshold percentage
    pub const MIN_DEVIATION_THRESHOLD_PCT: f64 = 0.0;
}
