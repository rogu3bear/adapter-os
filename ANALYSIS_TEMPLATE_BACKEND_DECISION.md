# Agent 21: Prompt Template Service Backend Analysis

**Analysis Date:** 2025-11-19
**Decision:** Backend API required for multi-user/tenant architecture
**Status:** Ready for implementation

---

## Executive Summary

The current prompt template management implementation uses **localStorage only** (Agents 14-15). The AdapterOS architecture is explicitly **multi-tenant and multi-user** with RBAC (role-based access control). Therefore, **localStorage is insufficient** and a backend API is required.

### Key Findings

1. **Multi-tenant Architecture:** Database schema includes explicit `tenant_id` foreign keys across all tables (users, adapters, policies, etc.)
2. **RBAC System:** 5 roles (Admin, Operator, SRE, Compliance, Viewer) with 20+ granular permissions
3. **Audit Logging:** All operations logged with user/action/resource/status for compliance
4. **Current Gap:** localStorage is single-device, non-persistent across sessions, not shareable between users
5. **Sharing Need:** Team collaboration requires server-side storage with permission boundaries

---

## Current Implementation Analysis

### Frontend Implementation (Working)
- **Hook:** `usePromptTemplates` in `/Users/star/Dev/aos/ui/src/hooks/usePromptTemplates.ts`
- **Component:** `PromptTemplateManager` in `/Users/star/Dev/aos/ui/src/components/PromptTemplateManager.tsx`
- **New Component:** `PromptTemplateManagerNew` in `/Users/star/Dev/aos/ui/src/components/inference/PromptTemplateManagerNew.tsx`

**Features:**
- CRUD operations (create, read, update, delete)
- Variable detection (`{{variable}}` syntax)
- Template categorization (code-review, documentation, testing, debugging, refactoring, custom)
- Search and filtering
- Import/export to JSON
- Favorites tracking
- Recent templates
- Built-in templates (10 templates)

**Storage Mechanism:**
```typescript
const STORAGE_KEY = 'aos_prompt_templates';
const RECENT_TEMPLATES_KEY = 'aos_recent_templates';
localStorage.getItem(STORAGE_KEY) // Retrieve
localStorage.setItem(STORAGE_KEY, JSON.stringify(data)) // Persist
```

**Limitations:**
- Only 5MB storage quota per domain (localStorage)
- Data lost on browser clear or uninstall
- No cross-device access
- No team sharing
- No version history
- No audit trail
- No permission boundaries
- Data format hard to backup/restore

---

## Architecture Context

### Multi-tenant Enforcement

The database schema enforces multi-tenancy at all levels:

```sql
-- From migrations/0001_init.sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    itar_flag INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- All adapter data isolated per tenant
);

CREATE TABLE audit_logs (
    -- Implicit: user_id, action, resource, status, timestamp
    -- All operations logged for compliance
);
```

### RBAC System

From `/Users/star/Dev/aos/crates/adapteros-server-api/src/permissions.rs`:

```rust
pub enum Permission {
    AdapterList, AdapterView, AdapterRegister, AdapterDelete, // ... 20+ more
}

// Role-based permission matrix:
// Admin - Full access
// Operator - Adapters, training, inference (not delete/tenant/policy)
// SRE - Infrastructure debug, audit access
// Compliance - Audit-only read
// Viewer - Read-only
```

### User Authentication

The app has:
- JWT-based authentication (Ed25519, 8hr TTL)
- Login page with dev bypass option (`/login`)
- User context in `useAuth()` hook
- Role-based route guards

---

## Decision: Backend API Required

### Rationale

**1. Multi-tenant Isolation (Critical)**
- Templates are user-created assets that need tenant boundaries
- Cannot expose one user's templates to another tenant
- localStorage mixes all data globally—no isolation possible

**2. Team Collaboration**
- Current implementation: Single-user per device
- Business requirement: Share templates between team members
- Solution: Server-side storage with explicit sharing/permissions

**3. Data Persistence & Availability**
- localStorage is ephemeral and single-device
- Server storage: persistent, accessible from any device, backed up
- Enterprise requirement: Templates as organizational knowledge assets

**4. Audit & Compliance**
- CLAUDE.md specifies: "Audit trail with quality thresholds"
- Templates are governance artifacts (standardize prompt patterns)
- Need: Who created/modified/deleted which template, when, why

**5. Integration with REST API**
- Project standardized on REST endpoints: `/api/*`
- Templates should follow same pattern as adapters, datasets, jobs
- Consistency with existing handlers (adapters.rs, datasets.rs, etc.)

**6. Permission-based Access**
- Some roles should be read-only (Compliance, Viewer)
- Some should manage (Admin, Operator)
- Can't enforce this in localStorage

---

## API Design

### Endpoints

```
# List templates (with tenant isolation)
GET    /v1/templates
       ?tenant_id={id}&category={cat}&search={q}&limit=50&offset=0
       Permission: TemplateView

# Create template
POST   /v1/templates
       { name, description, content, category, is_public }
       Permission: TemplateCreate
       Returns: { id, created_at, ... }

# Get single template
GET    /v1/templates/:id
       Permission: TemplateView

# Update template
PUT    /v1/templates/:id
       { name, description, content, category }
       Permission: TemplateEdit
       Returns: { id, updated_at, version_number }

# Delete template
DELETE /v1/templates/:id
       Permission: TemplateDelete
       Returns: { status: "deleted" }

# Share template with other users/teams
POST   /v1/templates/:id/share
       { user_ids: [id1, id2], permission: "view|edit" }
       Permission: TemplateShare
       Returns: { id, shared_with: [...] }

# Export templates (custom collection)
GET    /v1/templates/export?ids=id1,id2,id3
       Returns: JSON file download

# Import templates
POST   /v1/templates/import
       file: JSON
       Permission: TemplateCreate

# Get categories
GET    /v1/templates/categories
       Returns: [{ id, label, icon }]

# Get template usage metrics
GET    /v1/templates/:id/usage
       Returns: { created_by, created_at, last_used, use_count, shared_with_count }
```

### Request/Response Models

```rust
// Request: Create/Update template
#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: String,
    pub content: String,
    pub category: String,
    pub is_public: Option<bool>,  // Default: false (private)
}

// Response: Template detail
#[derive(Serialize)]
pub struct TemplateResponse {
    pub id: String,
    pub tenant_id: String,
    pub created_by: String,  // user_id
    pub name: String,
    pub description: String,
    pub content: String,
    pub category: String,
    pub variables: Vec<String>,  // Auto-detected
    pub is_public: bool,
    pub is_built_in: bool,  // System templates
    pub created_at: String,
    pub updated_at: String,
    pub version_number: u32,
}

// Response: List templates
#[derive(Serialize)]
pub struct TemplatesListResponse {
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
    pub templates: Vec<TemplateResponse>,
}
```

### Database Schema

```sql
-- Templates table
CREATE TABLE templates (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_by TEXT NOT NULL REFERENCES users(id),
    updated_by TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL,
    category TEXT NOT NULL,  -- 'code-review', 'documentation', etc.
    variables_json TEXT,  -- JSON array of variable names
    is_public INTEGER NOT NULL DEFAULT 0,
    is_built_in INTEGER NOT NULL DEFAULT 0,
    version_number INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

-- Template sharing (who can access which templates)
CREATE TABLE template_sharing (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    shared_by TEXT NOT NULL REFERENCES users(id),
    shared_with_user_id TEXT REFERENCES users(id),
    permission TEXT NOT NULL CHECK(permission IN ('view', 'edit')),  -- view-only or can modify
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(template_id, shared_with_user_id)
);

-- Template usage audit trail
CREATE TABLE template_usage_logs (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    action TEXT NOT NULL CHECK(action IN ('create', 'update', 'delete', 'view', 'use', 'share')),
    changes_json TEXT,  -- Track what changed
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for common queries
CREATE INDEX idx_templates_tenant_id ON templates(tenant_id);
CREATE INDEX idx_templates_category ON templates(category);
CREATE INDEX idx_templates_created_by ON templates(created_by);
CREATE INDEX idx_templates_is_public ON templates(is_public);
CREATE INDEX idx_template_sharing_template_id ON template_sharing(template_id);
CREATE INDEX idx_template_sharing_user_id ON template_sharing(shared_with_user_id);
```

---

## Implementation Plan

### Step 1: Database Schema
1. Create migration files:
   - `NNNN_prompt_templates.sql` - Main template table + indexes
   - `NNNN_template_sharing.sql` - Sharing/permissions
   - `NNNN_template_usage_logs.sql` - Audit trail
2. Add database module in `crates/adapteros-db/src/templates.rs`
3. Implement CRUD functions in Db trait

### Step 2: Permissions
1. Add to `Permission` enum:
   - `TemplateView`
   - `TemplateCreate`
   - `TemplateEdit`
   - `TemplateDelete`
   - `TemplateShare`
2. Update permission matrix in `permissions.rs`
3. Recommend permissions:
   - Admin: All
   - Operator: View, Create, Edit own, Share
   - SRE: View, Audit
   - Compliance: View only
   - Viewer: View public only

### Step 3: REST Handlers
1. Create `crates/adapteros-server-api/src/handlers/templates.rs`
2. Implement handlers:
   - `list_templates()`
   - `create_template()`
   - `get_template()`
   - `update_template()`
   - `delete_template()`
   - `share_template()`
   - `export_templates()`
   - `import_templates()`
3. Add to routes in `routes.rs`

### Step 4: Frontend Migration
1. Add API client functions to `ui/src/api/client.ts`
2. Update `usePromptTemplates.ts` hook:
   - Replace localStorage with API calls
   - Add caching layer (optional: keep recent in cache)
   - Add error handling
   - Add loading states
3. Update `PromptTemplateManager.tsx`:
   - Add permission checks (disable buttons for restricted roles)
   - Show sharing UI for templates
   - Add version history view

### Step 5: Audit Integration
1. Log all template operations in `audit_logs` table
2. Include in audit dashboard
3. Sample queries:
   ```sql
   SELECT * FROM template_usage_logs
   WHERE template_id = ?
   ORDER BY created_at DESC
   LIMIT 50;
   ```

---

## Migration Strategy: localStorage → Server

### Phase 1: Dual-write (Backward Compatible)
```typescript
// Pseudo-code: Read from server, fallback to localStorage
const getTemplate = async (id: string) => {
  try {
    return await fetchFromServer(id);  // Try server first
  } catch {
    return getFromLocalStorage(id);    // Fallback to old storage
  }
};
```

### Phase 2: Data Migration
1. Export user's localStorage templates: `localStorage.getItem('aos_prompt_templates')`
2. Call `/v1/templates/import` with exported JSON
3. Verify count matches, then clear localStorage

### Phase 3: Server-only
1. Remove all localStorage code from hook
2. Add sync mechanism for offline support (optional):
   - IndexedDB cache for offline mode
   - Auto-sync when online

### Migration Flow

```
User clicks "Sync Templates"
  ↓
Export from localStorage → Parse JSON
  ↓
POST /v1/templates/import
  ↓
Backend validates + stores in DB with tenant_id
  ↓
Clear localStorage on success
  ↓
Reload page to verify server state
```

---

## Benefits of Backend API

| Aspect | localStorage | Backend API |
|--------|-------------|------------|
| **Tenant Isolation** | ❌ No | ✅ Yes (foreign key) |
| **User Persistence** | ❌ Single device | ✅ Any device |
| **Team Sharing** | ❌ No | ✅ Yes (sharing table) |
| **Audit Trail** | ❌ No | ✅ Yes (audit logs) |
| **Permission Boundaries** | ❌ No | ✅ Yes (role-based) |
| **Cross-browser sync** | ❌ No | ✅ Yes |
| **Backup & Recovery** | ⚠️ Manual export | ✅ Automatic |
| **Search Across Tenants** | ❌ No | ✅ Yes (admin only) |
| **Analytics** | ❌ No | ✅ Yes (usage stats) |
| **Version History** | ❌ No | ✅ Yes (future enhancement) |
| **Built-in Templates** | ✅ Embedded | ✅ Seeded in DB |
| **Data Size Limit** | 5MB | Unlimited |

---

## Decision Criteria Met

✅ **Multi-user app?** YES → Backend required
✅ **Multi-tenant isolation?** YES (explicit in schema) → Backend required
✅ **Team collaboration?** YES (organizational use case) → Backend required
✅ **Audit/compliance?** YES (CLAUDE.md requirement) → Backend required
✅ **Follows project patterns?** YES (REST API style) → Backend required

---

## Conclusion

**Decision: BACKEND API REQUIRED**

**Rationale:**
1. AdapterOS explicitly enforces multi-tenancy (tenant_id in all tables)
2. RBAC system requires permission boundaries (localStorage can't enforce)
3. Team collaboration requires shared access (impossible with localStorage)
4. Audit logging is mandatory (compliance requirement)
5. Project standardized on REST API endpoints (consistency)

**Next Steps:**
1. Create database migrations (templates, sharing, usage_logs tables)
2. Implement REST handlers following existing patterns (adapters.rs, datasets.rs)
3. Add permissions to RBAC matrix
4. Update frontend hook to use API instead of localStorage
5. Implement data migration strategy for existing users

**Estimated Scope:**
- Backend: 1-2 steps (migrations + handlers)
- Frontend: 1 step (API integration)
- Testing: 1 step (unit + integration tests)
- Migration: 1 step (data import tool)

---

## Appendix: Built-in Templates

The system should seed these templates in the database during initialization:

1. **Code Review** (category: code-review)
2. **Documentation Generator** (category: documentation)
3. **Unit Test Generator** (category: testing)
4. **Bug Analysis** (category: debugging)
5. **Refactoring Assistant** (category: refactoring)
6. **API Design Review** (category: code-review)
7. **Security Audit** (category: code-review)
8. **Performance Optimization** (category: refactoring)
9. **Integration Test Generator** (category: testing)
10. **Code Explanation** (category: documentation)

These are marked as `is_built_in = 1` and can be viewed but not deleted by regular users.

---

**File References:**
- Frontend hook: `/Users/star/Dev/aos/ui/src/hooks/usePromptTemplates.ts`
- Template manager: `/Users/star/Dev/aos/ui/src/components/PromptTemplateManager.tsx`
- New template manager: `/Users/star/Dev/aos/ui/src/components/inference/PromptTemplateManagerNew.tsx`
- Permissions system: `/Users/star/Dev/aos/crates/adapteros-server-api/src/permissions.rs`
- Database schema: `/Users/star/Dev/aos/migrations/0001_init.sql`
- RBAC docs: `/Users/star/Dev/aos/CLAUDE.md` (RBAC section)

**Prepared by:** Analysis Agent
**Status:** Ready for implementation sprint
