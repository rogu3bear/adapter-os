-- Migration: Tutorial Statuses
-- Adds tutorial_statuses table for tracking user tutorial completion and dismissal
--
-- Citation: Database patterns from migrations/0048_workspaces_and_messaging.sql
--
-- This migration enables:
-- 1. Persistence of tutorial completion status per user
-- 2. Persistence of tutorial dismissal status per user
-- 3. Cross-device synchronization of tutorial progress

-- Tutorial statuses table: tracks user tutorial completion and dismissal
CREATE TABLE IF NOT EXISTS tutorial_statuses (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL,
    tutorial_id TEXT NOT NULL,
    completed_at TEXT,
    dismissed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE(user_id, tutorial_id)
);

CREATE INDEX IF NOT EXISTS idx_tutorial_statuses_user_id ON tutorial_statuses(user_id);
CREATE INDEX IF NOT EXISTS idx_tutorial_statuses_tutorial_id ON tutorial_statuses(tutorial_id);
CREATE INDEX IF NOT EXISTS idx_tutorial_statuses_completed_at ON tutorial_statuses(completed_at);
CREATE INDEX IF NOT EXISTS idx_tutorial_statuses_dismissed_at ON tutorial_statuses(dismissed_at);

