# Pitfalls (v1.1.14)

**Researched:** 2026-02-28

## P1: Vocabulary drift across surfaces

- **Risk:** Dashboard, Update Center, and Detail diverge in command terms.
- **Impact:** Operators lose trust and mis-execute actions.
- **Prevention:** Treat adapter detail command map as canonical and reconcile wording to it.
- **Grounding:** `dashboard.rs:240-247`, `update_center.rs:227-235`, `adapter_detail_panel.rs:788-799`

## P2: Overloaded recovery wording (“restore”) instead of explicit branch/version checkout semantics

- **Risk:** Mixing “restore” with checkout flows blurs commit-history vs file-state actions.
- **Impact:** Higher operator error probability during incident recovery.
- **Prevention:** Keep branch/version actions under checkout-first language and avoid introducing restore-first labels.
- **Best-practice citation:** git command distinctions: https://git-scm.com/docs/git

## P3: Assistive regressions from missing/uneven aria labels

- **Risk:** Buttons in one context are labeled while equivalent controls elsewhere are not.
- **Impact:** Screen-reader operators get inconsistent command discoverability.
- **Prevention:** Use shared `Button`/`ButtonLink` aria-label contract and verify parity across list/selected/detail actions.
- **Grounding:** `button.rs:100-123`, `adapter_detail_panel.rs:901-947`, `update_center.rs:116-133`

## P4: Broken provenance handoff into training flow

- **Risk:** Feed action launches training without `repo_id`/`branch`/`source_version_id` continuity.
- **Impact:** Dataset lineage and operator intent become opaque.
- **Prevention:** Treat training query-param contract as invariant and validate feed action links against it.
- **Grounding:** `adapter_detail_panel.rs:901-913`, `training/mod.rs:195-206`

## P5: Verbose or passive language reduces action clarity

- **Risk:** Guidance copy becomes long, passive, or non-actionable.
- **Impact:** Increased cognitive load and slower operator decisions.
- **Prevention:** Enforce short, active, action-first copy with explicit actor/action.
- **Best-practice citations:**
  - https://www.archives.gov/open/plain-writing/10-principles.html
  - https://digital.gov/guides/plain-language/writing
