# Terminology

AdapterOS uses a single user-facing term across the UI and documentation.

## Canonical Mapping

- **Workspace (UI)**: The name shown to users in the interface, guides, and UI copy.
- **Tenant (DB/internal)**: The internal name for the same concept; database column and internal fields use `tenant_id`.
- **API**: The canonical field name is `tenant_id`. Where supported, `workspace_id` is accepted as an alias for requests, but responses remain `tenant_id`.

## Practical Guidance

- UI components and copy must use **Workspace**.
- Internal identifiers, schema fields, and database columns remain **tenant**.
- When calling the API from UI code, map workspace identifiers to `tenant_id`.

---

## Adapter Domains

AdapterOS classifies adapters into three domain types, each with distinct lifecycle rules, storage patterns, and routing constraints.

### Domain Definitions

- **Core Adapter**: Baseline adapters (e.g., `adapteros.aos`) used as stable reference points. Core adapters serve as the delta base for codebase adapters. Rarely modified directly; versioned independently.

- **Codebase Adapter**: Stream-scoped adapters representing repository state combined with session context. Codebase adapters:
  - Must declare an explicit `base_adapter_id` pointing to a core adapter
  - Are bound exclusively to a single inference stream (one codebase adapter per stream)
  - Support auto-versioning when activation count exceeds threshold
  - Require deployment verification (repo clean state, manifest hash match)
  - Evolve over time and must be versioned when context grows

- **Portable Adapter** (Standard): General-purpose adapters packaged as `.aos` files that can be freely shared, loaded across streams/sessions, and registered from external sources. The default adapter type.

### Stream Binding

- **Stream-Bound Adapter**: An adapter tied to a specific inference stream. Currently, only codebase adapters support stream binding. A stream-bound adapter cannot serve requests in other streams simultaneously.

- **Inference Stream**: A session-scoped execution context where inference requests flow. Each stream has at most one codebase adapter bound to it at any time.

### Invariant Rule

> **One codebase adapter per inference stream.**

Enforced at the database level via unique constraint on `(stream_session_id, adapter_type='codebase', active=1)`.

### Freezing for Export

- **Frozen Adapter**: An adapter whose state has been locked for deterministic CoreML export. Freezing captures the adapter's weights, manifest, and configuration at a specific point in time. Frozen is a **property**, not a lifecycle state—an Active adapter can be frozen without changing its business state.

- **Frozen CoreML Package**: The output of the CoreML export pipeline for a frozen adapter. Contains fused weights and verification metadata (`adapteros_coreml_fusion.json`) with BLAKE3 hashes for the base manifest, fused manifest, and adapter payload.

### Adapter Domain Comparison

| Adapter Type | Can Be Shared | Stream Bound | Auto-Version | Requires Base |
|--------------|---------------|--------------|--------------|---------------|
| Core         | Yes           | No           | No           | No            |
| Codebase     | No            | Yes          | Yes          | Yes           |
| Portable     | Yes           | No           | No           | No            |

**Code References:**
- Adapter type enum: `crates/adapteros-core/src/adapter_type.rs`
- Database schema: `migrations/0261_codebase_adapter_type.sql`
- Session binding: `migrations/0262_session_codebase_binding.sql`
