//! Example demonstrating the EventMonitorBuilder

use alloy::primitives::address;
use omikuji::event_monitors::{EventMonitorBuilder, HttpMethod, ResponseType};

fn main() {
    // Example 1: Basic event monitor with builder
    let monitor = EventMonitorBuilder::new()
        .name("uniswap_swap_monitor")
        .network("ethereum-mainnet")
        .contract_address(address!("1234567890123456789012345678901234567890"))
        .event_signature("Swap(address,uint256,uint256,uint256,uint256,address)")
        .webhook_url("https://api.example.com/webhooks/uniswap")
        .webhook_timeout(60)
        .webhook_retries(5)
        .build()
        .unwrap();

    println!("Created monitor: {}", monitor.name);
    println!("  Network: {}", monitor.network);
    println!("  Contract: {}", monitor.contract_address);
    println!("  Event: {}", monitor.event_signature);
    println!("  Webhook URL: {}", monitor.webhook.url);

    // Example 2: Monitor with authentication headers
    let monitor_with_auth = EventMonitorBuilder::new()
        .name("aave_liquidation_monitor")
        .network("ethereum-mainnet")
        .contract_address(address!("2345678901234567890123456789012345678901"))
        .event_signature("LiquidationCall(address,address,address,uint256,uint256,address,bool)")
        .webhook_url("https://api.example.com/webhooks/aave")
        .webhook_header("Authorization", "Bearer ${API_KEY}")
        .webhook_header("X-Custom-Header", "monitoring-service")
        .webhook_method(HttpMethod::Post)
        .build()
        .unwrap();

    println!(
        "\nCreated authenticated monitor: {}",
        monitor_with_auth.name
    );
    println!(
        "  Headers: {} configured",
        monitor_with_auth.webhook.headers.len()
    );

    // Example 3: Monitor with contract call response
    let monitor_with_action = EventMonitorBuilder::new()
        .name("price_oracle_update_monitor")
        .network("base-mainnet")
        .contract_address(address!("3456789012345678901234567890123456789012"))
        .event_signature("PriceUpdate(address,uint256,uint256)")
        .webhook_url("https://oracle.example.com/price-feed")
        .response_type(ResponseType::ContractCall)
        .contract_call("0x4567890123456789012345678901234567890123", 50)
        .build()
        .unwrap();

    println!("\nCreated action monitor: {}", monitor_with_action.name);
    println!(
        "  Response type: {:?}",
        monitor_with_action.response.response_type
    );
    if let Some(contract_call) = &monitor_with_action.response.contract_call {
        println!("  Target contract: {}", contract_call.target_contract);
        println!("  Max gas price: {} gwei", contract_call.max_gas_price_gwei);
    }

    // Example 4: Monitor with signature validation
    let signers = vec![
        address!("5678901234567890123456789012345678901234"),
        address!("6789012345678901234567890123456789012345"),
    ];

    let monitor_with_validation = EventMonitorBuilder::new()
        .name("governance_proposal_monitor")
        .network("ethereum-mainnet")
        .contract_address(address!("7890123456789012345678901234567890123456"))
        .event_signature("ProposalCreated(uint256,address,address[],uint256[],string[],bytes[],uint256,uint256,string)")
        .webhook_url("https://governance.example.com/webhooks/proposals")
        .require_signature(signers.clone())
        .build()
        .unwrap();

    println!(
        "\nCreated validated monitor: {}",
        monitor_with_validation.name
    );
    if let Some(validation) = &monitor_with_validation.response.validation {
        println!("  Signature required: {}", validation.require_signature);
        println!("  Allowed signers: {}", validation.allowed_signers.len());
        println!(
            "  Max response age: {} seconds",
            validation.max_response_age_seconds
        );
    }

    // Example 5: Error handling
    let result = EventMonitorBuilder::new()
        .name("incomplete_monitor")
        .network("ethereum-mainnet")
        // Missing contract_address and other required fields
        .build();

    match result {
        Ok(_) => println!("\nUnexpected success!"),
        Err(e) => println!("\nExpected error: {e}"),
    }
}
