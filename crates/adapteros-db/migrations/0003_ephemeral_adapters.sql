-- Migration: Ephemeral Adapters table
-- Stores ephemeral adapters for commit-aware routing

CREATE TABLE IF NOT EXISTS ephemeral_adapters (
    id TEXT PRIMARY KEY NOT NULL,
    adapter_data TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ephemeral_adapters_created_at ON ephemeral_adapters(created_at);

