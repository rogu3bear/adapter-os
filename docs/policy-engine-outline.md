# Policy Engine Outline

## Core Structure
- **PolicyEngine** (`src/lib.rs`): Orchestrates packs.
  - Entry: `enforce(request)`.
  - Flow: Pre-inference → Runtime → Post-output.

## Key Packs (src/packs/)
- **Determinism** (determinism.rs): Seeded RNG, canonical JSON.
  - Checks: HKDF derivation, no println!.
  - Distinguish: Validation vs. Evidence logging.
- **Egress** (egress.rs): Zero network in inference.
  - Enforcement: UDS-only, PF deny.
- **Isolation** (isolation.rs): Tenant UID/GID, no shared mem.
- **Evidence** (evidence.rs): Audit trails for decisions.
  - Distinguish: Retrieval (RAG) vs. Tracking.

## Integration
- Dependencies: core (AosError), telemetry (logging violations).
- Tests: integration_tests.rs (22 packs coverage).

[source: crates/adapteros-policy/src/policy_packs.rs L1-L100]
[source: crates/adapteros-policy/src/packs/determinism.rs L1-L50]
[source: docs/POLICIES.md L1-L100]
