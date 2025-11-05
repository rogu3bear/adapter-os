-- Migration: Align Base Schema with Production Features
-- Purpose: Add production columns to base tables from 0001
-- Resolves: Schema conflicts between 0001_init.sql and 0030_cab_promotion_workflow.sql
-- Policy Compliance: Determinism Ruleset (#2), Build & Release Ruleset (#15)

-- Add cpid column to plans table (CAB workflow requirement)
-- This enables Control Plane ID tracking for promotion workflows
ALTER TABLE plans ADD COLUMN cpid TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_plans_cpid_unique 
    ON plans(cpid) WHERE cpid IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_plans_cpid ON plans(cpid);

-- Extend cp_pointers table for production workflow
-- These columns support rollback and approval tracking
ALTER TABLE cp_pointers ADD COLUMN active_cpid TEXT;
ALTER TABLE cp_pointers ADD COLUMN before_cpid TEXT;
ALTER TABLE cp_pointers ADD COLUMN approval_signature TEXT;

-- Note: plan_id column remains for backward compatibility
-- active_cpid is the new primary reference for production deployments

-- Extend artifacts table for production features
-- Adds type classification and content hashing for SBOM and signatures
-- Note: artifacts table in 0001 uses hash_b3 as primary key, no cpid
-- Migration 0030 creates a separate artifacts table with cpid for CAB workflow
ALTER TABLE artifacts ADD COLUMN artifact_type TEXT;
ALTER TABLE artifacts ADD COLUMN content_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_artifacts_type 
    ON artifacts(artifact_type);

-- Migration metadata
-- Applied: Post-integration schema alignment
-- Version: 0040
-- Dependencies: 0001_init.sql, 0030_cab_promotion_workflow.sql

