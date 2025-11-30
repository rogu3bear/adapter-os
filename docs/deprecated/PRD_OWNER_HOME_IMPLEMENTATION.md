# PRD: Owner Home Implementation

**Document Version:** 1.0
**Created:** 2025-11-25
**Status:** Implementation Ready

---

## Overview

The Owner Home is a unified dashboard for System Owner / Root operator access, providing:
- System overview with health metrics
- Tenant and adapter stack summaries
- Embedded system chat (talk to AdapterOS about itself)
- In-browser CLI console for aosctl commands
- Model load/unload/download controls

This is NOT a replacement for existing dashboards - it's a composition layer that aggregates existing functionality into a single "god view" for system owners.

---

## Implementation Structure

### PRD-OH-01: Owner Home Route & Layout Composition

**Files to Create:**
```
ui/src/pages/OwnerHome/
├── index.tsx                    # Main export
├── OwnerHomePage.tsx            # Page component with layout
├── components/
│   ├── SystemHealthStrip.tsx    # Top strip with system info
│   ├── SystemOverviewCard.tsx   # Left column - system snapshot
│   ├── TenantsCard.tsx          # Left column - tenant summary
│   ├── StacksAdaptersCard.tsx   # Left column - stacks/adapters
│   ├── ActivityCard.tsx         # Center column - recent activity
│   ├── UsageCard.tsx            # Center column - usage stats
│   └── RightColumnTabs.tsx      # Right column - chat/CLI tabs
```

**Route Configuration:**
```typescript
// Add to ui/src/config/routes.ts
{
  path: '/owner',
  component: OwnerHomePage,
  requiresAuth: true,
  requiredRoles: ['admin'],  // System Owner = admin role
  navGroup: 'Home',
  navTitle: 'Owner Home',
  navIcon: Crown,
  navOrder: 0,  // First in nav
  skeletonVariant: 'dashboard',
  breadcrumb: 'Owner Home',
}
```

**Layout:**
```
┌────────────────────────────────────────────────────────────────┐
│ TOP STRIP: System Name | Version | Health Summary | Owner Badge │
├──────────────────┬───────────────────────┬─────────────────────┤
│  LEFT COLUMN     │   CENTER COLUMN       │  RIGHT COLUMN       │
│  (1/4 width)     │   (1/3 width)         │  (5/12 width)       │
│                  │                       │                     │
│  ┌─────────────┐ │  ┌─────────────────┐  │  ┌───────────────┐  │
│  │System       │ │  │Models &         │  │  │ [Chat] [CLI]  │  │
│  │Overview     │ │  │Adapters Control │  │  ├───────────────┤  │
│  │             │ │  │                 │  │  │               │  │
│  │→ /system    │ │  │Load/Unload/DL   │  │  │ Chat or CLI   │  │
│  └─────────────┘ │  │                 │  │  │ Content       │  │
│                  │  └─────────────────┘  │  │               │  │
│  ┌─────────────┐ │                       │  │               │  │
│  │Tenants      │ │  ┌─────────────────┐  │  │               │  │
│  │Summary      │ │  │Recent Activity  │  │  │               │  │
│  │             │ │  │                 │  │  │               │  │
│  │→ /admin/    │ │  │→ /reports       │  │  │               │  │
│  │  tenants    │ │  └─────────────────┘  │  │               │  │
│  └─────────────┘ │                       │  │               │  │
│                  │  ┌─────────────────┐  │  │               │  │
│  ┌─────────────┐ │  │Usage Snapshot   │  │  │               │  │
│  │Stacks &     │ │  │24h stats        │  │  │               │  │
│  │Adapters     │ │  │                 │  │  │               │  │
│  │             │ │  │→ /reports       │  │  │               │  │
│  │→ /adapters  │ │  └─────────────────┘  │  │               │  │
│  └─────────────┘ │                       │  └───────────────┘  │
└──────────────────┴───────────────────────┴─────────────────────┘
```

**API Dependencies (all existing):**
- `GET /v1/system/overview` - System health and resources
- `GET /healthz/all` - Component health status
- `GET /v1/tenants` - Tenant list
- `GET /v1/adapter-stacks` - Stack list
- `GET /v1/adapters` - Adapter list
- `GET /v1/metrics/current` - Current metrics snapshot
- `GET /v1/activity/feed` - Recent activity

---

### PRD-OH-02: Embedded System Chat

**Files to Create:**
```
ui/src/components/OwnerChat/
├── index.tsx
├── SystemChatWidget.tsx         # Main chat component
├── SystemChatContext.tsx        # Context for chat state
├── SuggestedCliBlock.tsx        # CLI suggestion display
└── hooks/
    └── useSystemChat.ts         # Chat API hook
```

**Backend Endpoint (new):**
```
POST /v1/chat/owner-system
Request:
{
  "messages": [{ "role": "user", "content": "..." }],
  "context": {
    "route": "/owner",
    "metrics_snapshot": {...},
    "user_role": "admin"
  }
}

Response:
{
  "response": "...",
  "suggested_cli": "aosctl status --verbose",
  "relevant_links": ["/system", "/adapters"]
}
```

**Chat Behavior:**
- Context injected per turn: route, metrics snapshot, user role
- Output: natural language + suggested CLI + links
- Uses `owner-stack` (system-steward adapter)
- Read-only: no system mutations from chat

**Stack Configuration:**
```
owner-stack:
  - system-steward-r001 (trained on system docs)
  policies:
    - egress: denied
    - determinism: enabled
    - tenant_data: metadata_only
```

---

### PRD-OH-03: Web CLI Console

**Files to Create:**
```
ui/src/components/OwnerCli/
├── index.tsx
├── CliConsole.tsx               # Terminal-like component
├── CliHistory.tsx               # Command history
├── CommandValidator.ts          # Client-side validation
└── hooks/
    └── useCliExecute.ts         # CLI execution hook
```

**Backend Endpoint (new):**
```
POST /v1/cli/owner-run
Request:
{
  "command": "aosctl status",
  "session_id": "uuid"
}

Response:
{
  "stdout": "...",
  "stderr": "...",
  "exit_code": 0,
  "duration_ms": 150
}
```

**Allowed Commands (whitelist):**
- `aosctl status`
- `aosctl adapters list`
- `aosctl adapters describe <id>`
- `aosctl models list`
- `aosctl models status`
- `aosctl tenant list`
- `aosctl stack list`
- `aosctl stack describe <name>`
- `aosctl logs <component>`
- `help` (shows available commands)

**Security:**
- Command whitelist validation (server-side)
- No pipes, redirects, or subshells
- Audit logging: all commands logged with user, command, exit code, duration
- Rate limiting: 60 commands/minute

---

### PRD-OH-04: Model Control Strip

**Files to Create:**
```
ui/src/components/ModelControl/
├── index.tsx
├── ModelControlPanel.tsx        # Main panel component
├── BaseModelTable.tsx           # Base models with actions
├── AdapterSummary.tsx           # Key adapters summary
├── ModelActions.tsx             # Load/unload/download buttons
└── hooks/
    └── useModelControl.ts       # Model operations hook
```

**API Dependencies:**
- `GET /v1/models` - List base models
- `POST /v1/models/:id/load` - Load model
- `POST /v1/models/:id/unload` - Unload model
- `GET /v1/models/:id/export` - Download model (new endpoint if needed)
- `GET /v1/adapters` - Adapter list
- `POST /v1/adapters/:id/pin` - Pin adapter
- `DELETE /v1/adapters/:id/pin` - Unpin adapter

**UI Components:**
```
┌────────────────────────────────────────────────────┐
│ Models & Adapters                         [Refresh]│
├────────────────────────────────────────────────────┤
│ BASE MODELS                                         │
│ ┌────────────────────────────────────────────────┐ │
│ │ Name          │ Size   │ Status  │ Actions    │ │
│ │ Qwen2.5-7B    │ 3.8GB  │ Loaded  │ [Unload]   │ │
│ │ Llama-3-8B    │ 4.2GB  │ Avail   │ [Load][DL] │ │
│ └────────────────────────────────────────────────┘ │
├────────────────────────────────────────────────────┤
│ KEY ADAPTERS                                        │
│ ┌────────────────────────────────────────────────┐ │
│ │ Name               │ State │ Pinned │ Actions  │ │
│ │ system-steward-r1  │ Hot   │ Yes    │ [Unpin]  │ │
│ │ rust-expert-r1     │ Warm  │ No     │ [Pin]    │ │
│ │ code-assistant-r2  │ Cold  │ No     │ [Pin]    │ │
│ └────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────┘
```

---

### PRD-OH-05: aosctl init + First-Run Experience

**CLI Changes:**
```rust
// crates/adapteros-cli/src/commands/init.rs

pub async fn init_command(config: InitConfig) -> Result<()> {
    // 1. Check environment (DB, storage, UDS)
    // 2. Run migrations
    // 3. Create default tenant
    // 4. Create owner user with secure auth
    // 5. Write config to ~/.aos/config.toml
    // 6. Print credentials and UI URL
}
```

**First-Run UI Logic:**
```typescript
// In App.tsx or AuthProvider
useEffect(() => {
  if (user?.role === 'admin' && user?.is_first_login) {
    navigate('/owner');
  }
}, [user]);
```

**Onboarding Strip (on Owner Home):**
```
┌────────────────────────────────────────────────────┐
│ Welcome to AdapterOS! Complete these steps:        │
│                                                     │
│ [ ] Import a base model      [Start →]             │
│ [ ] Register your first adapter  [Start →]        │
│ [ ] Run a demo inference     [Start →]            │
│                                                     │
│ Or ask in the System Chat →                         │
└────────────────────────────────────────────────────┘
```

---

## Implementation Order

1. **PRD-OH-01** (Phase 1 - 2 days)
   - Create route and page structure
   - Implement all summary cards
   - Right column with placeholder tabs

2. **PRD-OH-04** (Phase 2 - 1 day)
   - Model Control Panel (most direct value)
   - Uses existing endpoints

3. **PRD-OH-02** (Phase 3 - 2 days)
   - System Chat widget
   - New backend endpoint
   - Prompt engineering

4. **PRD-OH-03** (Phase 4 - 2 days)
   - CLI Console
   - Command runner backend
   - Audit integration

5. **PRD-OH-05** (Phase 5 - 1 day)
   - aosctl init enhancements
   - First-run redirect logic
   - Onboarding strip

---

## Testing Requirements

**Unit Tests:**
- Card components render with mock data
- API hooks handle loading/error states
- CLI command validation
- Chat message formatting

**Integration Tests:**
- Owner Home loads all data correctly
- Model load/unload operations
- CLI command execution flow
- First-run redirect

**E2E Tests:**
- Full owner workflow: login → dashboard → model load → inference
- CLI console command execution
- Chat interaction

---

## Success Metrics

- Owner can load/unload models in < 3 clicks
- CLI commands execute in < 500ms
- All system metrics visible on one page
- No hunting through menus for common operations
