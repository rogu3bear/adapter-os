# Candidate Rectifications

- **fs-01 (path): finish /tmp bans**
  - Index root resolver still permits `/tmp`; apply `reject_tmp_persistent_path` there and cover with a unit test.
  - Follow-up audit of artifact/dataset roots to ensure no persistent `/tmp` usage.

- **telemetry-01 (schema enforcement)**
  - Tenant validation added via `TelemetryFilters::validate/with_tenant` and `TelemetryBuffer::query`; builder rejects empty tenant. Follow-up: confirm all emitters pass tenant/model context (router/inference telemetry).

- **routing-01**
  - Q15 denominator locked via `ROUTER_GATE_Q15_DENOM` test; policy hook parity covered for live vs replay. No further action unless new hooks appear.

- **tenant-01 coverage gaps**
  - Audit adapter/base model lifecycle DB queries for `tenant_id` predicates; add targeted tests in `crates/adapteros-db/tests` covering adapter/register/activation with cross-tenant denial.

- **test-01 coverage**
  - Update rule-to-test mapping for remaining tenant lifecycle/index-root gaps once above items land.

MLNavigator Inc 2025-12-10.
