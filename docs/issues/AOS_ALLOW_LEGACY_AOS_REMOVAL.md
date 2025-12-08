# Issue: Remove `AOS_ALLOW_LEGACY_AOS`

- Status: Planned removal in v0.15.0
- Scope: `adapteros-cli` `register`/`scenario` commands and any legacy AOS 1.x bundle ingestion.
- Background: `AOS_ALLOW_LEGACY_AOS` is a migration escape hatch to accept legacy AOS 1.x bundles. It must be removed once all adapters are repackaged as AOS2 bundles with manifest base_model metadata.
- Exit criteria:
  - All shipped adapters are repackaged as AOS2; no production tenant requires the flag.
  - CI forbids running tests with `AOS_ALLOW_LEGACY_AOS=1`.
  - `adapteros-cli` registration rejects legacy AOS unconditionally.
- Removal target: AdapterOS v0.15.0 (freeze flag; delete env parsing and counters).
- Owner: Control Plane + CLI (migration + packaging teams)

MLNavigator Inc 2025-12-08.

