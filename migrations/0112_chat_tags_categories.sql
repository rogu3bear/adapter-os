-- Migration 0112: Chat Session Tags and Categories
-- Tenant-scoped tags, hierarchical categories with materialized path

-- Tags table (tenant-scoped)
CREATE TABLE IF NOT EXISTS chat_session_tags (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    UNIQUE(tenant_id, name)
);

CREATE INDEX idx_chat_session_tags_tenant ON chat_session_tags(tenant_id);
CREATE INDEX idx_chat_session_tags_name ON chat_session_tags(tenant_id, name);

-- Tag assignments (many-to-many)
CREATE TABLE IF NOT EXISTS chat_session_tag_assignments (
    session_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    assigned_at TEXT NOT NULL DEFAULT (datetime('now')),
    assigned_by TEXT,
    PRIMARY KEY (session_id, tag_id),
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES chat_session_tags(id) ON DELETE CASCADE
);

CREATE INDEX idx_tag_assignments_session ON chat_session_tag_assignments(session_id);
CREATE INDEX idx_tag_assignments_tag ON chat_session_tag_assignments(tag_id);

-- Hierarchical categories (materialized path pattern)
CREATE TABLE IF NOT EXISTS chat_session_categories (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    parent_id TEXT,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    depth INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    icon TEXT,
    color TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES chat_session_categories(id) ON DELETE CASCADE,
    UNIQUE(tenant_id, path)
);

CREATE INDEX idx_categories_tenant ON chat_session_categories(tenant_id);
CREATE INDEX idx_categories_parent ON chat_session_categories(parent_id);
CREATE INDEX idx_categories_path ON chat_session_categories(tenant_id, path);

-- Add category to sessions
ALTER TABLE chat_sessions ADD COLUMN category_id TEXT REFERENCES chat_session_categories(id) ON DELETE SET NULL;
CREATE INDEX idx_chat_sessions_category ON chat_sessions(category_id);

-- Trigger: Enforce max 5 levels
CREATE TRIGGER validate_category_depth
BEFORE INSERT ON chat_session_categories
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT depth FROM chat_session_categories WHERE id = NEW.parent_id) >= 4
        THEN RAISE(ABORT, 'Category depth cannot exceed 5 levels')
    END;
END;
