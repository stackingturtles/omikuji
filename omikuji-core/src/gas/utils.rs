//! Gas utility functions for unit conversions and calculations
//!
//! This module provides standardized utilities for gas-related operations,
//! ensuring consistent handling of gas units across the codebase.

use alloy::primitives::U256;
use anyhow::{Context, Result};

/// Convert gwei to wei
///
/// # Arguments
/// * `gwei` - The amount in gwei (can be fractional)
///
/// # Returns
/// The equivalent amount in wei as U256
pub fn gwei_to_wei(gwei: f64) -> U256 {
    // 1 gwei = 10^9 wei
    U256::from((gwei * 1e9) as u64)
}

/// Convert wei to gwei
///
/// # Arguments
/// * `wei` - The amount in wei
///
/// # Returns
/// The equivalent amount in gwei as f64
pub fn wei_to_gwei(wei: U256) -> f64 {
    // Convert to u128 first to avoid overflow in f64 conversion
    let wei_u128 = wei.min(U256::from(u128::MAX)).to::<u128>();
    wei_u128 as f64 / 1e9
}

/// Convert ether to wei
///
/// # Arguments
/// * `ether` - The amount in ether (can be fractional)
///
/// # Returns
/// The equivalent amount in wei as U256
pub fn ether_to_wei(ether: f64) -> U256 {
    // 1 ether = 10^18 wei
    U256::from((ether * 1e18) as u128)
}

/// Convert wei to ether
///
/// # Arguments
/// * `wei` - The amount in wei
///
/// # Returns
/// The equivalent amount in ether as f64
pub fn wei_to_ether(wei: U256) -> f64 {
    // Convert to u128 first to avoid overflow in f64 conversion
    let wei_u128 = wei.min(U256::from(u128::MAX)).to::<u128>();
    wei_u128 as f64 / 1e18
}

/// Calculate gas cost in wei
///
/// # Arguments
/// * `gas_used` - The amount of gas used
/// * `gas_price` - The gas price in wei
///
/// # Returns
/// The total cost in wei
pub fn calculate_gas_cost(gas_used: u64, gas_price: U256) -> U256 {
    U256::from(gas_used) * gas_price
}

/// Calculate gas cost in gwei
///
/// # Arguments
/// * `gas_used` - The amount of gas used
/// * `gas_price_gwei` - The gas price in gwei
///
/// # Returns
/// The total cost in gwei
pub fn calculate_gas_cost_gwei(gas_used: u64, gas_price_gwei: f64) -> f64 {
    gas_used as f64 * gas_price_gwei
}

/// Apply a multiplier to a gas limit
///
/// # Arguments
/// * `gas_limit` - The base gas limit
/// * `multiplier` - The multiplier to apply (e.g., 1.2 for 20% buffer)
///
/// # Returns
/// The adjusted gas limit
pub fn apply_gas_multiplier(gas_limit: u64, multiplier: f64) -> u64 {
    (gas_limit as f64 * multiplier).ceil() as u64
}

/// Calculate fee bump for retry attempts
///
/// # Arguments
/// * `base_fee` - The original fee in wei
/// * `attempt` - The retry attempt number (1-based)
/// * `increase_percent` - The percentage increase per attempt
///
/// # Returns
/// The bumped fee in wei
pub fn calculate_fee_bump(base_fee: U256, attempt: u32, increase_percent: f64) -> U256 {
    if attempt <= 1 {
        return base_fee;
    }

    let multiplier = 1.0 + (increase_percent / 100.0);
    let total_multiplier = multiplier.powi(attempt as i32 - 1);

    // Calculate bumped fee, ensuring we don't overflow
    let base_as_f64 = base_fee.min(U256::from(u128::MAX)).to::<u128>() as f64;
    let bumped = (base_as_f64 * total_multiplier) as u128;

    U256::from(bumped)
}

/// Format wei as a human-readable string with unit
///
/// # Arguments
/// * `wei` - The amount in wei
///
/// # Returns
/// A formatted string like "1.5 ETH", "100 gwei", or "1000 wei"
pub fn format_wei(wei: U256) -> String {
    let wei_f64 = wei.min(U256::from(u128::MAX)).to::<u128>() as f64;

    if wei_f64 >= 1e18 {
        format!("{:.4} ETH", wei_f64 / 1e18)
    } else if wei_f64 >= 1e9 {
        format!("{:.2} gwei", wei_f64 / 1e9)
    } else {
        format!("{wei} wei")
    }
}

/// Parse a gas price string with unit (e.g., "50 gwei", "0.001 eth")
///
/// # Arguments
/// * `s` - The string to parse
///
/// # Returns
/// The parsed amount in wei
pub fn parse_gas_price(s: &str) -> Result<U256> {
    let s = s.trim().to_lowercase();

    if let Some(gwei_str) = s.strip_suffix(" gwei") {
        let gwei: f64 = gwei_str.parse().context("Invalid gwei amount")?;
        Ok(gwei_to_wei(gwei))
    } else if let Some(eth_str) = s.strip_suffix(" eth") {
        let eth: f64 = eth_str.parse().context("Invalid eth amount")?;
        Ok(ether_to_wei(eth))
    } else if let Some(wei_str) = s.strip_suffix(" wei") {
        let wei: u128 = wei_str.parse().context("Invalid wei amount")?;
        Ok(U256::from(wei))
    } else {
        // Assume wei if no unit specified
        let wei: u128 = s.parse().context("Invalid gas price")?;
        Ok(U256::from(wei))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gwei_to_wei() {
        assert_eq!(gwei_to_wei(1.0), U256::from(1_000_000_000u64));
        assert_eq!(gwei_to_wei(50.0), U256::from(50_000_000_000u64));
        assert_eq!(gwei_to_wei(0.1), U256::from(100_000_000u64));
    }

    #[test]
    fn test_wei_to_gwei() {
        assert_eq!(wei_to_gwei(U256::from(1_000_000_000u64)), 1.0);
        assert_eq!(wei_to_gwei(U256::from(50_000_000_000u64)), 50.0);
        assert_eq!(wei_to_gwei(U256::from(100_000_000u64)), 0.1);
    }

    #[test]
    fn test_ether_conversions() {
        assert_eq!(ether_to_wei(1.0), U256::from(1_000_000_000_000_000_000u128));
        assert_eq!(wei_to_ether(U256::from(1_000_000_000_000_000_000u128)), 1.0);
    }

    #[test]
    fn test_gas_cost_calculation() {
        let gas_used = 100_000u64;
        let gas_price = U256::from(50_000_000_000u64); // 50 gwei
        let cost = calculate_gas_cost(gas_used, gas_price);
        assert_eq!(cost, U256::from(5_000_000_000_000_000u128)); // 0.005 ETH
    }

    #[test]
    fn test_fee_bump() {
        let base_fee = U256::from(100_000_000_000u64); // 100 gwei

        // No bump for first attempt
        assert_eq!(calculate_fee_bump(base_fee, 1, 10.0), base_fee);

        // 10% bump for second attempt
        assert_eq!(
            calculate_fee_bump(base_fee, 2, 10.0),
            U256::from(110_000_000_000u64)
        );

        // 21% bump for third attempt (1.1^2)
        assert_eq!(
            calculate_fee_bump(base_fee, 3, 10.0),
            U256::from(121_000_000_000u64)
        );
    }

    #[test]
    fn test_format_wei() {
        assert_eq!(
            format_wei(U256::from(1_000_000_000_000_000_000u128)),
            "1.0000 ETH"
        );
        assert_eq!(format_wei(U256::from(50_000_000_000u64)), "50.00 gwei");
        assert_eq!(format_wei(U256::from(100u64)), "100 wei");
    }

    #[test]
    fn test_parse_gas_price() {
        assert_eq!(
            parse_gas_price("50 gwei").unwrap(),
            U256::from(50_000_000_000u64)
        );
        assert_eq!(
            parse_gas_price("0.001 eth").unwrap(),
            U256::from(1_000_000_000_000_000u128)
        );
        assert_eq!(parse_gas_price("1000 wei").unwrap(), U256::from(1000u64));
        assert_eq!(parse_gas_price("1000").unwrap(), U256::from(1000u64));
    }

    #[test]
    fn test_apply_gas_multiplier() {
        assert_eq!(apply_gas_multiplier(100_000, 1.2), 120_000);
        assert_eq!(apply_gas_multiplier(100_000, 1.5), 150_000);
        assert_eq!(apply_gas_multiplier(100_001, 1.1), 110_002); // Tests ceiling
    }
}
