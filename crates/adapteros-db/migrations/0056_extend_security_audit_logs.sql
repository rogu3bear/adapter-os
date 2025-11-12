-- Migration 0056: Extend process_security_audit_logs for hallucination tracking
-- Version: v0.1.0-db
-- Adds metric_type, metric_value, and context_json columns for hallucination tracking
-- Citation: MVP Database Protection plan

-- Add columns for hallucination metrics
ALTER TABLE process_security_audit_logs ADD COLUMN metric_type TEXT;
ALTER TABLE process_security_audit_logs ADD COLUMN metric_value REAL;
ALTER TABLE process_security_audit_logs ADD COLUMN context_json TEXT;

-- Create index for metric queries
CREATE INDEX IF NOT EXISTS idx_security_audit_metric_type ON process_security_audit_logs(metric_type);

