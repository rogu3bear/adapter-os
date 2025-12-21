-- Fix missing metadata_json column in document_collections table
-- The Rust code in collections.rs expects this column but migration 0094 didn't create it

ALTER TABLE document_collections ADD COLUMN metadata_json TEXT;
