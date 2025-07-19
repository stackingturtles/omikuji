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

## Future Enhancements

- Multi-node support with automatic failover
- WebSocket connection pooling
- Event replay capabilities
- Advanced filtering options