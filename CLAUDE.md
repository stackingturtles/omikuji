# CLAUDE.md - Omikuji Community Edition

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Edition Scope

This is the **Omikuji Community Edition** - the open-source, developer-focused version of Omikuji. This edition is fully MIT/Apache-2.0 licensed and contains all core functionality needed for development and testing.

## Community Features

The Community Edition includes:
- Environment variable and file-based secret management
- Single-node operation
- Standard Prometheus metrics
- All core datafeed functionality
- Support for multiple networks and datafeeds
- Chainlink FluxAggregator contract support
- PostgreSQL integration (optional)
- WebSocket support

## Project Overview

Omikuji is a lightweight EVM blockchain datafeed provider, written in Rust. It acts as a software daemon that provides external off-chain data to EVM blockchains such as Ethereum and BASE.

The core concept is the "datafeed" - a Solidity smart contract that reports a single value along with a timestamp and block number indicating when that value was last updated. This allows other client smart contracts to determine whether the datafeed values have become stale.

## Architecture

### Key Components

1. **Datafeed Management**: Omikuji manages datafeeds defined in YAML configuration files, each with sources, update frequency, and deviation thresholds.

2. **Network Support**: Supports multiple EVM blockchain networks (Ethereum, BASE, etc.) configured with RPC endpoints.

3. **Smart Contract Integration**: Specifically supports Chainlink Flux Monitor contracts for updating datafeed values, utilizing the FluxAggregator interface.

4. **Web Interface**: Provides a dashboard to monitor datafeed status at http://localhost:8080.

### Configuration

The system uses a YAML configuration file that defines:
- Networks with their RPC URLs
- Datafeeds with parameters such as:
  - Check frequency
  - Contract addresses and types
  - Minimum update frequency
  - Deviation thresholds for updates
  - External data source URLs and JSON paths

## Development Commands

### Building and Running

```bash
# Build the project
cargo build

# Run in development mode
cargo run

# Run with specific configuration file
cargo run -- -c /path/to/config.yaml

# Run with release optimizations
cargo build --release
cargo run --release
```

### Key Management

```bash
# Import a private key for a network
cargo run -- key import --network ethereum-mainnet

# Export a key (with confirmation)
cargo run -- key export --network ethereum-mainnet

# Remove a key
cargo run -- key remove --network ethereum-mainnet

# List stored keys
cargo run -- key list

# Migrate keys from environment variables to keyring
cargo run -- key migrate
```

### Testing

```bash
# Run all tests
cargo test

# Run specific tests
cargo test <test_name>

# Run tests with output
cargo test -- --nocapture

# Generate code coverage report
make coverage

# Generate LCOV coverage report (for CI)
make coverage-lcov

# Install coverage tools
make install-coverage-tools
```

### Code Quality

```bash
# IMPORTANT: Keep Rust updated to match GitHub Actions
rustup update stable
rustup component add clippy rustfmt

# Check code formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy linter
cargo clippy

# Check for common mistakes and improvements
cargo clippy -- -D warnings

# Run linting with GitHub Actions CI settings
./scripts/lint.sh

# Or use the Makefile commands
make lint         # Run all linting checks (matches CI)
make lint-fix     # Automatically fix issues where possible
make check        # Run linting + tests
make ci-check     # Run exact CI pipeline locally

# Run clippy with CI settings directly
cargo clippy -- -D warnings -D clippy::uninlined_format_args

# Fix clippy issues automatically where possible
cargo clippy --fix --allow-dirty -- -D warnings -D clippy::uninlined_format_args

# Use cargo aliases (defined in .cargo/config.toml)
cargo ci-check    # Run clippy with CI settings
cargo ci-fix      # Fix issues with CI settings
```

### Documentation

```bash
# Generate documentation
cargo doc --open
```

## Project Documentation

For comprehensive project documentation, see:
- [Documentation Index](docs/README.md) - Complete documentation overview
- [Architecture Reference](docs/reference/architecture.md) - System design details
- [Configuration Reference](docs/reference/configuration.md) - All configuration options
- [Contributing Guide](docs/development/contributing.md) - Development guidelines

## Plugin System

### Community Edition Plugins

The Community Edition includes the following built-in plugins:

**Secret Providers:**
- `env` - Environment variable secret storage
- `keyring` - OS keyring secret storage (macOS Keychain, Windows Credential Manager, Linux Secret Service)

**Cluster Providers:**
- `single-node` - No-op cluster provider for single-instance operation (always leader)

### Plugin Registration

Plugins are automatically registered during daemon startup in `src/main.rs`:

```rust
use omikuji::plugins;

// Register all community plugins
plugins::register_community_plugins()?;

// Build and run daemon
let daemon = builder.build().await?;
daemon.run().await
```

### Plugin Development Guidelines

When developing traits for the plugin system:
1. Define traits in a separate `omikuji-core` crate for shared interfaces
2. All traits must be `Send + Sync` for async compatibility
3. Use `async` methods where I/O operations are involved
4. Provide clear documentation for trait implementors
5. Consider backward compatibility when modifying existing traits

### Example Trait Definition
```rust
pub trait SecretProvider: Send + Sync {
    async fn get_private_key(&self, network: &str) -> Result<String>;
    fn provider_name(&self) -> &str;
}
```

### Creating a New Plugin

To create a new secret provider plugin:

1. Create a new file in `src/plugins/` (e.g., `my_provider.rs`)
2. Implement the `SecretProvider` trait from `omikuji_core::traits::secrets`
3. Create a factory that implements `SecretProviderFactory`
4. Register the factory in `src/plugins/mod.rs`

Example:
```rust
// src/plugins/my_provider.rs
use omikuji_core::traits::secrets::{SecretProvider, SecretProviderFactory, ...};

pub struct MyProvider;

#[async_trait]
impl SecretProvider for MyProvider {
    async fn get_private_key(&self, network: &str) -> Result<String> {
        // Implementation
    }
    // ... other methods
}

pub struct MyProviderFactory;

#[async_trait]
impl SecretProviderFactory for MyProviderFactory {
    async fn create(&self, config: SecretProviderConfig) -> Result<Box<dyn SecretProvider>> {
        Ok(Box::new(MyProvider::new()))
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::Custom
    }
}
```

## Development Rules

- **MUST** remain 100% open source (MIT/Apache-2.0 licensed)
- **CANNOT** include pro edition features (AWS Secrets Manager, Nitro Enclaves, clustering)
- **CANNOT** import or depend on `omikuji-pro` code
- All PRs must pass community CI pipeline
- Focus on developer experience and ease of use
- Maintain trait compatibility for pro edition plugins

## Claude Interactions

### Rust Format String Convention

The `uninlined_format_args` lint requires using the newer format string syntax where variables are directly embedded in the string rather than passed as separate arguments.

For example:
- Old: `format!("Error: {}", e)`
- New: `format!("Error: {e}")`

This applies to all formatting macros including `format!`, `println!`, `eprintln!`, `write!`, etc. The CI pipeline enforces this with `cargo clippy -- -D warnings -D clippy::uninlined_format_args`.