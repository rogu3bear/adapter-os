-- Migration 0266: Remove MoE (Mixture of Experts) related columns
-- Purpose: MoE routing has been removed in favor of dense-only K-sparse adapter routing
-- Dependencies: 0228 (added these columns)

-- SQLite doesn't support DROP COLUMN directly in older versions,
-- but modern SQLite (3.35+) does. These columns remain in the schema
-- but are no longer used by the application.

-- For backwards compatibility, we mark this migration as complete
-- but leave the columns in place as they have no impact on the dense-only routing.

-- Note: The recommended_for_moe column on adapters and routing_bias on models
-- are now legacy columns. They can be safely ignored by the application.

-- No schema changes needed - columns remain as legacy data
SELECT 1;
