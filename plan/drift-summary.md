# Drift Summary (human-facing)

- Path guard: `/tmp` now rejected for telemetry/manifest-cache/adapters/db resolvers; index root still needs the same guard and test.
- Telemetry: tenant context enforced via `TelemetryFilters::validate/with_tenant` and buffer query checks; builder rejects empty tenant.
- Routing/policy: Q15 denominator locked (32767) and policy hooks validated for live and replay paths via tests.
- Tenant isolation: adapter/base-model lifecycle queries still need a pass to confirm `tenant_id` predicates and add denial tests.
- Coverage: new tests cover /tmp bans, telemetry tenancy, Q15, and hook parity; remaining gaps are index-root guard and tenant lifecycle/db coverage.

Artifacts: `plan/drift-findings.json` (rules + matches/mismatches/gaps), `plan/drift-actions.md` (candidate rectifications).

MLNavigator Inc 2025-12-10.
