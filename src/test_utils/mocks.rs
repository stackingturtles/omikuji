//! Mock implementations for testing

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Configuration for controlling mock failures
#[derive(Debug, Clone, Default)]
pub struct FailureConfig {
    pub fail_on_get: bool,
    pub fail_on_store: bool,
    pub fail_on_create: bool,
    pub fail_on_remove: bool,
    pub fail_on_list: bool,
    pub fail_after_calls: Option<usize>,
}

impl FailureConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_all() -> Self {
        Self {
            fail_on_get: true,
            fail_on_store: true,
            fail_on_create: true,
            fail_on_remove: true,
            fail_on_list: true,
            fail_after_calls: None,
        }
    }

    pub fn fail_after(calls: usize) -> Self {
        Self {
            fail_after_calls: Some(calls),
            ..Default::default()
        }
    }

    pub fn fail_on_operation(operation: &str) -> Self {
        match operation {
            "get" => Self {
                fail_on_get: true,
                ..Default::default()
            },
            "store" => Self {
                fail_on_store: true,
                ..Default::default()
            },
            "create" => Self {
                fail_on_create: true,
                ..Default::default()
            },
            "remove" => Self {
                fail_on_remove: true,
                ..Default::default()
            },
            "list" => Self {
                fail_on_list: true,
                ..Default::default()
            },
            _ => Self::default(),
        }
    }
}

/// Mock key storage implementation for testing
#[derive(Debug)]
pub struct MockKeyStorage {
    storage: Arc<RwLock<HashMap<String, String>>>,
    failure_config: FailureConfig,
    call_count: Arc<RwLock<usize>>,
}

impl MockKeyStorage {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            failure_config: FailureConfig::default(),
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn with_failures(failure_config: FailureConfig) -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            failure_config,
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn with_initial_data(data: HashMap<String, String>) -> Self {
        Self {
            storage: Arc::new(RwLock::new(data)),
            failure_config: FailureConfig::default(),
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    fn increment_call_count(&self) -> bool {
        let mut count = self.call_count.write().unwrap();
        *count += 1;

        if let Some(fail_after) = self.failure_config.fail_after_calls {
            *count > fail_after
        } else {
            false
        }
    }

    pub async fn store(&self, key: &str, value: &str) -> Result<()> {
        if self.failure_config.fail_on_store || self.increment_call_count() {
            return Err(anyhow!("Mock store failure"));
        }

        let mut storage = self.storage.write().unwrap();
        storage.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<String> {
        if self.failure_config.fail_on_get || self.increment_call_count() {
            return Err(anyhow!("Mock get failure"));
        }

        let storage = self.storage.read().unwrap();
        storage
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow!("Key not found: {}", key))
    }

    pub async fn remove(&self, key: &str) -> Result<()> {
        if self.failure_config.fail_on_remove || self.increment_call_count() {
            return Err(anyhow!("Mock remove failure"));
        }

        let mut storage = self.storage.write().unwrap();
        storage.remove(key);
        Ok(())
    }

    pub async fn list_keys(&self) -> Result<Vec<String>> {
        if self.failure_config.fail_on_list || self.increment_call_count() {
            return Err(anyhow!("Mock list failure"));
        }

        let storage = self.storage.read().unwrap();
        Ok(storage.keys().cloned().collect())
    }

    pub fn key_exists(&self, key: &str) -> bool {
        let storage = self.storage.read().unwrap();
        storage.contains_key(key)
    }

    pub fn get_call_count(&self) -> usize {
        *self.call_count.read().unwrap()
    }

    pub fn clear(&self) {
        let mut storage = self.storage.write().unwrap();
        storage.clear();
    }

    pub fn size(&self) -> usize {
        let storage = self.storage.read().unwrap();
        storage.len()
    }
}

impl Default for MockKeyStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock HTTP server builder for testing external API calls
pub struct MockHttpServer {
    server: mockito::ServerGuard,
}

impl MockHttpServer {
    pub async fn new() -> Self {
        Self {
            server: mockito::Server::new_async().await,
        }
    }

    pub fn mock_success_response(&mut self, path: &str, body: &str) -> mockito::Mock {
        self.server
            .mock("GET", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create()
    }

    pub fn mock_error_response(&mut self, path: &str, status: usize) -> mockito::Mock {
        self.server.mock("GET", path).with_status(status).create()
    }

    pub fn mock_timeout_response(&mut self, path: &str) -> mockito::Mock {
        self.server
            .mock("GET", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create()
    }

    pub fn mock_json_response(
        &mut self,
        path: &str,
        json_value: serde_json::Value,
    ) -> mockito::Mock {
        self.server
            .mock("GET", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(&json_value.to_string())
            .create()
    }

    pub fn url(&self) -> String {
        self.server.url()
    }

    pub fn reset(&mut self) {
        self.server.reset();
    }
}

/// Mock price cache for testing gas price calculations
#[derive(Debug)]
pub struct MockPriceCache {
    prices: Arc<RwLock<HashMap<String, (f64, u64)>>>, // (price, timestamp)
    failure_config: FailureConfig,
}

impl MockPriceCache {
    pub fn new() -> Self {
        Self {
            prices: Arc::new(RwLock::new(HashMap::new())),
            failure_config: FailureConfig::default(),
        }
    }

    pub fn with_failures(failure_config: FailureConfig) -> Self {
        Self {
            prices: Arc::new(RwLock::new(HashMap::new())),
            failure_config,
        }
    }

    pub fn set_price(&self, token: &str, price: f64, timestamp: u64) {
        let mut prices = self.prices.write().unwrap();
        prices.insert(token.to_string(), (price, timestamp));
    }

    pub fn get_price(&self, token: &str) -> Result<Option<(f64, u64)>> {
        if self.failure_config.fail_on_get {
            return Err(anyhow!("Mock price cache get failure"));
        }

        let prices = self.prices.read().unwrap();
        Ok(prices.get(token).copied())
    }

    pub fn clear(&self) {
        let mut prices = self.prices.write().unwrap();
        prices.clear();
    }

    pub fn size(&self) -> usize {
        let prices = self.prices.read().unwrap();
        prices.len()
    }

    pub fn has_token(&self, token: &str) -> bool {
        let prices = self.prices.read().unwrap();
        prices.contains_key(token)
    }
}

impl Default for MockPriceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock network provider for testing blockchain interactions
#[derive(Debug)]
pub struct MockNetworkProvider {
    responses: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    failure_config: FailureConfig,
    call_count: Arc<RwLock<usize>>,
}

impl MockNetworkProvider {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(RwLock::new(HashMap::new())),
            failure_config: FailureConfig::default(),
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn with_failures(failure_config: FailureConfig) -> Self {
        Self {
            responses: Arc::new(RwLock::new(HashMap::new())),
            failure_config,
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn set_response(&self, method: &str, response: serde_json::Value) {
        let mut responses = self.responses.write().unwrap();
        responses.insert(method.to_string(), response);
    }

    pub fn get_response(&self, method: &str) -> Result<serde_json::Value> {
        if self.failure_config.fail_on_get {
            return Err(anyhow!("Mock provider call failure"));
        }

        let mut call_count = self.call_count.write().unwrap();
        *call_count += 1;

        if let Some(fail_after) = self.failure_config.fail_after_calls {
            if *call_count > fail_after {
                return Err(anyhow!("Mock provider failure after {} calls", fail_after));
            }
        }

        let responses = self.responses.read().unwrap();
        responses
            .get(method)
            .cloned()
            .ok_or_else(|| anyhow!("No mock response for method: {}", method))
    }

    pub fn get_call_count(&self) -> usize {
        *self.call_count.read().unwrap()
    }

    pub fn reset(&self) {
        let mut responses = self.responses.write().unwrap();
        responses.clear();
        let mut call_count = self.call_count.write().unwrap();
        *call_count = 0;
    }

    pub fn add_standard_responses(&self) {
        self.set_response("eth_chainId", serde_json::json!("0x1"));
        self.set_response("eth_gasPrice", serde_json::json!("0x1dcd6500"));
        self.set_response("eth_getBalance", serde_json::json!("0x1bc16d674ec80000"));
        self.set_response("eth_blockNumber", serde_json::json!("0x112a880"));
    }
}

impl Default for MockNetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock database pool for testing database operations
#[derive(Debug)]
pub struct MockDatabasePool {
    is_connected: Arc<RwLock<bool>>,
    failure_config: FailureConfig,
    query_count: Arc<RwLock<usize>>,
}

impl MockDatabasePool {
    pub fn new() -> Self {
        Self {
            is_connected: Arc::new(RwLock::new(true)),
            failure_config: FailureConfig::default(),
            query_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn with_failures(failure_config: FailureConfig) -> Self {
        Self {
            is_connected: Arc::new(RwLock::new(true)),
            failure_config,
            query_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn disconnect(&self) {
        let mut connected = self.is_connected.write().unwrap();
        *connected = false;
    }

    pub fn reconnect(&self) {
        let mut connected = self.is_connected.write().unwrap();
        *connected = true;
    }

    pub fn is_connected(&self) -> bool {
        *self.is_connected.read().unwrap()
    }

    pub fn execute_query(&self, _query: &str) -> Result<()> {
        if !self.is_connected() {
            return Err(anyhow!("Database not connected"));
        }

        if self.failure_config.fail_on_get {
            return Err(anyhow!("Mock database query failure"));
        }

        let mut count = self.query_count.write().unwrap();
        *count += 1;

        if let Some(fail_after) = self.failure_config.fail_after_calls {
            if *count > fail_after {
                return Err(anyhow!("Database failure after {} queries", fail_after));
            }
        }

        Ok(())
    }

    pub fn get_query_count(&self) -> usize {
        *self.query_count.read().unwrap()
    }

    pub fn reset_query_count(&self) {
        let mut count = self.query_count.write().unwrap();
        *count = 0;
    }
}

impl Default for MockDatabasePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_key_storage() {
        let mock = MockKeyStorage::new();

        // Test store and get
        mock.store("test_key", "test_value").await.unwrap();
        let value = mock.get("test_key").await.unwrap();
        assert_eq!(value, "test_value");

        // Test key exists
        assert!(mock.key_exists("test_key"));
        assert!(!mock.key_exists("nonexistent_key"));

        // Test list keys
        let keys = mock.list_keys().await.unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"test_key".to_string()));

        // Test remove
        mock.remove("test_key").await.unwrap();
        assert!(!mock.key_exists("test_key"));
    }

    #[tokio::test]
    async fn test_mock_key_storage_failures() {
        let mock = MockKeyStorage::with_failures(FailureConfig::fail_on_operation("get"));

        // Store should work
        mock.store("test_key", "test_value").await.unwrap();

        // Get should fail
        let result = mock.get("test_key").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Mock get failure"));
    }

    #[tokio::test]
    async fn test_mock_key_storage_fail_after() {
        let mock = MockKeyStorage::with_failures(FailureConfig::fail_after(2));

        // First two operations should succeed
        mock.store("key1", "value1").await.unwrap();
        mock.store("key2", "value2").await.unwrap();

        // Third operation should fail
        let result = mock.store("key3", "value3").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_price_cache() {
        let cache = MockPriceCache::new();

        // Set and get price
        cache.set_price("ethereum", 2500.0, 1234567890);
        let price = cache.get_price("ethereum").unwrap().unwrap();
        assert_eq!(price, (2500.0, 1234567890));

        // Test non-existent token
        let price = cache.get_price("bitcoin").unwrap();
        assert!(price.is_none());

        // Test has_token
        assert!(cache.has_token("ethereum"));
        assert!(!cache.has_token("bitcoin"));
    }

    #[test]
    fn test_mock_network_provider() {
        let provider = MockNetworkProvider::new();

        // Set and get response
        provider.set_response("eth_chainId", serde_json::json!("0x1"));
        let response = provider.get_response("eth_chainId").unwrap();
        assert_eq!(response, serde_json::json!("0x1"));

        // Test call count
        assert_eq!(provider.get_call_count(), 1);

        // Test non-existent method
        let result = provider.get_response("unknown_method");
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_database_pool() {
        let pool = MockDatabasePool::new();

        // Test connected state
        assert!(pool.is_connected());

        // Test query execution
        pool.execute_query("SELECT 1").unwrap();
        assert_eq!(pool.get_query_count(), 1);

        // Test disconnection
        pool.disconnect();
        assert!(!pool.is_connected());

        let result = pool.execute_query("SELECT 1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[test]
    fn test_failure_config() {
        let config = FailureConfig::fail_all();
        assert!(config.fail_on_get);
        assert!(config.fail_on_store);
        assert!(config.fail_on_create);
        assert!(config.fail_on_remove);
        assert!(config.fail_on_list);

        let config = FailureConfig::fail_after(5);
        assert_eq!(config.fail_after_calls, Some(5));

        let config = FailureConfig::fail_on_operation("get");
        assert!(config.fail_on_get);
        assert!(!config.fail_on_store);
    }

    #[tokio::test]
    async fn test_mock_key_storage_with_initial_data() {
        let mut initial_data = HashMap::new();
        initial_data.insert("preset_key".to_string(), "preset_value".to_string());

        let mock = MockKeyStorage::with_initial_data(initial_data);

        let value = mock.get("preset_key").await.unwrap();
        assert_eq!(value, "preset_value");
        assert_eq!(mock.size(), 1);
    }
}
