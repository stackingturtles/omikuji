use crate::config::models::Datafeed;
use crate::contracts::FluxAggregatorContract;
use alloy::{
    network::Ethereum,
    primitives::{Address, I256},
    providers::Provider,
    transports::Transport,
};
use anyhow::{Context, Result};
use tracing::warn;

/// Parses and validates an Ethereum address
pub fn parse_address(address_str: &str) -> Result<Address> {
    address_str
        .parse::<Address>()
        .with_context(|| format!("Invalid contract address: {address_str}"))
}

/// Creates a FluxAggregator contract instance with a provider
pub fn create_contract_with_provider<T, P>(
    address: Address,
    provider: P,
) -> FluxAggregatorContract<T, P>
where
    T: Transport + Clone,
    P: Provider<T, Ethereum> + Clone,
{
    FluxAggregatorContract::new(address, provider)
}

/// Scales a floating point value by decimals for contract submission
pub fn scale_value_for_contract(value: f64, decimals: u8) -> i128 {
    let multiplier = 10f64.powi(decimals as i32);
    (value * multiplier).round() as i128
}

/// Validates a scaled value against min/max bounds
pub fn validate_value_bounds(scaled_value: i128, datafeed: &Datafeed) -> Result<()> {
    // Convert scaled_value to I256 for comparison
    let scaled_value_i256 =
        I256::try_from(scaled_value).context("Failed to convert scaled value to I256")?;

    if let Some(ref min_value) = datafeed.min_value {
        if scaled_value_i256 < *min_value {
            warn!(
                "Value {} is below minimum {} for datafeed {}",
                scaled_value, min_value, datafeed.name
            );
            return Err(anyhow::anyhow!("Value below minimum submission value"));
        }
    }

    if let Some(ref max_value) = datafeed.max_value {
        if scaled_value_i256 > *max_value {
            warn!(
                "Value {} is above maximum {} for datafeed {}",
                scaled_value, max_value, datafeed.name
            );
            return Err(anyhow::anyhow!("Value above maximum submission value"));
        }
    }

    Ok(())
}

/// Gets current Unix timestamp in seconds
pub fn current_timestamp() -> Result<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current timestamp")
        .map(|d| d.as_secs())
}

/// Calculates the percentage deviation between two values using absolute deviation
/// Returns the deviation as a percentage (0.0 to 100.0+)
///
/// # Arguments
/// * `current_value` - The current value (on-chain)
/// * `new_value` - The new value to compare
///
/// # Returns
/// The absolute percentage deviation
pub fn calculate_deviation_percentage(current_value: i128, new_value: i128) -> f64 {
    // Handle edge case where current value is 0
    if current_value == 0 {
        // If both are 0, there's no deviation
        if new_value == 0 {
            return 0.0;
        }
        // If current is 0 but new is not, treat as 100% deviation
        return 100.0;
    }

    // Calculate absolute deviation
    let deviation = (new_value - current_value).abs() as f64;
    let base = current_value.abs() as f64;

    // Return percentage
    (deviation / base) * 100.0
}

/// Common error messages
pub mod errors {
    pub const NO_SIGNER_AVAILABLE: &str = "No signer available for network";
    pub const CONTRACT_SUBMISSION_FAILED: &str = "Contract submission failed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deviation_percentage_normal_cases() {
        // 10% increase: 100 -> 110
        assert_eq!(calculate_deviation_percentage(100, 110), 10.0);

        // 10% decrease: 100 -> 90
        assert_eq!(calculate_deviation_percentage(100, 90), 10.0);

        // 50% increase: 100 -> 150
        assert_eq!(calculate_deviation_percentage(100, 150), 50.0);

        // 50% decrease: 100 -> 50
        assert_eq!(calculate_deviation_percentage(100, 50), 50.0);
    }

    #[test]
    fn test_deviation_percentage_negative_values() {
        // From negative to negative
        assert_eq!(calculate_deviation_percentage(-100, -110), 10.0);
        assert_eq!(calculate_deviation_percentage(-100, -90), 10.0);

        // From negative to positive (200% change)
        assert_eq!(calculate_deviation_percentage(-100, 100), 200.0);

        // From positive to negative (200% change)
        assert_eq!(calculate_deviation_percentage(100, -100), 200.0);
    }

    #[test]
    fn test_deviation_percentage_zero_cases() {
        // Both zero
        assert_eq!(calculate_deviation_percentage(0, 0), 0.0);

        // From zero to non-zero (100% deviation)
        assert_eq!(calculate_deviation_percentage(0, 100), 100.0);
        assert_eq!(calculate_deviation_percentage(0, -100), 100.0);

        // From non-zero to zero (100% deviation)
        assert_eq!(calculate_deviation_percentage(100, 0), 100.0);
        assert_eq!(calculate_deviation_percentage(-100, 0), 100.0);
    }

    #[test]
    fn test_deviation_percentage_no_change() {
        assert_eq!(calculate_deviation_percentage(100, 100), 0.0);
        assert_eq!(calculate_deviation_percentage(-100, -100), 0.0);
        assert_eq!(calculate_deviation_percentage(12345, 12345), 0.0);
    }

    #[test]
    fn test_deviation_percentage_small_changes() {
        // 0.1% change
        assert_eq!(calculate_deviation_percentage(1000, 1001), 0.1);

        // 0.5% change
        assert_eq!(calculate_deviation_percentage(1000, 1005), 0.5);

        // 0.01% change (1 basis point)
        assert_eq!(calculate_deviation_percentage(10000, 10001), 0.01);
    }

    #[test]
    fn test_deviation_percentage_large_values() {
        // Using large i128 values (simulating scaled contract values)
        let large_base: i128 = 1_000_000_000_000; // 1 trillion
        let large_new: i128 = 1_010_000_000_000; // 1.01 trillion (1% increase)

        assert_eq!(calculate_deviation_percentage(large_base, large_new), 1.0);
    }
}
