# Milestone Research Summary (v1.1.14)

**Milestone proposal:** AdapterOps Command Language and Assistive Continuity
**Date:** 2026-02-28
**Decision:** Research supports proceeding directly to requirements and roadmap.

## What already exists (verified)

- Command-first guidance surfaces across Dashboard, Update Center, and Adapter Detail.
- Git-like command map and recommended-next-action language in adapter detail.
- Dataset-feed handoff path into training with branch/version query context.
- Shared button components that support accessible names via `aria-label`.

## What this milestone should tighten

1. Command vocabulary parity across all adapter operator surfaces.
2. Stronger dataset-feed lineage continuity wording and handoff guarantees.
3. Assistive-label parity and predictable live guidance behavior.
4. Natural-language compression: short, explicit, active-voice instructions.

## Citations

### Codebase
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs:783-823`
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs:901-947`
- `crates/adapteros-ui/src/pages/dashboard.rs:238-277`
- `crates/adapteros-ui/src/pages/update_center.rs:215-236`
- `crates/adapteros-ui/src/pages/training/mod.rs:195-206`
- `crates/adapteros-ui/src/components/button.rs:100-123`
- `crates/adapteros-ui/src/components/layout/nav_registry.rs:146-153`

### Best-practice
- W3C ARIA APG: https://www.w3.org/TR/wai-aria-practices/
- Plain language principles (NARA): https://www.archives.gov/open/plain-writing/10-principles.html
- Plain language writing guide (Digital.gov): https://digital.gov/guides/plain-language/writing
- Git command distinctions: https://git-scm.com/docs/git
- Git switch behavior: https://git-scm.com/docs/git-switch
