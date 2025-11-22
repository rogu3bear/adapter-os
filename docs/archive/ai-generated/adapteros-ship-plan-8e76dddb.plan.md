<!-- 8e76dddb-991b-4db6-9a67-f651c383a9f9 46504aa5-5317-4536-98f2-577efc180ed3 -->
# AdapterOS Full‑Ship Plan (M0 → M1 → M2)

### Top‑level 7‑step execution plan

1. Stand up runtime inference core: base LLM, LoRA loader, Metal kernels, router/policy/evidence
2. Wire API Gateway (M0) and Storage: loopback TCP, HMAC JWT, replay endpoint; PostgreSQL/pgvector/bundle/artifact stores
3. Build UI (M0) and Menu Bar app: pages/features, API client, dev/build/embed workflow
4. Add Observability & Replay: telemetry, traces, metrics; deterministic replay bundles and CLI
5. Package & Deploy (M0): embed static UI, serve at / and /api/*, local only
6. Production hardening (M1): UDS‑only, Ed25519 JWTs, per‑tenant rate limiter, zero egress, compliance UI
7. Extensions (M2) and patent filings: shared downsample, GPU orthogonality, hot‑swap, federated adapters, Replay Studio, auto‑promotion

---

### Milestone M0 — Functionality‑first (feature‑complete, local only)

- Runtime Inference
- Base LLM: Qwen2.5‑7B‑Instruct (int4, Metal)
- Adapter Loader (LoRA lifecycle)
- Router with Q15 gates; Policy Engine; Evidence Tracker
- Data Services: RAG, Response Cache, Memory Manager (≥15% headroom)
- Observability: Telemetry Logger, Trace Builder, Metrics Collector
- Storage: PostgreSQL, pgvector, Bundle Store, Artifact Store
- API Gateway (M0): Loopback TCP, HMAC‑SHA256 JWTs, Replay endpoint
- Web UI: React+TS, Vite, Tailwind/shadcn; features: auth, metrics, tenants, adapters, policies, telemetry, inference, audit
- API Integration: centralized client, types, token mgmt, endpoints (/api/auth, /api/tenants, /api/adapters, /api/metrics, /api/telemetry, /api/policies)
- Menu Bar App: SwiftUI, offline, 5s polling, status icons
- Deployment: embed static UI via rust‑embed; serve at / and /api/*; local TCP only
- Dev Workflow: pnpm dev/build, make targets
- Security deferrals explicitly accepted in M0

Acceptance criteria (M0)

- Deterministic run produces a Trace Bundle and replays identically with CLI
- UI builds with pnpm and is embedded; loads at / with working endpoints
- Local auth via HMAC JWT; all listed UI features accessible against local APIs

---

### Milestone M1 — Production hardening (security, isolation, compliance)

- Transport/Auth: switch to UDS‑only serving; Ed25519‑signed JWTs; token rotation; per‑tenant rate limiter (token bucket)
- Zero egress enforcement across services
- Observability isolation: local metrics over UDS; no external egress
- Control Plane: Promotion Service gates (validate hashes, replay, approval, promote)
- Compliance Dashboard and audit trail visualization
- Multi‑tenant isolation & RBAC in UI and APIs

Acceptance criteria (M1)

- All APIs listen on UDS only; network egress blocked by policy; Ed25519 JWTs enforced
- Deterministic replay succeeds under production topology and policy packs
- Promotion flow executes and records approvals; compliance dashboards render complete data

---

### Milestone M2 — Extensions and performance

- Shared Downsample Matrix integration (disabled → enabled), measure memory savings
- GPU Orthogonal Constraints integration; maintain CPU parity
- Hot‑Swap Adapter runtime integration
- Federated Adapters, Metal MLX support, Replay Studio, Auto‑Promotion
- Patent strategy: file provisional; continue with expanded claims as features graduate

Acceptance criteria (M2)

- Feature flags for advanced kernels; determinism parity and perf budgets met
- Documentation and demos for new capabilities; updated replay bundles

---

### Cross‑cutting execution tracks

- Runtime & Kernels: deterministic scheduling, tolerance checks, audits
- APIs & Auth: endpoints, auth, rate limiting, replay
- Storage & Schema: Postgres, pgvector, bundles, artifacts
- UI & Design System: Tailwind + shadcn, accessibility, dark/light
- Observability: telemetry, tracing, metrics/policies
- Deployment & Packaging: rust‑embed static, make/cargo releases

---

### Quality gates & test matrix

- Determinism: per‑kernel tolerance checks; periodic drift audits; replay identity tests
- Performance: router Top‑K and memory headroom (≥15%) validated under load
- Security: JWT (HMAC→Ed25519), UDS‑only, zero‑egress, rate limiting; policy pack coverage
- UI: e2e flows across all listed pages with API error handling

---

### Release management

- Dev: pnpm dev; M0 target runs at http://localhost:3200 for UI dev
- Build: pnpm build → embedded in server static; make ui; cargo release
- Serve: root path / with APIs at /api/*; M0 loopback TCP; target UDS in prod

---

### Risks and how we will test them

- Determinism drift across hardware/OS updates. Tests: replay bundles on matrix of machines; tolerance checks; drift audits
- UDS‑only and zero‑egress breaking integrations. Tests: staged canary with UDS sockets; policy egress tests; UI e2e against UDS
- GPU kernel integration regressions. Tests: CPU/GPU parity unit tests; perf baselines for router and kernels; feature flag rollouts

---

### Proceed?

Confirm and we will execute M0→M1→M2 with the above gates and artifacts.

### References

[1] docs/architecture/MasterPlan.md L23–26
[2] docs/architecture/MasterPlan.md L18–21
[3] docs/architecture/MasterPlan.md L8–13
[4] docs/architecture/MasterPlan.md L223–224
[5] docs/architecture/MasterPlan.md L37–41
[6] docs/architecture/MasterPlan.md L85–99
[7] docs/architecture/MasterPlan.md L144–151
[8] docs/architecture/MasterPlan.md L166–176
[9] docs/architecture/MasterPlan.md L123–131
[10] docs/architecture/MasterPlan.md L33–35
[11] docs/architecture/MasterPlan.md L301–305
[12] docs/architecture/MasterPlan.md L159–166
[13] docs/architecture/MasterPlan.md L10–14
[14] docs/architecture/MasterPlan.md L188–196
[15] docs/architecture/MasterPlan.md L215–223
[16] docs/architecture/MasterPlan.md L380–399
[17] docs/architecture/MasterPlan.md L343–348
[18] docs/architecture/MasterPlan.md L400–438
[19] docs/architecture/MasterPlan.md L23–26
[20] docs/architecture/MasterPlan.md L24–25
[21] docs/architecture/MasterPlan.md L18–21
[22] docs/architecture/MasterPlan.md L28–31
[23] docs/architecture/MasterPlan.md L33–35
[24] docs/architecture/MasterPlan.md L37–41
[25] docs/architecture/MasterPlan.md L8–13
[26] docs/architecture/MasterPlan.md L223–224
[27] docs/architecture/MasterPlan.md L87–104
[28] docs/architecture/MasterPlan.md L144–158
[29] docs/architecture/MasterPlan.md L123–143
[30] docs/architecture/MasterPlan.md L159–166
[31] docs/architecture/MasterPlan.md L166–178
[32] docs/architecture/MasterPlan.md L83–85
[33] docs/architecture/MasterPlan.md L301–305
[34] docs/architecture/MasterPlan.md L159–166
[35] docs/architecture/MasterPlan.md L144–158
[36] docs/architecture/MasterPlan.md L12–13
[37] docs/architecture/MasterPlan.md L93–104
[38] docs/architecture/MasterPlan.md L10–12
[39] docs/architecture/MasterPlan.md L221–223
[40] docs/architecture/MasterPlan.md L188–196
[41] docs/architecture/MasterPlan.md L246–247
[42] docs/architecture/MasterPlan.md L261–266
[43] docs/architecture/MasterPlan.md L193–194
[44] docs/architecture/MasterPlan.md L331–332
[45] docs/architecture/MasterPlan.md L189–192
[46] docs/architecture/MasterPlan.md L10–12
[47] docs/architecture/MasterPlan.md L188–196
[48] docs/architecture/MasterPlan.md L229–231
[49] docs/architecture/MasterPlan.md L301–305
[50] docs/architecture/MasterPlan.md L261–266
[51] docs/architecture/MasterPlan.md L331–332
[52] docs/architecture/MasterPlan.md L380–386
[53] docs/architecture/MasterPlan.md L387–393
[54] docs/architecture/MasterPlan.md L394–399
[55] docs/architecture/MasterPlan.md L343–348
[56] docs/architecture/MasterPlan.md L400–438
[57] docs/architecture/MasterPlan.md L294–298
[58] docs/architecture/MasterPlan.md L301–305
[59] docs/architecture/MasterPlan.md L294–298
[60] docs/architecture/MasterPlan.md L151–158
[61] docs/architecture/MasterPlan.md L223–224
[62] docs/architecture/MasterPlan.md L37–41
[63] docs/architecture/MasterPlan.md L180–187
[64] docs/architecture/MasterPlan.md L33–35
[65] docs/architecture/MasterPlan.md L159–166
[66] docs/architecture/MasterPlan.md L336–339
[67] docs/architecture/MasterPlan.md L294–305
[68] docs/architecture/MasterPlan.md L18–19
[69] docs/architecture/MasterPlan.md L30–31
[70] docs/architecture/MasterPlan.md L10–14
[71] docs/architecture/MasterPlan.md L229–231
[72] docs/architecture/MasterPlan.md L105–122
[73] docs/architecture/MasterPlan.md L144–151
[74] docs/architecture/MasterPlan.md L166–171
[75] docs/architecture/MasterPlan.md L172–178
[76] docs/architecture/MasterPlan.md L336–339
[77] docs/architecture/MasterPlan.md L159–166
[78] docs/architecture/MasterPlan.md L294–305
[79] docs/architecture/MasterPlan.md L10–14
[80] docs/architecture/MasterPlan.md L195–196
[81] docs/architecture/MasterPlan.md L234–237
[82] docs/architecture/MasterPlan.md L294–298

### To-dos

- [ ] Stand up runtime: base LLM, LoRA loader, Metal kernels, router/policy/evidence
- [ ] Implement API Gateway (M0): loopback TCP, HMAC JWT, replay endpoint
- [ ] Provision PostgreSQL/pgvector and implement bundle/artifact stores
- [ ] Build Control Plane UI pages and API client; embed static build
- [ ] Ship macOS menu bar app with offline polling and status icons
- [ ] Implement telemetry logger, trace builder, metrics collector
- [ ] Package server with embedded UI; local-only serving; make/cargo release
- [ ] Migrate to UDS-only, Ed25519 JWTs, per-tenant rate limiter, zero egress
- [ ] Implement Promotion Service gates and CPID lifecycle UX
- [ ] Deliver compliance dashboard and audit visualization
- [ ] Integrate shared downsample, GPU orthogonality, hot-swap adapters
- [ ] Prepare and file provisional; plan continuation claims