-- Add last_scan column to git_repositories table
-- Evidence: migrations/0013_git_repository_integration.sql:1-17
-- Pattern: Database schema extension for git repository tracking

ALTER TABLE git_repositories ADD COLUMN last_scan TIMESTAMP;
