use anyhow::Result;
use async_trait::async_trait;
use secrecy::SecretString;

pub mod aws_secrets;
pub mod env;
pub mod factory;
pub mod keyring;
#[cfg(test)]
mod tests;
pub mod vault;

pub use aws_secrets::AwsSecretsStorage;
pub use env::EnvVarStorage;
pub use factory::create_key_storage;
pub use keyring::KeyringStorage;
pub use vault::VaultStorage;

#[async_trait]
pub trait KeyStorage: Send + Sync {
    async fn get_key(&self, network: &str) -> Result<SecretString>;
    async fn store_key(&self, network: &str, key: SecretString) -> Result<()>;
    async fn remove_key(&self, network: &str) -> Result<()>;
    async fn list_keys(&self) -> Result<Vec<String>>;
}
