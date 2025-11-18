-- Create transaction_log table for tracking gas consumption
CREATE TABLE IF NOT EXISTS transaction_log (
    id SERIAL PRIMARY KEY,
    -- Transaction identification
    tx_hash VARCHAR(66) NOT NULL,
    feed_name VARCHAR(255) NOT NULL,
    network_name VARCHAR(255) NOT NULL,
    
    -- Gas metrics
    gas_limit BIGINT NOT NULL,
    gas_used BIGINT NOT NULL,
    gas_price_gwei DOUBLE PRECISION NOT NULL,
    total_cost_wei NUMERIC(78, 0) NOT NULL, -- Large enough for any wei amount
    efficiency_percent DOUBLE PRECISION NOT NULL,
    
    -- Transaction details
    tx_type VARCHAR(20) NOT NULL, -- 'legacy' or 'eip1559'
    status VARCHAR(20) NOT NULL, -- 'success', 'failed', 'error'
    block_number BIGINT NOT NULL,
    
    -- Additional info
    error_message TEXT,
    max_fee_per_gas_gwei DOUBLE PRECISION, -- For EIP-1559 transactions
    max_priority_fee_per_gas_gwei DOUBLE PRECISION, -- For EIP-1559 transactions
    
    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    
    -- Indexes for common queries
    CONSTRAINT unique_tx_hash UNIQUE (tx_hash)
);

-- Index for querying by feed and time
CREATE INDEX idx_transaction_log_feed_created 
ON transaction_log(feed_name, network_name, created_at DESC);

-- Index for querying by network and time
CREATE INDEX idx_transaction_log_network_created 
ON transaction_log(network_name, created_at DESC);

-- Index for status queries
CREATE INDEX idx_transaction_log_status 
ON transaction_log(status, created_at DESC);

-- Index for cost analysis
CREATE INDEX idx_transaction_log_cost 
ON transaction_log(total_cost_wei DESC, created_at DESC);

-- Comment on table
COMMENT ON TABLE transaction_log IS 'Stores gas consumption metrics for all transactions submitted by Omikuji';

-- Create a view for gas statistics by feed
CREATE VIEW transaction_stats AS
SELECT 
    feed_name,
    network_name,
    COUNT(*) as total_transactions,
    COUNT(CASE WHEN status = 'success' THEN 1 END) as successful_transactions,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_transactions,
    COUNT(CASE WHEN status = 'error' THEN 1 END) as error_transactions,
    AVG(gas_used) as avg_gas_used,
    AVG(gas_price_gwei) as avg_gas_price_gwei,
    AVG(efficiency_percent) as avg_efficiency_percent,
    SUM(total_cost_wei) as total_cost_wei,
    MIN(created_at) as first_transaction,
    MAX(created_at) as last_transaction
FROM transaction_log
GROUP BY feed_name, network_name;

-- Create a view for daily gas costs
CREATE VIEW daily_gas_costs AS
SELECT 
    DATE(created_at) as date,
    network_name,
    feed_name,
    COUNT(*) as transaction_count,
    SUM(gas_used) as total_gas_used,
    AVG(gas_price_gwei) as avg_gas_price_gwei,
    SUM(total_cost_wei) as total_cost_wei,
    AVG(efficiency_percent) as avg_efficiency_percent
FROM transaction_log
WHERE status = 'success'
GROUP BY DATE(created_at), network_name, feed_name
ORDER BY date DESC, network_name, feed_name;