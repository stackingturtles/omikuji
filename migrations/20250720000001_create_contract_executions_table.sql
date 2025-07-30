-- Create contract_executions table for tracking smart contract calls made by event monitors
-- This table stores all contract executions triggered by webhook responses

-- Ensure we're creating the table in the omikuji schema
SET search_path TO omikuji, public;

CREATE TABLE IF NOT EXISTS contract_executions (
    id SERIAL PRIMARY KEY,
    monitor_name VARCHAR(255) NOT NULL,
    network VARCHAR(255) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    contract_address VARCHAR(42) NOT NULL,
    function_selector VARCHAR(10) NOT NULL,
    call_data TEXT NOT NULL,
    value_wei VARCHAR(78) NOT NULL, -- Store as string to handle large numbers
    gas_limit BIGINT NOT NULL,
    gas_price_wei VARCHAR(78) NOT NULL, -- Store as string to handle large numbers
    gas_used BIGINT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    error_message TEXT,
    trigger_event_tx_hash VARCHAR(66) NOT NULL,
    trigger_event_log_index INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for efficient querying
CREATE INDEX idx_contract_executions_monitor_name ON contract_executions(monitor_name);
CREATE INDEX idx_contract_executions_network ON contract_executions(network);
CREATE INDEX idx_contract_executions_transaction_hash ON contract_executions(transaction_hash);
CREATE INDEX idx_contract_executions_status ON contract_executions(status);
CREATE INDEX idx_contract_executions_created_at ON contract_executions(created_at);
CREATE INDEX idx_contract_executions_trigger_event ON contract_executions(trigger_event_tx_hash, trigger_event_log_index);

-- Add comments to table and columns
COMMENT ON TABLE contract_executions IS 'Tracks smart contract calls made by event monitors in response to webhook calls';
COMMENT ON COLUMN contract_executions.id IS 'Auto-incrementing ID';
COMMENT ON COLUMN contract_executions.monitor_name IS 'Name of the event monitor that triggered this execution';
COMMENT ON COLUMN contract_executions.network IS 'Network where the execution occurred';
COMMENT ON COLUMN contract_executions.transaction_hash IS 'Transaction hash of the execution (with 0x prefix)';
COMMENT ON COLUMN contract_executions.contract_address IS 'Contract address that was called';
COMMENT ON COLUMN contract_executions.function_selector IS 'Function selector that was called (4 bytes hex)';
COMMENT ON COLUMN contract_executions.call_data IS 'Encoded function call data (hex)';
COMMENT ON COLUMN contract_executions.value_wei IS 'Value sent with the transaction in wei (as string for large numbers)';
COMMENT ON COLUMN contract_executions.gas_limit IS 'Gas limit used for the transaction';
COMMENT ON COLUMN contract_executions.gas_price_wei IS 'Gas price in wei (as string for large numbers)';
COMMENT ON COLUMN contract_executions.gas_used IS 'Actual gas used (filled after transaction is mined)';
COMMENT ON COLUMN contract_executions.status IS 'Transaction status: pending, success, failed';
COMMENT ON COLUMN contract_executions.error_message IS 'Error message if the transaction failed';
COMMENT ON COLUMN contract_executions.trigger_event_tx_hash IS 'Transaction hash of the event that triggered this execution';
COMMENT ON COLUMN contract_executions.trigger_event_log_index IS 'Log index of the event that triggered this execution';
COMMENT ON COLUMN contract_executions.created_at IS 'Timestamp when the execution was initiated';
COMMENT ON COLUMN contract_executions.updated_at IS 'Timestamp when the execution status was last updated';