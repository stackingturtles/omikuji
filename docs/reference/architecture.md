# Architecture Reference

This document describes the internal architecture of Omikuji, its components, and how they interact.

## System Overview

Omikuji is a multi-threaded, asynchronous daemon that bridges external data sources with blockchain smart contracts. It follows a modular architecture designed for reliability, performance, and extensibility.

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  External APIs  │────▶│   Omikuji    │────▶│ Smart Contracts │
└─────────────────┘     └──────────────┘     └─────────────────┘
                              │
                              ▼
                        ┌──────────────┐
                        │  PostgreSQL  │
                        │  (Optional)  │
                        └──────────────┘
```

## Core Components

### 1. Main Entry Point (`src/main.rs`)

The application entry point that:
- Parses command-line arguments
- Loads configuration
- Initializes all subsystems
- Manages the application lifecycle

### 2. Configuration Module (`src/config/`)

Handles configuration parsing and validation:
- **`parser.rs`**: YAML file parsing with serde
- **`models.rs`**: Configuration data structures
- **`validator`**: Ensures configuration validity

### 3. Network Module (`src/network/`)

Manages blockchain connections:
- **`provider.rs`**: RPC provider management
- **`ws_provider.rs`**: WebSocket provider with automatic reconnection
- Connection pooling per network
- Network-specific configurations
- Wallet management and signing
- Support for multiple nodes per network

### 4. Datafeed Module (`src/datafeed/`)

Core business logic for data feeds:

#### Components:
- **`manager.rs`**: Orchestrates all datafeeds
- **`monitor.rs`**: Individual feed monitoring tasks
- **`fetcher.rs`**: HTTP client for external APIs
- **`json_extractor.rs`**: JSON parsing and value extraction
- **`contract_updater.rs`**: Blockchain transaction submission
- **`contract_config.rs`**: Reads configuration from contracts

#### Flow:
1. Monitor polls external API
2. Extracts value using JSON path
3. Checks update conditions (time/deviation)
4. Submits transaction if needed

### 5. Contracts Module (`src/contracts/`)

Smart contract interfaces:
- **`flux_aggregator.rs`**: Chainlink FluxAggregator ABI
- Contract interaction abstractions
- Type-safe contract calls

### 6. Gas Module (`src/gas/`)

Transaction gas management:
- **`estimator.rs`**: Gas price estimation
- EIP-1559 and legacy transaction support
- Fee bumping for stuck transactions
- Network-specific gas strategies

### 7. Database Module (`src/database/`)

Optional persistent storage:
- **`connection.rs`**: PostgreSQL connection pooling
- **`models.rs`**: Database schemas
- **`repository.rs`**: Feed log operations
- **`transaction_repository.rs`**: Transaction history
- **`cleanup.rs`**: Data retention management

### 8. Metrics Module (`src/metrics/`)

Observability and monitoring:
- **`server.rs`**: Prometheus HTTP endpoint
- **`feed_metrics.rs`**: Feed-specific metrics
- **`gas_metrics.rs`**: Gas usage tracking
- Exposes metrics on port 9090

### 9. Wallet Module (`src/wallet/`)

Wallet and key management:
- **`balance_monitor.rs`**: Tracks wallet balances
- Balance alerts and notifications
- Multi-network wallet support

## Data Flow

### 1. Startup Sequence

```
main.rs
  ├─> Load Configuration
  ├─> Initialize Networks
  ├─> Load Wallets
  ├─> Connect Database (optional)
  ├─> Start Feed Manager
  ├─> Start Metrics Server
  └─> Start Balance Monitor
```

### 2. Feed Update Cycle

```
FeedMonitor (per feed)
  ├─> Check Timer
  ├─> Fetch External Data
  ├─> Extract Value
  ├─> Calculate Deviation
  ├─> Check Update Conditions
  │     ├─> Time elapsed > minimum_update_frequency
  │     └─> OR deviation > threshold
  ├─> Read Contract State
  ├─> Submit Transaction
  └─> Log Results
```

### 3. Transaction Lifecycle

```
Transaction Submission
  ├─> Estimate Gas
  ├─> Apply Gas Config
  ├─> Sign Transaction
  ├─> Submit to Network
  ├─> Wait for Confirmation
  ├─> Retry if Failed
  └─> Update Metrics
```

## Concurrency Model

Omikuji uses Tokio for asynchronous operations:

- **Main Thread**: Application lifecycle
- **Feed Tasks**: One tokio task per datafeed
- **HTTP Server**: Metrics endpoint
- **Database Tasks**: Background cleanup
- **Monitor Tasks**: Balance monitoring

### Task Communication

- Shared state via `Arc<T>`
- Channels for inter-task messaging
- Graceful shutdown propagation

## Database Schema

### feed_log Table

```sql
CREATE TABLE feed_log (
    id SERIAL PRIMARY KEY,
    datafeed_name VARCHAR(255) NOT NULL,
    network VARCHAR(255) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    external_value NUMERIC NOT NULL,
    submitted_value NUMERIC,
    deviation_percentage NUMERIC,
    transaction_hash VARCHAR(66),
    gas_used BIGINT,
    gas_price NUMERIC,
    status VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### transaction_log Table

```sql
CREATE TABLE transaction_log (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    network VARCHAR(255) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value NUMERIC NOT NULL,
    gas_used BIGINT,
    gas_price NUMERIC,
    gas_cost NUMERIC,
    status VARCHAR(50) NOT NULL,
    datafeed_name VARCHAR(255)
);
```

### processed_events Table

```sql
CREATE TABLE processed_events (
    id SERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    log_index INTEGER NOT NULL,
    contract_address VARCHAR(42) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    processed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    event_data JSONB NULL,
    UNIQUE(transaction_hash, log_index)
);
```

## Error Handling

### Retry Strategies

1. **HTTP Requests**: Exponential backoff with jitter
2. **Transactions**: Fee bumping after timeout
3. **Database**: Connection pooling with retry

### Failure Modes

- **Network Issues**: Continue other feeds
- **API Failures**: Log and retry
- **Contract Reverts**: Alert and skip
- **Database Down**: Continue without persistence

## Security Considerations

### Private Key Management

- Environment variables only
- Never logged or stored
- Per-network isolation

### Network Security

- HTTPS for external APIs
- Validated RPC endpoints
- Request timeouts

### Contract Security

- Address validation
- Bounds checking
- Access control verification

## Performance Characteristics

### Resource Usage

- **Memory**: ~50-200MB typical
- **CPU**: Low, mostly I/O waiting
- **Network**: Depends on feed count
- **Disk**: Minimal (logs + optional DB)

### Scaling Factors

- Number of datafeeds
- Update frequencies
- Network latency
- Gas prices

### Optimization Strategies

1. **Concurrent Feeds**: Independent task execution
2. **Connection Pooling**: Reuse HTTP/RPC connections
3. **Batch Updates**: Multiple feeds per transaction (future)
4. **Caching**: Contract configuration caching

## Monitoring and Observability

### Prometheus Metrics

Key metrics exposed:
- `omikuji_feed_value`: Current feed values
- `omikuji_feed_updates_total`: Update counters
- `omikuji_feed_errors_total`: Error counters
- `omikuji_gas_used_total`: Gas consumption
- `omikuji_wallet_balance`: Wallet balances

### Logging

Structured logging with tracing:
- `ERROR`: Critical failures
- `WARN`: Recoverable issues
- `INFO`: Normal operations
- `DEBUG`: Detailed diagnostics
- `TRACE`: Full execution flow

### Health Checks

- Metrics endpoint: `http://localhost:9090/metrics`
- Process monitoring via systemd/Docker
- Database connection health

## Extension Points

### Adding New Contract Types

1. Define contract ABI in `src/contracts/`
2. Implement contract interface
3. Add to configuration parser
4. Update contract updater

### Adding New Data Sources

1. Extend fetcher for new protocols
2. Add new extractor formats
3. Update configuration schema

### Custom Metrics

1. Define new metric types
2. Register in metrics module
3. Update from relevant module

## Development Workflow

### Local Testing

1. Use Anvil for local blockchain
2. Mock external APIs with mockito
3. Test database with Docker
4. Unit tests for components

### Integration Testing

1. Testnet deployment
2. Real API endpoints
3. Actual gas consumption
4. End-to-end validation

## Future Architecture Considerations

### Planned Improvements

1. **Horizontal Scaling**: Multiple Omikuji instances
2. **Queue System**: Decoupled feed processing
3. **WebSocket Support**: Real-time data sources
4. **Plugin System**: Dynamic feed types
5. **Multi-Oracle Coordination**: Consensus mechanisms

### Performance Enhancements

1. **Batch Transactions**: Multiple updates per TX
2. **Predictive Scheduling**: Anticipate updates
3. **Adaptive Polling**: Dynamic frequencies
4. **State Caching**: Reduce RPC calls

This architecture provides a solid foundation for reliable oracle operations while maintaining flexibility for future enhancements.