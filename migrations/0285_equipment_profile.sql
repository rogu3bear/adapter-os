-- Patent 3535886.0002 Compliance: Equipment Profile in Receipts (Claims 6, 9-10)
--
-- Adds equipment profile fields to inference_trace_receipts for cryptographic
-- binding of processor ID, MLX version, and ANE version to execution receipts.

-- Equipment profile digest (BLAKE3 of processor_id, mlx_version, ane_version, soc_id, metal_family)
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS equipment_profile_digest_b3 BYTEA;

-- Processor identifier (chip model + stepping/revision)
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS processor_id TEXT;

-- MLX framework version (e.g., "0.21.0")
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS mlx_version TEXT;

-- Apple Neural Engine version (e.g., "ANEv4-38core")
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS ane_version TEXT;

-- Citation binding (Patent 3535886.0002 Claim 6 enhancement)
-- Merkle root of all citation IDs used in this inference
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS citations_merkle_root_b3 BYTEA;

-- Count of citations for verification
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS citation_count INTEGER DEFAULT 0;

-- Index for equipment profile queries (useful for reproducibility verification)
CREATE INDEX IF NOT EXISTS idx_receipts_equipment_profile 
    ON inference_trace_receipts(equipment_profile_digest_b3) 
    WHERE equipment_profile_digest_b3 IS NOT NULL;
