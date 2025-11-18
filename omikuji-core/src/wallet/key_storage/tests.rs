#[cfg(test)]
mod tests {
    use super::super::*;
    use secrecy::SecretString;
    use std::env;

    #[tokio::test]
    async fn test_env_var_storage_get_key() {
        let storage = EnvVarStorage::new();

        // Test with network-specific env var
        env::set_var("OMIKUJI_PRIVATE_KEY_TEST_NETWORK", "test_key_1");
        let key = storage.get_key("test-network").await.unwrap();
        assert_eq!(secrecy::ExposeSecret::expose_secret(&key), "test_key_1");

        // Test with generic PRIVATE_KEY env var
        env::remove_var("OMIKUJI_PRIVATE_KEY_TEST_NETWORK");
        env::set_var("PRIVATE_KEY", "test_key_2");
        let key = storage.get_key("test-network").await.unwrap();
        assert_eq!(secrecy::ExposeSecret::expose_secret(&key), "test_key_2");

        // Clean up
        env::remove_var("PRIVATE_KEY");
    }

    #[tokio::test]
    async fn test_env_var_storage_list_keys() {
        let storage = EnvVarStorage::new();

        // Set up multiple network keys
        env::set_var("OMIKUJI_PRIVATE_KEY_ETHEREUM_MAINNET", "key1");
        env::set_var("OMIKUJI_PRIVATE_KEY_BASE_SEPOLIA", "key2");

        let keys = storage.list_keys().await.unwrap();
        assert!(keys.contains(&"ethereum-mainnet".to_string()));
        assert!(keys.contains(&"base-sepolia".to_string()));

        // Clean up
        env::remove_var("OMIKUJI_PRIVATE_KEY_ETHEREUM_MAINNET");
        env::remove_var("OMIKUJI_PRIVATE_KEY_BASE_SEPOLIA");
    }

    #[tokio::test]
    async fn test_env_var_storage_store_remove_fail() {
        let storage = EnvVarStorage::new();

        // Test that store fails
        let result = storage.store_key("test", SecretString::from("key")).await;
        assert!(result.is_err());

        // Test that remove fails
        let result = storage.remove_key("test").await;
        assert!(result.is_err());
    }

    // Note: Keyring tests are integration tests that require actual OS keyring access
    // They should be run manually or in a CI environment with proper setup
    #[tokio::test]
    #[ignore] // Ignore by default since it requires OS keyring
    async fn test_keyring_storage_roundtrip() {
        let storage = KeyringStorage::new(Some("omikuji-test".to_string()));
        let test_key = SecretString::from("test_private_key_12345");
        let network = "test-network";

        // Store key
        storage.store_key(network, test_key.clone()).await.unwrap();

        // Retrieve key
        let retrieved = storage.get_key(network).await.unwrap();
        assert_eq!(
            secrecy::ExposeSecret::expose_secret(&retrieved),
            secrecy::ExposeSecret::expose_secret(&test_key)
        );

        // Remove key
        storage.remove_key(network).await.unwrap();

        // Verify removal
        let result = storage.get_key(network).await;
        assert!(result.is_err());
    }
}
