use crate::metrics::ContractMetrics;
use alloy::{
    primitives::{Address, I256, U256},
    providers::Provider,
    rpc::types::{BlockId, TransactionRequest},
    sol_types::SolCall,
};
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

pub struct ContractCaller<P: Provider> {
    provider: P,
}

impl<P: Provider> ContractCaller<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub async fn call<'a, C, R>(
        &'a self,
        address: Address,
        call_data: C,
        feed_name: Option<&'a str>,
        network: Option<&'a str>,
        method_name: &'a str,
    ) -> Result<R>
    where
        C: SolCall + Send + Sync,
        R: Send + Sync,
        for<'b> <C as SolCall>::Return: Into<R>,
    {
        let start = Instant::now();
        let tx = TransactionRequest::default()
            .to(address)
            .input(call_data.abi_encode().into());

        let result = self.provider.call(tx.clone()).block(BlockId::latest()).await;
        let duration = start.elapsed();

        match result {
            Ok(res) => {
                if let (Some(feed), Some(net)) = (feed_name, network) {
                    ContractMetrics::record_contract_read(
                        feed,
                        net,
                        method_name,
                        true,
                        duration,
                        None,
                    );
                }
                let decoded = C::abi_decode_returns(&res)?;
                Ok(decoded.into())
            }
            Err(e) => {
                if let (Some(feed), Some(net)) = (feed_name, network) {
                    ContractMetrics::record_contract_read(
                        feed,
                        net,
                        method_name,
                        false,
                        duration,
                        Some(&e.to_string()),
                    );
                }
                Err(e.into())
            }
        }
    }
}
