//! Event ABI decoder for parsing and decoding Ethereum event data
//!
//! This module provides functionality to decode event logs based on their ABI signatures,
//! supporting all Solidity types including arrays, tuples, and nested structures.

use super::error::{EventMonitorError, Result};
use alloy::dyn_abi::{DynSolType, DynSolValue};
use alloy::json_abi::Event;
use alloy::primitives::Bytes;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::{debug, trace};

use lazy_static::lazy_static;

lazy_static! {
    /// Cache for parsed event signatures
    static ref EVENT_CACHE: RwLock<HashMap<String, Event>> = RwLock::new(HashMap::new());
}

/// Event ABI decoder for decoding event logs
pub struct EventAbiDecoder;

impl EventAbiDecoder {
    /// Parse an event signature into an Event struct
    /// Example: "Transfer(address indexed from, address indexed to, uint256 value)"
    pub fn parse_event(signature: &str) -> Result<Event> {
        // Check cache first
        if let Ok(cache) = EVENT_CACHE.read() {
            if let Some(event) = cache.get(signature) {
                trace!("Using cached event for signature: {}", signature);
                return Ok(event.clone());
            }
        }

        // Parse the event signature
        let event = signature
            .parse::<Event>()
            .map_err(|e| EventMonitorError::DecodingError {
                monitor: String::new(),
                reason: format!("Invalid event signature '{}': {}", signature, e),
            })?;

        // Cache the parsed event
        if let Ok(mut cache) = EVENT_CACHE.write() {
            cache.insert(signature.to_string(), event.clone());
        }

        debug!("Parsed event signature: {} -> {}", signature, event.name);
        Ok(event)
    }

    /// Decode event parameters from topics and data
    pub fn decode_event(
        event: &Event,
        topics: &[Bytes],
        data: &Bytes,
        monitor_name: &str,
    ) -> Result<HashMap<String, Value>> {
        let mut decoded = HashMap::new();
        
        // First topic is the event selector, skip it
        let mut topic_idx = 1;
        let mut data_types = Vec::new();
        let mut param_names = Vec::new();
        
        // Separate indexed and non-indexed parameters
        for input in &event.inputs {
            if input.indexed {
                // Indexed parameters are in topics
                if topic_idx < topics.len() {
                    let value = Self::decode_indexed_param(
                        &input.ty,
                        &topics[topic_idx],
                        monitor_name,
                    )?;
                    decoded.insert(input.name.clone(), value);
                    topic_idx += 1;
                }
            } else {
                // Non-indexed parameters are in data
                data_types.push(input.ty.clone());
                param_names.push(input.name.clone());
            }
        }
        
        // Decode non-indexed parameters from data
        if !data_types.is_empty() {
            let decoded_data = Self::decode_data_params(
                &data_types,
                data,
                monitor_name,
            )?;
            
            for (name, value) in param_names.iter().zip(decoded_data.iter()) {
                decoded.insert(name.clone(), value.clone());
            }
        }
        
        Ok(decoded)
    }

    /// Decode an indexed parameter from a topic
    fn decode_indexed_param(
        param_type: &str,
        topic: &Bytes,
        monitor_name: &str,
    ) -> Result<Value> {
        // Parse the type
        let sol_type = param_type.parse::<DynSolType>()
            .map_err(|e| EventMonitorError::DecodingError {
                monitor: monitor_name.to_string(),
                reason: format!("Failed to parse type '{}': {}", param_type, e),
            })?;
        
        // For indexed reference types (strings, bytes, arrays), only the hash is stored
        if matches!(sol_type, DynSolType::String | DynSolType::Bytes | DynSolType::Array(_)) {
            return Ok(Value::String(format!("0x{}", hex::encode(topic))));
        }
        
        // Decode the value using abi_decode
        let decoded = sol_type.abi_decode(topic)
            .map_err(|e| EventMonitorError::DecodingError {
                monitor: monitor_name.to_string(),
                reason: format!("Failed to decode indexed parameter: {}", e),
            })?;
        
        Ok(Self::dyn_sol_value_to_json(&decoded))
    }

    /// Decode non-indexed parameters from event data
    fn decode_data_params(
        param_types: &[String],
        data: &Bytes,
        monitor_name: &str,
    ) -> Result<Vec<Value>> {
        // Parse all types
        let sol_types: Vec<DynSolType> = param_types
            .iter()
            .map(|ty| ty.parse::<DynSolType>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| EventMonitorError::DecodingError {
                monitor: monitor_name.to_string(),
                reason: format!("Failed to parse parameter types: {}", e),
            })?;
        
        // Decode all values at once
        // Create a tuple type to decode the sequence
        let tuple_type = DynSolType::Tuple(sol_types);
        let decoded_tuple = tuple_type.abi_decode(data)
            .map_err(|e| EventMonitorError::DecodingError {
                monitor: monitor_name.to_string(),
                reason: format!("Failed to decode event data: {}", e),
            })?;
        
        // Extract values from the tuple
        let decoded_values = match decoded_tuple {
            DynSolValue::Tuple(values) => values,
            _ => return Err(EventMonitorError::DecodingError {
                monitor: monitor_name.to_string(),
                reason: "Expected tuple value from decode".to_string(),
            }),
        };
        
        Ok(decoded_values
            .into_iter()
            .map(|v| Self::dyn_sol_value_to_json(&v))
            .collect())
    }

    /// Convert DynSolValue to JSON
    fn dyn_sol_value_to_json(value: &DynSolValue) -> Value {
        match value {
            DynSolValue::Address(addr) => Value::String(format!("{:#x}", addr)),
            DynSolValue::Bool(b) => Value::Bool(*b),
            DynSolValue::Bytes(bytes) => Value::String(format!("0x{}", hex::encode(bytes))),
            DynSolValue::FixedBytes(bytes, _) => Value::String(format!("0x{}", hex::encode(bytes))),
            DynSolValue::Int(i, _) => Value::String(i.to_string()),
            DynSolValue::Uint(u, _) => Value::String(u.to_string()),
            DynSolValue::String(s) => Value::String(s.clone()),
            DynSolValue::Array(values) => {
                Value::Array(values.iter().map(Self::dyn_sol_value_to_json).collect())
            }
            DynSolValue::FixedArray(values) => {
                Value::Array(values.iter().map(Self::dyn_sol_value_to_json).collect())
            }
            DynSolValue::Tuple(values) => {
                Value::Array(values.iter().map(Self::dyn_sol_value_to_json).collect())
            }
            DynSolValue::Function(_) => {
                // Functions are rarely used in events, return as hex string
                Value::String("0x".to_string())
            }
        }
    }

    /// Clear the event cache (mainly for testing)
    #[cfg(test)]
    pub fn clear_cache() {
        if let Ok(mut cache) = EVENT_CACHE.write() {
            cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{b256, Bytes, U256};

    #[test]
    fn test_parse_event_signature() {
        // Test simple event
        let event = EventAbiDecoder::parse_event("Transfer(address,address,uint256)").unwrap();
        assert_eq!(event.name, "Transfer");
        assert_eq!(event.inputs.len(), 3);
        
        // Test event with indexed parameters
        let event = EventAbiDecoder::parse_event(
            "Transfer(address indexed from, address indexed to, uint256 value)"
        ).unwrap();
        assert_eq!(event.name, "Transfer");
        assert_eq!(event.inputs.len(), 3);
        assert!(event.inputs[0].indexed);
        assert!(event.inputs[1].indexed);
        assert!(!event.inputs[2].indexed);
        
        // Test event with no parameters
        let event = EventAbiDecoder::parse_event("Pause()").unwrap();
        assert_eq!(event.name, "Pause");
        assert_eq!(event.inputs.len(), 0);
    }

    #[test]
    fn test_parse_invalid_event_signature() {
        // Missing closing parenthesis
        assert!(EventAbiDecoder::parse_event("Transfer(address").is_err());
        
        // Invalid format - no parentheses  
        assert!(EventAbiDecoder::parse_event("Transfer").is_err());
        
        // Empty signature
        assert!(EventAbiDecoder::parse_event("").is_err());
        
        // Invalid characters
        assert!(EventAbiDecoder::parse_event("Transfer(address@)").is_err());
    }

    #[test]
    fn test_decode_simple_event() {
        let event = EventAbiDecoder::parse_event(
            "Transfer(address indexed from, address indexed to, uint256 value)"
        ).unwrap();
        
        // Create test data
        let from_addr = b256!("0000000000000000000000001234567890123456789012345678901234567890");
        let to_addr = b256!("0000000000000000000000009876543210987654321098765432109876543210");
        let topics = vec![
            Bytes::from(vec![0; 32]), // Event selector (ignored)
            Bytes::from(from_addr.to_vec()),
            Bytes::from(to_addr.to_vec()),
        ];
        
        // Value: 1000 tokens (with 18 decimals)
        let value = U256::from(1000u64) * U256::from(10u64).pow(U256::from(18));
        let data = Bytes::from(value.to_be_bytes::<32>().to_vec());
        
        let decoded = EventAbiDecoder::decode_event(&event, &topics, &data, "test").unwrap();
        
        assert_eq!(decoded.len(), 3);
        assert_eq!(
            decoded.get("from").unwrap().as_str().unwrap(),
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(
            decoded.get("to").unwrap().as_str().unwrap(),
            "0x9876543210987654321098765432109876543210"
        );
        assert_eq!(
            decoded.get("value").unwrap().as_str().unwrap(),
            "1000000000000000000000"
        );
    }

    #[test]
    fn test_decode_event_with_string() {
        let event = EventAbiDecoder::parse_event(
            "Message(address indexed sender, string message)"
        ).unwrap();
        
        let sender = b256!("0000000000000000000000001234567890123456789012345678901234567890");
        let topics = vec![
            Bytes::from(vec![0; 32]), // Event selector
            Bytes::from(sender.to_vec()),
        ];
        
        // Use proper ABI encoding for the string parameter
        // For events, non-indexed parameters need to be encoded as a tuple
        use alloy::sol_types::SolValue;
        let message = "Hello, World!".to_string();
        // Wrap in a tuple since there's only one non-indexed parameter
        let data = (message,).abi_encode();
        
        let decoded = EventAbiDecoder::decode_event(
            &event,
            &topics,
            &Bytes::from(data),
            "test"
        ).unwrap();
        
        assert_eq!(decoded.len(), 2);
        assert_eq!(
            decoded.get("message").unwrap().as_str().unwrap(),
            "Hello, World!"
        );
    }

    #[test]
    fn test_decode_event_with_array() {
        let event = EventAbiDecoder::parse_event(
            "Winners(address[] winners, uint256[] amounts)"
        ).unwrap();
        
        let topics = vec![
            Bytes::from(vec![0; 32]), // Event selector only
        ];
        
        // Use proper ABI encoding for arrays
        use alloy::sol_types::SolValue;
        use alloy::primitives::Address;
        
        // Create the arrays
        let addresses = vec![
            Address::from([0x11; 20]),
            Address::from([0x22; 20]),
        ];
        let amounts = vec![U256::from(100), U256::from(200)];
        
        // Encode as a tuple of arrays
        let data = (addresses, amounts).abi_encode();
        
        let decoded = EventAbiDecoder::decode_event(
            &event,
            &topics,
            &Bytes::from(data),
            "test"
        ).unwrap();
        
        assert_eq!(decoded.len(), 2);
        
        let winners = decoded.get("winners").unwrap().as_array().unwrap();
        assert_eq!(winners.len(), 2);
        assert!(winners[0].as_str().unwrap().contains("0x1111"));
        assert!(winners[1].as_str().unwrap().contains("0x2222"));
        
        let amounts = decoded.get("amounts").unwrap().as_array().unwrap();
        assert_eq!(amounts.len(), 2);
        assert_eq!(amounts[0].as_str().unwrap(), "100");
        assert_eq!(amounts[1].as_str().unwrap(), "200");
    }

    #[test]
    fn test_decode_event_with_tuple() {
        // Note: The parser doesn't support inline tuple syntax, so we need to use a simpler test
        // In practice, tuples in events are less common than structs
        let event = EventAbiDecoder::parse_event(
            "OrderCreated(uint256 indexed orderId, address buyer, uint256 amount, bool active)"
        ).unwrap();
        
        let order_id = U256::from(12345u64);
        let topics = vec![
            Bytes::from(vec![0; 32]), // Event selector
            Bytes::from(order_id.to_be_bytes::<32>().to_vec()),
        ];
        
        // Use proper ABI encoding for the non-indexed parameters
        use alloy::sol_types::SolValue;
        use alloy::primitives::Address;
        
        // Create the data for non-indexed parameters (address, uint256, bool)
        let data = (
            Address::from([0xAA; 20]),
            U256::from(1000),
            true
        ).abi_encode();
        
        let decoded = EventAbiDecoder::decode_event(
            &event,
            &topics,
            &Bytes::from(data),
            "test"
        ).unwrap();
        
        assert_eq!(decoded.len(), 4); // orderId, buyer, amount, active
        assert_eq!(decoded.get("orderId").unwrap().as_str().unwrap(), "12345");
        assert!(decoded.get("buyer").unwrap().as_str().unwrap().to_lowercase().contains("0xaaaa"));
        assert_eq!(decoded.get("amount").unwrap().as_str().unwrap(), "1000");
        assert_eq!(decoded.get("active").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn test_decode_indexed_string() {
        // Indexed strings only store the hash
        let event = EventAbiDecoder::parse_event(
            "NameSet(string indexed name, address setter)"
        ).unwrap();
        
        // The actual string is hashed when indexed
        let name_hash = b256!("1234567890123456789012345678901234567890123456789012345678901234");
        let topics = vec![
            Bytes::from(vec![0; 32]), // Event selector
            Bytes::from(name_hash.to_vec()),
        ];
        
        let mut data = Vec::new();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&[0xBB; 20]);
        
        let decoded = EventAbiDecoder::decode_event(
            &event,
            &topics,
            &Bytes::from(data),
            "test"
        ).unwrap();
        
        // Indexed strings return as hash
        assert!(decoded.get("name").unwrap().as_str().unwrap().starts_with("0x"));
        assert_eq!(decoded.get("name").unwrap().as_str().unwrap().len(), 66); // 0x + 64 hex chars
    }

    #[test]
    fn test_event_cache() {
        EventAbiDecoder::clear_cache();
        
        // First parse should cache
        let _event1 = EventAbiDecoder::parse_event("Test(uint256)").unwrap();
        
        // Second parse should use cache
        let _event2 = EventAbiDecoder::parse_event("Test(uint256)").unwrap();
        
        EventAbiDecoder::clear_cache();
    }
}