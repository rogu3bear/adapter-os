---
status: resolved
trigger: "docs-architecture-drift: Planning and architecture documentation has drifted from the actual codebase over many milestones."
created: 2026-03-04T07:00:00Z
updated: 2026-03-04T07:45:00Z
---

## Current Focus

hypothesis: CONFIRMED and FIXED - Multiple documentation drift items found and corrected across all four docs
test: All sections verified against codebase
expecting: N/A - complete
next_action: Archive

## Symptoms

expected: CLAUDE.md, PROJECT.md, ROADMAP.md, STATE.md accurately describe current codebase
actual: After 47 phases of development, docs had drifted significantly
errors: No runtime errors - documentation accuracy drift
reproduction: Read docs, compare to code
started: Accumulated across milestones v1.0 through v1.1.17

## Eliminated

## Evidence

- timestamp: 2026-03-04T07:10:00Z
  checked: Crate layers table in CLAUDE.md
  found: 85 crate directories exist, but table only listed ~28. 57 crates were unlisted including significant ones (adapteros-config, adapteros-orchestrator, adapteros-domain, adapteros-chat, adapteros-embeddings, adapteros-node, adapteros-system-metrics, adapteros-storage, etc.)
  implication: Crate table was severely incomplete

- timestamp: 2026-03-04T07:12:00Z
  checked: Migration path claim in CLAUDE.md
  found: CLAUDE.md said "Migrations in crates/adapteros-db/migrations/" but canonical location is top-level "migrations/" (333 files) vs crate location (3 files)
  implication: Wrong path documented

- timestamp: 2026-03-04T07:14:00Z
  checked: MLX FFI file names in CLAUDE.md
  found: Doc said "mlx_wrapper.cpp (real) and mlx_wrapper_stub.cpp (CI)" but actual files are "mlx_cpp_wrapper_real.cpp" and "mlx_cpp_wrapper.cpp"
  implication: File names were wrong

- timestamp: 2026-03-04T07:16:00Z
  checked: UI data fetching pattern in CLAUDE.md
  found: Doc said "Use create_local_resource for data fetching". This was a Leptos 0.6 API. Codebase uses custom `use_api_resource` hook. No occurrences of `create_local_resource` anywhere.
  implication: Stale Leptos 0.6 pattern documented

- timestamp: 2026-03-04T07:18:00Z
  checked: Middleware chain in CLAUDE.md
  found: Inner chain (auth->tenant_guard->csrf->context->policy->audit) was accurate. But outer global middleware chain was undocumented: drain, api_prefix_compat, request_tracking, client_ip, seed_isolation, trace_context, versioning, security_headers, request_size_limit, rate_limiting, cors, idempotency, ErrorCodeEnforcement.
  implication: Middleware documentation was incomplete (inner chain correct, outer chain missing)

- timestamp: 2026-03-04T07:20:00Z
  checked: ROADMAP.md milestone list
  found: v1.1.16 was mentioned in "Previous Milestone" header but NOT in the milestones checklist (jumped from v1.1.15 to v1.1.17). Phase 47 was marked [x] complete in phases list but milestone v1.1.17 still showed [ ] unchecked.
  implication: Milestone list incomplete and inconsistent

- timestamp: 2026-03-04T07:22:00Z
  checked: STATE.md phase status
  found: Said "Current phase: 47 (in progress)" but ROADMAP showed Phase 47 as complete. Last session log entry was 2026-03-03.
  implication: STATE.md not updated after Phase 47 completion

- timestamp: 2026-03-04T07:24:00Z
  checked: PROJECT.md current state
  found: Said "Latest shipped milestone: v1.1.16" and "Current execution milestone: v1.1.17 (in progress)". Phase 47 is complete so this needed updating.
  implication: PROJECT.md not updated after v1.1.17 completion

- timestamp: 2026-03-04T07:25:00Z
  checked: Architecture diagram, port, boot phases, feature flags, scripts, clap version, sqlx version, axum version
  found: All accurate. Port 18080 correct. Feature flags correct. All scripts exist. clap 4.4 correct. sqlx 0.8 correct. Boot uses phases numbered 2-12 with sub-phases.
  implication: Core architecture claims are still valid - no changes needed

- timestamp: 2026-03-04T07:26:00Z
  checked: Environment variables table
  found: Only 6 vars listed but many more significant ones exist (AOS_SERVER_PORT, AOS_WORKER_UID, AOS_VAR_DIR, AOS_LOG_PROFILE, AOS_CONFIG, AOS_QUICK_BOOT, etc.)
  implication: Env var table expanded with 6 additional important variables

- timestamp: 2026-03-04T07:27:00Z
  checked: SSE event contract reference
  found: CLAUDE.md said InferenceEvent is in "signals/chat.rs" - it exists there AND in sse.rs (duplicate definition). Reference was not wrong but incomplete.
  implication: Minor imprecision corrected

## Resolution

root_cause: Documentation accumulated incremental drift over 17 milestones because doc updates were not consistently applied after each code change. Major drift areas: (1) crate table severely incomplete (28/85 crates), (2) migration path wrong, (3) MLX file names wrong, (4) stale Leptos 0.6 API reference, (5) middleware chain missing outer layer, (6) ROADMAP missing v1.1.16 from checklist, (7) STATE.md not updated for Phase 47 completion, (8) PROJECT.md still showed v1.1.17 in progress.

fix: Applied 12 specific corrections across 4 files:
  CLAUDE.md:
  - Expanded crate layers table from 12 rows/28 crates to 22 rows covering all 85 crates
  - Corrected migration path from "crates/adapteros-db/migrations/" to "migrations/" (top-level)
  - Fixed MLX file names from "mlx_wrapper.cpp/mlx_wrapper_stub.cpp" to "mlx_cpp_wrapper_real.cpp/mlx_cpp_wrapper.cpp"
  - Updated UI data fetching from stale "create_local_resource" to actual "use_api_resource" hook
  - Expanded middleware chain to include global outer chain (13 middleware layers)
  - Added SSE event sse.rs reference alongside signals/chat.rs
  - Added 6 environment variables (AOS_SERVER_PORT, AOS_WORKER_UID, AOS_VAR_DIR, AOS_LOG_PROFILE, AOS_CONFIG, AOS_QUICK_BOOT)
  ROADMAP.md:
  - Updated header from "In Progress" to "Complete"
  - Added missing v1.1.16 to milestones checklist
  - Marked v1.1.17 as complete with date
  PROJECT.md:
  - Updated current state to reflect v1.1.17 shipped
  - Changed milestone section from "In Progress" to "Completed"
  - Updated last-updated date
  STATE.md:
  - Updated position to reflect Phase 47 complete
  - Added session log entry for documentation audit
  - Updated session metadata

verification: All four files re-read after edits. Each corrected claim verified against actual codebase state. Sections verified as accurate: architecture diagram, port (18080), feature flags, all contract check scripts, all build commands, clap 4.4, sqlx 0.8, axum 0.8, runtime paths, determinism patterns, error handling patterns.

files_changed:
  - CLAUDE.md
  - .planning/PROJECT.md
  - .planning/ROADMAP.md
  - .planning/STATE.md
