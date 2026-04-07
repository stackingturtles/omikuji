//! Alloy API surface integration tests
//!
//! These tests exercise every alloy API used throughout the codebase and serve as
//! a safety net for the alloy 1.x migration. Each test documents the API surface it
//! is protecting so that future migrations produce a clear signal.
//!
//! All tests require Anvil (a local EVM node bundled with Foundry). They are marked
//! `#[ignore]` so they do not run in CI by default. Run explicitly with:
//!
//! ```
//! cargo test --test alloy_integration -- --ignored
//! ```
//!
//! MIGRATION NOTES (alloy 0.8 → 1.x) - COMPLETED:
//! - `Provider<T, Ethereum>` → `Provider` (Transport generic removed from trait)
//! - `RootProvider<Http<Client>>` → `FillProvider<JoinedRecommendedFillers, RootProvider>`
//! - `ProviderBuilder::new().on_http(url)` → `ProviderBuilder::new().connect_http(url)`
//! - `ProviderBuilder::new().with_recommended_fillers()` removed (now default in 1.x)
//! - `abi_decode_returns(&bytes, validate)` → `abi_decode_returns(&bytes)` (validate arg removed)
//! - `decoded._0` for single returns → `decoded` (returns primitive directly)
//! - `provider.call(&tx)` → `provider.call(tx.clone())` (takes ownership)
//! - `estimate_gas(&tx)` → `estimate_gas(tx.clone())` (takes ownership)
//! - `receipt.gas_used` is now `u64` (was `u128`)
//! - `get_block_by_number(num, hydrated)` → `get_block_by_number(num)` (hydrated arg removed)

use alloy::{
    dyn_abi::{DynSolType, DynSolValue, FunctionExt, JsonAbiExt},
    json_abi::{Function, Param, StateMutability},
    network::{EthereumWallet, TransactionBuilder},
    node_bindings::Anvil,
    primitives::{
        utils::{format_units, parse_units},
        Address, Bytes, I256, U256,
    },
    providers::{
        fillers::FillProvider, utils::JoinedRecommendedFillers, Provider, ProviderBuilder,
        RootProvider,
    },
    rpc::types::{BlockId, TransactionRequest},
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Default Anvil account #0 private key (no 0x prefix).
const ANVIL_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

/// Address derived from ANVIL_KEY.
const ANVIL_ADDR: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Type alias for the HTTP provider returned by `connect_http`.
type EthHttpProvider = FillProvider<JoinedRecommendedFillers, RootProvider>;

/// Spawn Anvil and return `(instance, provider)`.
/// The instance **must** be kept alive for the test duration or the node exits.
fn spawn_anvil_http() -> (alloy::node_bindings::AnvilInstance, EthHttpProvider) {
    let anvil = Anvil::new().try_spawn().expect(
        "Anvil is required for alloy integration tests. \
         Install it via `curl -L https://foundry.paradigm.xyz | bash && foundryup`",
    );
    // API under test: ProviderBuilder::new().connect_http(url)
    // provider.rs — identical pattern in NetworkManager::create_provider
    let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
    (anvil, provider)
}

/// Spawn Anvil and return `(instance, endpoint_url)`.
/// Use when a signing provider is needed so the URL can be reused.
fn spawn_anvil_url() -> (alloy::node_bindings::AnvilInstance, url::Url) {
    let anvil = Anvil::new()
        .try_spawn()
        .expect("Anvil is required for alloy integration tests.");
    let url = anvil.endpoint_url();
    (anvil, url)
}

// ── Test 1: Provider creation & basic RPC ────────────────────────────────────

/// Tests: `ProviderBuilder::new().on_http(url)`, `get_chain_id()`, `get_block_number()`,
/// `get_balance()`.
///
/// Source: `network/provider.rs:376`, `network/provider.rs:196`, `network/provider.rs:228`.
#[tokio::test]
#[ignore]
async fn test_provider_creation_and_basic_rpc() {
    let (_anvil, provider) = spawn_anvil_http();

    // get_chain_id — network/provider.rs:196
    let chain_id = provider.get_chain_id().await.expect("get_chain_id failed");
    assert_eq!(chain_id, 31337, "Anvil default chain ID should be 31337");

    // get_block_number — network/provider.rs:228
    let block = provider
        .get_block_number()
        .await
        .expect("get_block_number failed");
    assert_eq!(block, 0, "fresh Anvil starts at block 0");

    // get_balance — wallet/balance_monitor.rs pattern
    let addr: Address = ANVIL_ADDR.parse().expect("parse address");
    let balance = provider
        .get_balance(addr)
        .await
        .expect("get_balance failed");
    assert!(balance > U256::ZERO, "Anvil accounts start with 10_000 ETH");
}

// ── Test 2: EthProvider type alias ───────────────────────────────────────────

/// Tests: `pub type EthProvider = RootProvider<Http<Client>>`
///
/// Source: `network/provider.rs:37`.
/// In alloy 1.x the type alias may change to `RootProvider` (no generics) or keep the
/// same form. This test forces a compilation check that the alias is usable for real RPC
/// calls, not just as a dead type.
#[tokio::test]
#[ignore]
async fn test_eth_provider_type_alias() {
    // Reproduce the exact type alias from network/provider.rs
    type EthProvider = FillProvider<JoinedRecommendedFillers, RootProvider>;

    let (_anvil, provider): (_, EthProvider) = spawn_anvil_http();

    let chain_id: u64 = provider.get_chain_id().await.expect("get_chain_id");
    assert_eq!(chain_id, 31337);
}

// ── Test 3: Signer & wallet creation ─────────────────────────────────────────

/// Tests: `private_key.parse::<PrivateKeySigner>()`, `EthereumWallet::from(signer)`,
/// `signer.address()`.
///
/// Source: `network/provider.rs:118-126`, `transaction/queue.rs:334-338`.
#[tokio::test]
#[ignore]
async fn test_signer_and_wallet_creation() {
    // With 0x prefix — network/provider.rs:118 trimmed key variant
    let with_prefix = format!("0x{ANVIL_KEY}");
    let signer: PrivateKeySigner = with_prefix.parse().expect("parse signer with 0x prefix");
    let address = signer.address();
    let expected: Address = ANVIL_ADDR.parse().expect("parse expected address");
    assert_eq!(address, expected, "derived address mismatch");

    // Without 0x prefix — network/provider.rs:413
    let signer_no_prefix: PrivateKeySigner = ANVIL_KEY.parse().expect("parse signer without 0x");
    assert_eq!(signer_no_prefix.address(), expected);

    // EthereumWallet::from(signer) — transaction/queue.rs:337
    let wallet = EthereumWallet::from(signer);
    // The wallet is used as a signing provider; construct one and verify it connects
    let (_anvil2, url) = spawn_anvil_url();
    let signing_provider = ProviderBuilder::new().wallet(wallet).connect_http(url);
    let chain_id = signing_provider
        .get_chain_id()
        .await
        .expect("signing provider get_chain_id");
    assert_eq!(chain_id, 31337);
}

// ── Test 4: EIP-1559 transaction building and sending ────────────────────────

/// Tests: `TransactionRequest::default()`, `.to()`, `.from()`, `.with_gas_limit(u64)`,
/// `.with_max_fee_per_gas(u128)`, `.with_max_priority_fee_per_gas(u128)`,
/// `provider.send_transaction(tx)`, `pending_tx.with_required_confirmations(1).get_receipt()`,
/// `receipt.status()`, `*pending_tx.tx_hash()`.
///
/// Source: `contracts/flux_aggregator.rs:350-474`, `transaction/queue.rs:370-401`.
#[tokio::test]
#[ignore]
async fn test_eip1559_transaction_build_and_send() {
    let (_anvil, url) = spawn_anvil_url();

    let signer: PrivateKeySigner = ANVIL_KEY.parse().expect("parse signer");
    let sender = signer.address();
    let wallet = EthereumWallet::from(signer);
    // In alloy 1.x, ProviderBuilder::new() already includes recommended fillers (NonceFiller,
    // GasFiller, ChainIdFiller) by default, so we can send a minimal TransactionRequest
    // without pre-populating every field.
    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    // Receiver is Anvil account #1
    let receiver: Address = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
        .parse()
        .expect("parse receiver");

    // Build an EIP-1559 transfer — mirrors transaction/queue.rs:370-401
    let tx = TransactionRequest::default()
        .to(receiver)
        .from(sender)
        .value(U256::from(1_000_000_000_000_000_u64)) // 0.001 ETH
        .with_gas_limit(21_000_u64)
        .with_max_fee_per_gas(2_000_000_000_u128) // 2 gwei
        .with_max_priority_fee_per_gas(1_000_000_000_u128); // 1 gwei

    let pending_tx = provider
        .send_transaction(tx)
        .await
        .expect("send_transaction failed");

    // tx_hash — flux_aggregator.rs:460
    let _tx_hash = *pending_tx.tx_hash();

    // with_required_confirmations(1).get_receipt() — flux_aggregator.rs:474
    let receipt = pending_tx
        .with_required_confirmations(1)
        .get_receipt()
        .await
        .expect("get_receipt failed");

    // receipt.status() — flux_aggregator.rs:607
    assert!(receipt.status(), "transaction should succeed");

    // Verify block number is populated on the receipt
    assert!(
        receipt.block_number.is_some(),
        "receipt.block_number should be Some"
    );
}

// ── Test 5: sol! macro, call encoding, selector size, and ._0 access ────────

/// Tests: `sol! { interface ... }`, `SomeFnCall {}`, `.abi_encode()` (selector is 4 bytes),
/// `SomeFnCall::abi_decode_returns(&bytes, validate)`, `decoded._0`.
///
/// Source: `contracts/flux_aggregator.rs:22-40`, `contracts/flux_aggregator.rs:92`.
/// Uses a minimal SimpleStorage interface instead of the full FluxAggregator.
#[tokio::test]
#[ignore]
async fn test_sol_macro_encoding_and_decoding() {
    // Minimal interface used in the test — mirrors the sol! block in flux_aggregator.rs
    sol! {
        interface ISimpleStorage {
            function get() external view returns (uint256);
            function set(uint256 x) external;
        }
    }

    // Encode a no-argument getter call — flux_aggregator.rs:71-73
    let get_call = ISimpleStorage::getCall {};
    let encoded = get_call.abi_encode();
    // ABI-encoded call = 4-byte selector only when no parameters
    assert_eq!(
        encoded.len(),
        4,
        "zero-parameter call should produce exactly a 4-byte selector"
    );

    // Encode a call with a parameter — mirrors submitCall encoding
    let set_call = ISimpleStorage::setCall {
        x: U256::from(42_u64),
    };
    let set_encoded = set_call.abi_encode();
    assert_eq!(
        set_encoded.len(),
        36, // 4-byte selector + 32-byte uint256
        "single uint256 param call = 4 + 32 bytes"
    );

    // Verify the selector bytes are exactly 4 bytes (not 0, not 32)
    let selector_bytes = &set_encoded[..4];
    assert_eq!(selector_bytes.len(), 4);

    // Decode a synthetic return value and access ._0 — flux_aggregator.rs:92
    // Manually ABI-encode a uint256(12345) as the "return value" the chain would give back.
    // In alloy 0.8, SolType::abi_encode is an associated function taking only the value:
    //   <SolDataType as SolType>::abi_encode(&value) -> Vec<u8>
    let return_value = U256::from(12345_u64);
    let encoded_return: Vec<u8> =
        <alloy::sol_types::sol_data::Uint<256> as alloy::sol_types::SolType>::abi_encode(
            &return_value,
        );

    // In alloy 1.x, abi_decode_returns no longer takes a validate bool parameter.
    // Single return values are returned as the primitive type directly (not a wrapper struct).
    let decoded = ISimpleStorage::getCall::abi_decode_returns(&encoded_return)
        .expect("abi_decode_returns failed");

    // In alloy 1.x single-return functions return the primitive directly (not decoded._0)
    assert_eq!(decoded, return_value, "decoded value should match original");
}

// ── Test 6: abi_decode_returns with validate:bool ────────────────────────────

/// Tests: `SomeFnCall::abi_decode_returns(&result, true)` and `decoded._0`.
///
/// Source: `contracts/flux_aggregator.rs:92`.
/// The `validate` parameter (second arg) enables strict ABI validation.  In alloy 1.x
/// this parameter may be removed or renamed.  Failure here means we need to drop the arg.
#[tokio::test]
#[ignore]
async fn test_abi_decode_returns_with_validate_flag() {
    sol! {
        interface ILatestAnswer {
            function latestAnswer() external view returns (int256);
        }
    }

    // Construct a synthetic ABI-encoded int256 return (mimics on-chain response)
    let price = I256::try_from(185_000_000_000_i64).expect("i256 from i64");
    let encoded_return: Vec<u8> =
        <alloy::sol_types::sol_data::Int<256> as alloy::sol_types::SolType>::abi_encode(&price);

    // In alloy 1.x, the validate bool parameter was removed from abi_decode_returns.
    // Single return values are returned as the primitive type directly.
    let decoded = ILatestAnswer::latestAnswerCall::abi_decode_returns(&encoded_return)
        .expect("abi_decode_returns failed");
    assert_eq!(decoded, price);
}

// ── Test 7: Contract read via provider.call() ────────────────────────────────

/// Tests: `provider.call(&tx).block(BlockId::latest()).await`
///
/// Source: `contracts/flux_aggregator.rs:76`.
/// Deploys a SimpleStorage contract using raw bytecode, sets a value, then reads it back.
#[tokio::test]
#[ignore]
async fn test_provider_call_with_block_id() {
    sol! {
        interface ISimpleStorageCall {
            function get() external view returns (uint256);
            function set(uint256 x) external;
        }
    }

    let (_anvil, url) = spawn_anvil_url();
    let signer: PrivateKeySigner = ANVIL_KEY.parse().expect("parse signer");
    let sender = signer.address();
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    // Deploy SimpleStorage.
    // Bytecode: compiled from:
    //   pragma solidity ^0.8.0;
    //   contract SimpleStorage {
    //       uint256 private value;
    //       function get() public view returns (uint256) { return value; }
    //       function set(uint256 x) public { value = x; }
    //   }
    // solc 0.8.30 --optimize --bin SimpleStorage.sol
    let bytecode = Bytes::from(hex::decode(
        "6080604052348015600e575f5ffd5b5060a580601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106030575f3560e01c806360fe47b11460345780636d4ce63c146045575b5f5ffd5b6043603f3660046059565b5f55565b005b5f5460405190815260200160405180910390f35b5f602082840312156068575f5ffd5b503591905056fea2646970667358221220c8d85d033b1d1bc317b58333b72f83b29f7328456688e15b17034cc89c7fc2ae64736f6c634300081e0033",
    ).expect("valid hex bytecode"));

    {
        // Contract creation requires .with_create() so the WalletFiller
        // does not reject the transaction for missing a `to` field.
        let deploy_tx = TransactionRequest::default()
            .from(sender)
            .into_create()
            .input(bytecode.into());

        let deploy_receipt = provider
            .send_transaction(deploy_tx)
            .await
            .expect("deploy send_transaction")
            .with_required_confirmations(1)
            .get_receipt()
            .await
            .expect("deploy get_receipt");

        let contract_addr = deploy_receipt
            .contract_address
            .expect("deploy should produce a contract address");

        // Call set(42)
        let set_call = ISimpleStorageCall::setCall {
            x: U256::from(42_u64),
        };
        let set_tx = TransactionRequest::default()
            .to(contract_addr)
            .from(sender)
            .input(set_call.abi_encode().into());

        provider
            .send_transaction(set_tx)
            .await
            .expect("set send_transaction")
            .with_required_confirmations(1)
            .get_receipt()
            .await
            .expect("set get_receipt");

        // Call get() via provider.call().block(BlockId::latest()) — flux_aggregator.rs:76
        let get_call = ISimpleStorageCall::getCall {};
        let call_tx = TransactionRequest::default()
            .to(contract_addr)
            .input(get_call.abi_encode().into());

        // In alloy 1.x, provider.call() takes ownership (not a reference)
        let result = provider
            .call(call_tx.clone())
            .block(BlockId::latest())
            .await
            .expect("provider.call failed");

        // In alloy 1.x, abi_decode_returns has no validate bool, single returns are primitives
        let decoded =
            ISimpleStorageCall::getCall::abi_decode_returns(&result).expect("abi_decode_returns");
        assert_eq!(decoded, U256::from(42_u64));
    }
}

// ── Test 8: Gas estimation ────────────────────────────────────────────────────

/// Tests: `provider.estimate_gas(&tx).await`, `provider.get_gas_price().await`.
///
/// Source: `gas/estimator.rs:108`, `gas/estimator.rs:142`.
#[tokio::test]
#[ignore]
async fn test_gas_estimation() {
    let (_anvil, provider) = spawn_anvil_http();
    let sender: Address = ANVIL_ADDR.parse().expect("parse sender");
    let receiver: Address = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
        .parse()
        .expect("parse receiver");

    // estimate_gas — gas/estimator.rs:108
    let tx = TransactionRequest::default()
        .from(sender)
        .to(receiver)
        .value(U256::from(1_u64));

    // In alloy 1.x, estimate_gas takes ownership (not a reference)
    let estimated = provider
        .estimate_gas(tx.clone())
        .await
        .expect("estimate_gas failed");

    // A simple ETH transfer should always cost exactly 21 000 gas
    assert_eq!(estimated, 21_000_u64, "ETH transfer gas = 21000");

    // get_gas_price — gas/estimator.rs:142
    let gas_price = provider
        .get_gas_price()
        .await
        .expect("get_gas_price failed");
    assert!(gas_price > 0, "gas price should be positive");
}

// ── Test 9: Dynamic ABI (DynSolValue, DynSolType, JsonAbiExt) ────────────────

/// Tests: `DynSolType`, `DynSolValue`, `JsonAbiExt` trait (`abi_encode_input`,
/// `abi_decode_output`), `Function` construction from `Param` and `StateMutability`,
/// `FunctionExt` (`abi_encode_input_raw`).
///
/// Source: `contracts/abi_utils.rs:7-9`, `event_monitors/abi_decoder.rs:4-5`.
#[test]
#[ignore]
fn test_dynamic_abi_encode_decode() {
    // Construct a Function definition — contracts/abi_utils.rs:45-83
    let func = Function {
        name: "set".to_string(),
        inputs: vec![Param {
            ty: "uint256".to_string(),
            name: "x".to_string(),
            components: vec![],
            internal_type: None,
        }],
        outputs: vec![],
        state_mutability: StateMutability::NonPayable,
    };

    // Encode inputs via JsonAbiExt — contracts/abi_utils.rs:155
    let input_value = DynSolValue::Uint(U256::from(9999_u64), 256);
    let encoded: Bytes = func
        .abi_encode_input(&[input_value])
        .expect("abi_encode_input failed")
        .into();

    // Should be 4 (selector) + 32 (uint256) = 36 bytes
    assert_eq!(encoded.len(), 36, "encoded function call length");

    // Construct a view function with a uint256 return for decode testing
    let view_func = Function {
        name: "get".to_string(),
        inputs: vec![],
        outputs: vec![Param {
            ty: "uint256".to_string(),
            name: "result".to_string(),
            components: vec![],
            internal_type: None,
        }],
        state_mutability: StateMutability::View,
    };

    // Manually ABI-encode a uint256 return (as the chain would return)
    let return_value = U256::from(42_u64);
    let encoded_return = DynSolValue::Uint(return_value, 256).abi_encode();

    // Decode via JsonAbiExt — contracts/abi_utils.rs:162
    // In alloy 1.x, abi_decode_output no longer takes a validate bool parameter
    let decoded: Vec<DynSolValue> = view_func
        .abi_decode_output(&encoded_return)
        .expect("abi_decode_output failed");

    assert_eq!(decoded.len(), 1, "one return value");
    match &decoded[0] {
        DynSolValue::Uint(val, 256) => assert_eq!(*val, return_value),
        other => panic!("unexpected decoded variant: {other:?}"),
    }

    // DynSolType::parse and coerce — event_monitors/abi_decoder.rs:80
    let sol_type: DynSolType = "uint256".parse().expect("parse DynSolType");
    let coerced = sol_type.coerce_str("12345").expect("coerce_str failed");
    match coerced {
        DynSolValue::Uint(v, 256) => assert_eq!(v, U256::from(12345_u64)),
        other => panic!("unexpected coerced variant: {other:?}"),
    }

    // DynSolValue variants used in abi_decoder.rs
    let addr: Address = "0x0000000000000000000000000000000000000001"
        .parse()
        .expect("parse address");
    let _addr_val = DynSolValue::Address(addr);
    let _bool_val = DynSolValue::Bool(true);
    let _bytes_val = DynSolValue::Bytes(vec![0xde, 0xad]);
    let _str_val = DynSolValue::String("hello".to_string());
}

// ── Test 10: Primitives & type conversions ───────────────────────────────────

/// Tests: `U256`, `I256`, `Address`, `Bytes`, `TxHash`, `U256::to::<u64>()`,
/// `U256::to::<u128>()`, `format_units`, `parse_units`.
///
/// Source: `contracts/flux_aggregator.rs`, `gas/estimator.rs:4`, throughout codebase.
#[test]
#[ignore]
fn test_primitives_and_type_conversions() {
    // U256 construction and .to::<T>() — gas/estimator.rs, transaction/queue.rs
    let big: U256 = U256::from(250_000_u64);
    let as_u64: u64 = big.to();
    assert_eq!(as_u64, 250_000_u64);

    let as_u128: u128 = big.to();
    assert_eq!(as_u128, 250_000_u128);

    // I256 — contracts/flux_aggregator.rs:674
    let positive = I256::MAX;
    assert!(positive > I256::ZERO);

    let negative = I256::MIN;
    assert!(negative < I256::ZERO);

    let zero_i256 = I256::ZERO;
    assert_eq!(zero_i256, I256::try_from(0_i64).unwrap());

    // I256 from i64 — transaction/queue.rs:36
    let price = I256::try_from(185_000_000_000_i64).expect("i256 from i64");
    assert!(price > I256::ZERO);

    // Address parsing — throughout codebase
    let addr_str = "0x1234567890123456789012345678901234567890";
    let addr: Address = addr_str.parse().expect("parse address");
    assert_eq!(format!("{addr:?}").to_lowercase(), addr_str.to_lowercase());

    // format_units — gas/estimator.rs:150, flux_aggregator.rs:589
    let thirty_gwei = U256::from(30_000_000_000_u64);
    let formatted = format_units(thirty_gwei, "gwei").expect("format_units failed");
    assert_eq!(formatted, "30.000000000", "30 gwei formatted");

    // format_units with "ether"
    let one_eth = U256::from(1_000_000_000_000_000_000_u64);
    let eth_str = format_units(one_eth, "ether").expect("format_units ether");
    assert_eq!(eth_str, "1.000000000000000000");

    // parse_units — gas/estimator.rs:136, 162
    // NOTE: In alloy 0.8 parse_units returns ParseUnits (newtype). `.into()` converts to U256.
    // In alloy 1.x this may return U256 directly. This test validates both the call and
    // the `.into()` conversion that is used throughout the codebase.
    let twenty_gwei_parsed: U256 = parse_units("20", "gwei")
        .expect("parse_units failed")
        .into();
    assert_eq!(twenty_gwei_parsed, U256::from(20_000_000_000_u64));

    let two_gwei_parsed: U256 = parse_units("2", "gwei").expect("parse_units failed").into();
    assert_eq!(two_gwei_parsed, U256::from(2_000_000_000_u64));

    // Bytes construction — throughout
    let raw: Bytes = vec![0xde, 0xad, 0xbe, 0xef].into();
    assert_eq!(raw.len(), 4);

    // TxHash is a fixed-bytes alias; verify it is re-exported from alloy::primitives
    let _: alloy::primitives::TxHash = alloy::primitives::TxHash::ZERO;
}

// ── Test 11: Provider generic bounds (alloy 1.x) ─────────────────────────────

/// Tests: `P: Provider` generic bounds (Transport generic removed in alloy 1.x).
///
/// Source: `contracts/flux_aggregator.rs`, `contracts/caller.rs`.
/// In alloy 1.x the `Provider` trait drops all transport and network type parameters.
/// The bound is simply `P: Provider`.
#[tokio::test]
#[ignore]
async fn test_provider_generic_bounds() {
    /// Mirrors the struct-level bounds used in FluxAggregatorContract and ContractCaller.
    ///
    /// In alloy 1.x: `Provider` (Transport and Network params removed)
    async fn call_chain_id<P>(provider: &P) -> u64
    where
        P: Provider,
    {
        provider
            .get_chain_id()
            .await
            .expect("get_chain_id in generic fn")
    }

    let (_anvil, provider) = spawn_anvil_http();
    let chain_id = call_chain_id(&provider).await;
    assert_eq!(chain_id, 31337);
}

/// Secondary variant: `Provider<N>` with explicit network.
/// Tests that the network-parameterized form also compiles and works.
#[tokio::test]
#[ignore]
async fn test_provider_with_network_param_bounds() {
    async fn estimate<P>(provider: &P) -> u64
    where
        P: Provider,
    {
        let tx = TransactionRequest::default();
        // In alloy 1.x, estimate_gas takes ownership
        provider
            .estimate_gas(tx)
            .await
            .expect("estimate_gas in generic fn")
    }

    let (_anvil, provider) = spawn_anvil_http();
    let gas = estimate(&provider).await;
    assert!(gas > 0, "should estimate some gas");
}

// ── Test 12: Pending transaction .tx_hash() dereference ──────────────────────

/// Tests: `*pending_tx.tx_hash()` — the dereference to copy out a `TxHash`.
///
/// Source: `contracts/flux_aggregator.rs:460`.
/// In alloy 1.x `tx_hash()` may return by value rather than reference.
/// If it does, the `*` dereref becomes a compile error. This test catches that.
#[tokio::test]
#[ignore]
async fn test_pending_tx_hash_deref() {
    let (_anvil, url) = spawn_anvil_url();
    let signer: PrivateKeySigner = ANVIL_KEY.parse().expect("parse signer");
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    let sender: Address = ANVIL_ADDR.parse().expect("parse sender");
    let receiver: Address = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
        .parse()
        .expect("parse receiver");

    let tx = TransactionRequest::default()
        .from(sender)
        .to(receiver)
        .value(U256::from(1_u64));

    let pending = provider
        .send_transaction(tx)
        .await
        .expect("send_transaction");

    // Exact pattern from flux_aggregator.rs: let tx_hash = *pending_tx.tx_hash();
    let tx_hash: alloy::primitives::TxHash = *pending.tx_hash();
    assert_ne!(tx_hash, alloy::primitives::TxHash::ZERO);
}

// ── Test 13: receipt.gas_used and receipt.block_number field access ──────────

/// Tests: `receipt.gas_used` (a `u64` in alloy 1.x), `receipt.block_number` (an `Option<u64>`),
/// `receipt.status()` method.
///
/// Source: `contracts/flux_aggregator.rs`, `metrics/gas_metrics.rs`.
/// In alloy 1.x `gas_used` is `u64` (changed from `u128` in 0.8).
#[tokio::test]
#[ignore]
async fn test_receipt_fields() {
    let (_anvil, url) = spawn_anvil_url();
    let signer: PrivateKeySigner = ANVIL_KEY.parse().expect("parse signer");
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    let sender: Address = ANVIL_ADDR.parse().expect("parse sender");
    let receiver: Address = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
        .parse()
        .expect("parse receiver");

    let tx = TransactionRequest::default()
        .from(sender)
        .to(receiver)
        .value(U256::from(1_u64));

    let receipt = provider
        .send_transaction(tx)
        .await
        .expect("send_transaction")
        .with_required_confirmations(1)
        .get_receipt()
        .await
        .expect("get_receipt");

    // receipt.status() — flux_aggregator.rs:607
    assert!(receipt.status());

    // receipt.gas_used — in alloy 1.x this is u64 (was u128 in 0.8)
    let gas_used: u64 = receipt.gas_used;
    assert_eq!(gas_used, 21_000, "ETH transfer gas used");

    // Cast to u128 for calculations that require u128 (gas cost = gas_used * gas_price)
    let gas_used_u128 = gas_used as u128;
    assert_eq!(gas_used_u128, 21_000_u128);

    // receipt.block_number — flux_aggregator.rs:610
    let block_number = receipt.block_number.expect("block_number should be Some");
    assert!(block_number >= 1, "should be mined in block 1 or later");
}

// ── Test 14: Function signature parsing (abi_decoder.rs pattern) ─────────────

/// Tests: `"transfer(address,uint256)".parse::<Function>()` — parsing a human-readable
/// function signature string directly into a `Function` via `FromStr`.
///
/// Source: `event_monitors/abi_decoder.rs:37`.
#[test]
#[ignore]
fn test_function_signature_from_str() {
    // abi_decoder.rs:37 uses `.parse::<Function>()` on a &str
    let func: Function = "transfer(address,uint256)"
        .parse()
        .expect("parse function signature");

    assert_eq!(func.name, "transfer");
    assert_eq!(func.inputs.len(), 2);
    assert_eq!(func.inputs[0].ty, "address");
    assert_eq!(func.inputs[1].ty, "uint256");
    assert_eq!(func.state_mutability, StateMutability::NonPayable);

    // View function
    let view_func: Function = "balanceOf(address)(uint256)"
        .parse()
        .expect("parse view function");
    assert_eq!(view_func.name, "balanceOf");

    // No-argument function
    let no_arg: Function = "owner()(address)".parse().expect("parse no-arg function");
    assert_eq!(no_arg.name, "owner");
    assert!(no_arg.inputs.is_empty());
}

// ── Test 15: sol! macro with #[sol(rpc)] attribute ───────────────────────────

/// Tests the `#[sol(rpc)]` attribute on an interface definition.
///
/// Source: `contracts/flux_aggregator.rs:23` — `#[sol(rpc)]` is used on IFluxAggregator.
/// In alloy 1.x this attribute is still present but may move to a different path or
/// change semantics. This test verifies that a generated rpc-enabled interface compiles
/// and that call encoding still works.
#[test]
#[ignore]
fn test_sol_rpc_attribute() {
    sol! {
        #[sol(rpc)]
        interface IOracle {
            function latestAnswer() external view returns (int256);
            function decimals() external view returns (uint8);
        }
    }

    // Call encoding should work identically with or without #[sol(rpc)]
    let call = IOracle::latestAnswerCall {};
    let encoded = call.abi_encode();
    assert_eq!(encoded.len(), 4, "no-param call = 4 bytes");

    // Selector should be deterministic
    let selector = &encoded[..4];
    let expected_selector = alloy::primitives::keccak256(b"latestAnswer()")[..4].to_vec();
    assert_eq!(selector, expected_selector.as_slice(), "selector mismatch");
}

// ── Test 16: TransactionRequest.input() with ABI-encoded bytes ───────────────

/// Tests: `TransactionRequest::default().input(call.abi_encode().into())`
///
/// Source: `contracts/flux_aggregator.rs:73` — the `.input()` builder method takes
/// `TransactionInput` which is constructed from `Bytes`.  In alloy 1.x this was briefly
/// renamed; verify the `.input(value.into())` chain still compiles.
#[test]
#[ignore]
fn test_transaction_request_input_field() {
    sol! {
        interface ITestInput {
            function doThing(uint256 val) external;
        }
    }

    let call = ITestInput::doThingCall {
        val: U256::from(777_u64),
    };
    let encoded: Bytes = call.abi_encode().into();

    // Both builder patterns used in codebase:
    // 1) .input(encoded.into()) — direct Bytes → TransactionInput
    let tx_via_input = TransactionRequest::default()
        .to(Address::ZERO)
        .input(encoded.clone().into());

    // 2) Verify the input round-trips.
    // In alloy 0.8, TransactionRequest.input is a TransactionInput struct with
    // two fields: `.input: Option<Bytes>` and `.data: Option<Bytes>`.
    // The `.input()` builder method populates `.input.input`.
    let input_data = tx_via_input
        .input
        .input
        .expect("TransactionInput.input field should be set by .input() builder");
    assert_eq!(input_data, encoded, "input bytes should round-trip");
}
