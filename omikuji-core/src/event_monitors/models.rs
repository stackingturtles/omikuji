//! Core data structures for event monitoring

use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::models::ExecutionLimits;

/// Configuration for monitoring blockchain events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMonitor {
    /// Unique name for this monitor
    pub name: String,

    /// Network to monitor events on (must match a configured network)
    pub network: String,

    /// Contract address to monitor
    pub contract_address: Address,

    /// Event signature to monitor (e.g., "Transfer(address,address,uint256)")
    pub event_signature: String,

    /// Webhook configuration
    pub webhook: WebhookConfig,

    /// Response handling configuration
    #[serde(default)]
    pub response: ResponseConfig,

    /// Execution limits for contract calls (optional, will use defaults if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_limits: Option<ExecutionLimits>,
}

/// HTTP method for webhook calls
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    #[default]
    Post,
    Put,
    Patch,
    Delete,
}

/// Configuration for webhook endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookConfig {
    /// Webhook URL to call
    pub url: String,

    /// HTTP method to use
    #[serde(default)]
    pub method: HttpMethod,

    /// HTTP headers to send with the request
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts on failure
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u8,

    /// Delay between retry attempts in seconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay_seconds: u64,
}

/// Response handling configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseConfig {
    /// Type of response handling
    #[serde(rename = "type")]
    pub response_type: ResponseType,

    /// Configuration for contract calls (if response_type is ContractCall)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_call: Option<ContractCallConfig>,

    /// Validation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationConfig>,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            response_type: ResponseType::LogOnly,
            contract_call: None,
            validation: None,
        }
    }
}

/// Types of response handling
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    /// Only log the response
    LogOnly,
    /// Execute a contract call based on the response
    ContractCall,
    /// Store response in database
    StoreDb,
    /// Handle multiple actions
    MultiAction,
}

/// Configuration for contract call responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractCallConfig {
    /// Target contract address (can use template variables)
    pub target_contract: String,

    /// Maximum gas price in gwei
    pub max_gas_price_gwei: u64,

    /// Gas limit multiplier for estimation
    #[serde(default = "default_gas_multiplier")]
    pub gas_limit_multiplier: f64,

    /// ETH value to send in wei
    #[serde(default)]
    pub value_wei: u64,
}

/// Configuration for response validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationConfig {
    /// Whether to require signed responses
    #[serde(default)]
    pub require_signature: bool,

    /// List of allowed signer addresses
    #[serde(default)]
    pub allowed_signers: Vec<Address>,

    /// Maximum age of response in seconds
    #[serde(default = "default_max_response_age")]
    pub max_response_age_seconds: u64,
}

// Default value functions
fn default_timeout() -> u64 {
    30
}

fn default_retry_attempts() -> u8 {
    3
}

fn default_retry_delay() -> u64 {
    5
}

fn default_gas_multiplier() -> f64 {
    1.2
}

fn default_max_response_age() -> u64 {
    300 // 5 minutes
}

impl EventMonitor {
    /// Validate the event monitor configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate name
        if self.name.is_empty() {
            return Err("Event monitor name cannot be empty".to_string());
        }

        // Validate network
        if self.network.is_empty() {
            return Err("Network name cannot be empty".to_string());
        }

        // Validate event signature format
        if !self.event_signature.contains('(') || !self.event_signature.ends_with(')') {
            return Err(format!(
                "Invalid event signature format '{}', expected 'EventName(type1,type2,...)'",
                self.event_signature
            ));
        }

        // Validate webhook configuration
        self.webhook.validate()?;

        // Validate response configuration
        self.response.validate()?;

        Ok(())
    }
}

impl WebhookConfig {
    /// Validate webhook configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate URL
        if self.url.is_empty() {
            return Err("Webhook URL cannot be empty".to_string());
        }

        // Basic URL validation
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err("Webhook URL must start with http:// or https://".to_string());
        }

        // Validate timeout
        if self.timeout_seconds == 0 {
            return Err("Webhook timeout must be greater than 0".to_string());
        }

        // Validate retry configuration
        if self.retry_delay_seconds == 0 && self.retry_attempts > 0 {
            return Err("Retry delay must be greater than 0 when retries are enabled".to_string());
        }

        Ok(())
    }
}

impl ResponseConfig {
    /// Validate response configuration
    pub fn validate(&self) -> Result<(), String> {
        match self.response_type {
            ResponseType::ContractCall => {
                if self.contract_call.is_none() {
                    return Err(
                        "Contract call configuration required when response type is contract_call"
                            .to_string(),
                    );
                }
                if let Some(ref config) = self.contract_call {
                    config.validate()?;
                }
            }
            _ => {
                // Other response types don't require additional config yet
            }
        }

        if let Some(ref validation) = self.validation {
            validation.validate()?;
        }

        Ok(())
    }
}

impl ContractCallConfig {
    /// Validate contract call configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.target_contract.is_empty() {
            return Err("Target contract cannot be empty".to_string());
        }

        if self.max_gas_price_gwei == 0 {
            return Err("Maximum gas price must be greater than 0".to_string());
        }

        if self.gas_limit_multiplier <= 0.0 {
            return Err("Gas limit multiplier must be greater than 0".to_string());
        }

        Ok(())
    }
}

impl ValidationConfig {
    /// Validate validation configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.require_signature && self.allowed_signers.is_empty() {
            return Err(
                "Allowed signers list cannot be empty when signature is required".to_string(),
            );
        }

        if self.max_response_age_seconds == 0 {
            return Err("Maximum response age must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    fn test_event_monitor() -> EventMonitor {
        EventMonitor {
            name: "test_monitor".to_string(),
            network: "ethereum-mainnet".to_string(),
            contract_address: address!("1234567890123456789012345678901234567890"),
            event_signature: "TestEvent(uint256)".to_string(),
            webhook: WebhookConfig {
                url: "https://example.com/webhook".to_string(),
                method: HttpMethod::Post,
                headers: HashMap::new(),
                timeout_seconds: 30,
                retry_attempts: 3,
                retry_delay_seconds: 5,
            },
            response: ResponseConfig::default(),
            execution_limits: None,
        }
    }

    #[test]
    fn test_event_monitor_validation() {
        let monitor = test_event_monitor();
        assert!(monitor.validate().is_ok());

        // Test invalid event signature
        let mut invalid = monitor.clone();
        invalid.event_signature = "InvalidSignature".to_string();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_webhook_config_validation() {
        let webhook = WebhookConfig {
            url: "https://example.com".to_string(),
            method: HttpMethod::Post,
            headers: HashMap::new(),
            timeout_seconds: 30,
            retry_attempts: 3,
            retry_delay_seconds: 5,
        };
        assert!(webhook.validate().is_ok());

        // Test invalid URL
        let mut invalid = webhook.clone();
        invalid.url = "not-a-url".to_string();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_response_config_validation() {
        // Test contract call without config
        let invalid = ResponseConfig {
            response_type: ResponseType::ContractCall,
            contract_call: None,
            validation: None,
        };
        assert!(invalid.validate().is_err());

        // Test valid contract call config
        let valid = ResponseConfig {
            response_type: ResponseType::ContractCall,
            contract_call: Some(ContractCallConfig {
                target_contract: "0x123".to_string(),
                max_gas_price_gwei: 100,
                gas_limit_multiplier: 1.2,
                value_wei: 0,
            }),
            validation: None,
        };
        assert!(valid.validate().is_ok());
    }
}
