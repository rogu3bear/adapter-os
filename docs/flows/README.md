# AdapterOS Flow Documentation

This directory contains detailed flow documentation for key system operations in AdapterOS.

## Purpose

These documents provide step-by-step technical references for debugging and understanding the complete execution path of critical operations. Each flow includes:
- Crate and module references
- Type signatures
- State transitions
- Telemetry events
- Implementation status (✅ Implemented, 🔧 Planned)

## Available Flows

| Flow | Description | Status |
|------|-------------|--------|
| **[Load](load.md)** | Adapter loading and state initialization | ✅ Implemented |
| **[Route](route.md)** | K-sparse adapter selection via Q15 gates | ✅ Implemented |
| **[Run](run.md)** | Deterministic execution and inference | ✅ Implemented |
| **[Record](record.md)** | Telemetry event capture and bundle signing | ✅ Implemented |
| **[Replay](replay.md)** | Event log replay and divergence detection | 🔧 Planned |

## Quick Navigation

```
Request Arrives
     ↓
  [LOAD] ──→ Adapter lifecycle state machine
     ↓
  [ROUTE] ──→ K-sparse gate scoring
     ↓
  [RUN] ────→ Deterministic executor
     ↓
  [RECORD] ─→ Telemetry bundle
     ↓
  [REPLAY] ─→ Verification (planned)
```

## Reading Guide

1. **Start with [Load](load.md)** - Understand how adapters enter the system
2. **Then [Route](route.md)** - See how adapters are selected for each request
3. **Follow [Run](run.md)** - Trace execution through the deterministic executor
4. **Review [Record](record.md)** - Learn how telemetry captures everything
5. **Check [Replay](replay.md)** - Understand verification capabilities (future)

## Related Documentation

- [ARCHITECTURE_INDEX.md](../ARCHITECTURE_INDEX.md) - Complete architecture overview
- [CLAUDE.md](../../CLAUDE.md) - Developer guide with code patterns
- [architecture/precision-diagrams.md](../architecture/PRECISION-DIAGRAMS.md) - Visual system diagrams

## Implementation Status Legend

- ✅ **Implemented** - Code exists, tested, production-ready
- 🔧 **Planned** - Designed but not yet implemented
- ⚠️ **Partial** - Partially implemented, see notes in flow doc
- 🚫 **Deprecated** - No longer used, see DEPRECATED_PATTERNS.md

## Documentation Accuracy

**Line Numbers**: Line numbers in flow diagrams are accurate as of 2025-11-18 but may drift as code evolves. They point to the correct module and general location. For exact current line numbers, use `grep` or code search.

**Test Names**: All test names are actual tests that exist in the codebase with exact file locations provided.

**Event Types**: All telemetry event types are real structs defined in the codebase (primarily `crates/adapteros-telemetry/src/events.rs` and `crates/adapteros-lora-lifecycle/src/lib.rs`).

---

**Last Updated**: 2025-11-18
**Maintained by**: James KC Auchterlonie
