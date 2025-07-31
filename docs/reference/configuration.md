# Configuration Reference

Complete reference for all Omikuji configuration options.

## Configuration File Format

Omikuji uses YAML format for configuration files. The file must contain two top-level sections: `networks` and `datafeeds`.

## Command Line Options

```bash
omikuji [OPTIONS]
```

### Options

- `-c, --config <FILE>`: Path to configuration file
  - Default: `config.yaml` in current directory, then `~/.omikuji/config.yaml`
  
- `-p, --private-key-env <ENV_VAR>`: Environment variable containing private key (backward compatibility fallback)
  - Default: `OMIKUJI_PRIVATE_KEY`
  - Note: This is only used when keyring storage fails. Use network-specific variables instead.
  
- `-V, --version`: Display version information

- `-h, --help`: Display help information

## Networks Section

Define blockchain network connections.

```yaml
networks:
  - name: <string>              # Required: Unique network identifier
    rpc_url: <string>           # Required: HTTP(S) RPC endpoint URL
    transaction_type: <string>  # Optional: "legacy" or "eip1559" (default: "eip1559")
    gas_config:                 # Optional: Gas configuration
      <gas_options>
```

### Network Fields

#### `name` (required)
- Type: `string`
- Description: Unique identifier for the network
- Example: `ethereum`, `base`, `polygon`

#### `rpc_url` (required)
- Type: `string`
- Description: HTTP or HTTPS URL for the network's RPC endpoint
- Example: `https://eth.llamarpc.com`

#### `transaction_type` (optional)
- Type: `string`
- Values: `legacy`, `eip1559`
- Default: `eip1559`
- Description: Transaction type to use for this network

#### `gas_config` (optional)
- Type: `object`
- Description: Gas configuration options
- See [Gas Configuration Reference](#gas-configuration) below

## Datafeeds Section

Define data sources and their associated contracts.

```yaml
datafeeds:
  - name: <string>                      # Required: Unique feed identifier
    networks: <string>                  # Required: Network name reference
    check_frequency: <integer>          # Required: Polling interval (seconds)
    contract_address: <string>          # Required: Contract address (0x...)
    contract_type: <string>             # Required: Contract type
    feed_url: <string>                  # Required: Data source URL
    feed_json_path: <string>            # Required: JSON path to value
    
    # Update triggers (at least one required)
    minimum_update_frequency: <integer> # Optional: Time-based trigger (seconds)
    deviation_threshold_pct: <float>    # Optional: Deviation trigger (percent)
    
    # Contract configuration
    read_contract_config: <boolean>     # Optional: Read config from contract
    decimals: <integer>                 # Conditional: Required if read_contract_config=false
    min_value: <number>                 # Optional: Minimum submission value
    max_value: <number>                 # Optional: Maximum submission value
    
    # Additional options
    feed_json_path_timestamp: <string>  # Optional: JSON path to timestamp
```

### Datafeed Fields

#### `name` (required)
- Type: `string`
- Description: Unique identifier for the datafeed
- Example: `eth_usd_price`

#### `networks` (required)
- Type: `string`
- Description: Network name from the networks section
- Example: `ethereum`

#### `check_frequency` (required)
- Type: `integer`
- Range: 1-86400
- Description: How often to poll the data source (seconds)
- Example: `60` (check every minute)

#### `contract_address` (required)
- Type: `string`
- Format: `0x` followed by 40 hexadecimal characters
- Description: Ethereum address of the contract to update
- Example: `0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419`

#### `contract_type` (required)
- Type: `string`
- Values: `fluxmon`
- Description: Type of contract interface
- Note: Currently only Chainlink FluxAggregator is supported

#### `feed_url` (required)
- Type: `string`
- Format: Valid HTTP or HTTPS URL
- Description: API endpoint returning JSON data
- Example: `https://api.coinbase.com/v2/exchange-rates?currency=ETH`

#### `feed_json_path` (required)
- Type: `string`
- Format: Dot-notation path
- Description: Path to extract value from JSON response
- Examples:
  - `price` - Top-level field
  - `data.USD` - Nested field
  - `rates.0.value` - Array access

#### `minimum_update_frequency` (optional)
- Type: `integer`
- Range: 0-2147483647
- Description: Minimum seconds between updates (time-based trigger)
- Example: `3600` (update at least hourly)

#### `deviation_threshold_pct` (optional)
- Type: `float`
- Range: 0.0-100.0
- Description: Percentage change to trigger update
- Example: `0.5` (update on 0.5% change)

#### `read_contract_config` (optional)
- Type: `boolean`
- Default: `true`
- Description: Whether to read decimals/bounds from contract

#### `decimals` (conditional)
- Type: `integer`
- Range: 0-18
- Description: Number of decimal places for the value
- Required if: `read_contract_config = false`

#### `min_value` (optional)
- Type: `number`
- Description: Minimum value the contract will accept
- Default: `0`

#### `max_value` (optional)
- Type: `number`
- Description: Maximum value the contract will accept
- Default: No limit

#### `feed_json_path_timestamp` (optional)
- Type: `string`
- Format: Dot-notation path
- Description: Path to extract Unix timestamp from JSON
- Example: `data.last_updated`

## Event Monitors Section

Configure real-time blockchain event monitoring with webhook notifications.

```yaml
event_monitors:
  - name: <string>                      # Required: Unique monitor identifier
    network: <string>                   # Required: Network name reference
    contract_address: <string>          # Required: Contract to monitor (0x...)
    event_signature: <string>           # Required: Event signature
    webhook:                            # Required: Webhook configuration
      url: <string>                     # Required: Webhook endpoint URL
      method: <string>                  # Required: HTTP method (POST/PUT)
      headers: <object>                 # Optional: HTTP headers
      timeout_seconds: <integer>        # Optional: Request timeout (default: 30)
      retry_attempts: <integer>         # Optional: Retry count (default: 3)
      retry_delay_seconds: <integer>    # Optional: Retry delay (default: 5)
    response:                           # Required: Response handling
      type: <string>                    # Required: Response type
      contract_call: <object>           # Conditional: Required if type=contract_call
      validation: <object>              # Optional: Response validation
    execution_limits: <object>          # Optional: Execution safety limits
```

### Event Monitor Fields

#### `name` (required)
- Type: `string`
- Description: Unique identifier for the event monitor
- Example: `price_oracle_requests`, `token_transfers`

#### `network` (required)
- Type: `string`
- Description: Network to monitor (must match a configured network)
- Example: `ethereum`, `base`

#### `contract_address` (required)
- Type: `string`
- Description: Contract address to monitor for events
- Format: `0x` prefixed hex address
- Example: `0x1234567890123456789012345678901234567890`

#### `event_signature` (required)
- Type: `string`
- Description: Event signature to monitor
- Format: `EventName(type1 name1, type2 name2, ...)`
- Example: `Transfer(address indexed from, address indexed to, uint256 value)`

#### `webhook` (required)
- Type: `object`
- Description: Webhook endpoint configuration
- Fields:
  - `url`: HTTP(S) endpoint to call
  - `method`: HTTP method (`POST` or `PUT`)
  - `headers`: Optional key-value pairs for HTTP headers
  - `timeout_seconds`: Request timeout (default: 30)
  - `retry_attempts`: Number of retries on failure (default: 3)
  - `retry_delay_seconds`: Delay between retries (default: 5)

#### `response` (required)
- Type: `object`
- Description: How to handle webhook responses
- Fields:
  - `type`: Response handling type
    - `log_only`: Only log the response
    - `contract_call`: Execute contract calls from response
    - `store_db`: Store response in database
    - `multi_action`: Multiple actions
  - `contract_call`: Configuration for contract calls (required if type=contract_call)
  - `validation`: Optional response validation rules

#### `contract_call` (conditional)
- Type: `object`
- Required when: `response.type = contract_call`
- Fields:
  - `target_contract`: Target contract (use `{event.address}` for same contract)
  - `max_gas_price_gwei`: Maximum gas price in gwei
  - `gas_limit_multiplier`: Gas estimation multiplier (default: 1.2)
  - `value_wei`: ETH value to send (default: 0)

#### `execution_limits` (optional)
- Type: `object`
- Description: Safety limits for contract execution
- Fields:
  - `max_value_wei`: Maximum ETH value per transaction (as string)
  - `max_gas_price_gwei`: Maximum gas price in gwei

### Default Execution Limits

Set global defaults for all event monitors:

```yaml
default_execution_limits:
  max_value_wei: "1000000000000000000"  # 1 ETH
  max_gas_price_gwei: 100
```

Individual monitors can override these defaults.

## Gas Configuration

Detailed gas configuration options for each network.

```yaml
gas_config:
  # Fee estimation
  gas_multiplier: <float>           # Multiply estimated gas (default: 1.1)
  max_fee_per_gas: <integer>        # Max fee in gwei (EIP-1559)
  max_priority_fee: <integer>       # Max priority fee in gwei (EIP-1559)
  gas_price: <integer>              # Gas price in gwei (legacy)
  
  # Limits
  gas_limit: <integer>              # Manual gas limit override
  max_gas_price: <integer>          # Maximum gas price in gwei
  
  # Retry behavior
  fee_bump_percentage: <integer>    # Fee increase on retry (default: 10)
  max_retries: <integer>            # Maximum retry attempts (default: 3)
  retry_delay_ms: <integer>         # Delay between retries (default: 5000)
```

See [Gas Configuration Guide](../guides/gas-configuration.md) for detailed explanations.

## Environment Variables

### Private Keys

**Required**: You must provide a private key for each network using one of these methods:

1. **Network-specific environment variables (Recommended)**: `OMIKUJI_PRIVATE_KEY_<NETWORK>` 
   - Example: `OMIKUJI_PRIVATE_KEY_ETHEREUM`, `OMIKUJI_PRIVATE_KEY_BASE`, `OMIKUJI_PRIVATE_KEY_ANVIL`
   - Network name is uppercase with hyphens replaced by underscores
   - This is the primary method for setting private keys

2. **Generic environment variable**: `PRIVATE_KEY`
   - Used as fallback for all networks if network-specific variable is not set
   - Not recommended for multi-network deployments

3. **OS Keyring (Recommended for production)**:
   - Import: `omikuji key import --network <network-name>`
   - More secure than environment variables

4. **Legacy backward compatibility**: The `-p` flag with `OMIKUJI_PRIVATE_KEY`
   - Only used as fallback when keyring storage fails
   - Not recommended for new deployments

### Optional

- `DATABASE_URL`: PostgreSQL connection string
- `RUST_LOG`: Logging level (`error`, `warn`, `info`, `debug`, `trace`)

## Complete Example

```yaml
# Network definitions
networks:
  - name: ethereum
    rpc_url: https://eth.llamarpc.com
    transaction_type: eip1559
    gas_config:
      gas_multiplier: 1.2
      max_fee_per_gas: 100
      max_priority_fee: 2
      fee_bump_percentage: 15
      max_retries: 5

  - name: base
    rpc_url: https://base.llamarpc.com
    gas_config:
      gas_multiplier: 1.1

# Datafeed definitions
datafeeds:
  # ETH/USD on Ethereum
  - name: eth_usd_mainnet
    networks: ethereum
    check_frequency: 60
    contract_address: "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419"
    contract_type: fluxmon
    read_contract_config: true
    minimum_update_frequency: 3600
    deviation_threshold_pct: 0.5
    feed_url: https://api.coinbase.com/v2/exchange-rates?currency=ETH
    feed_json_path: data.rates.USD
    
  # BTC/USD on Base with manual config
  - name: btc_usd_base
    networks: base
    check_frequency: 120
    contract_address: "0x64c911996D3c6aC71f9b455B1E8E7266BcbD848F"
    contract_type: fluxmon
    read_contract_config: false
    decimals: 8
    min_value: 0
    max_value: 10000000
    minimum_update_frequency: 7200
    deviation_threshold_pct: 1.0
    feed_url: https://api.coinbase.com/v2/exchange-rates?currency=BTC
    feed_json_path: data.rates.USD
    feed_json_path_timestamp: data.epoch

# Event monitor definitions
event_monitors:
  # Monitor oracle price requests
  - name: price_oracle_requests
    network: ethereum
    contract_address: "0x1234567890123456789012345678901234567890"
    event_signature: "PriceRequest(uint256 indexed requestId, address requester)"
    webhook:
      url: https://api.example.com/handle-price-request
      method: POST
      headers:
        Authorization: "Bearer YOUR_API_TOKEN"
      timeout_seconds: 30
      retry_attempts: 3
    response:
      type: contract_call
      contract_call:
        target_contract: "{event.address}"
        max_gas_price_gwei: 50
        gas_limit_multiplier: 1.2
        value_wei: 0
    execution_limits:
      max_value_wei: "0"
      max_gas_price_gwei: 100

  # Monitor large token transfers
  - name: large_transfers
    network: base
    contract_address: "0xABCDEF1234567890123456789012345678901234"
    event_signature: "Transfer(address indexed from, address indexed to, uint256 value)"
    webhook:
      url: https://api.example.com/monitor-transfers
      method: POST
    response:
      type: log_only

# Default execution limits for all monitors
default_execution_limits:
  max_value_wei: "1000000000000000000"  # 1 ETH
  max_gas_price_gwei: 150
```

## Scheduled Tasks Section

Configure automatic execution of smart contract functions on a time-based schedule.

```yaml
scheduled_tasks:
  - name: <string>                    # Required: Unique task identifier
    network: <string>                 # Required: Network to execute on
    schedule: <string>                # Required: Cron expression
    check_condition:                  # Optional: Condition to check before execution
      contract_address: <string>      # Required: Contract to check
      property: <string>              # Option 1: Boolean property name
      function: <string>              # Option 2: Parameterless view function
      expected_value: <any>           # Required: Expected return value
    target_function:                  # Required: Function to execute
      contract_address: <string>      # Required: Contract address
      function: <string>              # Required: Function signature
      parameters: <array>             # Required: Function parameters
    gas_config:                       # Optional: Gas configuration
      <gas_options>
```

### Scheduled Task Fields

#### `name` (required)
- Type: `string`
- Description: Unique identifier for the task
- Example: `daily_rewards`, `price_update`

#### `network` (required)
- Type: `string`
- Description: Network name where the task executes (must match a configured network)
- Example: `ethereum`, `polygon`

#### `schedule` (required)
- Type: `string`
- Description: Cron expression defining when to execute
- Format: `minute hour day month weekday`
- Examples:
  - `0 * * * *` - Every hour
  - `0 0 * * *` - Daily at midnight
  - `*/5 * * * *` - Every 5 minutes

#### `check_condition` (optional)
- Type: `object`
- Description: Condition to evaluate before execution
- Fields:
  - `contract_address`: Contract to read from
  - `property`: Name of boolean public property OR
  - `function`: Parameterless view function signature (e.g., `canExecute()`)
  - `expected_value`: Value to compare against (must match type)

#### `target_function` (required)
- Type: `object`
- Description: Smart contract function to execute
- Fields:
  - `contract_address`: Target contract address
  - `function`: Function signature with parameter types (e.g., `transfer(address,uint256)`)
  - `parameters`: Array of parameter values

#### `parameters`
- Type: `array`
- Description: Function parameters with types
- Format:
  ```yaml
  parameters:
    - value: <any>      # The parameter value
      type: <string>    # The Solidity type
  ```
- Supported types:
  - `uint256`: Unsigned integer
  - `address`: Ethereum address
  - `bool`: Boolean value
  - `address[]`: Array of addresses

### Example Scheduled Task

```yaml
scheduled_tasks:
  - name: "compound_yield"
    network: "ethereum-mainnet"
    schedule: "0 */6 * * *"  # Every 6 hours
    check_condition:
      contract_address: "0xYieldContract"
      function: "hasYieldToCompound()"
      expected_value: true
    target_function:
      contract_address: "0xYieldContract"
      function: "compound(uint256,address[])"
      parameters:
        - value: 1000000
          type: "uint256"
        - value: ["0xToken1", "0xToken2"]
          type: "address[]"
    gas_config:
      max_gas_price_gwei: 50
      gas_limit: 300000
```

## Metrics Section

Configure Prometheus metrics collection and export.

```yaml
metrics:
  enabled: <boolean>           # Optional: Enable metrics collection (default: true)
  port: <number>              # Optional: Prometheus metrics port (default: 9090)
  detailed_metrics: <boolean> # Optional: Enable high-cardinality metrics (default: false)
  categories:                 # Optional: Toggle specific metric categories
    datasource: <boolean>     # Data source health metrics (default: true)
    update_decisions: <boolean> # Update decision metrics (default: true)
    network: <boolean>        # Network/RPC metrics (default: true)
    contract: <boolean>       # Contract interaction metrics (default: true)
    quality: <boolean>        # Data quality metrics (default: true, requires detailed_metrics)
    economic: <boolean>       # Economic/cost metrics (default: true)
    performance: <boolean>    # Performance metrics (default: true, requires detailed_metrics)
    config: <boolean>         # Configuration info metrics (default: true)
    alerts: <boolean>         # Alert-worthy metrics (default: true)
```

### Metrics Fields

#### `enabled` (optional)
- Type: `boolean`
- Default: `true`
- Description: Master switch for metrics collection. When false, no metrics are collected or exposed.

#### `port` (optional)
- Type: `number`
- Default: `9090`
- Description: TCP port for the Prometheus metrics endpoint
- Note: If the port is already in use, Omikuji will log an error and continue without metrics

#### `detailed_metrics` (optional)
- Type: `boolean`
- Default: `false`
- Description: Enable collection of high-cardinality metrics (quality and performance categories)
- Warning: May increase memory usage and metrics storage requirements

#### `categories` (optional)
- Type: `object`
- Description: Fine-grained control over which metric categories to collect
- Note: All categories default to `true` except quality and performance which also require `detailed_metrics`

### Example Metrics Configuration

```yaml
# Basic configuration with custom port
metrics:
  port: 8080

# Disable metrics entirely
metrics:
  enabled: false

# Enable detailed metrics for debugging
metrics:
  port: 9090
  detailed_metrics: true

# Selective metric categories
metrics:
  port: 9090
  categories:
    datasource: true
    update_decisions: true
    network: false      # Disable network metrics
    contract: true
    quality: false      # Disable quality metrics
    economic: true
    performance: false  # Disable performance metrics
    config: true
    alerts: true
```

## Validation Rules

1. **Unique Names**: All network, datafeed, and scheduled task names must be unique
2. **Network References**: Datafeed networks and scheduled task networks must reference existing network names
3. **Valid Addresses**: Contract addresses must be valid Ethereum addresses
4. **Cron Expressions**: Schedule fields must be valid cron expressions
5. **Function Signatures**: Function signatures must include parameter types in parentheses
6. **URL Format**: Feed URLs must be valid HTTP/HTTPS URLs
7. **Update Triggers**: At least one of `minimum_update_frequency` or `deviation_threshold_pct` must be set
8. **Decimal Range**: Decimals must be between 0 and 18
9. **Positive Values**: Frequencies, percentages, and gas values must be positive

## Default Locations

Configuration files are searched in order:
1. Path specified with `-c` flag
2. `./config.yaml` (current directory)
3. `~/.omikuji/config.yaml` (user home directory)

## See Also

- [Configuration Guide](../getting-started/configuration.md) - Basic configuration tutorial
- [Gas Configuration Guide](../guides/gas-configuration.md) - Detailed gas settings
- [Environment Variables Guide](../guides/environment-variables.md) - Security best practices