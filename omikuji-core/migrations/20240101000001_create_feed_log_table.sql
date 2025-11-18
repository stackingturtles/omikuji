-- Create feed_log table for storing historical feed values
CREATE TABLE IF NOT EXISTS feed_log (
    id SERIAL PRIMARY KEY,
    feed_name VARCHAR(255) NOT NULL,
    network_name VARCHAR(255) NOT NULL,
    feed_value DOUBLE PRECISION NOT NULL,
    feed_timestamp BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    error_status_code INTEGER,
    network_error BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for common queries
CREATE INDEX idx_feed_log_feed_name ON feed_log(feed_name);
CREATE INDEX idx_feed_log_network_name ON feed_log(network_name);
CREATE INDEX idx_feed_log_created_at ON feed_log(created_at);
CREATE INDEX idx_feed_log_feed_timestamp ON feed_log(feed_timestamp);
CREATE INDEX idx_feed_log_feed_network ON feed_log(feed_name, network_name);

-- Add a comment to the table
COMMENT ON TABLE feed_log IS 'Historical log of all feed values retrieved by Omikuji';
COMMENT ON COLUMN feed_log.id IS 'Auto-incrementing internal feed ID';
COMMENT ON COLUMN feed_log.feed_name IS 'Feed name as defined in config.yaml';
COMMENT ON COLUMN feed_log.network_name IS 'Network name for the feed';
COMMENT ON COLUMN feed_log.feed_value IS 'The value retrieved from the feed';
COMMENT ON COLUMN feed_log.feed_timestamp IS 'Timestamp as reported by the feed (Unix timestamp)';
COMMENT ON COLUMN feed_log.updated_at IS 'Timestamp when the system recorded the value';
COMMENT ON COLUMN feed_log.error_status_code IS 'HTTP status code if different from 200';
COMMENT ON COLUMN feed_log.network_error IS 'Whether there was a network error (no HTTP response)';
COMMENT ON COLUMN feed_log.created_at IS 'Timestamp when the record was created';