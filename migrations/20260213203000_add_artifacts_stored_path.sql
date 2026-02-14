-- Add optional artifact storage path to support artifact persistence.
ALTER TABLE artifacts ADD COLUMN stored_path TEXT;
