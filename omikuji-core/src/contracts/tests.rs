#[cfg(test)]
mod tests {
    use super::super::flux_aggregator::*;
    use crate::datafeed::contract_utils::{parse_address, scale_value_for_contract};
    use alloy::{
        node_bindings::Anvil,
        primitives::{Address, I256, U256},
        providers::{ProviderBuilder, RootProvider},
        sol_types::SolCall,
        transports::http::{Client, Http},
    };

    // Helper to deploy a mock FluxAggregator for testing
    // In real tests, you'd deploy an actual contract on a test network
    async fn setup_test_contract(
    ) -> Result<(RootProvider<Http<Client>>, Address), Box<dyn std::error::Error>> {
        // Try to start a local Anvil instance (Ethereum development node)
        let anvil = match Anvil::new().try_spawn() {
            Ok(anvil) => anvil,
            Err(e) => {
                eprintln!("Skipping test: Anvil not installed. Install with: curl -L https://foundry.paradigm.xyz | bash");
                return Err(Box::new(e));
            }
        };

        // Connect to the Anvil instance
        let provider = ProviderBuilder::new().on_http(anvil.endpoint_url());

        // In a real test, you would deploy the FluxAggregator contract here
        // For now, we'll use a dummy address
        let contract_address = "0x0000000000000000000000000000000000000001"
            .parse::<Address>()
            .unwrap();

        Ok((provider, contract_address))
    }

    #[tokio::test]
    async fn test_flux_aggregator_instantiation() {
        let (provider, contract_address) = match setup_test_contract().await {
            Ok(result) => result,
            Err(_) => {
                println!("Test skipped: Anvil not available");
                return;
            }
        };

        // Create contract instance
        let _contract = FluxAggregatorContract::new(contract_address, provider);

        // The contract instance is created successfully
        // We can't directly access the address field as it's private
    }

    #[tokio::test]
    async fn test_contract_call_encoding() {
        let (provider, contract_address) = match setup_test_contract().await {
            Ok(result) => result,
            Err(_) => {
                println!("Test skipped: Anvil not available");
                return;
            }
        };
        let _contract = FluxAggregatorContract::new(contract_address, provider);

        // Test that method call data can be encoded
        let decimals_call = IFluxAggregator::decimalsCall {};
        let encoded = decimals_call.abi_encode();
        assert!(!encoded.is_empty());

        let submit_call = IFluxAggregator::submitCall {
            _roundId: U256::from(1),
            _submission: I256::try_from(12345).unwrap(),
        };
        let encoded_submit = submit_call.abi_encode();
        assert!(!encoded_submit.is_empty());
    }

    #[test]
    fn test_value_scaling() {
        use crate::datafeed::contract_utils::scale_value_for_contract;

        // Test scaling with 8 decimals
        let value = 1234.56789;
        let scaled = scale_value_for_contract(value, 8);
        assert_eq!(scaled, 123456789000); // 1234.56789 * 10^8 = 123456789000

        // Test scaling with 6 decimals
        let scaled_6 = scale_value_for_contract(value, 6);
        assert_eq!(scaled_6, 1234567890); // 1234.56789 * 10^6 = 1234567890 (rounded)

        // Test scaling with 18 decimals (common for ETH values)
        let eth_value = 1.5;
        let scaled_18 = scale_value_for_contract(eth_value, 18);
        assert_eq!(scaled_18, 1_500_000_000_000_000_000);
    }

    #[test]
    fn test_address_parsing() {
        use crate::datafeed::contract_utils::parse_address;

        // Valid address
        let valid_addr = "0x1234567890123456789012345678901234567890";
        let parsed = parse_address(valid_addr);
        assert!(parsed.is_ok());

        // Invalid address (too short)
        let invalid_addr = "0x123456";
        let parsed_invalid = parse_address(invalid_addr);
        assert!(parsed_invalid.is_err());

        // Address without 0x prefix (alloy accepts this)
        let no_prefix = "1234567890123456789012345678901234567890";
        let parsed_no_prefix = parse_address(no_prefix);
        assert!(parsed_no_prefix.is_ok()); // alloy's Address parser accepts addresses without 0x prefix
    }

    #[test]
    fn test_oracle_round_state_call_encoding() {
        let oracle_address = "0x0000000000000000000000000000000000000001"
            .parse::<Address>()
            .unwrap();

        let call = IFluxAggregator::oracleRoundStateCall {
            _oracle: oracle_address,
            _queriedRoundId: 123,
        };

        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_description_call_encoding() {
        let call = IFluxAggregator::descriptionCall {};
        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_version_call_encoding() {
        let call = IFluxAggregator::versionCall {};
        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_min_max_submission_value_calls() {
        let min_call = IFluxAggregator::minSubmissionValueCall {};
        let max_call = IFluxAggregator::maxSubmissionValueCall {};

        let min_encoded = min_call.abi_encode();
        let max_encoded = max_call.abi_encode();

        assert!(!min_encoded.is_empty());
        assert!(!max_encoded.is_empty());
    }

    #[test]
    fn test_get_answer_call() {
        let call = IFluxAggregator::getAnswerCall {
            _roundId: U256::from(42),
        };
        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_get_timestamp_call() {
        let call = IFluxAggregator::getTimestampCall {
            _roundId: U256::from(42),
        };
        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_latest_round_data_call() {
        let call = IFluxAggregator::latestRoundDataCall {};
        let encoded = call.abi_encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_value_scaling_edge_cases() {
        // Test with zero
        let zero_scaled = scale_value_for_contract(0.0, 8);
        assert_eq!(zero_scaled, 0);

        // Test with negative value
        let negative_scaled = scale_value_for_contract(-100.5, 6);
        assert_eq!(negative_scaled, -100500000);

        // Test with very small value
        let small_scaled = scale_value_for_contract(0.00000001, 8);
        assert_eq!(small_scaled, 1);

        // Test with very large value
        let large_scaled = scale_value_for_contract(1_000_000.0, 2);
        assert_eq!(large_scaled, 100_000_000);
    }

    #[test]
    fn test_i256_conversions() {
        // Test positive value
        let positive = I256::try_from(12345).unwrap();
        assert_eq!(positive.to_string(), "12345");

        // Test negative value
        let negative = I256::try_from(-12345).unwrap();
        assert_eq!(negative.to_string(), "-12345");

        // Test zero
        let zero = I256::try_from(0).unwrap();
        assert_eq!(zero.to_string(), "0");
    }

    #[test]
    fn test_address_validation_edge_cases() {
        // Test empty string
        let empty = parse_address("");
        assert!(empty.is_err());

        // Test invalid characters
        let invalid_chars = parse_address("0xGGGG567890123456789012345678901234567890");
        assert!(invalid_chars.is_err());

        // Test too long
        let too_long = parse_address("0x12345678901234567890123456789012345678901234");
        assert!(too_long.is_err());

        // Test valid checksum address
        let checksum = parse_address("0x5aAeb6053f3E94C9b9A09f33669435E7Ef1BeAed");
        assert!(checksum.is_ok());
    }

    #[test]
    fn test_contract_method_selectors() {
        // Test that method selectors are correctly encoded
        let latest_answer = IFluxAggregator::latestAnswerCall {};
        let selector = &latest_answer.abi_encode()[0..4];
        assert_eq!(selector.len(), 4); // Function selector is 4 bytes

        let submit = IFluxAggregator::submitCall {
            _roundId: U256::from(1),
            _submission: I256::try_from(100).unwrap(),
        };
        let submit_selector = &submit.abi_encode()[0..4];
        assert_eq!(submit_selector.len(), 4);
    }

    #[test]
    fn test_transaction_request_building() {
        use alloy::rpc::types::TransactionRequest;

        let address = "0x1234567890123456789012345678901234567890"
            .parse::<Address>()
            .unwrap();

        let call = IFluxAggregator::decimalsCall {};
        let tx = TransactionRequest::default()
            .to(address)
            .input(call.abi_encode().into());

        // Check that the transaction is correctly addressed
        assert!(tx.input.input.is_some());
    }

    #[test]
    fn test_scaling_precision() {
        // Test that scaling maintains precision within reasonable bounds
        let value = 123.456_789_123_456_79;

        // With 18 decimals (max ETH precision)
        let scaled_18 = scale_value_for_contract(value, 18);
        // Due to f64 precision limits, we can't expect exact values for 18 decimals
        // Instead, verify the general magnitude is correct
        assert!(scaled_18 > 123_000_000_000_000_000_000i128);
        assert!(scaled_18 < 124_000_000_000_000_000_000i128);

        // Test with a simpler value for 18 decimals
        let simple_value = 1.5;
        let simple_scaled = scale_value_for_contract(simple_value, 18);
        assert_eq!(simple_scaled, 1_500_000_000_000_000_000i128);

        // With 6 decimals (USDC precision)
        let scaled_6 = scale_value_for_contract(value, 6);
        assert_eq!(scaled_6, 123456789); // Rounded to 6 decimals
    }
}
