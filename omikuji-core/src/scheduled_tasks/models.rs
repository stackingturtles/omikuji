use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub network: String,
    pub schedule: String, // Cron expression
    pub check_condition: Option<CheckCondition>,
    pub target_function: TargetFunction,
    pub gas_config: Option<GasConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CheckCondition {
    Property {
        contract_address: String,
        property: String,
        expected_value: serde_json::Value,
    },
    Function {
        contract_address: String,
        function: String,
        expected_value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetFunction {
    pub contract_address: String,
    pub function: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub value: serde_json::Value,
    #[serde(rename = "type")]
    pub param_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    pub max_gas_price_gwei: Option<u64>,
    pub gas_limit: Option<u64>,
    pub priority_fee_gwei: Option<u64>,
}

impl ScheduledTask {
    pub fn validate(&self) -> Result<(), String> {
        // Validate cron expression
        cron::Schedule::from_str(&self.schedule)
            .map_err(|e| format!("Invalid cron expression '{}': {}", self.schedule, e))?;

        // Validate addresses
        self.validate_address(&self.target_function.contract_address)?;

        if let Some(condition) = &self.check_condition {
            match condition {
                CheckCondition::Property {
                    contract_address, ..
                }
                | CheckCondition::Function {
                    contract_address, ..
                } => {
                    self.validate_address(contract_address)?;
                }
            }
        }

        Ok(())
    }

    fn validate_address(&self, address: &str) -> Result<(), String> {
        address
            .parse::<Address>()
            .map_err(|e| format!("Invalid address '{address}': {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_scheduled_task_validation() {
        let task = ScheduledTask {
            name: "test_task".to_string(),
            network: "ethereum-mainnet".to_string(),
            schedule: "0 0 * * * *".to_string(), // Every hour at minute 0
            check_condition: Some(CheckCondition::Property {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                property: "isReady".to_string(),
                expected_value: serde_json::json!(true),
            }),
            target_function: TargetFunction {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "execute()".to_string(),
                parameters: vec![],
            },
            gas_config: None,
        };

        match task.validate() {
            Ok(()) => {}
            Err(e) => panic!("Validation failed: {e}"),
        }
    }

    #[test]
    fn test_invalid_cron_expression() {
        let task = ScheduledTask {
            name: "test_task".to_string(),
            network: "ethereum-mainnet".to_string(),
            schedule: "invalid cron".to_string(),
            check_condition: None,
            target_function: TargetFunction {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "execute()".to_string(),
                parameters: vec![],
            },
            gas_config: None,
        };

        assert!(task.validate().is_err());
    }

    #[test]
    fn test_gas_config_creation() {
        let gas_config = GasConfig {
            gas_limit: Some(500000),
            max_gas_price_gwei: Some(100),
            priority_fee_gwei: Some(2),
        };

        assert_eq!(gas_config.gas_limit, Some(500000));
        assert_eq!(gas_config.max_gas_price_gwei, Some(100));
        assert_eq!(gas_config.priority_fee_gwei, Some(2));
    }

    #[test]
    fn test_parameter_types() {
        let address_param = Parameter {
            param_type: "address".to_string(),
            value: json!("0x9876543210987654321098765432109876543210"),
        };
        assert_eq!(address_param.param_type, "address");

        let uint_param = Parameter {
            param_type: "uint256".to_string(),
            value: json!("1000000000000000000"),
        };
        assert_eq!(uint_param.param_type, "uint256");

        let bool_param = Parameter {
            param_type: "bool".to_string(),
            value: json!(true),
        };
        assert_eq!(bool_param.param_type, "bool");

        let array_param = Parameter {
            param_type: "address[]".to_string(),
            value: json!(["0x1111111111111111111111111111111111111111"]),
        };
        assert_eq!(array_param.param_type, "address[]");
    }

    #[test]
    fn test_target_function_variations() {
        // No parameters
        let func1 = TargetFunction {
            contract_address: "0xABCDEF1234567890123456789012345678901234".to_string(),
            function: "pause()".to_string(),
            parameters: vec![],
        };
        assert_eq!(func1.parameters.len(), 0);

        // Single parameter
        let func2 = TargetFunction {
            contract_address: "0xABCDEF1234567890123456789012345678901234".to_string(),
            function: "setThreshold(uint256)".to_string(),
            parameters: vec![Parameter {
                param_type: "uint256".to_string(),
                value: json!("1000"),
            }],
        };
        assert_eq!(func2.parameters.len(), 1);

        // Multiple parameters
        let func3 = TargetFunction {
            contract_address: "0xABCDEF1234567890123456789012345678901234".to_string(),
            function: "transferBatch(address[],uint256[])".to_string(),
            parameters: vec![
                Parameter {
                    param_type: "address[]".to_string(),
                    value: json!([
                        "0x1111111111111111111111111111111111111111",
                        "0x2222222222222222222222222222222222222222"
                    ]),
                },
                Parameter {
                    param_type: "uint256[]".to_string(),
                    value: json!(["100", "200"]),
                },
            ],
        };
        assert_eq!(func3.parameters.len(), 2);
    }

    #[test]
    fn test_check_condition_property() {
        let condition = CheckCondition::Property {
            contract_address: "0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string(),
            property: "isActive".to_string(),
            expected_value: json!(true),
        };

        match condition {
            CheckCondition::Property {
                property,
                expected_value,
                ..
            } => {
                assert_eq!(property, "isActive");
                assert_eq!(expected_value, json!(true));
            }
            _ => panic!("Wrong condition type"),
        }
    }

    #[test]
    fn test_check_condition_function() {
        let condition = CheckCondition::Function {
            contract_address: "0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string(),
            function: "getBalance() (uint256)".to_string(),
            expected_value: json!("1000000"),
        };

        match condition {
            CheckCondition::Function {
                function,
                expected_value,
                ..
            } => {
                assert_eq!(function, "getBalance() (uint256)");
                assert_eq!(expected_value, json!("1000000"));
            }
            _ => panic!("Wrong condition type"),
        }
    }

    #[test]
    fn test_scheduled_task_serialization() {
        let task = ScheduledTask {
            name: "serialize_test".to_string(),
            network: "testnet".to_string(),
            schedule: "*/5 * * * *".to_string(),
            check_condition: None,
            target_function: TargetFunction {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "ping()".to_string(),
                parameters: vec![],
            },
            gas_config: Some(GasConfig {
                gas_limit: Some(100000),
                max_gas_price_gwei: Some(30),
                priority_fee_gwei: Some(2),
            }),
        };

        // Serialize
        let serialized = serde_json::to_string(&task).unwrap();

        // Deserialize
        let deserialized: ScheduledTask = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.name, task.name);
        assert_eq!(deserialized.network, task.network);
        assert_eq!(deserialized.schedule, task.schedule);
        assert!(deserialized.gas_config.is_some());
    }

    #[test]
    fn test_various_cron_expressions() {
        let valid_schedules = vec![
            "0 0 0 * * *",    // Daily at midnight
            "0 */15 * * * *", // Every 15 minutes
            "0 0 */6 * * *",  // Every 6 hours
            "0 0 0 1 * *",    // Monthly on the 1st
            "0 0 0 * * 1",    // Weekly on Monday
            "*/30 * * * * *", // Every 30 seconds
            "0 0 0 1 1 *",    // Yearly on January 1st
        ];

        for schedule in valid_schedules {
            let task = ScheduledTask {
                name: "cron_test".to_string(),
                network: "test".to_string(),
                schedule: schedule.to_string(),
                check_condition: None,
                target_function: TargetFunction {
                    contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                    function: "test()".to_string(),
                    parameters: vec![],
                },
                gas_config: None,
            };

            assert!(
                task.validate().is_ok(),
                "Schedule '{schedule}' should be valid"
            );
        }
    }
}
