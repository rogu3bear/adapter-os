-- Migration: Workspaces and Messaging
-- Adds workspace resource containers, messaging, notifications, and activity tracking
--
-- Citation: Database patterns from migrations/0001_init.sql and migrations/0014_contacts_and_streams.sql
--
-- This migration enables:
-- 1. Workspace resource containers with cross-tenant collaboration
-- 2. Workspace-scoped messaging (no direct tenant-to-tenant messaging)
-- 3. Unified notification center (system alerts, messages, mentions, activity)
-- 4. Activity tracking (user actions and collaboration events)
--
-- All features maintain tenant isolation while enabling cross-tenant collaboration

-- Workspaces table: resource containers with permission layers
-- Workspaces allow tenants to share resources (adapters, nodes, models) without organizational hierarchy
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    created_by TEXT NOT NULL,  -- user_id
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_workspaces_created_by ON workspaces(created_by);
CREATE INDEX IF NOT EXISTS idx_workspaces_created_at ON workspaces(created_at);

-- Workspace members table: tenant/user membership in workspaces
-- Enables cross-tenant collaboration while maintaining tenant ownership of resources
CREATE TABLE IF NOT EXISTS workspace_members (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    user_id TEXT,  -- NULL means entire tenant is member, specific user_id means individual user
    role TEXT NOT NULL CHECK(role IN ('owner', 'member', 'viewer')),
    permissions_json TEXT,  -- JSON array of permissions: ["read", "write", "execute"]
    added_by TEXT NOT NULL,  -- user_id who added this member
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (added_by) REFERENCES users(id) ON DELETE RESTRICT,
    UNIQUE(workspace_id, tenant_id, user_id)  -- Can't add same user twice
);

CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace ON workspace_members(workspace_id);
CREATE INDEX IF NOT EXISTS idx_workspace_members_tenant ON workspace_members(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workspace_members_user ON workspace_members(user_id);
CREATE INDEX IF NOT EXISTS idx_workspace_members_role ON workspace_members(role);

-- Workspace resources table: shared resources (adapters, nodes, models) in workspaces
-- Resources remain tenant-owned; workspace grants visibility/access
CREATE TABLE IF NOT EXISTS workspace_resources (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT NOT NULL,
    resource_type TEXT NOT NULL CHECK(resource_type IN ('adapter', 'node', 'model')),
    resource_id TEXT NOT NULL,  -- References adapters.id, nodes.id, or models.id
    shared_by TEXT NOT NULL,  -- user_id who shared the resource
    shared_by_tenant_id TEXT NOT NULL,  -- tenant_id who owns the resource
    shared_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (shared_by) REFERENCES users(id) ON DELETE RESTRICT,
    FOREIGN KEY (shared_by_tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
    UNIQUE(workspace_id, resource_type, resource_id)  -- Can't share same resource twice to same workspace
);

CREATE INDEX IF NOT EXISTS idx_workspace_resources_workspace ON workspace_resources(workspace_id);
CREATE INDEX IF NOT EXISTS idx_workspace_resources_type_id ON workspace_resources(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_workspace_resources_tenant ON workspace_resources(shared_by_tenant_id);
CREATE INDEX IF NOT EXISTS idx_workspace_resources_shared_at ON workspace_resources(shared_at);

-- Messages table: workspace-scoped messaging (no direct tenant-to-tenant)
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT NOT NULL,
    from_user_id TEXT NOT NULL,
    from_tenant_id TEXT NOT NULL,
    content TEXT NOT NULL,
    thread_id TEXT,  -- NULL for top-level messages, message.id for replies
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    edited_at TEXT,  -- NULL if never edited
    
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (from_user_id) REFERENCES users(id) ON DELETE RESTRICT,
    FOREIGN KEY (from_tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
    FOREIGN KEY (thread_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_workspace ON messages(workspace_id);
CREATE INDEX IF NOT EXISTS idx_messages_from_user ON messages(from_user_id);
CREATE INDEX IF NOT EXISTS idx_messages_from_tenant ON messages(from_tenant_id);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at DESC);

-- Notifications table: unified notification center (system alerts, messages, mentions, activity)
CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL,  -- Notification recipient
    workspace_id TEXT,  -- NULL for system-wide notifications, workspace_id for workspace-scoped
    type TEXT NOT NULL CHECK(type IN ('alert', 'message', 'mention', 'activity', 'system')),
    target_type TEXT,  -- 'adapter', 'node', 'model', 'message', 'workspace', etc.
    target_id TEXT,  -- ID of the target resource
    title TEXT NOT NULL,
    content TEXT,
    read_at TEXT,  -- NULL if unread, timestamp when read
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_user_read ON notifications(user_id, read_at);
CREATE INDEX IF NOT EXISTS idx_notifications_workspace ON notifications(workspace_id);
CREATE INDEX IF NOT EXISTS idx_notifications_type ON notifications(type);
CREATE INDEX IF NOT EXISTS idx_notifications_target ON notifications(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at DESC);

-- Activity events table: user actions and collaboration events
-- Merged with telemetry events in UI to create unified activity feed
CREATE TABLE IF NOT EXISTS activity_events (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT,  -- NULL for tenant-wide activity, workspace_id for workspace-scoped
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN (
        'adapter_created', 'adapter_updated', 'adapter_deleted',
        'adapter_shared', 'adapter_unshared',
        'resource_shared', 'resource_unshared',
        'message_sent', 'message_edited',
        'user_mentioned', 'user_joined_workspace', 'user_left_workspace',
        'workspace_created', 'workspace_updated',
        'member_added', 'member_removed', 'member_role_changed'
    )),
    target_type TEXT,  -- 'adapter', 'node', 'model', 'message', 'workspace', 'user'
    target_id TEXT,  -- ID of the target resource
    metadata_json TEXT,  -- Additional context (JSON object)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_activity_events_workspace ON activity_events(workspace_id);
CREATE INDEX IF NOT EXISTS idx_activity_events_user ON activity_events(user_id);
CREATE INDEX IF NOT EXISTS idx_activity_events_tenant ON activity_events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_activity_events_type ON activity_events(event_type);
CREATE INDEX IF NOT EXISTS idx_activity_events_target ON activity_events(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_activity_events_created_at ON activity_events(created_at DESC);

-- View: Workspace summary with member and resource counts
CREATE VIEW IF NOT EXISTS workspace_summary AS
SELECT 
    w.id,
    w.name,
    w.description,
    w.created_by,
    w.created_at,
    w.updated_at,
    COUNT(DISTINCT wm.id) as member_count,
    COUNT(DISTINCT wr.id) as resource_count,
    COUNT(DISTINCT m.id) as message_count
FROM workspaces w
LEFT JOIN workspace_members wm ON w.id = wm.workspace_id
LEFT JOIN workspace_resources wr ON w.id = wr.workspace_id
LEFT JOIN messages m ON w.id = m.workspace_id
GROUP BY w.id;

-- View: Notification summary with unread counts
CREATE VIEW IF NOT EXISTS notification_summary AS
SELECT 
    user_id,
    workspace_id,
    COUNT(*) as total_count,
    COUNT(CASE WHEN read_at IS NULL THEN 1 END) as unread_count,
    MAX(created_at) as latest_notification_at
FROM notifications
GROUP BY user_id, workspace_id;

-- Trigger: Update workspace updated_at on member/resource changes
CREATE TRIGGER IF NOT EXISTS update_workspace_on_member_change
AFTER INSERT ON workspace_members
BEGIN
    UPDATE workspaces 
    SET updated_at = datetime('now')
    WHERE id = NEW.workspace_id;
END;

CREATE TRIGGER IF NOT EXISTS update_workspace_on_resource_change
AFTER INSERT ON workspace_resources
BEGIN
    UPDATE workspaces 
    SET updated_at = datetime('now')
    WHERE id = NEW.workspace_id;
END;

