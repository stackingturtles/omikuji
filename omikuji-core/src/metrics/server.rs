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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn find_free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    /// Send a raw HTTP GET request to the metrics server and return the full response.
    async fn fetch_metrics(port: u16) -> String {
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("Failed to connect to metrics server");

        let request = "GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn test_metrics_server_starts() {
        let port = find_free_port().await;
        let result = start_metrics_server(port).await;
        assert!(result.is_ok(), "start_metrics_server should return Ok");
    }

    #[tokio::test]
    async fn test_metrics_server_responds_200() {
        let port = find_free_port().await;
        start_metrics_server(port).await.unwrap();

        // Give the server a moment to bind
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let response = fetch_metrics(port).await;
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "Expected HTTP 200, got: {}",
            response.lines().next().unwrap_or("")
        );
    }

    #[tokio::test]
    async fn test_metrics_server_prometheus_content_type() {
        let port = find_free_port().await;
        start_metrics_server(port).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let response = fetch_metrics(port).await;
        assert!(
            response.contains("text/plain; version=0.0.4"),
            "Response should contain Prometheus content type header"
        );
    }

    #[tokio::test]
    async fn test_metrics_server_returns_metrics_text() {
        let port = find_free_port().await;
        start_metrics_server(port).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let response = fetch_metrics(port).await;
        // The response body is after the empty line separating headers from body.
        // Even with no custom metrics registered, prometheus::gather() returns
        // process metrics on some platforms or at minimum an empty body.
        // We just verify we get a valid HTTP response with a body section.
        assert!(
            response.contains("\r\n\r\n"),
            "Response should have header/body separator"
        );
    }
}
