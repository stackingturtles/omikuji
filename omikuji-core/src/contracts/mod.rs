pub mod abi_utils;
pub mod flux_aggregator;
pub mod flux_aggregator_v2;
pub mod generic_caller;
pub mod interaction;

pub use abi_utils::{
    common_calls, encode_function_call, encode_parameter, encode_parameters,
    parse_function_signature, ContractCallBuilder,
};
pub use flux_aggregator::FluxAggregatorContract;
pub use flux_aggregator_v2::FluxAggregatorContractV2;
pub use generic_caller::{create_contract_reader, MetricsAwareContractCaller};
pub use interaction::{ContractInteraction, ContractReader};

#[cfg(test)]
mod tests;
