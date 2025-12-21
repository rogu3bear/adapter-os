-- Migration: Contacts and Streaming Support
-- Adds contacts table and interaction tracking for inference-driven discovery
--
-- Citation: Database patterns from migrations/0001_initial.sql
--
-- This migration enables:
-- 1. Inference-driven contact discovery (§5.1 Signal Protocol)
-- 2. Real-time streaming support via SSE
-- 3. Tenant-isolated contact management

-- Contacts table: inference-discovered entities
-- Citation: Schema patterns from migrations/0001_initial.sql lines 1-150
CREATE TABLE IF NOT EXISTS contacts (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    
    -- Contact identity
    name TEXT NOT NULL,
    email TEXT,
    category TEXT NOT NULL CHECK(category IN (
        'user',          -- Human users
        'system',        -- System accounts
        'adapter',       -- LoRA adapters
        'repository',    -- Code repositories
        'external'       -- External APIs/services
    )),
    
    -- Metadata
    role TEXT,                    -- e.g., "developer", "maintainer", "api_service"
    metadata_json TEXT,           -- Arbitrary JSON metadata
    avatar_url TEXT,
    
    -- Interaction tracking
    discovered_at TEXT NOT NULL DEFAULT (datetime('now')),
    discovered_by TEXT,           -- Trace ID of discovery inference
    last_interaction TEXT,
    interaction_count INTEGER DEFAULT 0,
    
    -- Permissions (for System/External contacts)
    permissions_json TEXT,        -- ["read", "write", "execute"]
    
    -- Audit
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    UNIQUE(tenant_id, name, category)
);

CREATE INDEX IF NOT EXISTS idx_contacts_tenant ON contacts(tenant_id);
CREATE INDEX IF NOT EXISTS idx_contacts_category ON contacts(category);
CREATE INDEX IF NOT EXISTS idx_contacts_discovered_at ON contacts(discovered_at);
CREATE INDEX IF NOT EXISTS idx_contacts_name ON contacts(name);
CREATE INDEX IF NOT EXISTS idx_contacts_last_interaction ON contacts(last_interaction);

-- Contact interactions table: log every inference that mentions a contact
-- Citation: Audit pattern from migrations/0001_initial.sql
CREATE TABLE IF NOT EXISTS contact_interactions (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    contact_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    cpid TEXT NOT NULL,
    interaction_type TEXT NOT NULL CHECK(interaction_type IN (
        'mentioned',      -- Contact mentioned in prompt/response
        'invoked',        -- Contact's capability invoked (adapter, API)
        'queried',        -- Database query about contact
        'updated'         -- Contact metadata updated
    )),
    context_json TEXT,    -- Additional context
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_interactions_contact ON contact_interactions(contact_id);
CREATE INDEX IF NOT EXISTS idx_contact_interactions_trace ON contact_interactions(trace_id);
CREATE INDEX IF NOT EXISTS idx_contact_interactions_created ON contact_interactions(created_at);
CREATE INDEX IF NOT EXISTS idx_contact_interactions_cpid ON contact_interactions(cpid);

-- Stream subscriptions table: track active SSE connections
-- Enables cleanup of stale connections and subscription management
CREATE TABLE IF NOT EXISTS stream_subscriptions (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    stream_type TEXT NOT NULL CHECK(stream_type IN (
        'training',       -- Adapter lifecycle and metrics
        'discovery',      -- Repository scanning
        'contacts'        -- Contact updates
    )),
    filter_json TEXT,     -- Optional filters (e.g., {"repo_id": "acme/payments"})
    connection_id TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_activity TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_stream_subscriptions_tenant ON stream_subscriptions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_stream_subscriptions_type ON stream_subscriptions(stream_type);
CREATE INDEX IF NOT EXISTS idx_stream_subscriptions_connection ON stream_subscriptions(connection_id);
CREATE INDEX IF NOT EXISTS idx_stream_subscriptions_activity ON stream_subscriptions(last_activity);

-- View: Contact summary with interaction stats
-- Provides efficient querying for UI display
CREATE VIEW IF NOT EXISTS contact_summary AS
SELECT 
    c.id,
    c.tenant_id,
    c.name,
    c.email,
    c.category,
    c.role,
    c.discovered_at,
    c.last_interaction,
    c.interaction_count,
    COUNT(DISTINCT ci.id) as total_logged_interactions,
    MAX(ci.created_at) as latest_interaction_time
FROM contacts c
LEFT JOIN contact_interactions ci ON c.id = ci.contact_id
GROUP BY c.id;

-- Trigger: Update contact interaction count and last_interaction
CREATE TRIGGER IF NOT EXISTS update_contact_on_interaction
AFTER INSERT ON contact_interactions
BEGIN
    UPDATE contacts 
    SET 
        interaction_count = interaction_count + 1,
        last_interaction = NEW.created_at,
        updated_at = datetime('now')
    WHERE id = NEW.contact_id;
END;

-- Trigger: Cleanup stale stream subscriptions (older than 1 hour)
-- This prevents memory leaks from abandoned SSE connections
CREATE TRIGGER IF NOT EXISTS cleanup_stale_subscriptions
AFTER INSERT ON stream_subscriptions
BEGIN
    DELETE FROM stream_subscriptions
    WHERE last_activity < datetime('now', '-1 hour');
END;

