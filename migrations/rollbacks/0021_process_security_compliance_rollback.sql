-- Rollback Migration 0021: Process Security and Compliance
-- Purpose: Reverse the creation of security, compliance, and access control tables
-- Author: Migration Rollback System
-- Date: 2025-11-19
--
-- Dependencies to handle:
-- - process_compliance_findings references process_compliance_assessments
-- - process_compliance_assessments references process_compliance_standards
-- - process_vulnerability_findings references process_vulnerability_scans
-- - process_security_audit_logs references process_security_policies
-- - process_access_controls may be referenced by other systems

-- Step 1: Drop dependent tables first (bottom-up dependency order)
DROP TABLE IF EXISTS process_compliance_findings;
DROP TABLE IF EXISTS process_vulnerability_findings;

-- Step 2: Drop the parent tables they reference
DROP TABLE IF EXISTS process_compliance_assessments;
DROP TABLE IF EXISTS process_vulnerability_scans;
DROP TABLE IF EXISTS process_security_audit_logs;
DROP TABLE IF EXISTS process_access_controls;

-- Step 3: Drop the standalone tables they reference
DROP TABLE IF EXISTS process_compliance_standards;
DROP TABLE IF EXISTS process_security_policies;
