-- Create gas token prices table
CREATE TABLE IF NOT EXISTS gas_token_prices (
    id SERIAL PRIMARY KEY,
    token_id VARCHAR(50) NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    price_usd DECIMAL(20, 8) NOT NULL,
    source VARCHAR(50) NOT NULL,
    fetched_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index for efficient querying by token and time
CREATE INDEX idx_gas_prices_token_time ON gas_token_prices(token_id, fetched_at DESC);

-- Create gas costs USD table for tracking historical costs
CREATE TABLE IF NOT EXISTS gas_costs_usd (
    id SERIAL PRIMARY KEY,
    network VARCHAR(50) NOT NULL,
    feed_name VARCHAR(100) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    gas_used BIGINT NOT NULL,
    gas_price_wei NUMERIC(78, 0) NOT NULL,
    gas_token_price_usd DECIMAL(20, 8) NOT NULL,
    total_cost_usd DECIMAL(20, 8) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index for efficient querying by network and feed
CREATE INDEX idx_gas_costs_network_feed ON gas_costs_usd(network, feed_name, created_at DESC);

-- Index for finding costs by transaction hash
CREATE INDEX idx_gas_costs_tx_hash ON gas_costs_usd(transaction_hash);

-- View for hourly gas costs aggregation
CREATE VIEW hourly_gas_costs AS
SELECT 
    network,
    feed_name,
    DATE_TRUNC('hour', created_at) AS hour,
    COUNT(*) AS transaction_count,
    SUM(gas_used) AS total_gas_used,
    SUM(total_cost_usd) AS total_cost_usd,
    AVG(gas_token_price_usd) AS avg_token_price_usd
FROM gas_costs_usd
GROUP BY network, feed_name, DATE_TRUNC('hour', created_at);

-- View for daily gas costs aggregation (replace existing view)
DROP VIEW IF EXISTS daily_gas_costs;
CREATE VIEW daily_gas_costs AS
SELECT 
    network,
    feed_name,
    DATE_TRUNC('day', created_at) AS day,
    COUNT(*) AS transaction_count,
    SUM(gas_used) AS total_gas_used,
    SUM(total_cost_usd) AS total_cost_usd,
    AVG(gas_token_price_usd) AS avg_token_price_usd
FROM gas_costs_usd
GROUP BY network, feed_name, DATE_TRUNC('day', created_at);