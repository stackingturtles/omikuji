# Webhook Response Examples Guide

This guide provides examples of webhook responses for Omikuji event monitors that support contract execution.

## Overview

When an event monitor with `response.type: contract_call` receives a blockchain event, it calls your webhook endpoint with event data. Your webhook should respond with instructions for what contract calls to execute.

## Request Format (What Omikuji Sends)

Your webhook will receive a POST request with this structure:

```json
{
  "event": {
    "monitor_name": "price_oracle_requests",
    "network": "ethereum",
    "transaction_hash": "0x123...",
    "block_number": 18500000,
    "log_index": 42,
    "address": "0x1234567890123456789012345678901234567890",
    "topics": ["0xabc...", "0xdef..."],
    "data": "0x000...",
    "decoded": {
      "requestId": "1",
      "requester": "0x5678..."
    }
  },
  "context": {
    "network": "ethereum",
    "timestamp": "2024-01-20T10:30:00Z",
    "omikuji_version": "0.4.0"
  }
}
```

## Response Format (What Your Webhook Returns)

### Basic Structure

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "functionName(type1,type2)",
      "params": [param1, param2],
      "value": "0"
    }
  ],
  "metadata": {}
}
```

### Field Descriptions

- **action** (required): What to do with this response
  - `"execute"`: Execute the contract calls
  - `"log"`: Only log, don't execute
  - `"error"`: Indicate an error occurred

- **calls** (required for execute): Array of contract calls to make
  - **target**: Contract address (must match the event's contract due to security restrictions)
  - **function**: Function signature with parameter types
  - **params**: Array of parameter values (matching the function signature)
  - **value**: ETH to send in wei (as string, default "0")

- **metadata** (optional): Additional data for logging/debugging

## Examples

### 1. Simple Price Update

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "updatePrice(uint256)",
      "params": [245000000000],
      "value": "0"
    }
  ],
  "metadata": {
    "price": "2450.00 USD",
    "source": "coinbase"
  }
}
```

### 2. Multiple Operations

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "updatePrice(uint256)",
      "params": [245000000000],
      "value": "0"
    },
    {
      "target": "0x1234567890123456789012345678901234567890", 
      "function": "setLastUpdated(uint256)",
      "params": [1705749000],
      "value": "0"
    }
  ]
}
```

### 3. Complex Parameters

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "batchUpdate(uint256[],address[],bool)",
      "params": [
        [100, 200, 300],
        ["0x111...", "0x222...", "0x333..."],
        true
      ],
      "value": "0"
    }
  ]
}
```

### 4. Call with ETH Value

```json
{
  "action": "execute",
  "calls": [
    {
      "target": "0x1234567890123456789012345678901234567890",
      "function": "depositAndExecute(bytes32)",
      "params": ["0x0000000000000000000000000000000000000000000000000000000000000001"],
      "value": "1000000000000000000"
    }
  ]
}
```

### 5. Error Response

```json
{
  "action": "error",
  "error": {
    "code": "PRICE_OUT_OF_RANGE",
    "message": "Price deviation exceeds 10%"
  }
}
```

### 6. Log Only Response

```json
{
  "action": "log",
  "metadata": {
    "reason": "No action needed",
    "threshold": "Not met"
  }
}
```

## Parameter Type Examples

### Basic Types
- `uint256`: `42` or `"42"`
- `address`: `"0x1234567890123456789012345678901234567890"`
- `bool`: `true` or `false`
- `bytes32`: `"0x0000000000000000000000000000000000000000000000000000000000000001"`

### Array Types
- `uint256[]`: `[1, 2, 3]`
- `address[]`: `["0x111...", "0x222..."]`
- `bool[]`: `[true, false, true]`

### String and Bytes
- `string`: `"Hello, World!"`
- `bytes`: `"0x48656c6c6f"`

## Security Considerations

1. **Same Contract Restriction**: Responses can only execute calls on the contract that emitted the event
2. **Value Limits**: Execution limits prevent sending too much ETH
3. **Gas Limits**: Maximum gas price limits prevent excessive fees
4. **Validation**: All responses are validated before execution

## Testing Your Webhook

1. Use the examples in `webhook_response_examples.json`
2. Test with low value limits first
3. Monitor the Omikuji logs for execution status
4. Check metrics at `http://localhost:9090/metrics`

## Common Patterns

### Oracle Updates
```json
{
  "action": "execute",
  "calls": [{
    "target": "{event.address}",
    "function": "submitAnswer(uint256)",
    "params": [priceInWei],
    "value": "0"
  }]
}
```

### Keeper Operations
```json
{
  "action": "execute",
  "calls": [{
    "target": "{event.address}",
    "function": "performUpkeep(bytes)",
    "params": ["0x"],
    "value": "0"
  }]
}
```

### DeFi Automation
```json
{
  "action": "execute",
  "calls": [{
    "target": "{event.address}",
    "function": "rebalance(uint256,uint256)",
    "params": [minAmount, maxAmount],
    "value": "0"
  }]
}
```

## Error Handling

If your webhook encounters an error:
1. Return appropriate HTTP status code (4xx or 5xx)
2. Include error details in response body
3. Omikuji will retry based on webhook configuration

## Best Practices

1. **Validate Input**: Always validate the event data
2. **Use Metadata**: Include debugging info in metadata
3. **Handle Errors**: Return clear error messages
4. **Test Thoroughly**: Test all response scenarios
5. **Monitor Execution**: Track success/failure rates