-- Add lora_strength runtime scaling factor to adapters
-- Defaults to 1.0 to preserve existing behavior

ALTER TABLE adapters
    ADD COLUMN lora_strength REAL DEFAULT 1.0;

-- Backfill existing rows (SQLite treats new column default for future inserts only)
UPDATE adapters
SET lora_strength = 1.0
WHERE lora_strength IS NULL;

