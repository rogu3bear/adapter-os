# Deep Research Runbook (Sequential, No-Corners)

## Mission Frame
- Goal: Produce a code-evidenced, non-duplicative deep-research program for AdapterOS and start execution immediately.
- Non-goals: broad refactors, speculative architecture rewrites, duplicate implementations of existing behavior.
- Constraints: existing-code-first, minimal-diff, deterministic-safe, repo hygiene safe paths only, strict phase order.
- Acceptance criteria:
  - Phase instructions are written before any planning/execution artifacts.
  - Codebase scan captures concrete reuse opportunities with file evidence.
  - Multi-phase plan artifacts live in `wishlist/`.
  - Final execution phase completes with concrete deliverables and verification notes.

## Caution Level (+10)
- Prefer proof over assumption for every claim.
- Capture evidence with exact file paths.
- Favor extension of existing modules over new abstractions.
- If overlap risk appears, route ownership to one team and reference existing work.

## Team Topology (Non-overlapping Ownership)
- Team A: Runtime + Determinism surfaces (`crates/adapteros-core`, `crates/adapteros-deterministic-exec`, `crates/adapteros-lora-router`).
- Team B: Control Plane + API surfaces (`crates/adapteros-server`, `crates/adapteros-server-api`).
- Team C: UI + Route surfaces (`crates/adapteros-ui`, `crates/adapteros-server/static*`).
- Team D: Docs + Scripts + Build surfaces (`docs`, `scripts`, root runbooks).

## Sequential Phases
1. Phase 0: Instructions lock
- Write this runbook.
- Define stop condition: completion of Phase 3 deliverables and verification.

2. Phase 1: Reading and evidence inventory
- Scan code using team ownership boundaries.
- Build an evidence index of existing capabilities and duplicate-work risks.

3. Phase 2: Plan authoring
- Create a long-horizon plan (900-hour envelope) split into execution waves.
- Prioritize by impact, risk, and leverage of existing code.

4. Phase 3: Execute plan (initial tranche)
- Execute highest-leverage, low-risk tranche from Phase 2.
- Deliver concrete artifacts under `wishlist/` that are implementation-ready.
- Verify outputs with minimal relevant commands.

## Verification (Smallest Relevant Set)
- `test -d wishlist`
- `ls -1 wishlist`
- `rg -n "^#|^- " wishlist/*.md`

