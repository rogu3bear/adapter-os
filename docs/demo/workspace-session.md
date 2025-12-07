# Workspace & Chat Session Experience

**Purpose:** This document describes the workspace and chat session features in AdapterOS, enabling users to interact with adapter stacks in a conversational interface with full traceability.

**Status:** Implemented (PRD-UX-01)
**Last Updated:** 2025-01-25

---

## Overview

The workspace and chat session experience makes "Workspace + Chat" the primary way users interact with AdapterOS. Users can:

1. Pick an adapter stack (or use the default)
2. Have a conversation with the system
3. See real-time traces of router decisions and adapter usage
4. Link sessions to training jobs and datasets
5. Debug and analyze system behavior through the UI

---

## Architecture

### Database Schema

The chat session system consists of three tables:

#### `chat_sessions`
Stores persistent chat sessions with stack context:
- `id` - Unique session identifier
- `tenant_id` - Tenant scope
- `user_id` - Optional user association
- `stack_id` - Linked adapter stack (optional)
- `name` - Human-readable session name
- `created_at` - Creation timestamp
- `last_activity_at` - Last message/activity timestamp
- `metadata_json` - Additional metadata

#### `chat_messages`
Stores individual messages within sessions:
- `id` - Unique message identifier
- `session_id` - Parent session
- `role` - Message role: 'user', 'assistant', 'system'
- `content` - Message text
- `timestamp` - Message timestamp
- `metadata_json` - Router decisions, evidence, etc.

#### `chat_session_traces`
Links sessions to system entities for traceability:
- `id` - Auto-incrementing ID
- `session_id` - Parent session
- `trace_type` - Type: 'router_decision', 'adapter', 'training_job', 'audit_event'
- `trace_id` - ID of the traced entity
- `created_at` - Link timestamp

### API Endpoints

All chat session endpoints require authentication and `InferenceExecute` permission.

#### Session Management

**Create Session**
```http
POST /v1/chat/sessions
Content-Type: application/json

{
  "name": "Code Review Session",
  "stack_id": "stack-123",
  "metadata_json": "{\"tags\": [\"review\", \"rust\"]}"
}
```

Response:
```json
{
  "session_id": "session-abc-123",
  "tenant_id": "default",
  "name": "Code Review Session",
  "created_at": "2025-01-25T14:30:00Z"
}
```

**List Sessions**
```http
GET /v1/chat/sessions?user_id=user-123&limit=50
```

Response: Array of `ChatSession` objects

**Get Session**
```http
GET /v1/chat/sessions/{session_id}
```

Response: Single `ChatSession` object

**Delete Session**
```http
DELETE /v1/chat/sessions/{session_id}
```

Response: 204 No Content

#### Message Management

**Add Message**
```http
POST /v1/chat/sessions/{session_id}/messages
Content-Type: application/json

{
  "role": "user",
  "content": "Explain this function",
  "metadata_json": "{\"code_context\": \"...\"}"
}
```

Response: Created `ChatMessage` object

**Get Messages**
```http
GET /v1/chat/sessions/{session_id}/messages?limit=100
```

Response: Array of `ChatMessage` objects ordered by timestamp

#### Session Summary

**Get Summary**
```http
GET /v1/chat/sessions/{session_id}/summary
```

Response:
```json
{
  "session_id": "session-abc-123",
  "tenant_id": "default",
  "stack_id": "stack-123",
  "name": "Code Review Session",
  "created_at": "2025-01-25T14:30:00Z",
  "last_activity_at": "2025-01-25T15:45:00Z",
  "message_count": 12,
  "trace_counts": {
    "adapter": 24,
    "router_decision": 48,
    "training_job": 0
  }
}
```

### Inference Integration

The `/v1/infer` endpoint now accepts optional `session_id` and `tenant_id` fields:

```http
POST /v1/infer
Content-Type: application/json

{
  "prompt": "Explain async Rust",
  "max_tokens": 200,
  "session_id": "session-abc-123",
  "adapter_stack": ["rust-expert", "code-assistant"]
}
```

When `session_id` is provided:
- Adapters used are automatically linked via `chat_session_traces`
- Session `last_activity_at` is updated
- Router decisions can be correlated for debugging

---

## UI Integration

### Chat Interface Component

The `ChatInterface` component (`ui/src/components/ChatInterface.tsx`) provides the primary chat experience:

**Features:**
- Tenant and stack selection at top
- Real-time chat area with message history
- Right-hand panel with session summary:
  - Active adapters
  - Router decision count
  - Quick links to training jobs
- Session list sidebar with recent conversations
- Auto-save to backend (when implemented)

**Current Implementation:**
- Sessions are stored in localStorage (per-tenant)
- Backend session API ready for integration
- Trace linkage automatic on inference

**Next Steps:**
1. Replace localStorage with backend API calls
2. Add real-time trace updates via SSE
3. Implement session search/filtering

### Context Panel

The "Currently Loaded" collapsible panel shows:
- Active adapter stack
- Stack classification (default/custom)
- Lifecycle state
- Adapter count and base model

---

## Usage Patterns

### Typical Workflow

1. **User logs in and navigates to Chat**
   ```
   → UI creates default session if none exists
   → Session linked to tenant's default stack
   ```

2. **User sends a message**
   ```
   → Message stored via POST /v1/chat/sessions/{id}/messages
   → Inference called with session_id
   → Router decisions and adapters automatically traced
   → Assistant response added to session
   ```

3. **User views session summary**
   ```
   → GET /v1/chat/sessions/{id}/summary
   → Shows message count, trace counts, active adapters
   → Links to router inspector for debugging
   ```

4. **User switches stacks**
   ```
   → New session created with new stack_id
   → Old session preserved in history
   → Traces isolated per session
   ```

### Debugging with Traces

When debugging adapter behavior:

1. Navigate to session summary
2. View trace counts by type
3. Click "Router Decisions" to see inspector
4. Filter by session_id to see only relevant decisions
5. Identify which adapters were used and why

### Training Correlation

When training a new adapter:

1. Create session with intent (e.g., "Train Rust expert")
2. Run training job (job_id returned)
3. Link job to session via `chat_session_traces`
4. Test adapter in same session
5. Compare before/after traces

---

## Implementation Details

### Database Methods

All methods are in `crates/adapteros-db/src/chat_sessions.rs`:

- `create_chat_session(params)` - Create new session
- `list_chat_sessions(tenant_id, user_id, source_type, document_id, limit)` - List sessions
- `get_chat_session(session_id)` - Get single session
- `update_session_activity(session_id)` - Update last_activity_at
- `add_chat_message(params)` - Add message to session
- `get_chat_messages(session_id, limit)` - Get session messages
- `add_session_trace(session_id, trace_type, trace_id)` - Link trace
- `get_session_traces(session_id)` - Get all traces
- `get_session_summary(session_id)` - Get summary with counts
- `delete_chat_session(session_id)` - Delete session (cascades)

### Inference Handler Integration

In `crates/adapteros-server-api/src/handlers.rs`, the `infer` handler:

```rust
// After successful inference
if let Some(session_id) = &req.session_id {
    // Link adapters used
    for adapter_id in &response.adapters_used {
        state.db.add_session_trace(session_id, "adapter", adapter_id).await?;
    }

    // Update session activity
    state.db.update_session_activity(session_id).await?;
}
```

### Permission Model

All chat session endpoints require:
- Valid JWT authentication
- `Permission::InferenceExecute` (granted to Operator, SRE, Admin)
- Tenant-scoped access (sessions isolated by tenant_id)

### Migration

Migration `0085_chat_sessions.sql` creates all three tables with:
- Foreign key constraints (CASCADE on delete)
- Indexes for performance (tenant_id, user_id, timestamp, trace lookups)
- ISO-8601 timestamps via SQLite `datetime('now')`

---

## Testing

### Manual Testing

```bash
# 1. Create session
curl -X POST http://localhost:8080/v1/chat/sessions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Test Session", "stack_id": "stack-1"}'

# 2. Add user message
curl -X POST http://localhost:8080/v1/chat/sessions/session-123/messages \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"role": "user", "content": "Hello"}'

# 3. Run inference with session
curl -X POST http://localhost:8080/v1/infer \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Explain Rust", "session_id": "session-123"}'

# 4. Get session summary
curl http://localhost:8080/v1/chat/sessions/session-123/summary \
  -H "Authorization: Bearer $TOKEN"
```

### Unit Tests

See `crates/adapteros-db/src/chat_sessions.rs` for database tests:
- `test_create_and_retrieve_session`
- `test_add_and_retrieve_messages`
- `test_session_traces`

---

## Future Enhancements

### Phase 2: Real-time Updates
- SSE stream for session updates
- Live router decision notifications
- Adapter hot-swap events

### Phase 3: Advanced Features
- Session templates (pre-configured stacks + context)
- Multi-user sessions (collaborative debugging)
- Session export/import for reproducibility
- Voice input/output integration

### Phase 4: Analytics
- Session analytics dashboard
- Adapter usage patterns per session
- Quality metrics by session type
- Cost tracking per session

---

## References

- [CLAUDE.md](../../CLAUDE.md) - AdapterOS developer guide
- [UI_INTEGRATION.md](../UI_INTEGRATION.md) - UI integration patterns
- [RBAC.md](../RBAC.md) - Permission model
- [ARCHITECTURE_PATTERNS.md](../ARCHITECTURE_PATTERNS.md) - System architecture

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-01-25
