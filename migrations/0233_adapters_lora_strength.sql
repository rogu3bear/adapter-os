-- Migration: Add lora_strength column to adapters table
-- Replaces runtime patch: ensure_adapter_lora_strength_column()
--
-- This column controls the LoRA adapter strength during inference.
-- Default value 1.0 means full adapter contribution.

ALTER TABLE adapters ADD COLUMN lora_strength REAL DEFAULT 1.0;
UPDATE adapters SET lora_strength = 1.0 WHERE lora_strength IS NULL;
