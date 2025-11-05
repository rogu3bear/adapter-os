# Telemetry Endpoints Documentation

## Overview

The telemetry endpoints provide unified access to activity events from both the in-memory telemetry buffer and the persisted activity events database. This enables real-time dashboard updates while maintaining historical event records.

## Endpoints

### GET /v1/telemetry/events/recent

Returns recent activity events combining telemetry buffer entries with database activity events.

**Authentication:** Required (JWT via Authorization header, Cookie, or query parameter)

**Query Parameters:**
- `limit` (optional, default: 50, max: 200): Maximum number of events to return
- `event_types[]` (optional): Filter by event types (can specify multiple)

**Response:** Array of `ActivityEventResponse` objects, sorted by timestamp (newest first)

**Example:**
```bash
curl -H "Authorization: Bearer <token>" \
  "https://api.example.com/v1/telemetry/events/recent?limit=20&event_types[]=adapter_created"
```

**Implementation Details:**
- Queries both telemetry buffer (`limit * 2` events) and database (`limit * 2` events)
- Deduplicates by event ID
- Filters by tenant_id (from JWT claims)
- Filters by event_types if provided
- Sorts by timestamp descending
- Truncates to requested limit

**Location:** `crates/adapteros-server-api/src/handlers/telemetry.rs:372-404`

### GET /v1/telemetry/events/recent/stream

Server-Sent Events (SSE) stream of recent activity events with real-time updates.

**Authentication:** Required (JWT via Authorization header, Cookie, or query parameter)

**Query Parameters:**
- `limit` (optional, default: 50, max: 200): Initial backlog size
- `event_types[]` (optional): Filter by event types

**Response:** SSE stream with:
- Initial backlog of recent events
- Real-time updates as new events occur
- Keep-alive messages every 30 seconds

**Event Format:**
- Event type: `activity`
- Data: JSON object matching `ActivityEventResponse`

**Example:**
```javascript
const es = new EventSource('/v1/telemetry/events/recent/stream?limit=50&token=<jwt_token>');
es.addEventListener('activity', (event) => {
  const data = JSON.parse(event.data);
  console.log('Activity event:', data);
});
```

**Implementation Details:**
- Sends backlog first (via `load_recent_activity_events`)
- Chains with real-time stream from `telemetry_tx` broadcast channel
- Filters by tenant_id and event_types
- Deduplicates events
- Maintains connection with keep-alive

**Location:** `crates/adapteros-server-api/src/handlers/telemetry.rs:417-503`

## Architecture Patterns

### Event Merging Strategy

The endpoints merge events from two sources:

1. **Telemetry Buffer** (in-memory): Recent events (< 1000 events capacity)
   - Fast access
   - Includes real-time system events
   - Lost on server restart

2. **Activity Events Database** (persisted): Historical events
   - Permanent storage
   - Includes user actions, adapter lifecycle events
   - Survives server restarts

### Deduplication

Events are deduplicated by ID using a `HashSet<String>`. This prevents duplicate events when the same event exists in both the buffer and database.

### Filtering

- **Tenant Isolation:** All endpoints filter by `tenant_id` from JWT claims
- **Event Type Filtering:** Optional filtering by event type (case-insensitive)
- **Limit Enforcement:** Responses are truncated to the requested limit

### Authentication

All endpoints support multiple authentication methods:
- **Authorization Header:** `Bearer <token>`
- **Cookie:** `auth_token=<token>`
- **Query Parameter:** `?token=<token>` (useful for SSE streams)

The authentication middleware (`auth_middleware`) checks all three methods in order.

**Location:** `crates/adapteros-server-api/src/middleware.rs:58-158`

## Usage in Frontend

### React Hook: `useActivityFeed`

The frontend uses a custom hook that:
- Fetches initial events from REST endpoint
- Connects to SSE stream for real-time updates
- Falls back to polling if SSE fails
- Deduplicates and sorts events
- Maintains maximum event count

**Location:** `ui/src/hooks/useActivityFeed.ts`

### Example Usage

```typescript
const { events, loading, error } = useActivityFeed({
  enabled: true,
  maxEvents: 50,
  useSSE: true,
  eventTypes: ['adapter_created', 'adapter_updated'],
});
```

## Related Endpoints

- `GET /v1/telemetry/events` - Legacy endpoint (telemetry buffer only)
- `GET /v1/activity` - Database activity events only
- `GET /v1/stream/telemetry` - Legacy SSE stream (telemetry buffer only)

## Testing

Integration tests are available in `tests/telemetry_endpoints.rs`:

```bash
cargo test --test telemetry_endpoints -- --ignored --nocapture
```

Tests verify:
- Event merging from buffer and database
- Event type filtering
- Tenant isolation
- SSE stream functionality
- Query parameter authentication

## Performance Considerations

- **Query Strategy:** Fetches `limit * 2` events from each source, then filters/deduplicates to requested limit
- **Caching:** Frontend components cache workspace activity data (30s TTL)
- **Debouncing:** WorkspaceCard debounces fetches by 300ms to avoid rapid refetches
- **Keep-Alive:** SSE streams send keep-alive every 30s to prevent connection timeouts

## Policy Compliance

- **Policy Pack #9 (Telemetry):** Events logged with canonical JSON structure
- **Policy Pack #1 (Egress):** Uses relative API paths only
- **Tenant Isolation:** All events filtered by tenant_id from JWT claims

---

MLNavigator Inc 2025-01-15.

