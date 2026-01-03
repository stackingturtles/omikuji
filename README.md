# Omikuji - a lightweight EVM blockchain datafeed provider

[![codecov](https://codecov.io/gh/ijonas/omikuji/branch/main/graph/badge.svg)](https://codecov.io/gh/ijonas/omikuji)

Omikuji is a software daemon, written in Rust, that provides external off-chain data to EVM blockchains such as Ethereum and BASE.

Some may call it a [blockchain oracle](https://en.wikipedia.org/wiki/Blockchain_oracle)

The core model of Omikuji is the datafeed, which is a Solidity smart contract, that reports a single value and an accompanying timestamp and block number for when that value was last updated.

Omikuji will monitor external datafeeds such as price feeds (the price of gold, temperature in London, etc.) and when it notices significant change in the reported data it will write that data to a blockchain.


## ✨ Key Features

  Core Functionality

  - Multi-Network Support: Connect to multiple EVM-compatible blockchains simultaneously
  - Datafeed Management: Define and manage multiple datafeeds through YAML configuration
  - Smart Contract Integration: Full support for Chainlink FluxAggregator contracts
  - Flexible Data Sources: Fetch data from any HTTP/HTTPS JSON API endpoint

  Update Mechanisms

  - Time-Based Updates: Automatically submit new values when minimum update frequency has elapsed
  - Deviation-Based Updates: Submit updates when price changes exceed configured percentage thresholds
  - Dual-Trigger System: Updates occur when either time OR deviation conditions are met
  - Oracle Coordination: Timestamp safety checks prevent nodes from overwriting newer data with stale values

  Configuration

  - Contract Configuration Reading: Automatically read decimals, min/max values from deployed contracts
  - Environment Variable Support: Secure wallet management through environment variables
  - Flexible JSON Path Extraction: Support for complex nested JSON structures using dot notation
  - EIP-1559 Transaction Support: Modern gas pricing with automatic fee estimation
  - Fee Bumping: Automatic retry with increased fees for stuck transactions
  - Gas Configuration: Per-network gas settings with manual override options

  Monitoring & Reliability

  - Concurrent Feed Monitoring: Each datafeed runs independently in its own async task
  - Comprehensive Logging: Detailed logs for debugging and monitoring
  - Error Recovery: Graceful handling of network errors and API failures
  - Historical Data Storage: Optional PostgreSQL database for feed value history
  - Automatic Cleanup: Configurable data retention with scheduled cleanup tasks
  - Prometheus Metrics: Export comprehensive metrics for wallet balances, feed values, contract state, and deviations
  - Gas Consumption Tracking: Prometheus metrics and database logging for all transactions
  - Cost Analysis: Monitor gas efficiency and optimize transaction costs

## 📋 Requirements

  - Rust 1.70 or higher
  - Access to EVM-compatible blockchain RPC endpoints
  - Private key for transaction signing (via environment variable)
  - PostgreSQL 12+ (optional, for historical data storage)


## 📥 Installation

### Binary Installation

Download pre-built binaries for your platform from the [latest release](https://github.com/ijonas/omikuji/releases/latest):

```bash
# Linux x64 (requires glibc 2.31+, standard on Ubuntu 20.04+, Debian 11+)
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-linux-x64
chmod +x omikuji-linux-x64
sudo mv omikuji-linux-x64 /usr/local/bin/omikuji

# macOS (Intel)
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-macos-x64
chmod +x omikuji-macos-x64
sudo mv omikuji-macos-x64 /usr/local/bin/omikuji

# macOS (Apple Silicon)
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-macos-arm64
chmod +x omikuji-macos-arm64
sudo mv omikuji-macos-arm64 /usr/local/bin/omikuji
```

Verify the checksum:
```bash
wget https://github.com/ijonas/omikuji/releases/latest/download/checksums.txt
sha256sum -c checksums.txt --ignore-missing
```

### Docker Installation

Pull the latest Docker image (no authentication required):

```bash
docker pull public.ecr.aws/stackingturtles/omikuji/omikuji-core:latest
```

Run with your config file:

```bash
docker run -v $(pwd)/config.yaml:/config/config.yaml \
           -e OMIKUJI_PRIVATE_KEY=$OMIKUJI_PRIVATE_KEY \
           public.ecr.aws/stackingturtles/omikuji/omikuji-core:latest
```

### Build from Source

```bash
git clone https://github.com/ijonas/omikuji.git
cd omikuji

# Setup git hooks (optional but recommended for contributors)
./.githooks/setup.sh

cargo build --release
sudo mv target/release/omikuji /usr/local/bin/
```

### Development Setup

For contributors, we use git hooks to ensure code quality:

```bash
# Enable pre-commit hooks (runs cargo fmt and clippy)
./.githooks/setup.sh

# The pre-commit hook will:
# - Check code formatting with `cargo fmt`
# - Run linting with `cargo clippy`
# - Prevent commits if issues are found

# To run checks manually:
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## 🚀 Getting Started

  1. Create a configuration file (default: config.yaml):

    networks:
      - name: local
        url: http://localhost:8545

    datafeeds:
      - name: eth_usd
        networks: local
        check_frequency: 60
        contract_address: 0x...
        contract_type: fluxmon
        minimum_update_frequency: 3600
        deviation_threshold_pct: 0.5
        feed_url: https://api.example.com/price
        feed_json_path: data.price

  2. Set up your private key (choose one method):

    **Option A: OS Keyring (Recommended)**
    ```bash
    # Import a private key for a network
    omikuji key import --network ethereum-mainnet
    # You'll be prompted to enter the private key securely
    
    # Or import from a file
    omikuji key import --network ethereum-mainnet --file /path/to/key.txt
    ```

    **Option B: Environment Variable (Legacy)**
    ```bash
    export OMIKUJI_PRIVATE_KEY=your_private_key_here
    ```

  3. Run Omikuji:
    
    omikuji --config config.yaml

  🔧 Command Line Options

  **General Options:**
  - -c, --config <FILE>: Path to configuration file
  - -p, --private-key-env <ENV_VAR>: Private key environment variable name (default: OMIKUJI_PRIVATE_KEY)
  - -V, --version: Display version information
  - -h, --help: Display help information

  **Key Management Commands:**
  - `omikuji key import`: Import a private key to the configured storage backend
  - `omikuji key export`: Export a private key (with confirmation)
  - `omikuji key remove`: Remove a private key from storage
  - `omikuji key list`: List available keys
  - `omikuji key migrate`: Migrate keys from environment variables to configured storage
  
  Note: Key commands now respect your configured storage backend (keyring, vault, aws-secrets, or env)

## 📊 Technical Specifications

  - Language: Rust
  - Async Runtime: Tokio
  - Blockchain Library: alloy-rs
  - Configuration Format: YAML
  - Supported Contract Types: Chainlink FluxAggregator
  - Update Precision: Configurable decimals (0-18)
  - Value Bounds: Support for min/max submission values

## 🧪 Testing

  The release includes comprehensive test coverage with 52 unit tests covering:
  - Configuration parsing and validation
  - Contract interaction and ABI encoding
  - JSON data extraction
  - Deviation calculations
  - Network provider management
  - Value scaling and bounds checking

## 🙏 Acknowledgments

  This initial release represents the foundation of the Omikuji project. We look forward to community feedback and contributions to make
  blockchain data feeds more accessible and reliable.

## 📝 License

  Omikuji is licensed under the MIT License. See [LICENSE](LICENSE) for details.
  
  Copyright (c) 2025 Stacking Turtles Ltd.

## 📚 Documentation

  For comprehensive documentation, see the [Documentation Index](docs/README.md).

  ### Quick Links

  - [Installation Guide](docs/getting-started/installation.md) - Binary, Docker, and source installation
  - [Quick Start Tutorial](docs/getting-started/quickstart.md) - Get running in 5 minutes
  - [Configuration Guide](docs/getting-started/configuration.md) - Basic configuration
  - [Configuration Reference](docs/reference/configuration.md) - Complete configuration specification
  - [Gas Configuration](docs/guides/gas-configuration.md) - Transaction types and fee strategies
  - [Database Setup](docs/guides/database-setup.md) - PostgreSQL setup and monitoring
  - [Prometheus Metrics](docs/guides/prometheus-metrics.md) - Monitoring and alerting

  For more information and contribution guidelines, visit: https://github.com/ijonas/omikuji
