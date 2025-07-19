use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;
use validator::Validate;

use super::models::OmikujiConfig;

/// Errors that can occur during configuration parsing
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to open config file: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Failed to parse YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("Configuration validation error: {0}")]
    ValidationError(#[from] validator::ValidationErrors),

    #[error("Configuration error: {0}")]
    Other(String),
}

/// Provides default configuration file path
pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".omikuji")
        .join("config.yaml")
}

/// Loads and validates the Omikuji configuration
pub fn load_config<P: AsRef<Path>>(config_path: P) -> Result<OmikujiConfig, ConfigError> {
    // Open the configuration file
    let mut file = File::open(&config_path).map_err(ConfigError::FileError)?;

    // Read the file content
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(ConfigError::FileError)?;

    // Parse YAML with backward compatibility
    let mut raw_config: serde_yaml::Value = serde_yaml::from_str(&content).map_err(ConfigError::ParseError)?;
    
    // Apply backward compatibility transformation for networks
    if let Some(networks) = raw_config.get_mut("networks") {
        if let Some(networks_array) = networks.as_sequence_mut() {
            for network in networks_array {
                apply_network_backward_compatibility(network);
            }
        }
    }
    
    // Now deserialize the transformed config
    let config: OmikujiConfig = serde_yaml::from_value(raw_config).map_err(ConfigError::ParseError)?;

    // Validate the configuration
    config.validate().map_err(ConfigError::ValidationError)?;

    // Check if networks referenced by datafeeds exist
    for datafeed in &config.datafeeds {
        if !config.networks.iter().any(|n| n.name == datafeed.networks) {
            return Err(ConfigError::Other(format!(
                "Datafeed '{}' references network '{}' which is not defined",
                datafeed.name, datafeed.networks
            )));
        }
    }

    // Check if networks referenced by scheduled tasks exist
    for task in &config.scheduled_tasks {
        if !config.networks.iter().any(|n| n.name == task.network) {
            return Err(ConfigError::Other(format!(
                "Scheduled task '{}' references network '{}' which is not defined",
                task.name, task.network
            )));
        }

        // Validate the scheduled task
        task.validate().map_err(|e| {
            ConfigError::Other(format!(
                "Scheduled task '{}' validation failed: {}",
                task.name, e
            ))
        })?;
    }

    // Check if networks referenced by event monitors exist
    for monitor in &config.event_monitors {
        if !config.networks.iter().any(|n| n.name == monitor.network) {
            return Err(ConfigError::Other(format!(
                "Event monitor '{}' references network '{}' which is not defined",
                monitor.name, monitor.network
            )));
        }

        // Validate the event monitor
        monitor.validate().map_err(|e| {
            ConfigError::Other(format!(
                "Event monitor '{}' validation failed: {}",
                monitor.name, e
            ))
        })?;
    }

    // Parse and validate event monitors with environment variable substitution
    let parsed_monitors =
        crate::event_monitors::config::parse_event_monitors(config.event_monitors.clone())
            .map_err(|e| ConfigError::Other(format!("Event monitor parsing failed: {e}")))?;

    // Create a new config with parsed event monitors
    let mut config = config;
    config.event_monitors = parsed_monitors;

    Ok(config)
}

/// Apply backward compatibility transformations to network configuration
fn apply_network_backward_compatibility(network: &mut serde_yaml::Value) {
    // Check if the old format is being used (has rpc_url but no nodes)
    if network.get("rpc_url").is_some() && network.get("nodes").is_none() {
        // Extract the old fields
        let rpc_url = network.get("rpc_url").cloned();
        let ws_url = network.get("ws_url").cloned();
        
        if let Some(rpc_url) = rpc_url {
            // Create a new node from the old fields
            let mut node = serde_yaml::Mapping::new();
            node.insert(
                serde_yaml::Value::String("name".to_string()),
                serde_yaml::Value::String("Default Node".to_string()),
            );
            node.insert(
                serde_yaml::Value::String("rpc_url".to_string()),
                rpc_url.clone(),
            );
            
            if let Some(ws_url) = ws_url {
                node.insert(
                    serde_yaml::Value::String("ws_url".to_string()),
                    ws_url,
                );
            }
            
            // Create nodes array
            let nodes = vec![serde_yaml::Value::Mapping(node)];
            
            // Add nodes to the network and remove old fields
            if let Some(network_map) = network.as_mapping_mut() {
                network_map.insert(
                    serde_yaml::Value::String("nodes".to_string()),
                    serde_yaml::Value::Sequence(nodes),
                );
                network_map.remove(&serde_yaml::Value::String("rpc_url".to_string()));
                network_map.remove(&serde_yaml::Value::String("ws_url".to_string()));
                network_map.remove(&serde_yaml::Value::String("url".to_string())); // Also handle 'url' field
            }
        }
    }
    // Also handle 'url' field (another variation of the old format)
    else if network.get("url").is_some() && network.get("nodes").is_none() {
        let url = network.get("url").cloned();
        
        if let Some(url) = url {
            // Create a new node from the old field
            let mut node = serde_yaml::Mapping::new();
            node.insert(
                serde_yaml::Value::String("name".to_string()),
                serde_yaml::Value::String("Default Node".to_string()),
            );
            node.insert(
                serde_yaml::Value::String("rpc_url".to_string()),
                url,
            );
            
            // Create nodes array
            let nodes = vec![serde_yaml::Value::Mapping(node)];
            
            // Add nodes to the network and remove old field
            if let Some(network_map) = network.as_mapping_mut() {
                network_map.insert(
                    serde_yaml::Value::String("nodes".to_string()),
                    serde_yaml::Value::Sequence(nodes),
                );
                network_map.remove(&serde_yaml::Value::String("url".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function for testing configuration parsing from string
    fn load_config_from_str(content: &str) -> Result<OmikujiConfig, ConfigError> {
        // Parse YAML with backward compatibility
        let mut raw_config: serde_yaml::Value = serde_yaml::from_str(content).map_err(ConfigError::ParseError)?;
        
        // Apply backward compatibility transformation for networks
        if let Some(networks) = raw_config.get_mut("networks") {
            if let Some(networks_array) = networks.as_sequence_mut() {
                for network in networks_array {
                    apply_network_backward_compatibility(network);
                }
            }
        }
        
        // Now deserialize the transformed config
        let config: OmikujiConfig = serde_yaml::from_value(raw_config).map_err(ConfigError::ParseError)?;
        
        Ok(config)
    }

    #[test]
    fn test_parse_valid_config_new_format() {
        let config_str = r#"
            networks:
              - name: ethereum
                transaction_type: eip1559
                nodes:
                  - name: Primary
                    rpc_url: https://eth.llamarpc.com
                    ws_url: wss://eth.llamarpc.com
              - name: base
                transaction_type: legacy
                nodes:
                  - name: Primary
                    rpc_url: https://base.llamarpc.com

            datafeeds:
              - name: eth_usd
                networks: ethereum
                check_frequency: 60
                contract_address: 0x1234567890123456789012345678901234567890
                contract_type: fluxmon
                read_contract_config: true
                minimum_update_frequency: 3600
                deviation_threshold_pct: 0.5
                feed_url: https://min-api.cryptocompare.com/data/pricemultifull?fsyms=ETH&tsyms=USD
                feed_json_path: RAW.ETH.USD.PRICE
                feed_json_path_timestamp: RAW.ETH.USD.LASTUPDATE
        "#;

        let config: OmikujiConfig = serde_yaml::from_str(config_str).unwrap();
        assert_eq!(config.networks.len(), 2);
        assert_eq!(config.datafeeds.len(), 1);
        assert_eq!(config.networks[0].name, "ethereum");
        assert_eq!(config.networks[0].nodes.len(), 1);
        assert_eq!(config.networks[0].nodes[0].name, "Primary");
        assert_eq!(config.datafeeds[0].name, "eth_usd");
    }

    #[test]
    fn test_backward_compatibility_rpc_url() {
        let config_str = r#"
            networks:
              - name: ethereum
                rpc_url: https://eth.llamarpc.com
                ws_url: wss://eth.llamarpc.com
                transaction_type: eip1559

            datafeeds: []
        "#;

        // This should parse successfully with backward compatibility
        let config = load_config_from_str(config_str).unwrap();
        assert_eq!(config.networks.len(), 1);
        assert_eq!(config.networks[0].nodes.len(), 1);
        assert_eq!(config.networks[0].nodes[0].rpc_url, "https://eth.llamarpc.com");
        assert_eq!(config.networks[0].nodes[0].ws_url.as_ref().unwrap(), "wss://eth.llamarpc.com");
    }
}
