-- Create processed_events table for tracking contract events that have been processed
-- This prevents duplicate processing of events across restarts and reconnections
CREATE TABLE IF NOT EXISTS processed_events (
    id SERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    log_index INTEGER NOT NULL,
    contract_address VARCHAR(42) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    processed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    event_data JSONB NULL,
    -- Ensure we never process the same event twice
    UNIQUE(transaction_hash, log_index)
);

-- Create indexes for efficient querying
CREATE INDEX idx_processed_events_event_type ON processed_events(event_type);
CREATE INDEX idx_processed_events_contract_address ON processed_events(contract_address);
CREATE INDEX idx_processed_events_processed_at ON processed_events(processed_at);

-- Add comment to table
COMMENT ON TABLE processed_events IS 'Tracks contract events that have been processed to prevent duplicates';
COMMENT ON COLUMN processed_events.transaction_hash IS 'Transaction hash containing the event (with 0x prefix)';
COMMENT ON COLUMN processed_events.log_index IS 'Log index of the event within the transaction';
COMMENT ON COLUMN processed_events.contract_address IS 'Address of the contract that emitted the event';
COMMENT ON COLUMN processed_events.event_type IS 'Type of event (e.g., mint, burn, deposit, HasItRainedSince)';
COMMENT ON COLUMN processed_events.processed_at IS 'Timestamp when the event was processed by Omikuji';
COMMENT ON COLUMN processed_events.event_data IS 'Optional JSON data about the event for debugging/auditing';