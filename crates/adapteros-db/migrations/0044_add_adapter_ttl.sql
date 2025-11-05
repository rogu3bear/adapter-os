-- Add expires_at column to adapters table for ephemeral adapter TTL
ALTER TABLE adapters ADD COLUMN expires_at DATETIME;
