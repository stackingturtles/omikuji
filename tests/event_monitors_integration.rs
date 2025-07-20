//! Integration tests for event monitors

use omikuji::config::models::OmikujiConfig;
use omikuji::event_monitors::models::{
    EventMonitor, HttpMethod, ResponseConfig, ResponseType, WebhookConfig,
};

#[test]
fn test_event_monitor_config_parsing() {
    let yaml = r#"
networks:
  - name: ethereum-mainnet
    nodes:
      - name: "Primary Node"
        rpc_url: https://eth.llamarpc.com
        ws_url: wss://eth.llamarpc.com

datafeeds: []

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
      type: log_only
"#;

    let config: OmikujiConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.event_monitors.len(), 1);
    assert_eq!(config.event_monitors[0].name, "test_monitor");
    assert_eq!(config.event_monitors[0].network, "ethereum-mainnet");
}

#[test]
fn test_event_monitor_validation() {
    use alloy::primitives::address;
    use omikuji::event_monitors::models::EventMonitor;
    use std::collections::HashMap;

    let monitor = EventMonitor {
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
    };

    assert!(monitor.validate().is_ok());
}

#[test]
fn test_webhook_config_validation() {
    use std::collections::HashMap;

    let mut webhook = WebhookConfig {
        url: "https://example.com".to_string(),
        method: HttpMethod::Post,
        headers: HashMap::new(),
        timeout_seconds: 30,
        retry_attempts: 3,
        retry_delay_seconds: 5,
    };

    assert!(webhook.validate().is_ok());

    // Test invalid URL
    webhook.url = "not-a-url".to_string();
    assert!(webhook.validate().is_err());

    // Test invalid timeout
    webhook.url = "https://example.com".to_string();
    webhook.timeout_seconds = 0;
    assert!(webhook.validate().is_err());
}

#[test]
fn test_response_types() {
    assert_eq!(
        serde_yaml::from_str::<ResponseType>("log_only").unwrap(),
        ResponseType::LogOnly
    );
    assert_eq!(
        serde_yaml::from_str::<ResponseType>("contract_call").unwrap(),
        ResponseType::ContractCall
    );
    assert_eq!(
        serde_yaml::from_str::<ResponseType>("store_db").unwrap(),
        ResponseType::StoreDb
    );
}

#[test]
fn test_env_var_substitution() {
    use alloy::primitives::address;
    use omikuji::event_monitors::config::parse_event_monitors;
    use std::collections::HashMap;

    // Set test env var
    std::env::set_var("TEST_WEBHOOK_KEY", "secret123");

    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer ${TEST_WEBHOOK_KEY}".to_string(),
    );

    let monitor = EventMonitor {
        name: "test_monitor".to_string(),
        network: "ethereum-mainnet".to_string(),
        contract_address: address!("1234567890123456789012345678901234567890"),
        event_signature: "TestEvent(uint256)".to_string(),
        webhook: WebhookConfig {
            url: "https://example.com/webhook".to_string(),
            method: HttpMethod::Post,
            headers,
            timeout_seconds: 30,
            retry_attempts: 3,
            retry_delay_seconds: 5,
        },
        response: ResponseConfig::default(),
    };

    let parsed = parse_event_monitors(vec![monitor]).unwrap();
    assert_eq!(
        parsed[0].webhook.headers.get("Authorization").unwrap(),
        "Bearer secret123"
    );

    // Clean up
    std::env::remove_var("TEST_WEBHOOK_KEY");
}
