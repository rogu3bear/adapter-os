-- Rollback Migration 0048: Workspaces and Messaging
-- Purpose: Reverse the creation of workspace, messaging, and activity tracking tables
-- Author: Migration Rollback System
-- Date: 2025-11-19
--
-- Dependencies to handle:
-- - activity_events references workspaces
-- - notifications references workspaces
-- - messages references workspaces and threads (self-reference)
-- - workspace_resources references workspaces
-- - workspace_members references workspaces
-- - Views reference tables being dropped

-- Step 1: Drop dependent views first
DROP VIEW IF EXISTS notification_summary;
DROP VIEW IF EXISTS workspace_summary;

-- Step 2: Drop triggers before dropping tables
DROP TRIGGER IF EXISTS update_workspace_on_resource_change;
DROP TRIGGER IF EXISTS update_workspace_on_member_change;

-- Step 3: Drop tables with self or cross-references (leaf nodes)
DROP TABLE IF EXISTS activity_events;
DROP TABLE IF EXISTS notifications;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS workspace_resources;
DROP TABLE IF EXISTS workspace_members;

-- Step 4: Drop the parent workspace table
DROP TABLE IF EXISTS workspaces;
