//! Integration tests for transaction execution from event monitors

use alloy::primitives::{address, Address, U256};
use omikuji::config::models::{ExecutionLimits, OmikujiConfig};
use omikuji::event_monitors::models::{
    EventMonitor, HttpMethod, ResponseConfig, ResponseType, WebhookConfig,
};
use omikuji::event_monitors::{ContractCall, WebhookResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// Create a test configuration with execution limits
fn create_test_config() -> OmikujiConfig {
    let yaml = r#"
networks:
  - name: ethereum-mainnet
    nodes:
      - name: "Primary Node"
        rpc_url: https://eth.llamarpc.com
        ws_url: wss://eth.llamarpc.com

datafeeds: []

default_execution_limits:
  max_value_wei: "1000000000000000000"  # 1 ETH
  max_gas_price_gwei: 100

event_monitors:
  - name: test_monitor
    network: ethereum-mainnet
    contract_address: "0x1234567890123456789012345678901234567890"
    event_signature: "TestEvent(uint256)"
    webhook:
      url: https://example.com/webhook
      method: POST
      timeout_seconds: 30
      retry_attempts: 3
      retry_delay_seconds: 5
    response:
      type: contract_call
    execution_limits:
      max_value_wei: "500000000000000000"  # 0.5 ETH (override default)
      max_gas_price_gwei: 50
"#;

    serde_yaml::from_str(yaml).unwrap()
}

#[test]
fn test_execution_limits_parsing() {
    let config = create_test_config();

    // Check default execution limits
    assert_eq!(
        config.default_execution_limits.max_value_wei,
        "1000000000000000000"
    );
    assert_eq!(config.default_execution_limits.max_gas_price_gwei, 100);

    // Check monitor-specific execution limits
    assert!(config.event_monitors[0].execution_limits.is_some());
    let limits = config.event_monitors[0].execution_limits.as_ref().unwrap();
    assert_eq!(limits.max_value_wei, "500000000000000000");
    assert_eq!(limits.max_gas_price_gwei, 50);
}

#[test]
fn test_webhook_response_with_contract_calls() {
    let response = WebhookResponse {
        action: "execute".to_string(),
        calls: Some(vec![
            ContractCall {
                target: "0x1234567890123456789012345678901234567890".to_string(),
                function: "updateValue(uint256,address)".to_string(),
                params: vec![
                    serde_json::json!(42),
                    serde_json::json!("0x5678901234567890123456789012345678901234"),
                ],
                value: "1000000000000000000".to_string(), // 1 ETH
            },
            ContractCall {
                target: "0x1234567890123456789012345678901234567890".to_string(),
                function: "setFlag(bool)".to_string(),
                params: vec![serde_json::json!(true)],
                value: "0".to_string(),
            },
        ]),
        metadata: Some(serde_json::json!({
            "log": "Processing event"
        })),
        extra: serde_json::Map::new(),
    };

    assert!(response.calls.is_some());
    let calls = response.calls.unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].function, "updateValue(uint256,address)");
    assert_eq!(calls[1].function, "setFlag(bool)");
}

#[test]
fn test_same_contract_validation() {
    let config = Arc::new(create_test_config());
    let monitor = &config.event_monitors[0];

    // Test call to same contract (should be valid)
    let valid_call = ContractCall {
        target: "0x1234567890123456789012345678901234567890".to_string(),
        function: "updateValue(uint256)".to_string(),
        params: vec![serde_json::json!(42)],
        value: "0".to_string(),
    };

    // Test call to different contract (should be invalid)
    let invalid_call = ContractCall {
        target: "0x9999999999999999999999999999999999999999".to_string(),
        function: "updateValue(uint256)".to_string(),
        params: vec![serde_json::json!(42)],
        value: "0".to_string(),
    };

    // Validate same contract restriction
    let valid_address = valid_call.target.parse::<Address>().unwrap();
    let invalid_address = invalid_call.target.parse::<Address>().unwrap();
    assert_eq!(valid_address, monitor.contract_address);
    assert_ne!(invalid_address, monitor.contract_address);
}

#[test]
fn test_execution_limits_validation() {
    let limits = ExecutionLimits {
        max_value_wei: "1000000000000000000".to_string(), // 1 ETH
        max_gas_price_gwei: 100,
    };

    // Test value validation
    let value_ok = U256::from_str_radix("500000000000000000", 10).unwrap(); // 0.5 ETH
    let value_too_high = U256::from_str_radix("2000000000000000000", 10).unwrap(); // 2 ETH
    let max_value = U256::from_str_radix(&limits.max_value_wei, 10).unwrap();

    assert!(value_ok <= max_value);
    assert!(value_too_high > max_value);

    // Test gas price validation
    let gas_price_ok = 50_u64; // 50 gwei
    let gas_price_too_high = 150_u64; // 150 gwei

    assert!(gas_price_ok <= limits.max_gas_price_gwei);
    assert!(gas_price_too_high > limits.max_gas_price_gwei);
}

#[test]
fn test_contract_call_encoding() {
    // Test encoding a simple function call
    let call = ContractCall {
        target: "0x1234567890123456789012345678901234567890".to_string(),
        function: "transfer(address,uint256)".to_string(),
        params: vec![
            serde_json::json!("0x9999999999999999999999999999999999999999"),
            serde_json::json!("1000000000000000000"),
        ],
        value: "0".to_string(),
    };

    // Verify the structure
    assert_eq!(call.params.len(), 2);
    assert_eq!(call.value, "0");
}

#[test]
fn test_multiple_contract_calls() {
    let response = WebhookResponse {
        action: "batch_execute".to_string(),
        calls: Some(vec![
            ContractCall {
                target: "0x1234567890123456789012345678901234567890".to_string(),
                function: "approve(address,uint256)".to_string(),
                params: vec![
                    serde_json::json!("0x5678901234567890123456789012345678901234"),
                    serde_json::json!("1000000000000000000000"),
                ],
                value: "0".to_string(),
            },
            ContractCall {
                target: "0x1234567890123456789012345678901234567890".to_string(),
                function: "transfer(address,uint256)".to_string(),
                params: vec![
                    serde_json::json!("0x9999999999999999999999999999999999999999"),
                    serde_json::json!("500000000000000000"),
                ],
                value: "0".to_string(),
            },
            ContractCall {
                target: "0x1234567890123456789012345678901234567890".to_string(),
                function: "updateState(bool,uint256)".to_string(),
                params: vec![serde_json::json!(true), serde_json::json!(42)],
                value: "100000000000000000".to_string(), // 0.1 ETH
            },
        ]),
        metadata: Some(serde_json::json!({
            "log": "Batch operation"
        })),
        extra: serde_json::Map::new(),
    };

    let calls = response.calls.unwrap();
    assert_eq!(calls.len(), 3);

    // Verify all calls are to the same contract
    let contract_addr = &calls[0].target;
    assert!(calls.iter().all(|c| &c.target == contract_addr));

    // Verify different functions are called
    let function_names: Vec<&str> = calls
        .iter()
        .map(|c| c.function.split('(').next().unwrap_or(&c.function))
        .collect();
    assert_eq!(function_names, vec!["approve", "transfer", "updateState"]);

    // Verify values
    assert_eq!(calls[0].value, "0");
    assert_eq!(calls[1].value, "0");
    assert_eq!(calls[2].value, "100000000000000000");
}

#[test]
fn test_contract_call_values() {
    let call_with_value = ContractCall {
        target: "0x1234567890123456789012345678901234567890".to_string(),
        function: "test()".to_string(),
        params: vec![],
        value: "300000".to_string(),
    };

    let call_without_value = ContractCall {
        target: "0x1234567890123456789012345678901234567890".to_string(),
        function: "test()".to_string(),
        params: vec![],
        value: "0".to_string(),
    };

    assert_eq!(call_with_value.value, "300000");
    assert_eq!(call_without_value.value, "0");
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use omikuji::event_monitors::metrics::EventMonitorMetrics;

    #[test]
    fn test_metrics_recording() {
        let metrics = EventMonitorMetrics::global();

        // Record contract execution metrics
        metrics.record_contract_execution("test_monitor", true);
        metrics.record_contract_execution_gas("test_monitor", 150000);
        metrics.record_contract_execution_value("test_monitor", 1000000000000000000); // 1 ETH

        // Record validation failures
        metrics.record_validation_failure("test_monitor", "value_exceeds_limit");
        metrics.record_validation_failure("test_monitor", "gas_price_too_high");
        metrics.record_validation_failure("test_monitor", "wrong_contract");
    }

    #[test]
    fn test_event_monitor_with_execution_config() {
        let monitor = EventMonitor {
            name: "defi_monitor".to_string(),
            network: "ethereum-mainnet".to_string(),
            contract_address: address!("1234567890123456789012345678901234567890"),
            event_signature: "Swap(address,address,uint256,uint256)".to_string(),
            webhook: WebhookConfig {
                url: "https://api.example.com/handle-swap".to_string(),
                method: HttpMethod::Post,
                headers: HashMap::from([
                    ("Authorization".to_string(), "Bearer token123".to_string()),
                    ("Content-Type".to_string(), "application/json".to_string()),
                ]),
                timeout_seconds: 30,
                retry_attempts: 3,
                retry_delay_seconds: 5,
            },
            response: ResponseConfig {
                response_type: ResponseType::ContractCall,
                contract_call: Some(omikuji::event_monitors::models::ContractCallConfig {
                    target_contract: "{event.address}".to_string(),
                    max_gas_price_gwei: 100,
                    gas_limit_multiplier: 1.2,
                    value_wei: 0,
                }),
                validation: None,
            },
            execution_limits: Some(ExecutionLimits {
                max_value_wei: "5000000000000000000".to_string(), // 5 ETH
                max_gas_price_gwei: 200,
            }),
        };

        // Validate the monitor configuration
        assert!(monitor.validate().is_ok());
        assert_eq!(monitor.response.response_type, ResponseType::ContractCall);
        assert!(monitor.execution_limits.is_some());
    }
}
