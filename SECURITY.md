# Security Policy

## Reporting Vulnerabilities

Report security issues to MLNavigator Inc R&D. Include:

- Description of the vulnerability
- Steps to reproduce
- Impact and severity
- Suggested mitigations if known

We will acknowledge receipt and provide updates on remediation.

---

## Security Measures

adapterOS implements:

- **Deterministic execution** — HKDF-seeded randomness, no unseeded RNG
- **Policy enforcement** — Runtime validation via canonical policy packs
- **Zero egress** — No network during serving; Unix domain sockets only
- **Memory safety** — Rust; secure FFI boundaries
- **Audit logging** — Telemetry and evidence for decisions

---

## Supported Versions

Security updates are provided for the current release line. Check the repository for the latest supported version.
