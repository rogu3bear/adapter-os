# Phase 44: Command Deck AdapterOps Parity - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Extend command palette parity for adapter operations so operators can invoke `Run Promote`, `Run Checkout`, and `Feed Dataset` intents from adapter/update-center contexts.

No new command system is introduced; this phase extends existing contextual search and command execution paths.

</domain>

<decisions>
## Implementation Decisions

### Contextual actions
- Reuse `generate_contextual_actions` for route-aware adapter operation intents.
- Keep operation verbs identical to UI buttons: `Run Promote`, `Run Checkout`, `Feed Dataset`.

### Command execution
- Add command handlers that route to Update Center with selected adapter context.
- Deep-link `command` + `adapter_id` query values so intent survives navigation.

### Assistive/NL quality
- Keep command labels short and action-first.
- Keep update-center intent banner explicit for screen-reader and keyboard-first users.

</decisions>

<deferred>
## Deferred Ideas

- Direct in-palette version selection remains out of scope (requires richer context model).
- Multi-select batch command operations remain out of scope.

</deferred>

---

*Phase: 44-command-deck-adapter-ops-parity*
*Context gathered: 2026-02-28*
