# WebSocket Event Monitoring

This guide explains how to configure and use WebSocket connections for real-time event monitoring in Omikuji.

## Overview

Omikuji supports WebSocket connections for event monitoring, providing real-time updates with lower latency compared to HTTP polling. When a WebSocket URL is configured, Omikuji will automatically use it for event subscriptions.

## Configuration

### Network Configuration

Networks now support multiple nodes with separate RPC and WebSocket URLs:

```yaml
networks:
  - name: ethereum
    transaction_type: eip1559
    nodes:
      - name: Infura
        rpc_url: https://mainnet.infura.io/v3/YOUR_API_KEY
        ws_url: wss://mainnet.infura.io/ws/v3/YOUR_API_KEY
      - name: Alchemy  # Future: support for multiple nodes
        rpc_url: https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
        ws_url: wss://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
```

### Event Monitor Configuration

Event monitors automatically use WebSocket when available:

```yaml
event_monitors:
  - name: token_transfers
    network: ethereum
    contract_address: "0x..."
    event_signature: "Transfer(address indexed from, address indexed to, uint256 value)"
    webhook:
      url: "https://your-api.com/webhook"
      method: "POST"
```

## Features

### Automatic Connection Management

- **Automatic Reconnection**: If the WebSocket connection drops, Omikuji automatically attempts to reconnect with exponential backoff
- **Connection Health Monitoring**: Regular health checks ensure the connection is alive
- **Graceful Degradation**: Falls back to HTTP polling if WebSocket is unavailable

### Duplicate Event Prevention

Omikuji tracks processed events in a database table to prevent duplicate processing:

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

This ensures that:
- Events are processed exactly once, even across restarts
- Webhook calls are not duplicated for the same event
- Historical event data is preserved for auditing

## Migration Guide

### From Old Configuration Format

If you're upgrading from the old configuration format:

**Old format:**
```yaml
networks:
  - name: ethereum
    rpc_url: https://mainnet.infura.io/v3/YOUR_API_KEY
    ws_url: wss://mainnet.infura.io/ws/v3/YOUR_API_KEY  # Optional
```

**New format:**
```yaml
networks:
  - name: ethereum
    transaction_type: eip1559
    nodes:
      - name: Default Node
        rpc_url: https://mainnet.infura.io/v3/YOUR_API_KEY
        ws_url: wss://mainnet.infura.io/ws/v3/YOUR_API_KEY
```

The configuration parser automatically handles backward compatibility, but it's recommended to update to the new format.

### Database Migration

Run the database migration to create the `processed_events` table:

```bash
# Migrations run automatically on startup if DATABASE_URL is set
export DATABASE_URL=postgresql://user:password@localhost/omikuji
omikuji -c config.yaml
```

## Performance Considerations

### WebSocket vs HTTP Polling

| Feature | WebSocket | HTTP Polling |
|---------|-----------|--------------|
| Latency | Low (real-time) | Higher (poll interval) |
| Resource Usage | Lower | Higher |
| Connection Overhead | One persistent connection | Multiple requests |
| Reliability | Requires reconnection logic | Simpler, stateless |

### Best Practices

1. **Use Reliable Providers**: Choose WebSocket providers with good uptime
2. **Monitor Connection Health**: Watch the logs for reconnection events
3. **Set Appropriate Timeouts**: Configure webhook timeouts to handle delays
4. **Database Maintenance**: Periodically clean old events from `processed_events`

## Troubleshooting

### Common Issues

#### WebSocket Connection Drops
```
WARN WebSocket connection appears to be dead: ...
INFO Attempting to reconnect to WebSocket endpoint
```

This is normal behavior. Omikuji will automatically reconnect.

#### Duplicate Event Warnings
```
WARN Event already processed: tx=0x..., log_index=0 for monitor 'token_transfers'
```

This indicates the duplicate prevention is working correctly.

#### No WebSocket URL Configured
```
INFO Using HTTP polling for event monitoring on network 'ethereum'
```

Add a `ws_url` to your node configuration to enable WebSocket support.

### Debug Logging

Enable debug logging to see detailed WebSocket activity:

```bash
RUST_LOG=omikuji::event_monitors=debug,omikuji::network=debug omikuji -c config.yaml
```

## Contract Execution from Webhook Responses

Event monitors can now execute smart contract calls based on webhook responses. This enables automated on-chain actions triggered by off-chain logic.

### Configuration

Configure contract execution with response type and execution limits:

```yaml
event_monitors:
  - name: automated_responder
    network: ethereum
    contract_address: "0x..."
    event_signature: "RequestReceived(uint256 indexed id, address requester)"
    webhook:
      url: "https://your-api.com/process-request"
      method: "POST"
    response:
      type: contract_call
      contract_call:
        target_contract: "{event.address}"  # Use the same contract that emitted the event
        max_gas_price_gwei: 100
        gas_limit_multiplier: 1.2
        value_wei: 0
    execution_limits:
      max_value_wei: "1000000000000000000"  # 1 ETH max
      max_gas_price_gwei: 150
```

### Webhook Response Format

Your webhook should return a JSON response with contract calls:

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "updateValue(uint256,address)",
      "params": [42, "0x5678901234567890123456789012345678901234"],
      "value": "0"
    }
  ],
  "metadata": {
    "reason": "Updating based on external data",
    "timestamp": "2024-01-20T10:30:00Z"
  }
}
```

### Security Features

1. **Same-Contract Restriction**: Webhook responses can only execute calls on the contract that emitted the event
2. **Execution Limits**: Configure maximum ETH value and gas price limits
3. **Gas Management**: Automatic gas estimation with configurable multiplier
4. **Transaction Tracking**: All executions are logged in the database

### Database Tracking

Contract executions are tracked in a dedicated table:

```sql
CREATE TABLE contract_executions (
    id SERIAL PRIMARY KEY,
    monitor_name VARCHAR(255) NOT NULL,
    network VARCHAR(255) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    contract_address VARCHAR(42) NOT NULL,
    function_selector VARCHAR(10) NOT NULL,
    call_data TEXT NOT NULL,
    value_wei VARCHAR(78) NOT NULL,
    gas_limit BIGINT NOT NULL,
    gas_price_wei VARCHAR(78) NOT NULL,
    gas_used BIGINT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    error_message TEXT,
    trigger_event_tx_hash VARCHAR(66) NOT NULL,
    trigger_event_log_index INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Metrics

New metrics for monitoring contract executions:

- `omikuji_contract_executions_total` - Total executions by status
- `omikuji_contract_execution_gas_used` - Gas usage histogram
- `omikuji_contract_execution_value_wei` - ETH value histogram
- `omikuji_execution_validation_failures_total` - Validation failure reasons

### Best Practices

1. **Test Webhook Responses**: Ensure your webhook returns valid contract calls
2. **Set Conservative Limits**: Start with low value and gas limits
3. **Monitor Execution Status**: Track success/failure rates
4. **Handle Failures Gracefully**: Webhook should handle cases where execution might fail

## Future Enhancements

- Multi-node support with automatic failover
- WebSocket connection pooling
- Event replay capabilities
- Advanced filtering options
- Support for cross-contract calls (with additional security checks)
- Batch transaction support