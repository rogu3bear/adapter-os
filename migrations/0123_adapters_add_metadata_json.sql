-- Add metadata_json column to adapters table for model reference tracking
-- This column can store JSON metadata including base model references

ALTER TABLE adapters ADD COLUMN metadata_json TEXT;

