use prometheus::{Encoder, TextEncoder};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

/// Start a simple Prometheus metrics server
pub async fn start_metrics_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    info!(
        "Starting Prometheus metrics server on http://{}/metrics",
        addr
    );

    // Spawn metrics server in background
    tokio::spawn(async move {
        if let Err(e) = run_metrics_server(addr).await {
            error!("Metrics server error: {}", e);
        }
    });

    Ok(())
}

async fn run_metrics_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (mut stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buffer = [0; 1024];

            // Read request (we don't parse it, just assume it's for /metrics)
            match stream.read(&mut buffer).await {
                Ok(_) => {
                    // Get metrics
                    let encoder = TextEncoder::new();
                    let metric_families = prometheus::gather();
                    let mut metrics_buffer = Vec::new();

                    if let Ok(()) = encoder.encode(&metric_families, &mut metrics_buffer) {
                        let metrics_string = String::from_utf8_lossy(&metrics_buffer);

                        // Simple HTTP response
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                            metrics_string.len(),
                            metrics_string
                        );

                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                }
                Err(e) => {
                    error!("Failed to read from stream: {}", e);
                }
            }
        });
    }
}
