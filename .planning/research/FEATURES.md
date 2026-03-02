# Feature Landscape (v1.1.14)

**Milestone:** v1.1.14 AdapterOps Command Language and Assistive Continuity
**Researched:** 2026-02-28

## Category 1: Command Language Continuity

### Table stakes
- Command labels and microcopy stay consistent across Dashboard, Update Center, and Adapter Detail.
- Recommended default path is explicit and action-ordered.

### Differentiators
- Git-like mental model remains discoverable and reversible without exposing destructive wording.

### Grounding
- Dashboard guided flow command path: `crates/adapteros-ui/src/pages/dashboard.rs:240-247`
- Update Center command map and default: `crates/adapteros-ui/src/pages/update_center.rs:227-235`
- Adapter Detail command map + recommended next action: `crates/adapteros-ui/src/components/adapter_detail_panel.rs:788-823`

## Category 2: Dataset Feed Version-Context Continuity

### Table stakes
- Feeding a dataset from a selected version must preserve branch/version context into training.
- Operators should understand prefilled provenance before launch.

### Differentiators
- One-step “Feed Dataset from This Version” handoff that maintains lineage context.

### Grounding
- Selected-version feed action: `crates/adapteros-ui/src/components/adapter_detail_panel.rs:901-913`
- Training query-param intake for branch/source version: `crates/adapteros-ui/src/pages/training/mod.rs:199-206`

## Category 3: Assistive Foundation

### Table stakes
- Action controls have stable accessible labels.
- Live status messaging uses polite announcements for guidance changes.

### Differentiators
- Consistent assistive wording across list/selected/detail contexts.

### Grounding
- Shared button ARIA behavior: `crates/adapteros-ui/src/components/button.rs:100-123`
- Update Center and detail ARIA action labels: `crates/adapteros-ui/src/pages/update_center.rs:116-133`, `crates/adapteros-ui/src/components/adapter_detail_panel.rs:901-947`

## Category 4: Natural-Language Quality

### Table stakes
- Action text is short, active voice, and specific (“Run Checkout”, “Feed Dataset from This Version”).

### Differentiators
- Minimal ambiguity under operator stress (recovery vs promotion flows).

### Best-practice citations
- W3C APG: https://www.w3.org/TR/wai-aria-practices/
- NARA Plain Language: https://www.archives.gov/open/plain-writing/10-principles.html
- Digital.gov Plain Language: https://digital.gov/guides/plain-language/writing
