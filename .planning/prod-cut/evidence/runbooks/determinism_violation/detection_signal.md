# Detection Signal

Signals captured:

- Initial endpoint probe:
  - `curl -sS --max-time 10 http://localhost:18080/v1/diagnostics/determinism-status`
  - Returned `"freshness_status":"unknown"` and `"freshness_reason":"no_determinism_checks"`.
- Synthetic alert trigger fired as designed:
  - `simulated_alert: expected 32768.0 but repository constant differs (drill trigger)`
  - `synthetic_violation_exit=42`
- Core deterministic invariant test passed:
  - `test_router_ordering_and_q15_gates_are_stable ... ok`
