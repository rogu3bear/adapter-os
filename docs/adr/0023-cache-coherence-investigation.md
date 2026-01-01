# ADR-0023: Cache Coherence Investigation

**Status:** Accepted
**Date:** 2026-01-01
**Context:** P1 "Unify cache coherence with generation tracking" investigation

## Decision

**Downgrade P1 to 3 targeted bug fixes.** The "unification" framing is incorrect - the codebase already has proper cache separation and existing coordination infrastructure.

## Findings

### What We Found

**13+ caches identified** with intentionally different invalidation strategies:
- KV attention cache (generation-based)
- Model handle cache (LRU + pinning)
- Prefix KV cache (content-addressed)
- Dashboard cache (TTL-based)
- HTTP ETag cache (browser-managed)
- etc.

**Different strategies are correct.** A KV attention cache (needs generation tracking for determinism) is fundamentally different from an HTTP ETag cache (needs TTL). Forcing unification would over-engineer.

### Existing Infrastructure (No Build Needed)

| Component | Location | Status |
|-----------|----------|--------|
| Generation counters | `adapter_hotswap.rs:40` | Exists |
| Broadcast channels | `state.rs:698-735` | Exists |
| Event bus | `event_bus.rs:39` | Exists |
| TTL-based caching | `caching.rs:186-304` | Exists |
| Logical clock | `logical_clock.rs:44` | Exists |

### Actual Bugs Found (3)

#### Bug 1: Delete Order Race in `adapters.rs:4100-4110`
```
SQL delete commits → concurrent read sees SQL empty → falls back to KV → gets stale data → KV delete happens too late
```
**Fix:** Delete from KV first, then SQL.

#### Bug 2: Lock Orphan Race in `idempotency/store.rs:140-143`
```
cleanup_expired() can remove locks between cache entry creation and lock insertion
```
**Fix:** Create lock before cache entry is visible, or use atomic entry creation.

#### Bug 3: Read-Modify-Write Race in `adapters_kv.rs:581-597`
```
get() → modify state → update() has no synchronization, causing lost updates
```
**Fix:** Add optimistic locking with version field, or use atomic update.

## Consequences

1. Close P1 "Unify cache coherence" as **not needed**
2. Create 3 P2 bugs for the actual race conditions
3. No new "CacheCoordinator" abstraction required
4. Existing infrastructure is sufficient

## References

- `crates/adapteros-lora-worker/src/kvcache.rs` - Generation tracking already exists
- `crates/adapteros-server-api/src/state.rs` - Broadcast channels already exist
- `crates/adapteros-server-api/src/event_bus.rs` - Event coordination already exists
