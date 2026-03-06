# Phase 54: Performance and Security Hardening - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Optimize inference latency, minimize memory footprint, and harden all attack surfaces. Deliver a system that feels instant and is bulletproof against auth bypass, injection, and secret leakage. No new user-facing features — pure speed and safety improvements on the existing surface.

</domain>

<decisions>
## Implementation Decisions

### Inference Latency Targets
- Time-to-first-token (TTFT) under 500ms on cold adapter load — aggressive target
- Warm adapter throughput must match raw MLX baseline — our orchestration/routing layer adds zero meaningful overhead
- Include a reproducible benchmark suite as a deliverable: TTFT, tok/s, memory peak — can run before/after to prove gains and catch future regressions

### Memory Budget Strategy
- Hard UMA ceiling with LRU eviction — never OOM, evict least-recently-used adapters when approaching limit
- Default ceiling is configurable (ship a sensible default like 75% UMA), operators tune per-machine via config
- Toast notification when an adapter is evicted — "Adapter X unloaded to free memory" — keeps operators informed
- Evicted adapters reload transparently on next use

### Security Audit Scope
- All attack surfaces audited at equal depth: auth/access control, input validation/injection, secret exposure
- Rate limiting tuned per route group tier (health/public/internal/protected) — maps to existing route architecture
- Formal security audit report as a deliverable: findings, severity, remediation, verification. Publishable artifact
- Dependency vulnerability scanning included (cargo-audit, advisory review, upgrade/pin as needed)

### Hardening Philosophy
- Fail closed on all auth ambiguity in production. AOS_DEV_NO_AUTH=1 escape hatch preserved for local dev
- Structured security audit trail: failed auth, rate limit hits, suspicious input logged with severity, source IP, endpoint, reason — feeds into existing audit log system
- Model weight protection: OS-level file permissions on var/models/ plus API auth on any endpoint that serves/references weights
- CI security smoke tests: automated checks for secrets in logs, auth enforcement on protected routes, input validation fuzzing — prevents regressions

### Claude's Discretion
- Inference concurrency model (serialize vs parallel) — based on UMA constraints and MLX capabilities
- Adapter hot/cold tiering strategy — based on what the architecture supports
- Exact UMA ceiling default percentage
- Specific optimizations to hit TTFT target (preloading, caching, lazy init)

</decisions>

<specifics>
## Specific Ideas

- Benchmark suite should be runnable as a script (like the existing contract checks) — not just test assertions
- Security audit report follows the same pattern as existing contract check scripts in scripts/contracts/
- Eviction toast should be brief and non-blocking — operator acknowledges by dismissing, not by taking action
- Rate limit tiers should respect the existing middleware chain ordering (rate_limiting already exists in the global chain)

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 54-performance-and-security-hardening*
*Context gathered: 2026-03-04*
