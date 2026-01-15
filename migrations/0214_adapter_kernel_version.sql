-- Migration: Add kernel_version to adapters table
-- This enables exact-match validation at adapter load time (PRD-RECT-005)
--
-- kernel_version: The adapterOS kernel version this adapter was packaged with.
-- Must exactly match runtime version for loading.

ALTER TABLE adapters ADD COLUMN kernel_version TEXT;

-- Index for efficient queries by kernel version
CREATE INDEX IF NOT EXISTS idx_adapters_kernel_version
    ON adapters(kernel_version) WHERE kernel_version IS NOT NULL;
