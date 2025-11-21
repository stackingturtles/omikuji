use thiserror::Error;

#[derive(Debug, Error)]
pub enum OmikujiError {
    #[error("Configuration error: {0}")]
    Config(#[from] crate::config::parser::ConfigError),

    // TODO: Re-enable this once network module is moved to core
    // #[error("Network error: {0}")]
    // Network(#[from] crate::network::NetworkError),
    #[error("Network error: {0}")]
    Network(String),

    #[error("Contract error: {0}")]
    Contract(#[from] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Metrics error: {0}")]
    Metrics(#[from] prometheus::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
