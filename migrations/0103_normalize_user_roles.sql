-- Migration: Normalize user roles to lowercase
-- Fixes case mismatch between DB storage and Role::from_str parsing
-- Previously roles were stored as "Admin", "Operator", etc. but parser expects "admin", "operator"

UPDATE users SET role = LOWER(role) WHERE role != LOWER(role);

-- Also update any audit_logs that reference roles in metadata (if applicable)
-- This is a no-op if the column doesn't exist or data is already lowercase
