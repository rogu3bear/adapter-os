# Observability & Audit Trails Validation

This document captures validation findings for telemetry, tracing, and audit flows to ensure canonical JSON serialization and proper UI surfacing.

## Telemetry Canonical JSON

### Unified Event Schema
- All telemetry events use `UnifiedTelemetryEvent` structure with canonical JSON serialization
- Schema defined in `crates/adapteros-telemetry/src/unified_events.rs`
- Events serialized via `serde_json::to_string()` ensuring consistent formatting

### Event Structure
- Required fields: `id`, `timestamp`, `event_type`, `level`, `message`
- Optional fields: `component`, `tenant_id`, `user_id`, `metadata`, `trace_id`, `span_id`, `event_hash`
- ISO 8601 timestamps for deterministic ordering
- BLAKE3 event hashes for integrity verification

### Bundle Storage
- Telemetry bundles written as NDJSON (newline-delimited JSON)
- Per-event canonical JSON serialization ensures no formatting drift
- Bundle metadata includes event count, size, CPID, creation timestamp
- BLAKE3 hash of bundle contents for integrity

## Trace Correlation

### Request ID Propagation
- UI client generates deterministic request IDs via SHA-256 hash of method + path + body
- Request IDs sent in `X-Request-ID` header and validated on response
- Request IDs stored in local audit buffer (last 1000 requests)

### Trace ID Support
- Telemetry events support optional `trace_id` and `span_id` fields
- Trace buffer maintains in-memory trace storage with search capability
- Trace API endpoint: `/api/traces/:trace_id` for trace retrieval
- Trace search endpoint: `/api/traces/search` with query filters

### Distributed Tracing
- Trace IDs can be propagated across service boundaries
- Span IDs support hierarchical trace structure
- Logical clock used for deterministic ordering in trace bundles

## Audit Trail Persistence

### Database Storage
- Audit records stored in `audits` table with extended fields
- Audit API endpoints:
  - `GET /v1/audits` - List audits with filtering
  - `GET /v1/audits/export` - Export audit logs for compliance
- Audit records include: tenant_id, user_id, action, resource, timestamp, result

### UI Surfacing
- Audit dashboard component: `ui/src/components/AuditDashboard.tsx`
- Audit page: `ui/src/pages/AuditPage.tsx`
- Activity feed hook: `ui/src/hooks/useActivityFeed.ts` fetches audit logs
- Dashboard link to audit trails: `/audit` route

### Export Capabilities
- Audit logs exportable via `/v1/audits/export` endpoint
- Supports filtering by tenant, user, date range
- CSV export format for compliance reporting

## Telemetry Bundle Management

### Bundle Lifecycle
- Bundles created on-demand via `POST /v1/telemetry/bundles/generate`
- Bundle metadata exposed via `GET /v1/telemetry/bundles`
- Bundle export via `GET /v1/telemetry/bundles/:id/export`
- Bundle signature verification via `GET /v1/telemetry/bundles/:id/verify`
- Old bundle purging via `POST /v1/telemetry/bundles/purge`

### Bundle Streaming
- Broadcast channel for live telemetry streaming (`telemetry_tx`)
- Bundle update notifications via `telemetry_bundles_tx` channel
- UI can subscribe to real-time bundle events

## Evidence Tracker Integration

### Evidence Records
- Evidence tracker maintains append-only evidence log
- Records include: model provenance, router scores (Q15), kernel checks, seed hash
- Evidence records stored in telemetry bundles for audit

### Policy Decision Evidence
- Policy violations logged with evidence context
- Evidence includes: policy pack ID, violation details, remediation steps
- Evidence hash stored for integrity verification

## Telemetry Sampling

### Deterministic Sampling
- First N tokens logged fully (default: 128 per Telemetry Ruleset #9)
- After threshold, deterministic 5% sampling via BLAKE3(seed || token_count)
- Security events always logged at 100% sampling
- Policy violations always logged at 100% sampling

### Router Decision Logging
- Router decisions logged with Q15 quantized gates
- Adapter IDs, gate values, entropy tracked per decision
- Token index included for temporal correlation

## Validation Status

✅ Canonical JSON: All events serialized via serde_json  
✅ Trace correlation: Request IDs and trace IDs supported  
✅ Audit persistence: Database storage with export capabilities  
✅ UI surfacing: Audit dashboard and telemetry bundle UI  
✅ Bundle management: Full lifecycle API endpoints  
✅ Evidence integration: Policy decisions tracked with evidence  
✅ Deterministic sampling: BLAKE3-based sampling for telemetry  

## Recommendations

1. Add trace ID generation middleware to automatically assign trace IDs to requests
2. Implement span context propagation across worker UDS boundaries
3. Add telemetry bundle retention policy enforcement
4. Surface telemetry bundle verification status in UI
5. Add audit log retention policy configuration
6. Implement audit log encryption for compliance requirements

