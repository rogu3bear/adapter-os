# Documentation Grounding

**Purpose:** Prevent AI/agent hallucinations by documenting verified facts and running periodic checks against known stale patterns.

**Last Updated:** 2026-02-18

---

## Verified Facts (Canonical)

| Claim | Source | Notes |
|-------|--------|-------|
| Main UI is Leptos/WASM | `crates/adapteros-ui/`, `scripts/build-ui.sh` | Not React/pnpm |
| UI dev: `cd crates/adapteros-ui && trunk serve` | QUICKSTART.md, start script | Port 3200 |
| UI prod: `./scripts/build-ui.sh` then `./start` | Backend serves static/ on 8080 | Port 8080 |
| CLI: `aosctl` for db migrate, init-tenant, config show | `crates/adapteros-cli/` | Not adapteros-orchestrator |
| API prefix: `/v1/` | `crates/adapteros-server-api/src/routes/` | Not `/api/v1/` |
| Model download: `./scripts/download-model.sh` or `aosctl models seed` | DEPRECATIONS.md | Not download_model.sh (underscore) |
| service-manager.sh start ui | No-op; UI served by backend | For dev UI use trunk serve |
| Readiness gates (Models, Workers, Stacks, Documents) | `system_not_ready` / `inference_not_ready` in pages | See [BACKEND_FRONTEND_READINESS_MAP.md](BACKEND_FRONTEND_READINESS_MAP.md) |

---

## Forbidden Patterns (Hallucination Risks)

These patterns must not appear in docs. Run `./scripts/ci/check_docs_grounding.sh` to verify.

| Pattern | Why |
|---------|-----|
| `adapteros-orchestrator.*(db migrate\|init-tenant\|config show)` | Wrong binary; use aosctl |
| `cd ui && pnpm` | No ui/ at root; main UI is Leptos |
| `React-based using pnpm` (for main UI) | Main UI is Leptos/WASM |
| `scripts/download_model.sh` | Use download-model.sh (hyphen) or aosctl models seed |
| `http://localhost:8080/api/v1/` | API uses /v1/ not /api/v1/ |
| `plan/drift-findings.json` (unqualified) | File removed; cite DOCUMENTATION_DRIFT workflow |

---

## Verification Commands

```bash
# Run full grounding check (CI)
./scripts/ci/check_docs_grounding.sh

# Manual spot checks
rg "adapteros-orchestrator.*(db migrate|init-tenant|config show)" docs/ && echo "FAIL" || echo "OK"
rg "cd ui && pnpm" docs/ && echo "FAIL" || echo "OK"
rg "React-based using pnpm" docs/ && echo "FAIL" || echo "OK"
rg "scripts/download_model\.sh" docs/ && echo "FAIL" || echo "OK"
rg "http://localhost:8080/api/v1/" docs/ && echo "FAIL" || echo "OK"
```

---

## Related

- [DOCUMENTATION_DRIFT.md](DOCUMENTATION_DRIFT.md) — Validation framework
- [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md) — Code-level sources of truth
- [DEPRECATIONS.md](DEPRECATIONS.md) — Deprecated scripts and replacements
