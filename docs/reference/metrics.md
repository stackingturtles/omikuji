# Metrics Reference

Omikuji provides comprehensive Prometheus metrics for monitoring all aspects of the system. Metrics are exposed on port 9090 by default at the `/metrics` endpoint.

## Metric Categories

### 1. Data Source Health Metrics

Monitor the health and performance of external data sources.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_datasource_http_requests_total` | Counter | Total HTTP requests to data sources | feed_name, network, status, method |
| `omikuji_datasource_http_request_duration_seconds` | Histogram | HTTP request duration | feed_name, network, status |
| `omikuji_datasource_availability` | Gauge | Data source availability (0/1) | feed_name, network, url |
| `omikuji_datasource_consecutive_errors` | Gauge | Consecutive error count | feed_name, network, error_type |
| `omikuji_datasource_http_response_size_bytes` | Histogram | Response size in bytes | feed_name, network |
| `omikuji_datasource_latency_seconds` | Histogram | Total operation latency | feed_name, network, operation |
| `omikuji_datasource_parse_errors_total` | Counter | Parsing error count | feed_name, network, error_type |
| `omikuji_datasource_rate_limits_total` | Counter | Rate limit hits | feed_name, network, url |

### 2. Update Decision Metrics

Track why and when updates occur or are skipped.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_update_decisions_total` | Counter | Update decision count | feed_name, network, decision, reason |
| `omikuji_time_since_update_seconds` | Gauge | Time since last update | feed_name, network, update_type |
| `omikuji_deviation_breaches_total` | Counter | Deviation threshold breaches | feed_name, network, severity |
| `omikuji_update_frequency_violations_total` | Counter | Frequency violations | feed_name, network, violation_type |
| `omikuji_update_check_interval_seconds` | Histogram | Check interval timing | feed_name, network |
| `omikuji_consecutive_skipped_updates` | Gauge | Skipped update count | feed_name, network, skip_reason |
| `omikuji_update_lag_seconds` | Histogram | Update lag time | feed_name, network |
| `omikuji_update_deviation_percent` | Histogram | Deviation at update | feed_name, network |
| `omikuji_update_attempts_total` | Counter | Update attempt count | feed_name, network, result |

### 3. Network/RPC Metrics

Monitor blockchain network interactions. These metrics are continuously updated by the NetworkHealthMonitor.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_rpc_requests_total` | Counter | RPC request count | network, method, status |
| `omikuji_rpc_request_latency_seconds` | Histogram | RPC request latency | network, method |
| `omikuji_chain_head_block` | Gauge | Current block number (updated every 30s) | network |
| `omikuji_chain_reorgs_total` | Counter | Chain reorganizations detected | network, depth |
| `omikuji_network_sync_status` | Gauge | Sync status (0/1, based on block timestamp) | network |
| `omikuji_rpc_endpoint_health` | Gauge | Endpoint health (0/1) | network, endpoint |
| `omikuji_block_time_seconds` | Gauge | Average block time (rolling 10-block average) | network |
| `omikuji_pending_transactions` | Gauge | Pending transaction count | network, feed_name |
| `omikuji_network_gas_price_gwei` | Gauge | Current gas price | network, percentile |
| `omikuji_network_base_fee_gwei` | Gauge | Current base fee for EIP-1559 networks | network |
| `omikuji_network_priority_fee_percentiles_gwei` | Gauge | Priority fee percentiles (p25, p50, p75, p90) | network, percentile |
| `omikuji_network_congestion_level` | Gauge | Network congestion level (0=low, 100=high) | network |
| `omikuji_last_block_timestamp` | Gauge | Unix timestamp of the last processed block | network |
| `omikuji_rpc_connection_pool_size` | Gauge | Connection pool stats | network, state |
| `omikuji_rpc_errors_total` | Counter | RPC errors by type | network, error_type, method |

### 4. Contract Interaction Metrics

Track smart contract interactions.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_contract_reads_total` | Counter | Contract read operations | feed_name, network, method, status |
| `omikuji_contract_writes_total` | Counter | Contract write operations | feed_name, network, status |
| `omikuji_contract_operation_latency_seconds` | Histogram | Operation latency | feed_name, network, operation_type |
| `omikuji_transaction_queue_size` | Gauge | Transaction queue depth | feed_name, network, state |
| `omikuji_nonce_gaps_total` | Counter | Nonce gap occurrences | network, severity |
| `omikuji_transaction_reverts_total` | Counter | Transaction reverts | feed_name, network, reason |
| `omikuji_contract_permission_errors_total` | Counter | Permission errors | feed_name, network, method |
| `omikuji_transaction_confirmation_time_seconds` | Histogram | Confirmation time | feed_name, network |
| `omikuji_transaction_retries_total` | Counter | Retry attempts | feed_name, network, retry_reason |
| `omikuji_contract_state_sync` | Gauge | State sync status (0/1) | feed_name, network |
| `omikuji_transaction_mempool_time_seconds` | Histogram | Mempool wait time | feed_name, network |

### 5. Data Quality Metrics

Monitor feed data quality and consistency.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_feed_value_change_rate_percent` | Histogram | Value change rate | feed_name, network |
| `omikuji_outlier_detections_total` | Counter | Outlier detections | feed_name, network, outlier_type, action |
| `omikuji_data_consistency_score` | Gauge | Consistency score (0-100) | feed_name, network |
| `omikuji_stale_data_duration_seconds` | Gauge | Staleness duration | feed_name, network, staleness_type |
| `omikuji_value_deviation_from_ma_percent` | Histogram | Moving average deviation | feed_name, network, ma_period |
| `omikuji_data_source_agreement` | Gauge | Source agreement (0-100%) | feed_name, network |
| `omikuji_invalid_values_total` | Counter | Invalid value count | feed_name, network, validation_type |
| `omikuji_data_gaps_total` | Counter | Data gap occurrences | feed_name, network, gap_duration_category |
| `omikuji_feed_reliability_score` | Gauge | Reliability score (0-100) | feed_name, network |
| `omikuji_timestamp_drift_seconds` | Histogram | Timestamp drift | feed_name, network |

### 6. Economic/Cost Metrics

Track gas costs and economic efficiency.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_cumulative_gas_cost_usd` | Counter | Cumulative gas costs | feed_name, network, time_period |
| `omikuji_wallet_balance_usd` | Gauge | Wallet balance in USD | network, address |
| `omikuji_estimated_runway_days` | Gauge | Days until depletion | network, address |
| `omikuji_cost_per_update_usd` | Histogram | Cost per update | feed_name, network |
| `omikuji_daily_spending_rate_usd` | Gauge | Daily spending rate | network |
| `omikuji_gas_price_ratio` | Histogram | Paid vs average ratio | feed_name, network |
| `omikuji_low_balance_alert` | Gauge | Low balance alert (0/1) | network, address, severity |
| `omikuji_cost_efficiency_score` | Gauge | Efficiency score (0-100) | feed_name, network |
| `omikuji_budget_utilization_percent` | Gauge | Budget usage | network |
| `omikuji_gas_optimization_savings_usd` | Counter | Optimization savings | feed_name, network, optimization_type |

### 7. Performance Metrics

Monitor system performance and resource usage.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_concurrent_feed_updates` | Gauge | Concurrent updates | network |
| `omikuji_memory_usage_bytes` | Gauge | Memory usage | memory_type |
| `omikuji_open_connections` | Gauge | Open connections | connection_type, network |
| `omikuji_task_execution_time_seconds` | Histogram | Task execution time | task_type, network |
| `omikuji_cpu_usage_percent` | Gauge | CPU usage | cpu_type |
| `omikuji_thread_pool_utilization` | Gauge | Thread pool stats | pool_name, state |
| `omikuji_event_loop_lag_seconds` | Histogram | Event loop lag | runtime |
| `omikuji_db_connection_pool` | Gauge | DB connection stats | pool_state |
| `omikuji_cache_operations_total` | Counter | Cache operations | cache_name, operation, result |
| `omikuji_startup_time_seconds` | Histogram | Startup time | component |

### 8. Configuration Info Metrics

Expose configuration and system information.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_active_datafeeds` | Gauge | Active datafeed count | network, status |
| `omikuji_datafeed_config_info` | Info | Datafeed configuration | feed_name, network, contract_type, etc. |
| `omikuji_network_config_info` | Info | Network configuration | network, rpc_url, transaction_type, etc. |
| `omikuji_monitoring_cycle_duration_seconds` | Gauge | Cycle duration | cycle_type |
| `omikuji_version_info` | Info | Version information | version, git_commit, build_date, rust_version |
| `omikuji_feature_flags` | Gauge | Feature flags (0/1) | feature_name |
| `omikuji_config_reload_count` | Gauge | Config reload count | reload_type, status |
| `omikuji_environment_info` | Info | Environment info | environment, deployment_type, region |
| `omikuji_key_storage_config` | Info | Key storage config | storage_type, keyring_service |

### 9. Alert-Worthy Metrics

Critical metrics for alerting.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_critical_errors_total` | Counter | Critical errors | error_type, component, network |
| `omikuji_feed_update_lag_alert` | Gauge | Update lag alert (0/1) | feed_name, network, severity |
| `omikuji_transaction_retry_exhausted_total` | Counter | Exhausted retries | feed_name, network, final_error |
| `omikuji_system_health_score` | Gauge | Health score (0-100) | component |
| `omikuji_alert_suppression_active` | Gauge | Alert suppression (0/1) | alert_type, reason |
| `omikuji_cascading_failure_risk` | Gauge | Failure risk (0-100) | network, risk_factor |
| `omikuji_emergency_shutdown_triggered_total` | Counter | Emergency shutdowns | component, reason |
| `omikuji_degraded_mode_active` | Gauge | Degraded mode (0/1) | component, degradation_type |
| `omikuji_sla_violations_total` | Counter | SLA violations | feed_name, network, sla_type |
| `omikuji_alert_queue_depth` | Gauge | Alert queue size | severity, destination |

### 10. Legacy Metrics (Refactored)

Original metrics that have been integrated into the new structure.

| Metric Name | Type | Description | Labels |
|------------|------|-------------|--------|
| `omikuji_wallet_balance_wei` | Gauge | Wallet balance in wei | network, address |
| `omikuji_feed_value` | Gauge | Latest feed value | feed, network |
| `omikuji_feed_last_update_timestamp` | Gauge | Feed update timestamp | feed, network |
| `omikuji_contract_last_update_timestamp` | Gauge | Contract update timestamp | feed, network |
| `omikuji_contract_value` | Gauge | Contract value | feed, network |
| `omikuji_contract_round` | Gauge | Contract round | feed, network |
| `omikuji_feed_deviation_percent` | Gauge | Feed deviation | feed, network |
| `omikuji_data_staleness_seconds` | Gauge | Data staleness | feed, network, data_type |
| `omikuji_feed_last_submission_seconds` | Gauge | Seconds since this node last submitted a value to the feed contract (NaN if no history) | feed_name, network |
| `omikuji_gas_used_total` | Counter | Total gas used | feed_name, network, status |
| `omikuji_gas_price_gwei` | Histogram | Gas price | network, tx_type |
| `omikuji_gas_efficiency_percent` | Gauge | Gas efficiency | feed_name, network |
| `omikuji_transaction_cost_wei` | Histogram | Transaction cost | feed_name, network |
| `omikuji_transaction_count` | Counter | Transaction count | feed_name, network, status, tx_type |
| `omikuji_gas_limit` | Gauge | Gas limit | feed_name, network |

## Prometheus Query Examples

### Alert Rules

```yaml
groups:
  - name: omikuji_alerts
    rules:
      - alert: FeedUpdateLag
        expr: omikuji_time_since_update_seconds{update_type="contract"} > 3600
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Feed {{ $labels.feed_name }} hasn't updated in over 1 hour"
          
      - alert: LowWalletBalance
        expr: omikuji_wallet_balance_usd < 50
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Wallet balance below $50 on {{ $labels.network }}"
          
      - alert: HighErrorRate
        expr: rate(omikuji_critical_errors_total[5m]) > 0.1
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "High critical error rate on {{ $labels.component }}"
```

### Dashboard Queries

```promql
# Feed update frequency
rate(omikuji_update_decisions_total{decision="update"}[5m])

# Average gas cost per update
rate(omikuji_cumulative_gas_cost_usd[1h]) / rate(omikuji_update_decisions_total{decision="update"}[1h])

# Data source availability percentage
avg_over_time(omikuji_datasource_availability[5m]) * 100

# System health overview
min(omikuji_system_health_score) by (component)

# Top 5 feeds by gas consumption
topk(5, sum(rate(omikuji_gas_used_total[1h])) by (feed_name))
```

## Metric Configuration

Enable/disable metric categories via configuration:

```yaml
metrics:
  enabled: true
  port: 9090
  detailed_metrics: true  # Enable high-cardinality metrics
  categories:
    datasource: true
    update_decisions: true
    network: true
    contract: true
    quality: true
    economic: true
    performance: true
    config: true
    alerts: true
```

## Best Practices

1. **Cardinality Management**: Use `detailed_metrics: false` to reduce cardinality in production
2. **Retention**: Configure appropriate retention for different metric types
3. **Aggregation**: Use recording rules for frequently-queried aggregations
4. **Alerting**: Focus on SLO-based alerts rather than individual metrics
5. **Dashboards**: Create role-specific dashboards (operations, finance, development)

## Integration Examples

### Grafana Dashboard

Import the provided dashboard JSON from `examples/grafana-dashboard.json` or create custom dashboards using the metrics above.

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'omikuji'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    metrics_path: '/metrics'
```

### Alertmanager Integration

Configure alert routing based on severity labels:

```yaml
route:
  group_by: ['alertname', 'network', 'severity']
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
    - match:
        severity: warning
      receiver: 'slack'
```

## Network Health Monitoring

The NetworkHealthMonitor component continuously monitors network health and updates metrics every 30 seconds (configurable). This ensures that network-specific metrics remain current for alerting and observability.

### Monitored Metrics

The NetworkHealthMonitor updates the following metrics:

1. **Chain Head Block** - Current block number with automatic reorg detection
2. **Block Time** - Rolling average of the last 10 blocks
3. **Network Sync Status** - Determines if node is synced based on block timestamp
4. **Gas Prices** - Base fee and priority fee percentiles for optimal transaction pricing
5. **Network Congestion** - Calculated score (0-100) based on current gas prices
6. **RPC Endpoint Health** - Validates each endpoint remains responsive

### Example Prometheus Queries

Monitor network health:
```promql
# Network congestion over time
omikuji_network_congestion_level{network="ethereum"}

# Average block time
omikuji_block_time_seconds{network="ethereum"}

# Check if network is synced
omikuji_network_sync_status{network="ethereum"} == 0

# Gas price percentiles for cost optimization
omikuji_network_priority_fee_percentiles_gwei{network="ethereum", percentile="p50"}

# Detect chain reorganizations
increase(omikuji_chain_reorgs_total[1h]) > 0
```

### Configuration

The NetworkHealthMonitor can be configured with:
- `polling_interval_secs`: How often to check network health (default: 30)
- `block_history_size`: Number of blocks to keep for averaging (default: 10)
- `sync_threshold_secs`: Maximum age for a block to be considered synced (default: 120)

## Node-Specific Submission Tracking

The `omikuji_feed_last_submission_seconds` metric tracks how long it has been since the current node last successfully submitted a value to each feed contract. This is particularly useful in multi-node oracle environments.

### Use Cases

1. **Monitor Node Health**: Identify nodes that are failing to submit updates
2. **Multi-Oracle Coordination**: Track submission patterns across multiple oracle nodes
3. **Alerting**: Create alerts when a node hasn't submitted for extended periods
4. **Performance Analysis**: Analyze submission frequency and patterns

### Example Prometheus Queries

```promql
# Alert when a node hasn't submitted for over 1 hour
omikuji_feed_last_submission_seconds > 3600

# Find feeds with no submission history (NaN values)
omikuji_feed_last_submission_seconds != omikuji_feed_last_submission_seconds

# Average time between submissions for a specific feed
avg_over_time(omikuji_feed_last_submission_seconds{feed_name="eth_usd"}[1h])

# Identify the most active submitter (lowest average time since submission)
topk(5, avg by (feed_name) (omikuji_feed_last_submission_seconds))
```

### Notes

- Returns `NaN` when no submission history exists for a feed
- Updates with each datafeed monitoring cycle
- Only tracks successful submissions (status='success' in transaction_log)
- Requires database configuration with transaction logging enabled