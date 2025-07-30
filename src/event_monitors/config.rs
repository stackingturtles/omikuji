//! Configuration parsing for event monitors

use super::models::EventMonitor;
use std::env;
use tracing::debug;

/// Substitute environment variables in a string
/// Format: ${VAR_NAME} will be replaced with the value of VAR_NAME
fn substitute_env_vars(input: &str) -> String {
    let mut result = input.to_string();

    // Find all ${VAR_NAME} patterns
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = env::var(var_name).unwrap_or_else(|_| {
                debug!(
                    "Environment variable {} not found, keeping placeholder",
                    var_name
                );
                format!("${{{var_name}}}")
            });
            result.replace_range(start..start + end + 1, &replacement);
        } else {
            break;
        }
    }

    result
}

/// Parse and validate event monitor configurations
pub fn parse_event_monitors(
    monitors: Vec<EventMonitor>,
) -> Result<Vec<EventMonitor>, crate::event_monitors::error::EventMonitorError> {
    use crate::event_monitors::error::EventMonitorError;
    let mut parsed_monitors = Vec::new();

    for mut monitor in monitors {
        // Substitute environment variables in webhook headers
        for (_, value) in monitor.webhook.headers.iter_mut() {
            *value = substitute_env_vars(value);
        }

        // Substitute environment variables in webhook URL
        monitor.webhook.url = substitute_env_vars(&monitor.webhook.url);

        // Validate the monitor configuration
        monitor
            .validate()
            .map_err(|reason| EventMonitorError::ConfigError {
                monitor: monitor.name.clone(),
                reason,
            })?;

        debug!(
            "Parsed event monitor '{}' for network '{}' monitoring contract {}",
            monitor.name, monitor.network, monitor.contract_address
        );

        parsed_monitors.push(monitor);
    }

    // Check for duplicate monitor names
    let mut seen_names = std::collections::HashSet::new();
    for monitor in &parsed_monitors {
        if !seen_names.insert(&monitor.name) {
            return Err(EventMonitorError::ConfigError {
                monitor: monitor.name.clone(),
                reason: "Duplicate monitor name".to_string(),
            });
        }
    }

    Ok(parsed_monitors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_monitors::models::{HttpMethod, ResponseConfig, WebhookConfig};
    use alloy::primitives::address;
    use std::collections::HashMap;

    #[test]
    fn test_parse_event_monitors() {
        let monitors = vec![EventMonitor {
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
        }];

        let result = parse_event_monitors(monitors);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_duplicate_monitor_names() {
        let monitor = EventMonitor {
            name: "duplicate".to_string(),
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
        };

        let monitors = vec![monitor.clone(), monitor];
        let result = parse_event_monitors(monitors);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate monitor name"));
    }
}
