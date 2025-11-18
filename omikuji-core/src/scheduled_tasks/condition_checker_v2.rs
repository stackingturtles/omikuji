//! Refactored condition checker using ABI utilities
//!
//! This demonstrates how to use the new ABI abstractions to simplify
//! contract interaction code.

use crate::contracts::abi_utils::{create_function_definition, ContractCallBuilder};
use crate::contracts::generic_caller::create_contract_reader;
use crate::scheduled_tasks::models::CheckCondition;
use alloy::{
    dyn_abi::DynSolValue,
    json_abi::StateMutability,
    network::Network,
    primitives::{Address, U256},
    providers::Provider,
    transports::Transport,
};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::{debug, error};

pub struct ConditionCheckerV2;

impl ConditionCheckerV2 {
    pub async fn check_condition<T, N, P>(
        provider: Arc<P>,
        network_name: &str,
        condition: &CheckCondition,
    ) -> Result<bool>
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
        N::TransactionRequest: Default + alloy::network::TransactionBuilder<N>,
    {
        debug!("Checking condition: {:?}", condition);
        
        match condition {
            CheckCondition::Property {
                contract_address,
                property,
                expected_value,
            } => {
                Self::check_property(
                    provider,
                    network_name,
                    contract_address,
                    property,
                    expected_value,
                )
                .await
            }
            CheckCondition::Function {
                contract_address,
                function,
                expected_value,
            } => {
                Self::check_function(
                    provider,
                    network_name,
                    contract_address,
                    function,
                    expected_value,
                )
                .await
            }
        }
    }
    
    async fn check_property<T, N, P>(
        provider: Arc<P>,
        network_name: &str,
        contract_address: &str,
        property: &str,
        expected_value: &serde_json::Value,
    ) -> Result<bool>
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
        N::TransactionRequest: Default + alloy::network::TransactionBuilder<N>,
    {
        let address = contract_address.parse::<Address>()?;
        
        // Use the new ContractCallBuilder for encoding
        let call_data = ContractCallBuilder::new(format!("{}()", property))
            .build()?;
        
        // Use the metrics-aware contract caller
        let reader = create_contract_reader(provider, address, network_name);
        
        let result = reader
            .call(call_data, property, |bytes| {
                // Create function definition for decoding
                let func = create_function_definition(
                    &format!("{}()", property),
                    Some("bool"),
                    StateMutability::View,
                )?;
                
                let decoded = func.abi_decode_output(bytes, true)?;
                
                match decoded.first() {
                    Some(DynSolValue::Bool(b)) => Ok(*b),
                    _ => Err(anyhow!("Expected boolean return value")),
                }
            })
            .await?;
        
        let expected_bool = expected_value
            .as_bool()
            .ok_or_else(|| anyhow!("Expected value must be a boolean"))?;
        
        debug!(
            "Property '{}' returned {}, expected {} => match: {}",
            property,
            result,
            expected_bool,
            result == expected_bool
        );
        
        Ok(result == expected_bool)
    }
    
    async fn check_function<T, N, P>(
        provider: Arc<P>,
        network_name: &str,
        contract_address: &str,
        function: &str,
        expected_value: &serde_json::Value,
    ) -> Result<bool>
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
        N::TransactionRequest: Default + alloy::network::TransactionBuilder<N>,
    {
        let address = contract_address.parse::<Address>()?;
        
        // Parse function signature to get return type
        let (func_name, return_type) = parse_function_with_return_type(function)?;
        
        // Use ContractCallBuilder for encoding
        let call_data = ContractCallBuilder::new(format!("{}()", func_name))
            .build()?;
        
        // Use the metrics-aware contract caller
        let reader = create_contract_reader(provider, address, network_name);
        
        let result = reader
            .call(call_data, &func_name, |bytes| {
                let func = create_function_definition(
                    &format!("{}()", func_name),
                    Some(&return_type),
                    StateMutability::View,
                )?;
                
                let decoded = func.abi_decode_output(bytes, true)?;
                decoded.first()
                    .cloned()
                    .ok_or_else(|| anyhow!("No return value"))
            })
            .await?;
        
        // Compare based on type
        let matches = match (&result, &return_type.as_str()) {
            (DynSolValue::Bool(actual), "bool") => {
                let expected = expected_value
                    .as_bool()
                    .ok_or_else(|| anyhow!("Expected boolean value"))?;
                *actual == expected
            }
            (DynSolValue::Uint(actual, _), "uint256") => {
                let expected = parse_uint256_value(expected_value)?;
                *actual == expected
            }
            (DynSolValue::Address(actual), "address") => {
                let expected = expected_value
                    .as_str()
                    .and_then(|s| s.parse::<Address>().ok())
                    .ok_or_else(|| anyhow!("Expected address string"))?;
                *actual == expected
            }
            _ => {
                error!(
                    "Unsupported return type '{}' or value mismatch",
                    return_type
                );
                return Err(anyhow!("Unsupported return type or value mismatch"));
            }
        };
        
        debug!(
            "Function '{}' returned {:?}, expected {:?} => match: {}",
            func_name, result, expected_value, matches
        );
        
        Ok(matches)
    }
}

/// Parse function signature with return type (e.g., "isPaused() returns (bool)")
fn parse_function_with_return_type(signature: &str) -> Result<(String, String)> {
    // Handle format: "functionName() returns (type)"
    if let Some(returns_pos) = signature.find(" returns ") {
        let func_part = &signature[..returns_pos];
        let return_part = &signature[returns_pos + 9..]; // Skip " returns "
        
        let func_name = func_part
            .trim()
            .trim_end_matches("()")
            .trim_end_matches("(")
            .to_string();
        
        let return_type = return_part
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .to_string();
        
        Ok((func_name, return_type))
    } else {
        // Default to bool if no return type specified
        let func_name = signature
            .trim()
            .trim_end_matches("()")
            .trim_end_matches("(")
            .to_string();
        Ok((func_name, "bool".to_string()))
    }
}

/// Parse uint256 value from JSON
fn parse_uint256_value(value: &serde_json::Value) -> Result<U256> {
    if let Some(s) = value.as_str() {
        U256::from_str_radix(s, 10)
            .map_err(|e| anyhow!("Failed to parse uint256 string: {}", e))
    } else if let Some(n) = value.as_u64() {
        Ok(U256::from(n))
    } else {
        Err(anyhow!("Expected string or number for uint256 value"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_function_with_return_type() {
        let (name, ret) = parse_function_with_return_type("isPaused() returns (bool)").unwrap();
        assert_eq!(name, "isPaused");
        assert_eq!(ret, "bool");
        
        let (name, ret) = parse_function_with_return_type("getBalance() returns (uint256)").unwrap();
        assert_eq!(name, "getBalance");
        assert_eq!(ret, "uint256");
        
        let (name, ret) = parse_function_with_return_type("owner()").unwrap();
        assert_eq!(name, "owner");
        assert_eq!(ret, "bool"); // Default
    }
}