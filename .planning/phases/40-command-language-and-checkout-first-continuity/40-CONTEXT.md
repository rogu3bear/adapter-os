# Phase 40: Command Language and Checkout-First Continuity - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Align command terms and action sequencing across Dashboard, Update Center, and Adapter Detail so operators get one consistent checkout/promote/feed-dataset model. This phase clarifies language and guidance behavior only; it does not add new backend capabilities.

</domain>

<decisions>
## Implementation Decisions

### Command vocabulary canonicalization
- Canonical operator verbs are `Run Checkout`, `Run Promote`, and `Feed Dataset` / `Feed Dataset from This Version`.
- Do not introduce restore-first terminology in adapter version operations.
- Keep command-map snippets aligned to `checkout <branch>@<version>`, `promote <version> --to production`, and `feed-dataset --branch <branch> --from <version>`.

### Recommended-path language
- Use short, imperative, action-first wording: resolve version -> run checkout or promote -> feed-dataset.
- Default guidance remains explicit that recommended options are the primary path for operator flow.
- Recovery copy must distinguish fast rollback intent (`checkout`) from deployment intent (`promote`).

### Cross-surface parity
- Dashboard Guided Flow, Update Center command map/default text, and Adapter Detail recommendation card must communicate the same action order and terms.
- Navigation/discovery keywords should continue to include command vocabulary (`checkout`, `feed-dataset`, `promote`) for findability.

### Claude's Discretion
- Exact phrasing cadence and sentence length per surface.
- Minor presentation choices for command-map helper copy as long as canonical terms remain unchanged.

</decisions>

<specifics>
## Specific Ideas

- Treat Adapter Detail command map as canonical source text, then reconcile Dashboard and Update Center language to match it.
- Preserve concise plain-language style for operators under load: active voice, one action per sentence.

</specifics>

<deferred>
## Deferred Ideas

- Operator-visible per-adapter command history timeline (candidate future milestone requirement `UX-41-01`).
- Extended command-palette parity for adapter operations across all surfaces (`UX-41-02`).

</deferred>

---

*Phase: 40-command-language-and-checkout-first-continuity*
*Context gathered: 2026-02-28*
