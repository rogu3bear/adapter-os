# Action Taken

Commands executed:

1. `vm_stat`
2. `top -l 1 | head -n 20`
3. Synthetic constrained-memory probe (`ulimit -v 65536` + allocation attempt)
4. `curl -sS -X POST http://localhost:18080/v1/lifecycle/evict ...`
5. `curl -sS --max-time 5 http://localhost:18080/readyz`

Operator decision:

- Recorded this drill as a live telemetry + negative-path validation run.
- Preserved observed endpoint/shell limitations as explicit evidence for follow-up environment hardening.
- Verified service readiness after stress attempt.
