//! Builder pattern for event monitor configuration

use super::error::{EventMonitorError, Result};
use super::models::{
    ContractCallConfig, EventMonitor, HttpMethod, ResponseConfig, ResponseType, ValidationConfig,
    WebhookConfig,
};
use alloy::primitives::Address;
use std::collections::HashMap;

/// Builder for creating EventMonitor configurations
pub struct EventMonitorBuilder {
    name: Option<String>,
    network: Option<String>,
    contract_address: Option<Address>,
    event_signature: Option<String>,
    webhook_url: Option<String>,
    webhook_method: HttpMethod,
    webhook_headers: HashMap<String, String>,
    webhook_timeout_seconds: u64,
    webhook_retry_attempts: u8,
    webhook_retry_delay_seconds: u64,
    response_type: ResponseType,
    contract_call_config: Option<ContractCallConfig>,
    validation_config: Option<ValidationConfig>,
}

impl Default for EventMonitorBuilder {
    fn default() -> Self {
        Self {
            name: None,
            network: None,
            contract_address: None,
            event_signature: None,
            webhook_url: None,
            webhook_method: HttpMethod::Post,
            webhook_headers: HashMap::new(),
            webhook_timeout_seconds: 30,
            webhook_retry_attempts: 3,
            webhook_retry_delay_seconds: 5,
            response_type: ResponseType::LogOnly,
            contract_call_config: None,
            validation_config: None,
        }
    }
}

impl EventMonitorBuilder {
    /// Create a new event monitor builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the monitor name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the network
    pub fn network(mut self, network: impl Into<String>) -> Self {
        self.network = Some(network.into());
        self
    }

    /// Set the contract address
    pub fn contract_address(mut self, address: Address) -> Self {
        self.contract_address = Some(address);
        self
    }

    /// Set the event signature
    pub fn event_signature(mut self, signature: impl Into<String>) -> Self {
        self.event_signature = Some(signature.into());
        self
    }

    /// Set the webhook URL
    pub fn webhook_url(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }

    /// Set the webhook HTTP method
    pub fn webhook_method(mut self, method: HttpMethod) -> Self {
        self.webhook_method = method;
        self
    }

    /// Add a webhook header
    pub fn webhook_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.webhook_headers.insert(key.into(), value.into());
        self
    }

    /// Set webhook headers
    pub fn webhook_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.webhook_headers = headers;
        self
    }

    /// Set webhook timeout
    pub fn webhook_timeout(mut self, seconds: u64) -> Self {
        self.webhook_timeout_seconds = seconds;
        self
    }

    /// Set webhook retry attempts
    pub fn webhook_retries(mut self, attempts: u8) -> Self {
        self.webhook_retry_attempts = attempts;
        self
    }

    /// Set webhook retry delay
    pub fn webhook_retry_delay(mut self, seconds: u64) -> Self {
        self.webhook_retry_delay_seconds = seconds;
        self
    }

    /// Set response type
    pub fn response_type(mut self, response_type: ResponseType) -> Self {
        self.response_type = response_type;
        self
    }

    /// Set contract call configuration
    pub fn contract_call(mut self, target: impl Into<String>, max_gas_price_gwei: u64) -> Self {
        self.contract_call_config = Some(ContractCallConfig {
            target_contract: target.into(),
            max_gas_price_gwei,
            gas_limit_multiplier: 1.2,
            value_wei: 0,
        });
        self.response_type = ResponseType::ContractCall;
        self
    }

    /// Set validation configuration
    pub fn require_signature(mut self, allowed_signers: Vec<Address>) -> Self {
        self.validation_config = Some(ValidationConfig {
            require_signature: true,
            allowed_signers,
            max_response_age_seconds: 300,
        });
        self
    }

    /// Set validation with custom age
    pub fn validation(
        mut self,
        require_signature: bool,
        allowed_signers: Vec<Address>,
        max_age_seconds: u64,
    ) -> Self {
        self.validation_config = Some(ValidationConfig {
            require_signature,
            allowed_signers,
            max_response_age_seconds: max_age_seconds,
        });
        self
    }

    /// Build the EventMonitor
    pub fn build(self) -> Result<EventMonitor> {
        let name = self.name.ok_or_else(|| EventMonitorError::ConfigError {
            monitor: "<unnamed>".to_string(),
            reason: "Monitor name is required".to_string(),
        })?;

        let network = self.network.ok_or_else(|| EventMonitorError::ConfigError {
            monitor: name.clone(),
            reason: "Network is required".to_string(),
        })?;

        let contract_address =
            self.contract_address
                .ok_or_else(|| EventMonitorError::ConfigError {
                    monitor: name.clone(),
                    reason: "Contract address is required".to_string(),
                })?;

        let event_signature =
            self.event_signature
                .ok_or_else(|| EventMonitorError::ConfigError {
                    monitor: name.clone(),
                    reason: "Event signature is required".to_string(),
                })?;

        let webhook_url = self
            .webhook_url
            .ok_or_else(|| EventMonitorError::ConfigError {
                monitor: name.clone(),
                reason: "Webhook URL is required".to_string(),
            })?;

        let webhook = WebhookConfig {
            url: webhook_url,
            method: self.webhook_method,
            headers: self.webhook_headers,
            timeout_seconds: self.webhook_timeout_seconds,
            retry_attempts: self.webhook_retry_attempts,
            retry_delay_seconds: self.webhook_retry_delay_seconds,
        };

        let response = ResponseConfig {
            response_type: self.response_type,
            contract_call: self.contract_call_config,
            validation: self.validation_config,
        };

        let monitor = EventMonitor {
            name,
            network,
            contract_address,
            event_signature,
            webhook,
            response,
            execution_limits: None,
        };

        // Validate the built monitor
        monitor
            .validate()
            .map_err(|reason| EventMonitorError::ConfigError {
                monitor: monitor.name.clone(),
                reason,
            })?;

        Ok(monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_basic_builder() {
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .build()
            .unwrap();

        assert_eq!(monitor.name, "test_monitor");
        assert_eq!(monitor.network, "ethereum-mainnet");
        assert_eq!(monitor.webhook.method, HttpMethod::Post);
        assert_eq!(monitor.webhook.retry_attempts, 3);
    }

    #[test]
    fn test_builder_with_contract_call() {
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .contract_call("0xabcd", 100)
            .build()
            .unwrap();

        assert_eq!(monitor.response.response_type, ResponseType::ContractCall);
        assert!(monitor.response.contract_call.is_some());
    }

    #[test]
    fn test_builder_with_validation() {
        let signers = vec![address!("1234567890123456789012345678901234567890")];

        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .require_signature(signers.clone())
            .build()
            .unwrap();

        let validation = monitor.response.validation.unwrap();
        assert!(validation.require_signature);
        assert_eq!(validation.allowed_signers, signers);
    }

    #[test]
    fn test_builder_missing_required() {
        let result = EventMonitorBuilder::new().build();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name is required"));
    }

    #[test]
    fn test_builder_with_headers() {
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .webhook_header("Authorization", "Bearer token")
            .webhook_header("X-Custom", "value")
            .build()
            .unwrap();

        assert_eq!(monitor.webhook.headers.len(), 2);
        assert_eq!(
            monitor.webhook.headers.get("Authorization").unwrap(),
            "Bearer token"
        );
    }

    #[test]
    fn test_webhook_method_setter() {
        // Test setting webhook method (covers lines 85-88)
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .webhook_method(HttpMethod::Get)
            .build()
            .unwrap();

        assert_eq!(monitor.webhook.method, HttpMethod::Get);

        // Test with PUT method
        let monitor_put = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .webhook_method(HttpMethod::Put)
            .build()
            .unwrap();

        assert_eq!(monitor_put.webhook.method, HttpMethod::Put);
    }

    #[test]
    fn test_webhook_headers_setter() {
        // Test setting webhook headers with HashMap (covers lines 97-100)
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("X-API-Key".to_string(), "secret".to_string());

        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .webhook_headers(headers.clone())
            .build()
            .unwrap();

        assert_eq!(monitor.webhook.headers, headers);
    }

    #[test]
    fn test_webhook_configuration_setters() {
        // Test webhook timeout, retries, and retry delay setters (covers lines 103-118)
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .webhook_timeout(60)
            .webhook_retries(5)
            .webhook_retry_delay(10)
            .build()
            .unwrap();

        assert_eq!(monitor.webhook.timeout_seconds, 60);
        assert_eq!(monitor.webhook.retry_attempts, 5);
        assert_eq!(monitor.webhook.retry_delay_seconds, 10);
    }

    #[test]
    fn test_response_type_setter() {
        // Test response type setter (covers lines 121-124)
        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .response_type(ResponseType::StoreDb)
            .build()
            .unwrap();

        assert_eq!(monitor.response.response_type, ResponseType::StoreDb);

        // Test with MultiAction type
        let monitor_multi = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .response_type(ResponseType::MultiAction)
            .build()
            .unwrap();

        assert_eq!(
            monitor_multi.response.response_type,
            ResponseType::MultiAction
        );
    }

    #[test]
    fn test_validation_with_custom_age() {
        // Test validation setter with custom parameters (covers lines 149-161)
        let signers = vec![
            address!("1234567890123456789012345678901234567890"),
            address!("abcdefabcdefabcdefabcdefabcdefabcdefabcd"),
        ];

        let monitor = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .validation(true, signers.clone(), 600)
            .build()
            .unwrap();

        let validation = monitor.response.validation.unwrap();
        assert!(validation.require_signature);
        assert_eq!(validation.allowed_signers, signers);
        assert_eq!(validation.max_response_age_seconds, 600);

        // Test with signature not required
        let monitor_no_sig = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .validation(false, vec![], 120)
            .build()
            .unwrap();

        let validation_no_sig = monitor_no_sig.response.validation.unwrap();
        assert!(!validation_no_sig.require_signature);
        assert!(validation_no_sig.allowed_signers.is_empty());
        assert_eq!(validation_no_sig.max_response_age_seconds, 120);
    }

    #[test]
    fn test_builder_missing_network() {
        // Test error when network is missing (covers lines 171-173)
        let result = EventMonitorBuilder::new()
            .name("test_monitor")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Network is required"));
        assert!(err.to_string().contains("test_monitor"));
    }

    #[test]
    fn test_builder_missing_contract_address() {
        // Test error when contract address is missing (covers lines 178-180)
        let result = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("https://example.com/webhook")
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Contract address is required"));
        assert!(err.to_string().contains("test_monitor"));
    }

    #[test]
    fn test_builder_missing_event_signature() {
        // Test error when event signature is missing (covers lines 185-187)
        let result = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .webhook_url("https://example.com/webhook")
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Event signature is required"));
        assert!(err.to_string().contains("test_monitor"));
    }

    #[test]
    fn test_builder_missing_webhook_url() {
        // Test error when webhook URL is missing (covers lines 192-194)
        let result = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Webhook URL is required"));
        assert!(err.to_string().contains("test_monitor"));
    }

    #[test]
    fn test_builder_validation_error() {
        // Test validation error propagation (covers lines 224-226)
        // Create a monitor with invalid event signature format
        let result = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("InvalidEventFormat") // Missing parentheses
            .webhook_url("https://example.com/webhook")
            .build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid event signature format"));
        assert!(err.to_string().contains("test_monitor"));

        // Test with empty webhook URL that passes builder but fails validation
        let result2 = EventMonitorBuilder::new()
            .name("test_monitor")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("Transfer(address,address,uint256)")
            .webhook_url("invalid-url") // Invalid URL format
            .build();

        assert!(result2.is_err());
        let err2 = result2.unwrap_err();
        assert!(err2
            .to_string()
            .contains("Webhook URL must start with http://"));
    }

    #[test]
    fn test_builder_complex_scenario() {
        // Test a complex scenario with all setters used together
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            "Bearer complex-token".to_string(),
        );

        let signers = vec![address!("1111111111111111111111111111111111111111")];

        let monitor = EventMonitorBuilder::new()
            .name("complex_monitor")
            .network("base-mainnet")
            .contract_address(address!("2222222222222222222222222222222222222222"))
            .event_signature("ComplexEvent(address,uint256,bytes)")
            .webhook_url("https://complex.example.com/webhook")
            .webhook_method(HttpMethod::Patch)
            .webhook_headers(headers.clone())
            .webhook_header("X-Extra", "value") // Add extra header
            .webhook_timeout(120)
            .webhook_retries(10)
            .webhook_retry_delay(15)
            .response_type(ResponseType::ContractCall)
            .contract_call("0x3333", 200)
            .validation(true, signers.clone(), 900)
            .build()
            .unwrap();

        // Verify all settings
        assert_eq!(monitor.name, "complex_monitor");
        assert_eq!(monitor.network, "base-mainnet");
        assert_eq!(monitor.webhook.method, HttpMethod::Patch);
        assert_eq!(monitor.webhook.timeout_seconds, 120);
        assert_eq!(monitor.webhook.retry_attempts, 10);
        assert_eq!(monitor.webhook.retry_delay_seconds, 15);
        assert_eq!(monitor.webhook.headers.len(), 2); // Original + extra
        assert_eq!(monitor.response.response_type, ResponseType::ContractCall);
        assert!(monitor.response.contract_call.is_some());
        assert!(monitor.response.validation.is_some());

        let validation = monitor.response.validation.unwrap();
        assert_eq!(validation.max_response_age_seconds, 900);
    }

    #[test]
    fn test_builder_defaults() {
        // Test that default values are correctly applied
        let monitor = EventMonitorBuilder::new()
            .name("default_test")
            .network("ethereum-mainnet")
            .contract_address(address!("1234567890123456789012345678901234567890"))
            .event_signature("DefaultEvent()")
            .webhook_url("https://example.com")
            .build()
            .unwrap();

        // Check default values
        assert_eq!(monitor.webhook.method, HttpMethod::Post);
        assert_eq!(monitor.webhook.timeout_seconds, 30);
        assert_eq!(monitor.webhook.retry_attempts, 3);
        assert_eq!(monitor.webhook.retry_delay_seconds, 5);
        assert_eq!(monitor.response.response_type, ResponseType::LogOnly);
        assert!(monitor.response.contract_call.is_none());
        assert!(monitor.response.validation.is_none());
    }
}
