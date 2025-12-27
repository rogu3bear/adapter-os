-- Migration 0228: Add routing bias for models and MoE recommendation flag for adapters
-- Purpose: enable model-aware routing bias and MoE-specific adapter filtering
-- Dependencies: models (0001), adapters (0001)

-- Add routing_bias to models with default neutral bias
ALTER TABLE models ADD COLUMN routing_bias REAL NOT NULL DEFAULT 1.0;

-- Add recommended_for_moe to adapters; default to true to avoid unexpected penalties
ALTER TABLE adapters ADD COLUMN recommended_for_moe BOOLEAN NOT NULL DEFAULT 1;
