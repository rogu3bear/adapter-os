-- Add load_state column to adapters table to track runtime state
-- Values: 'cold' (not loaded), 'loading', 'warm' (loaded), 'unloading'

ALTER TABLE adapters ADD COLUMN load_state TEXT NOT NULL DEFAULT 'cold' 
  CHECK(load_state IN ('cold', 'loading', 'warm', 'unloading'));

-- Index for filtering by load state
CREATE INDEX IF NOT EXISTS idx_adapters_load_state ON adapters(load_state);

-- Add last_loaded_at timestamp
ALTER TABLE adapters ADD COLUMN last_loaded_at TEXT;

-- Note: memory_bytes column already added in migration 0012

