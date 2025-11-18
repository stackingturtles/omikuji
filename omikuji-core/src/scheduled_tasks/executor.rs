use crate::scheduled_tasks::models::{GasConfig, Parameter, TargetFunction};
use crate::utils::TransactionLogger;
use alloy::{
    dyn_abi::{DynSolValue, JsonAbiExt},
    json_abi::{Function, JsonAbi, Param, StateMutability},
    network::{Network, ReceiptResponse, TransactionBuilder},
    primitives::{Address, U256},
    providers::Provider,
    transports::Transport,
};
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tracing::{debug, error, info, trace};

pub struct FunctionExecutor<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    provider: Arc<P>,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_n: std::marker::PhantomData<N>,
}

impl<T, N, P> FunctionExecutor<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    pub fn new(provider: Arc<P>) -> Self {
        Self {
            provider,
            _phantom_t: std::marker::PhantomData,
            _phantom_n: std::marker::PhantomData,
        }
    }

    pub async fn execute_function(
        &self,
        task_name: &str,
        _network: &str,
        target_function: &TargetFunction,
        gas_config: Option<&GasConfig>,
    ) -> Result<N::ReceiptResponse>
    where
        N::TransactionRequest: Default + TransactionBuilder<N>,
        N::ReceiptResponse: ReceiptResponse,
    {
        TransactionLogger::log_execution_start("scheduled_task", task_name);
        debug!("Target function details: {:?}", target_function);
        debug!("Gas config: {:?}", gas_config);

        debug!(
            "Parsing contract address: {}",
            target_function.contract_address
        );
        let address = target_function
            .contract_address
            .parse::<Address>()
            .map_err(|e| {
                error!(
                    "Failed to parse contract address '{}': {:?}",
                    target_function.contract_address, e
                );
                e
            })
            .context("Invalid contract address")?;
        debug!("Parsed address successfully: {:?}", address);

        // Parse function signature and encode parameters
        let (func_name, param_types) = Self::parse_function_signature(&target_function.function)?;
        debug!(
            "Parsed function: name='{}', param_types={:?}",
            func_name, param_types
        );

        debug!("Encoding {} parameters", target_function.parameters.len());
        for (i, param) in target_function.parameters.iter().enumerate() {
            debug!(
                "Parameter {}: value={:?}, type={}",
                i, param.value, param.param_type
            );
        }

        let encoded_params = Self::encode_parameters(&target_function.parameters, &param_types)
            .map_err(|e| {
                error!("Failed to encode parameters: {}", e);
                error!("Parameters: {:?}", target_function.parameters);
                error!("Expected types: {:?}", param_types);
                e
            })?;
        debug!("Successfully encoded parameters");

        // Create function definition
        let function = Function {
            name: func_name.clone(),
            inputs: Self::create_param_definitions(&param_types),
            outputs: vec![], // We don't care about outputs for execution
            state_mutability: StateMutability::NonPayable,
        };

        // Create minimal ABI
        let _abi = JsonAbi {
            constructor: None,
            functions: vec![(func_name.clone(), vec![function.clone()])]
                .into_iter()
                .collect(),
            events: Default::default(),
            errors: Default::default(),
            receive: None,
            fallback: None,
        };

        // Encode function call
        let encoded_call = function.abi_encode_input(&encoded_params).map_err(|e| {
            error!("Failed to encode function call: {:?}", e);
            e
        })?;
        debug!("Encoded function call: 0x{}", hex::encode(&encoded_call));

        debug!(
            "Executing function {} on contract {} with {} parameters",
            func_name,
            address,
            target_function.parameters.len()
        );

        // Build transaction
        debug!("Building transaction request...");
        let mut tx = N::TransactionRequest::default();
        tx.set_to(address);
        tx.set_input(alloy::primitives::Bytes::from(encoded_call));
        debug!("Set to address and input data");

        // Apply gas configuration
        if let Some(gas_cfg) = gas_config {
            debug!("Applying gas configuration: {:?}", gas_cfg);
            if let Some(gas_limit) = gas_cfg.gas_limit {
                tx.set_gas_limit(gas_limit);
                debug!("Set gas limit: {}", gas_limit);
            }

            // Handle gas pricing based on transaction type
            if let Some(max_gas_price) = gas_cfg.max_gas_price_gwei {
                let max_price = U256::from(max_gas_price) * U256::from(10).pow(U256::from(9));
                tx.set_max_fee_per_gas(max_price.to::<u128>());
                debug!("Set max fee per gas: {} wei", max_price);

                if let Some(priority_fee) = gas_cfg.priority_fee_gwei {
                    let priority = U256::from(priority_fee) * U256::from(10).pow(U256::from(9));
                    tx.set_max_priority_fee_per_gas(priority.to::<u128>());
                    debug!("Set max priority fee per gas: {} wei", priority);
                }
            }
        } else {
            debug!("No gas configuration provided, using defaults");
        }

        // Send transaction
        debug!("Sending transaction...");
        let pending_tx = self
            .provider
            .send_transaction(tx)
            .await
            .map_err(|e| {
                TransactionLogger::log_failure("scheduled_task", task_name, &e.to_string());
                e
            })
            .context("Failed to send transaction")?;

        let tx_hash = *pending_tx.tx_hash();
        debug!("Submitted transaction: 0x{:x}", tx_hash);

        // Wait for confirmation
        debug!("Waiting for transaction confirmation...");
        let receipt = pending_tx
            .get_receipt()
            .await
            .map_err(|e| {
                error!("Failed to get transaction receipt: {:?}", e);
                e
            })
            .context("Failed to get transaction receipt")?;

        if receipt.status() {
            // Transaction succeeded - return receipt for standardized handling
            Ok(receipt)
        } else {
            error!("Transaction failed: 0x{:x}", tx_hash);
            error!("Receipt: {:?}", receipt);
            Err(anyhow!("Transaction failed: 0x{:x}", tx_hash))
        }
    }

    fn parse_function_signature(signature: &str) -> Result<(String, Vec<String>)> {
        let signature = signature.trim();

        // Find the opening parenthesis
        let open_paren = signature
            .find('(')
            .ok_or_else(|| anyhow!("Invalid function signature: missing '('"))?;

        let func_name = &signature[..open_paren];

        // Extract parameters section
        let close_paren = signature
            .rfind(')')
            .ok_or_else(|| anyhow!("Invalid function signature: missing ')'"))?;

        let params_str = &signature[open_paren + 1..close_paren];

        let param_types: Vec<String> = if params_str.is_empty() {
            vec![]
        } else {
            params_str
                .split(',')
                .map(|s| {
                    let trimmed = s.trim();
                    // Extract just the type, removing parameter name
                    // e.g. "uint256 amount" -> "uint256", "address[] recipients" -> "address[]"
                    trimmed
                        .split_whitespace()
                        .next()
                        .unwrap_or(trimmed)
                        .to_string()
                })
                .collect()
        };

        trace!(
            "Parsed function: {} with params: {:?}",
            func_name,
            param_types
        );
        Ok((func_name.to_string(), param_types))
    }

    fn create_param_definitions(param_types: &[String]) -> Vec<Param> {
        param_types
            .iter()
            .enumerate()
            .map(|(i, ty)| Param {
                ty: ty.clone(),
                name: format!("param{i}"),
                components: vec![],
                internal_type: None,
            })
            .collect()
    }

    fn encode_parameters(
        parameters: &[Parameter],
        param_types: &[String],
    ) -> Result<Vec<DynSolValue>> {
        if parameters.len() != param_types.len() {
            return Err(anyhow!(
                "Parameter count mismatch: {} provided, {} expected",
                parameters.len(),
                param_types.len()
            ));
        }

        parameters
            .iter()
            .zip(param_types.iter())
            .map(|(param, ty)| Self::encode_single_parameter(&param.value, ty))
            .collect()
    }

    fn encode_single_parameter(value: &serde_json::Value, param_type: &str) -> Result<DynSolValue> {
        trace!(
            "Encoding parameter of type '{}' with value: {:?}",
            param_type,
            value
        );

        match param_type {
            "uint256" => {
                let val = if let Some(num) = value.as_u64() {
                    debug!("Encoding uint256 from u64: {}", num);
                    U256::from(num)
                } else if let Some(s) = value.as_str() {
                    debug!("Encoding uint256 from string: {}", s);
                    U256::from_str_radix(s, 10).map_err(|e| {
                        error!("Failed to parse uint256 from string '{}': {:?}", s, e);
                        e
                    })?
                } else {
                    error!("Invalid uint256 value format: {:?}", value);
                    return Err(anyhow!("Invalid uint256 value: {:?}", value));
                };
                debug!("Encoded uint256: {}", val);
                Ok(DynSolValue::Uint(val, 256))
            }
            "address" => {
                let addr_str = value
                    .as_str()
                    .ok_or_else(|| anyhow!("Address must be a string"))?;
                debug!("Encoding address: {}", addr_str);
                let addr = addr_str.parse::<Address>().map_err(|e| {
                    error!("Failed to parse address '{}': {:?}", addr_str, e);
                    e
                })?;
                Ok(DynSolValue::Address(addr))
            }
            "bool" => {
                let val = value
                    .as_bool()
                    .ok_or_else(|| anyhow!("Bool value required"))?;
                debug!("Encoding bool: {}", val);
                Ok(DynSolValue::Bool(val))
            }
            "address[]" => {
                info!("Encoding address[] parameter, value: {:?}", value);

                // Try parsing as JSON string first
                let arr = if let Some(json_str) = value.as_str() {
                    info!(
                        "Attempting to parse address[] from JSON string: {}",
                        json_str
                    );
                    serde_json::from_str::<Vec<serde_json::Value>>(json_str).map_err(|e| {
                        error!("Failed to parse JSON array: {}", e);
                        anyhow!("Failed to parse JSON array: {}", e)
                    })?
                } else {
                    value
                        .as_array()
                        .ok_or_else(|| {
                            error!("Expected array for address[] but got: {:?}", value);
                            anyhow!("Array expected for address[]")
                        })?
                        .clone()
                };

                info!("Parsed array with {} elements", arr.len());
                let addresses: Result<Vec<DynSolValue>> = arr
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        debug!("Encoding address[{}]: {:?}", i, v);
                        let addr_str = v.as_str().ok_or_else(|| {
                            error!("Address at index {} must be string, got: {:?}", i, v);
                            anyhow!("Address must be string")
                        })?;
                        let addr = addr_str.parse::<Address>().map_err(|e| {
                            error!(
                                "Failed to parse address at index {}: '{}', error: {:?}",
                                i, addr_str, e
                            );
                            e
                        })?;
                        Ok(DynSolValue::Address(addr))
                    })
                    .collect();
                let result = addresses?;
                info!("Successfully encoded {} addresses", result.len());
                Ok(DynSolValue::Array(result))
            }
            _ => {
                error!("Unsupported parameter type: {}", param_type);
                Err(anyhow!("Unsupported parameter type: {}", param_type))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_function_signature() {
        // Use a dummy type for testing
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        let (name, params) =
            DummyExecutor::parse_function_signature("transfer(address,uint256)").unwrap();
        assert_eq!(name, "transfer");
        assert_eq!(params, vec!["address", "uint256"]);

        let (name, params) = DummyExecutor::parse_function_signature("execute()").unwrap();
        assert_eq!(name, "execute");
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn test_encode_single_parameter() {
        // Use a dummy type for testing
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        // Test uint256
        let val = serde_json::json!(12345);
        let encoded = DummyExecutor::encode_single_parameter(&val, "uint256").unwrap();
        match encoded {
            DynSolValue::Uint(v, 256) => assert_eq!(v, U256::from(12345)),
            _ => panic!("Expected Uint256"),
        }

        // Test address
        let val = serde_json::json!("0x1234567890123456789012345678901234567890");
        let encoded = DummyExecutor::encode_single_parameter(&val, "address").unwrap();
        match encoded {
            DynSolValue::Address(addr) => {
                assert_eq!(
                    format!("{addr:?}"),
                    "0x1234567890123456789012345678901234567890"
                );
            }
            _ => panic!("Expected Address"),
        }
    }

    #[test]
    fn test_parse_function_signature_complex() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        // Test function with multiple parameters
        let (name, params) =
            DummyExecutor::parse_function_signature("updatePrices(address[],uint256[])").unwrap();
        assert_eq!(name, "updatePrices");
        assert_eq!(params, vec!["address[]", "uint256[]"]);

        // Test function with spaces
        let (name, params) =
            DummyExecutor::parse_function_signature("approve(address spender, uint256 amount)")
                .unwrap();
        assert_eq!(name, "approve");
        assert_eq!(params, vec!["address", "uint256"]);

        // Test function with no spaces after comma
        let (name, params) =
            DummyExecutor::parse_function_signature("transferFrom(address,address,uint256)")
                .unwrap();
        assert_eq!(name, "transferFrom");
        assert_eq!(params, vec!["address", "address", "uint256"]);
    }

    #[test]
    fn test_encode_parameter_array_types() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        // Test address array from JSON array
        let addresses = json!([
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222"
        ]);
        let encoded = DummyExecutor::encode_single_parameter(&addresses, "address[]").unwrap();
        match encoded {
            DynSolValue::Array(arr) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    DynSolValue::Address(_) => {}
                    _ => panic!("Expected Address in array"),
                }
            }
            _ => panic!("Expected Array"),
        }

        // Test address array from JSON string
        let addresses_str = json!("[\"0x3333333333333333333333333333333333333333\"]");
        let encoded = DummyExecutor::encode_single_parameter(&addresses_str, "address[]").unwrap();
        match encoded {
            DynSolValue::Array(arr) => {
                assert_eq!(arr.len(), 1);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_encode_parameter_bool() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        let val_true = json!(true);
        let encoded = DummyExecutor::encode_single_parameter(&val_true, "bool").unwrap();
        match encoded {
            DynSolValue::Bool(b) => assert!(b),
            _ => panic!("Expected Bool"),
        }

        let val_false = json!(false);
        let encoded = DummyExecutor::encode_single_parameter(&val_false, "bool").unwrap();
        match encoded {
            DynSolValue::Bool(b) => assert!(!b),
            _ => panic!("Expected Bool"),
        }
    }

    #[test]
    fn test_encode_parameter_uint256_string() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        // Test large uint256 as string
        let val = json!("1000000000000000000"); // 1e18
        let encoded = DummyExecutor::encode_single_parameter(&val, "uint256").unwrap();
        match encoded {
            DynSolValue::Uint(v, 256) => {
                assert_eq!(v, U256::from_str_radix("1000000000000000000", 10).unwrap());
            }
            _ => panic!("Expected Uint256"),
        }
    }

    #[test]
    fn test_encode_parameters_mismatch() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        let params = vec![Parameter {
            param_type: "address".to_string(),
            value: json!("0x1234567890123456789012345678901234567890"),
        }];
        let param_types = vec!["address".to_string(), "uint256".to_string()];

        // Should fail due to parameter count mismatch
        assert!(DummyExecutor::encode_parameters(&params, &param_types).is_err());
    }

    #[test]
    fn test_encode_parameter_invalid_types() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        // Invalid address (not a string)
        let val = json!(12345);
        assert!(DummyExecutor::encode_single_parameter(&val, "address").is_err());

        // Invalid bool (not a boolean)
        let val = json!("true");
        assert!(DummyExecutor::encode_single_parameter(&val, "bool").is_err());

        // Invalid uint256 (not a number or numeric string)
        let val = json!("not a number");
        assert!(DummyExecutor::encode_single_parameter(&val, "uint256").is_err());

        // Unsupported type
        let val = json!("test");
        assert!(DummyExecutor::encode_single_parameter(&val, "bytes32").is_err());
    }

    #[test]
    fn test_create_param_definitions() {
        type DummyExecutor = FunctionExecutor<
            alloy::transports::http::Http<alloy::transports::http::Client>,
            alloy::network::Ethereum,
            alloy::providers::RootProvider<
                alloy::transports::http::Http<alloy::transports::http::Client>,
            >,
        >;

        let param_types = vec![
            "address".to_string(),
            "uint256".to_string(),
            "bool".to_string(),
        ];
        let params = DummyExecutor::create_param_definitions(&param_types);

        assert_eq!(params.len(), 3);
        assert_eq!(params[0].ty, "address");
        assert_eq!(params[0].name, "param0");
        assert_eq!(params[1].ty, "uint256");
        assert_eq!(params[1].name, "param1");
        assert_eq!(params[2].ty, "bool");
        assert_eq!(params[2].name, "param2");
    }
}
