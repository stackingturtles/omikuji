use super::*;
use secrecy::ExposeSecret;

/// Helper: build a VaultStorage pointing at the given mockito server URL.
async fn vault_with_server(server_url: &str) -> VaultStorage {
    VaultStorage::new(
        server_url,
        "secret",
        "omikuji",
        "token",
        Some("test-token".into()),
        None,
    )
    .await
    .expect("VaultStorage::new should succeed with valid token auth")
}

/// Vault KV v2 read response wrapped in the standard Vault envelope.
fn vault_read_response(key: &str, value: &str) -> String {
    format!(
        r#"{{"request_id":"test","lease_id":"","renewable":false,"lease_duration":0,"data":{{"data":{{"{key}":"{value}"}},"metadata":{{"created_time":"2024-01-01T00:00:00Z","deletion_time":"","destroyed":false,"version":1}}}}}}"#
    )
}

/// Vault KV v2 set response wrapped in the standard Vault envelope.
fn vault_set_response() -> &'static str {
    r#"{"request_id":"test","lease_id":"","renewable":false,"lease_duration":0,"data":{"created_time":"2024-01-01T00:00:00Z","deletion_time":"","destroyed":false,"version":1,"custom_metadata":null}}"#
}

/// Vault KV v2 list response wrapped in the standard Vault envelope.
fn vault_list_response(keys: &[&str]) -> String {
    let keys_json: Vec<String> = keys.iter().map(|k| format!(r#""{k}""#)).collect();
    format!(
        r#"{{"request_id":"test","lease_id":"","renewable":false,"lease_duration":0,"data":{{"keys":[{}]}}}}"#,
        keys_json.join(",")
    )
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_vault_new_token_auth() {
    let server = mockito::Server::new_async().await;
    let storage = VaultStorage::new(
        &server.url(),
        "secret",
        "omikuji",
        "token",
        Some("test-token".into()),
        None,
    )
    .await;
    assert!(
        storage.is_ok(),
        "VaultStorage::new should succeed with token auth"
    );
}

#[tokio::test]
async fn test_vault_new_missing_token() {
    let server = mockito::Server::new_async().await;
    let result = VaultStorage::new(
        &server.url(),
        "secret",
        "omikuji",
        "token",
        None, // missing token
        None,
    )
    .await;
    assert!(result.is_err());
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("Token required"),
        "Expected 'Token required' error, got: {err}"
    );
}

#[tokio::test]
async fn test_vault_new_unsupported_auth() {
    let server = mockito::Server::new_async().await;
    let result = VaultStorage::new(
        &server.url(),
        "secret",
        "omikuji",
        "kubernetes", // unsupported
        None,
        None,
    )
    .await;
    assert!(result.is_err());
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("Unsupported auth method"),
        "Expected 'Unsupported auth method' error, got: {err}"
    );
}

#[tokio::test]
async fn test_vault_get_key_from_server() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/secret/data/omikuji/ethereum")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_read_response("private_key", "0xdeadbeef"))
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;
    let key = storage.get_key("ethereum").await.unwrap();
    assert_eq!(key.expose_secret(), "0xdeadbeef");

    mock.assert_async().await;
}

#[tokio::test]
async fn test_vault_get_key_caches_result() {
    let mut server = mockito::Server::new_async().await;
    // Only expect ONE request — the second get_key should use cache.
    let mock = server
        .mock("GET", "/v1/secret/data/omikuji/ethereum")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_read_response("private_key", "0xcached"))
        .expect(1)
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;

    let key1 = storage.get_key("ethereum").await.unwrap();
    assert_eq!(key1.expose_secret(), "0xcached");

    // Second call should return cached value without making another HTTP request.
    let key2 = storage.get_key("ethereum").await.unwrap();
    assert_eq!(key2.expose_secret(), "0xcached");

    mock.assert_async().await;
}

#[tokio::test]
async fn test_vault_get_key_fallback_to_cache() {
    let mut server = mockito::Server::new_async().await;

    // First call succeeds and populates cache.
    let mock_ok = server
        .mock("GET", "/v1/secret/data/omikuji/ethereum")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_read_response("private_key", "0xfallback"))
        .create_async()
        .await;

    let storage = VaultStorage::new(
        &server.url(),
        "secret",
        "omikuji",
        "token",
        Some("test-token".into()),
        Some(0), // TTL = 0 seconds so cache expires immediately
    )
    .await
    .unwrap();

    let key1 = storage.get_key("ethereum").await.unwrap();
    assert_eq!(key1.expose_secret(), "0xfallback");
    mock_ok.assert_async().await;

    // Wait a tiny bit so the cache entry is expired (TTL=0).
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Second call: Vault returns 500, but should fall back to expired cached value.
    let mock_err = server
        .mock("GET", "/v1/secret/data/omikuji/ethereum")
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_body(r#"{"errors":["internal error"]}"#)
        .create_async()
        .await;

    let key2 = storage.get_key("ethereum").await.unwrap();
    assert_eq!(
        key2.expose_secret(),
        "0xfallback",
        "Should fall back to expired cache on Vault error"
    );
    mock_err.assert_async().await;
}

#[tokio::test]
async fn test_vault_store_key() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/secret/data/omikuji/polygon")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_set_response())
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;
    let secret = SecretString::from("0xnewkey".to_string());
    storage.store_key("polygon", secret).await.unwrap();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_vault_get_secret_path() {
    let server = mockito::Server::new_async().await;

    // With prefix
    let storage = vault_with_server(&server.url()).await;
    assert_eq!(storage.get_secret_path("ethereum"), "omikuji/ethereum");

    // Without prefix (empty)
    let storage_no_prefix = VaultStorage::new(
        &server.url(),
        "secret",
        "",
        "token",
        Some("test-token".into()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(storage_no_prefix.get_secret_path("ethereum"), "ethereum");
}

#[tokio::test]
async fn test_vault_remove_key() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("DELETE", "/v1/secret/data/omikuji/ethereum")
        .match_query(mockito::Matcher::Any)
        .with_status(204)
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;
    storage.remove_key("ethereum").await.unwrap();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_vault_list_keys() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("LIST", "/v1/secret/metadata/omikuji")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_list_response(&["ethereum", "polygon"]))
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;
    let keys = storage.list_keys().await.unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"ethereum".to_string()));
    assert!(keys.contains(&"polygon".to_string()));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_vault_list_keys_filters_directories() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("LIST", "/v1/secret/metadata/omikuji")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_list_response(&["ethereum", "subdir/", "polygon"]))
        .create_async()
        .await;

    let storage = vault_with_server(&server.url()).await;
    let keys = storage.list_keys().await.unwrap();
    // "subdir/" should be filtered out
    assert_eq!(keys.len(), 2);
    assert!(!keys.contains(&"subdir/".to_string()));

    mock.assert_async().await;
}
