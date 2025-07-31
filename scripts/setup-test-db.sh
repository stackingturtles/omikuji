#!/bin/bash
# Script to set up test database for Omikuji

set -e

# Configuration
TEST_DB_NAME="omikuji_test"
TEST_DB_USER="omikuji"
TEST_DB_PASSWORD="omikuji_password"
TEST_DB_HOST="localhost"
TEST_DB_PORT="5433"

echo "=== Setting up Omikuji test database ==="

# Export DATABASE_URL for test environment
export DATABASE_URL="postgresql://${TEST_DB_USER}:${TEST_DB_PASSWORD}@${TEST_DB_HOST}:${TEST_DB_PORT}/${TEST_DB_NAME}"

# Check if PostgreSQL container is running
if ! docker ps | grep -q omikuji-postgres; then
    echo "Error: PostgreSQL container 'omikuji-postgres' is not running"
    echo "Please start it with: docker-compose up -d postgres"
    exit 1
fi

# Drop existing test database if it exists
echo "Dropping existing test database (if exists)..."
docker exec omikuji-postgres psql -U omikuji -d omikuji_db -c "DROP DATABASE IF EXISTS ${TEST_DB_NAME};" 2>/dev/null || true

# Create fresh test database
echo "Creating test database..."
docker exec omikuji-postgres psql -U omikuji -d omikuji_db -c "CREATE DATABASE ${TEST_DB_NAME};"

# Check if sqlx-cli is installed
if ! command -v sqlx &> /dev/null; then
    echo "Warning: sqlx-cli is not installed. Installing it now..."
    cargo install sqlx-cli --no-default-features --features postgres
fi

# Apply a patched version of the problematic migration
echo "Applying migration workaround..."

# First, create a temporary migrations directory
TEMP_MIGRATIONS_DIR="/tmp/omikuji_test_migrations_$$"
mkdir -p "$TEMP_MIGRATIONS_DIR"

# Copy all migrations to temp directory
cp -r migrations/* "$TEMP_MIGRATIONS_DIR/"

# Patch the problematic migration to avoid duplicate view creation
# We'll modify migration 20240622000001 to drop the view first
if [ -f "$TEMP_MIGRATIONS_DIR/20240622000001_create_gas_price_tables.sql" ]; then
    # Create a patched version that drops the view first
    cat > "$TEMP_MIGRATIONS_DIR/20240622000001_create_gas_price_tables.sql" << 'EOF'
-- Drop the old daily_gas_costs view if it exists (created by earlier migration)
DROP VIEW IF EXISTS daily_gas_costs CASCADE;

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

-- View for daily gas costs aggregation (newer version replacing the old one)
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
EOF
fi

# Run migrations using the patched directory
echo "Running migrations on test database..."
DATABASE_URL="${DATABASE_URL}" sqlx migrate run --source "$TEMP_MIGRATIONS_DIR"

# Clean up temp directory
rm -rf "$TEMP_MIGRATIONS_DIR"

echo ""
echo "✓ Test database setup complete!"
echo "DATABASE_URL=${DATABASE_URL}"
echo ""
echo "You can now run tests with:"
echo "  DATABASE_URL=\"${DATABASE_URL}\" cargo test"