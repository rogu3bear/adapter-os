-- Migration: Add copy_bytes field to inference_trace_receipts
-- This tracks the number of bytes copied during inference for accounting purposes

ALTER TABLE inference_trace_receipts ADD COLUMN copy_bytes INTEGER;
