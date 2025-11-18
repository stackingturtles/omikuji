//! Custom assertion helpers for common test patterns

use crate::datafeed::contract_utils::parse_address;
use alloy::primitives::Address;

/// Custom assertions for address validation
pub mod address_assertions {
    use super::*;

    /// Assert that an address string is valid
    pub fn assert_valid_address(addr: &str) {
        let parsed = parse_address(addr);
        assert!(parsed.is_ok(), "Address should be valid: {addr}");
    }

    /// Assert that an address string is invalid
    pub fn assert_invalid_address(addr: &str) {
        let parsed = parse_address(addr);
        assert!(parsed.is_err(), "Address should be invalid: {addr}");
    }

    /// Assert that two addresses are equal (case-insensitive)
    pub fn assert_addresses_equal(addr1: &str, addr2: &str) {
        let parsed1 = parse_address(addr1).expect("First address should be valid");
        let parsed2 = parse_address(addr2).expect("Second address should be valid");
        assert_eq!(
            parsed1, parsed2,
            "Addresses should be equal: {addr1} vs {addr2}"
        );
    }

    /// Assert that an address is the zero address
    pub fn assert_zero_address(addr: Address) {
        assert_eq!(addr, Address::ZERO, "Address should be zero address");
    }

    /// Assert that an address is not the zero address
    pub fn assert_not_zero_address(addr: Address) {
        assert_ne!(addr, Address::ZERO, "Address should not be zero address");
    }
}

/// Custom assertions for gas price conversions and calculations
pub mod gas_assertions {

    /// Assert gwei to wei conversion is correct
    pub fn assert_gwei_to_wei_conversion(gwei: f64, expected_wei: u64) {
        let actual_wei = (gwei * 1e9) as u64;
        assert_eq!(
            actual_wei, expected_wei,
            "Gwei to Wei conversion failed: {gwei} gwei should be {expected_wei} wei, got {actual_wei}"
        );
    }

    /// Assert wei to gwei conversion is correct
    pub fn assert_wei_to_gwei_conversion(wei: u64, expected_gwei: f64) {
        let actual_gwei = wei as f64 / 1e9;
        let tolerance = 0.000001; // 1 wei tolerance
        assert!(
            (actual_gwei - expected_gwei).abs() < tolerance,
            "Wei to Gwei conversion failed: {wei} wei should be {expected_gwei} gwei, got {actual_gwei}"
        );
    }

    /// Assert that gas efficiency is within expected range
    pub fn assert_gas_efficiency(gas_used: u64, gas_limit: u64, expected_efficiency: f64) {
        let actual_efficiency = (gas_used as f64 / gas_limit as f64) * 100.0;
        let tolerance = 0.1; // 0.1% tolerance
        assert!(
            (actual_efficiency - expected_efficiency).abs() < tolerance,
            "Gas efficiency should be {expected_efficiency}%, got {actual_efficiency}%"
        );
    }

    /// Assert that gas limit is reasonable (not too high or too low)
    pub fn assert_reasonable_gas_limit(gas_limit: u64) {
        assert!(gas_limit >= 21_000, "Gas limit too low: {gas_limit}");
        assert!(gas_limit <= 30_000_000, "Gas limit too high: {gas_limit}");
    }

    /// Assert that gas price is reasonable (in wei)
    pub fn assert_reasonable_gas_price(gas_price_wei: u64) {
        let gas_price_gwei = gas_price_wei as f64 / 1e9;
        assert!(
            gas_price_gwei >= 1.0,
            "Gas price too low: {gas_price_gwei} gwei"
        );
        assert!(
            gas_price_gwei <= 1000.0,
            "Gas price too high: {gas_price_gwei} gwei"
        );
    }
}

/// Custom assertions for value scaling and calculations
pub mod value_assertions {

    /// Assert that value scaling is correct
    pub fn assert_value_scaling(original_value: f64, decimals: u8, expected_scaled: i128) {
        let multiplier = 10_i128.pow(decimals as u32);
        let actual_scaled = (original_value * multiplier as f64) as i128;
        assert_eq!(
            actual_scaled, expected_scaled,
            "Value scaling failed: {original_value} with {decimals} decimals should be {expected_scaled}, got {actual_scaled}"
        );
    }

    /// Assert that deviation percentage is within tolerance
    pub fn assert_deviation_within_tolerance(
        old_value: f64,
        new_value: f64,
        max_deviation_percent: f64,
    ) {
        let deviation = ((new_value - old_value) / old_value * 100.0).abs();
        assert!(
            deviation <= max_deviation_percent,
            "Deviation {deviation}% exceeds maximum {max_deviation_percent}%"
        );
    }

    /// Assert that a value is within expected bounds
    pub fn assert_value_in_bounds(value: f64, min_value: f64, max_value: f64) {
        assert!(
            value >= min_value && value <= max_value,
            "Value {value} should be between {min_value} and {max_value}"
        );
    }

    /// Assert that two floating point values are approximately equal
    pub fn assert_float_approx_eq(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "Values not approximately equal: expected {expected}, got {actual}, tolerance {tolerance}"
        );
    }
}

/// Custom assertions for timestamp and time-related values
pub mod time_assertions {

    use std::time::{SystemTime, UNIX_EPOCH};

    /// Assert that a timestamp is recent (within last minute)
    pub fn assert_recent_timestamp(timestamp: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let age = now.saturating_sub(timestamp);
        assert!(
            age <= 60,
            "Timestamp should be recent, but is {age} seconds old"
        );
    }

    /// Assert that a timestamp is within a specific age range
    pub fn assert_timestamp_age(timestamp: u64, min_age_secs: u64, max_age_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let age = now.saturating_sub(timestamp);
        assert!(
            age >= min_age_secs && age <= max_age_secs,
            "Timestamp age {age} should be between {min_age_secs} and {max_age_secs} seconds"
        );
    }

    /// Assert that two timestamps are within a certain duration of each other
    pub fn assert_timestamps_close(timestamp1: u64, timestamp2: u64, max_diff_secs: u64) {
        let diff = timestamp1.abs_diff(timestamp2);
        assert!(
            diff <= max_diff_secs,
            "Timestamps should be within {max_diff_secs} seconds of each other, but differ by {diff}"
        );
    }

    /// Assert that a timestamp is valid (not zero, not too far in future)
    pub fn assert_valid_timestamp(timestamp: u64) {
        assert!(timestamp > 0, "Timestamp should not be zero");

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Allow up to 1 hour in the future for clock skew
        assert!(
            timestamp <= now + 3600,
            "Timestamp {timestamp} should not be more than 1 hour in the future (now: {now})"
        );
    }
}

/// Custom assertions for network and transaction validation
pub mod transaction_assertions {

    use alloy::primitives::TxHash;

    /// Assert that a transaction hash is valid (non-zero)
    pub fn assert_valid_tx_hash(tx_hash: TxHash) {
        assert_ne!(tx_hash, TxHash::ZERO, "Transaction hash should not be zero");
    }

    /// Assert that a transaction status is successful
    pub fn assert_transaction_success(status: &str) {
        assert_eq!(status, "success", "Transaction should be successful");
    }

    /// Assert that a transaction status indicates failure
    pub fn assert_transaction_failed(status: &str) {
        assert!(
            status == "failed" || status == "reverted",
            "Transaction should be failed or reverted, got: {status}"
        );
    }

    /// Assert that block number is reasonable
    pub fn assert_reasonable_block_number(block_number: u64) {
        assert!(block_number > 0, "Block number should be positive");
        // Allow up to 50M blocks (very generous upper bound)
        assert!(
            block_number < 50_000_000,
            "Block number seems unreasonably high: {block_number}"
        );
    }
}

/// Custom assertions for configuration validation
pub mod config_assertions {

    /// Assert that a network name is valid
    pub fn assert_valid_network_name(name: &str) {
        assert!(!name.is_empty(), "Network name should not be empty");
        assert!(
            name.len() <= 50,
            "Network name should not be too long: {name}"
        );
        assert!(
            name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
            "Network name should only contain alphanumeric characters, hyphens, and underscores: {name}"
        );
    }

    /// Assert that a feed name is valid
    pub fn assert_valid_feed_name(name: &str) {
        assert!(!name.is_empty(), "Feed name should not be empty");
        assert!(name.len() <= 50, "Feed name should not be too long: {name}");
        assert!(
            name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "Feed name should only contain alphanumeric characters and underscores: {name}"
        );
    }

    /// Assert that a URL is valid
    pub fn assert_valid_url(url: &str) {
        assert!(!url.is_empty(), "URL should not be empty");
        assert!(
            url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("ws://")
                || url.starts_with("wss://"),
            "URL should have a valid scheme: {url}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{Address, TxHash};

    #[test]
    fn test_address_assertions() {
        address_assertions::assert_valid_address("0x1234567890123456789012345678901234567890");
        address_assertions::assert_invalid_address("0x123"); // too short
        address_assertions::assert_invalid_address(""); // empty

        address_assertions::assert_zero_address(Address::ZERO);
        address_assertions::assert_not_zero_address(Address::from([1; 20]));
    }

    #[test]
    fn test_gas_assertions() {
        gas_assertions::assert_gwei_to_wei_conversion(1.0, 1_000_000_000);
        gas_assertions::assert_gwei_to_wei_conversion(50.5, 50_500_000_000);

        gas_assertions::assert_wei_to_gwei_conversion(1_000_000_000, 1.0);
        gas_assertions::assert_wei_to_gwei_conversion(50_500_000_000, 50.5);

        gas_assertions::assert_gas_efficiency(150_000, 200_000, 75.0);
        gas_assertions::assert_reasonable_gas_limit(100_000);
        gas_assertions::assert_reasonable_gas_price(30_000_000_000); // 30 gwei
    }

    #[test]
    fn test_value_assertions() {
        value_assertions::assert_value_scaling(123.456, 8, 12345600000);
        value_assertions::assert_deviation_within_tolerance(100.0, 105.0, 10.0);
        value_assertions::assert_value_in_bounds(50.0, 0.0, 100.0);
        value_assertions::assert_float_approx_eq(1.0, 1.0001, 0.001);
    }

    #[test]
    fn test_time_assertions() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        time_assertions::assert_recent_timestamp(now);
        time_assertions::assert_valid_timestamp(now);
        time_assertions::assert_timestamps_close(now, now - 5, 10);
    }

    #[test]
    fn test_transaction_assertions() {
        let tx_hash = TxHash::from([1; 32]);
        transaction_assertions::assert_valid_tx_hash(tx_hash);
        transaction_assertions::assert_transaction_success("success");
        transaction_assertions::assert_transaction_failed("failed");
        transaction_assertions::assert_reasonable_block_number(18_000_000);
    }

    #[test]
    fn test_config_assertions() {
        config_assertions::assert_valid_network_name("ethereum-mainnet");
        config_assertions::assert_valid_feed_name("eth_usd");
        config_assertions::assert_valid_url("https://example.com");
    }

    #[test]
    #[should_panic(expected = "Address should be invalid")]
    fn test_invalid_address_assertion_fails() {
        address_assertions::assert_invalid_address("0x1234567890123456789012345678901234567890");
    }

    #[test]
    #[should_panic(expected = "Gas limit too low")]
    fn test_gas_limit_too_low_fails() {
        gas_assertions::assert_reasonable_gas_limit(10_000);
    }
}
