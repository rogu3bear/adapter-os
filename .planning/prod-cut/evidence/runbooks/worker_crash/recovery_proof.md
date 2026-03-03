# Recovery Proof

Recovery criteria for this drill were service continuity and explicit degraded-state observability.

Evidence:

- `/v1/status` remained `"ready":true`.
- `/readyz` remained `"ready":true`.
- Degradation surfaced explicitly as non-critical worker degradation (`workers`, `ui`) rather than silent failure.

Conclusion:

- Incident path was detected and contained with control-plane continuity preserved.
