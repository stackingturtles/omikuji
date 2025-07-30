-- Create omikuji schema if it doesn't exist
-- This migration must run before all other migrations
CREATE SCHEMA IF NOT EXISTS omikuji;

-- Set search_path to include omikuji schema
-- This ensures that all subsequent migrations create tables in the omikuji schema
SET search_path TO omikuji, public;

-- Add comment to schema
COMMENT ON SCHEMA omikuji IS 'Schema for Omikuji datafeed provider tables';