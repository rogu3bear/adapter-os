# Adapter Pinning & TTL Reference

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22
**Purpose:** Reference documentation for adapter pinning and TTL enforcement

---

## Overview

The pinning system prevents critical adapters from being evicted or deleted, while TTL (Time-To-Live) enforcement automatically cleans up ephemeral/temporary adapters.

---

## Pinning System

### Purpose

Prevent critical adapters from being evicted or deleted during memory pressure or cleanup operations.

### API Usage

```rust
use adapteros_db::Db;

// Pin adapter (optional TTL)
db.pin_adapter(
    tenant_id,
    adapter_id,
    Some("2025-12-31 23:59:59"),  // pinned_until
    "production-critical",         // reason
    Some("ops@example.com")        // pinned_by
).await?;

// Unpin adapter
db.unpin_adapter(tenant_id, adapter_id).await?;

// Check pin status
let is_pinned = db.is_pinned(tenant_id, adapter_id).await?;
```

### REST API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/adapters/:adapter_id/pin` | Get pin status |
| POST | `/v1/adapters/:adapter_id/pin` | Pin adapter |
| DELETE | `/v1/adapters/:adapter_id/pin` | Unpin adapter |

### Database Schema

Table: `pinned_adapters` (Migration 0060)

| Column | Type | Description |
|--------|------|-------------|
| `tenant_id` | TEXT | Tenant identifier |
| `adapter_id` | TEXT | Adapter identifier |
| `pinned_at` | TEXT | Pin timestamp |
| `pinned_until` | TEXT | Expiration (NULL = permanent) |
| `reason` | TEXT | Pin justification |
| `pinned_by` | TEXT | User/system who pinned |

**View:** `active_pinned_adapters` - Returns only currently active pins.

---

## TTL (Time-To-Live) Enforcement

### Purpose

Automatic cleanup of ephemeral/temporary adapters that have exceeded their lifetime.

### Registration with TTL

```rust
use adapteros_db::AdapterRegistrationParams;

let params = AdapterRegistrationParams {
    adapter_id: "temp-adapter".to_string(),
    expires_at: Some("2025-02-15 23:59:59".to_string()),
    ..Default::default()
};
db.register_adapter_with_params(&params).await?;
```

### Three-Tier Enforcement

| Tier | Mechanism | Interval | Location |
|------|-----------|----------|----------|
| 1 | Database query | On-demand | `db.find_expired_adapters()` |
| 2 | Background cleanup | 5 minutes | `adapteros-server` cleanup loop |
| 3 | Lifecycle manager | Eviction priority | `adapteros-lora-lifecycle` |

### Eviction Order

When memory pressure occurs, adapters are evicted in this order:

1. **Expired TTL adapters** (first priority)
2. **Ephemeral TTL** adapters (lowest tier)
3. **Cold LRU** adapters (least recently used)
4. **Warm LRU** adapters (if still under pressure)

Pinned adapters are **never** evicted regardless of memory pressure.

---

## Lifecycle Integration

The pinning and TTL systems integrate with the adapter lifecycle state machine:

```
Unloaded → Cold → Warm → Hot → Resident
    ↑                          ↓
    └──── (eviction) ──────────┘
```

- **Pinned adapters**: Can be promoted to `Resident` state, immune to eviction
- **TTL adapters**: Automatically evicted when `expires_at` passes
- **Heartbeat**: 5-minute timeout auto-resets stale adapters

---

## Related Documentation

- [CLAUDE.md](../CLAUDE.md) - Developer guide with pinning/TTL quick reference
- [LIFECYCLE.md](LIFECYCLE.md) - Complete lifecycle state machine
- [DATABASE_REFERENCE.md](DATABASE_REFERENCE.md) - Schema reference

---

*Citation: [source: CLAUDE.md L273-308]*
