# adapterOS Policies

Policy engine and canonical policy packs.

---

## Overview

adapterOS enforces policy at multiple layers: pre-request, pre-inference, and post-inference. Policy packs govern security, determinism, egress, and operational behavior.

---

## Key Principles

- **Zero egress** — No data exfiltration during serving
- **Determinism** — Identical inputs produce identical outputs
- **Audit** — All operations logged for compliance

---

## Enforcement Points

1. **Server layer** — Pre-validation before routing
2. **Worker layer** — Pre-inference and post-inference gates
3. **Middleware** — Auth → Tenant guard → CSRF → Policy → Audit

---

## Development Bypass

For UI iteration without RBAC:

```bash
AOS_DEV_NO_AUTH=1 ./start
```

Or set `security.dev_bypass = true` in config (debug builds only).

---

## See Also

- [docs/POLICIES.md](docs/POLICIES.md) — Full policy documentation
- [docs/SECURITY.md](docs/SECURITY.md) — Auth and access control
