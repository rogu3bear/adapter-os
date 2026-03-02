# Architecture Integration (v1.1.14)

**Researched:** 2026-02-28
**Confidence:** HIGH

## Existing Integration Points to Reuse

1. **Dashboard as high-level operator primer**
   - Owner: `crates/adapteros-ui/src/pages/dashboard.rs`
   - Existing guidance rail already frames the command sequence and recommended default path.
   - Key lines: `238-247`, `272-277`

2. **Update Center as command-oriented list surface**
   - Owner: `crates/adapteros-ui/src/pages/update_center.rs`
   - Existing command map, recommended default, and adapter list entry to detail panel are in place.
   - Key lines: `105-107`, `215-236`

3. **Adapter Detail as canonical command execution surface**
   - Owner: `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
   - Holds command map, quick operator guide, recommended next action, promote/checkout/feed controls.
   - Key lines: `783-823`, `901-947`

4. **Training entry as continuity target**
   - Owner: `crates/adapteros-ui/src/pages/training/mod.rs`
   - Already accepts query params needed to carry branch/version provenance.
   - Key lines: `195-206`

5. **Navigation keyword discoverability**
   - Owner: `crates/adapteros-ui/src/components/layout/nav_registry.rs`
   - Update Center keyword map already includes `checkout` and `feed-dataset`.
   - Key lines: `146-153`

## Architecture Rule for This Milestone

- Extend existing surfaces; do not introduce a parallel adapter-operations UI path.
- Keep command semantics centralized in existing pages/components to avoid vocabulary drift.
- Preserve query-param handoff contract to training for lineage continuity.
