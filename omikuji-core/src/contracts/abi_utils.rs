//! ABI encoding and decoding utilities
//!
//! This module provides abstractions for common ABI-related operations,
//! reducing code duplication and ensuring consistent handling across the codebase.

use alloy::{
    dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt},
    json_abi::{Function, Param, StateMutability},
    primitives::{Address, Bytes, I256, U256},
};
use anyhow::{Context, Result};
use std::str::FromStr;

/// Parse a function signature string into name and parameter types
///
/// # Arguments
/// * `signature` - Function signature like "transfer(address,uint256)" or "balanceOf(address)"
///
/// # Returns
/// Tuple of (function_name, vec_of_param_types)
pub fn parse_function_signature(signature: &str) -> Result<(String, Vec<String>)> {
    let signature = signature.trim();

    // Find the opening parenthesis
    let paren_pos = signature
        .find('(')
        .context("Invalid function signature: missing opening parenthesis")?;

    let func_name = signature[..paren_pos].trim().to_string();
    let params_str = signature[paren_pos + 1..].trim_end_matches(')').trim();

    let param_types = if params_str.is_empty() {
        Vec::new()
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    Ok((func_name, param_types))
}

/// Create a Function definition from a signature and return type
pub fn create_function_definition(
    signature: &str,
    return_type: Option<&str>,
    state_mutability: StateMutability,
) -> Result<Function> {
    let (func_name, param_types) = parse_function_signature(signature)?;

    // Convert parameter types to Param
    let inputs = param_types
        .into_iter()
        .enumerate()
        .map(|(i, type_str)| {
            Ok(Param {
                ty: type_str.clone(),
                name: format!("param{i}"),
                components: vec![],
                internal_type: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Handle return type
    let outputs = if let Some(ret_type) = return_type {
        vec![Param {
            ty: ret_type.to_string(),
            name: "result".to_string(),
            components: vec![],
            internal_type: None,
        }]
    } else {
        Vec::new()
    };

    Ok(Function {
        name: func_name,
        inputs,
        outputs,
        state_mutability,
    })
}

/// Encode a single parameter based on its type
pub fn encode_parameter(param_type: &str, value: &str) -> Result<DynSolValue> {
    match param_type {
        "uint256" => {
            let val = U256::from_str(value).context("Failed to parse uint256 value")?;
            Ok(DynSolValue::Uint(val, 256))
        }
        "int256" => {
            let val = I256::from_str(value).context("Failed to parse int256 value")?;
            Ok(DynSolValue::Int(val, 256))
        }
        "address" => {
            let addr = Address::from_str(value).context("Failed to parse address")?;
            Ok(DynSolValue::Address(addr))
        }
        "bool" => {
            let val = value
                .parse::<bool>()
                .context("Failed to parse boolean value")?;
            Ok(DynSolValue::Bool(val))
        }
        "string" => Ok(DynSolValue::String(value.to_string())),
        "bytes32" => {
            // For now, we'll skip bytes32 support as it requires specific type handling
            Err(anyhow::anyhow!("bytes32 parameter type not yet supported"))
        }
        "address[]" => {
            // Parse comma-separated addresses
            let addresses = value
                .split(',')
                .map(|s| {
                    Address::from_str(s.trim())
                        .map(DynSolValue::Address)
                        .context("Failed to parse address in array")
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(DynSolValue::Array(addresses))
        }
        _ => Err(anyhow::anyhow!(
            "Unsupported parameter type: {}",
            param_type
        )),
    }
}

/// Encode multiple parameters based on their types
pub fn encode_parameters(values: &[String], param_types: &[String]) -> Result<Vec<DynSolValue>> {
    if values.len() != param_types.len() {
        return Err(anyhow::anyhow!(
            "Parameter count mismatch: {} values provided, {} expected",
            values.len(),
            param_types.len()
        ));
    }

    values
        .iter()
        .zip(param_types.iter())
        .map(|(value, param_type)| encode_parameter(param_type, value))
        .collect()
}

/// Encode a function call with parameters
pub fn encode_function_call(
    signature: &str,
    parameters: &[String],
    state_mutability: StateMutability,
) -> Result<Bytes> {
    let (_, param_types) = parse_function_signature(signature)?;
    let function = create_function_definition(signature, None, state_mutability)?;
    let encoded_params = encode_parameters(parameters, &param_types)?;

    function
        .abi_encode_input(&encoded_params)
        .map(Into::into)
        .context("Failed to encode function call")
}

/// Decode a function return value using a Function definition
pub fn decode_function_return(function: &Function, data: &Bytes) -> Result<Vec<DynSolValue>> {
    function
        .abi_decode_output(data, true)
        .context("Failed to decode function return value")
}

/// Generic contract call builder
pub struct ContractCallBuilder {
    signature: String,
    parameters: Vec<String>,
    state_mutability: StateMutability,
}

impl ContractCallBuilder {
    /// Create a new call builder
    pub fn new(signature: impl Into<String>) -> Self {
        Self {
            signature: signature.into(),
            parameters: Vec::new(),
            state_mutability: StateMutability::View,
        }
    }

    /// Set the state mutability
    pub fn with_mutability(mut self, mutability: StateMutability) -> Self {
        self.state_mutability = mutability;
        self
    }

    /// Add a parameter
    pub fn with_param(mut self, value: impl ToString) -> Self {
        self.parameters.push(value.to_string());
        self
    }

    /// Add multiple parameters
    pub fn with_params(mut self, values: Vec<impl ToString>) -> Self {
        self.parameters
            .extend(values.into_iter().map(|v| v.to_string()));
        self
    }

    /// Build the encoded call data
    pub fn build(self) -> Result<Bytes> {
        encode_function_call(&self.signature, &self.parameters, self.state_mutability)
    }
}

/// Helper for encoding common contract calls
pub mod common_calls {
    use super::*;

    /// Encode a standard ERC20 balanceOf call
    pub fn balance_of(address: Address) -> Result<Bytes> {
        ContractCallBuilder::new("balanceOf(address)")
            .with_param(format!("{address:?}"))
            .build()
    }

    /// Encode a standard ERC20 transfer call
    pub fn transfer(to: Address, amount: U256) -> Result<Bytes> {
        ContractCallBuilder::new("transfer(address,uint256)")
            .with_mutability(StateMutability::NonPayable)
            .with_param(format!("{to:?}"))
            .with_param(amount.to_string())
            .build()
    }

    /// Encode a standard owner() call
    pub fn owner() -> Result<Bytes> {
        ContractCallBuilder::new("owner()").build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_signature() {
        let (name, params) = parse_function_signature("transfer(address,uint256)").unwrap();
        assert_eq!(name, "transfer");
        assert_eq!(params, vec!["address", "uint256"]);

        let (name, params) = parse_function_signature("balanceOf(address)").unwrap();
        assert_eq!(name, "balanceOf");
        assert_eq!(params, vec!["address"]);

        let (name, params) = parse_function_signature("owner()").unwrap();
        assert_eq!(name, "owner");
        assert!(params.is_empty());

        // Test with spaces
        let (name, params) = parse_function_signature("transfer( address , uint256 )").unwrap();
        assert_eq!(name, "transfer");
        assert_eq!(params, vec!["address", "uint256"]);
    }

    #[test]
    fn test_encode_parameter() {
        // Test uint256
        let val = encode_parameter("uint256", "12345").unwrap();
        assert!(matches!(val, DynSolValue::Uint(_, 256)));

        // Test address
        let val =
            encode_parameter("address", "0x0000000000000000000000000000000000000001").unwrap();
        assert!(matches!(val, DynSolValue::Address(_)));

        // Test bool
        let val = encode_parameter("bool", "true").unwrap();
        assert!(matches!(val, DynSolValue::Bool(true)));

        // Test string
        let val = encode_parameter("string", "hello world").unwrap();
        assert!(matches!(val, DynSolValue::String(s) if s == "hello world"));
    }

    #[test]
    fn test_contract_call_builder() {
        let call_data = ContractCallBuilder::new("transfer(address,uint256)")
            .with_mutability(StateMutability::NonPayable)
            .with_param("0x0000000000000000000000000000000000000001")
            .with_param("1000")
            .build()
            .unwrap();

        assert!(!call_data.is_empty());
    }

    #[test]
    fn test_common_calls() {
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();

        let call_data = common_calls::balance_of(addr).unwrap();
        assert!(!call_data.is_empty());

        let call_data = common_calls::transfer(addr, U256::from(1000)).unwrap();
        assert!(!call_data.is_empty());

        let call_data = common_calls::owner().unwrap();
        assert!(!call_data.is_empty());
    }
}
