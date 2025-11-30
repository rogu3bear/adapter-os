# Production Baseline Gap Analysis

This document captures configuration gaps discovered while reconciling the master architecture plan with the current production manifests and server configuration code.

## Runtime Transport & Authentication

- The master plan requires Unix Domain Socket (UDS) transport and Ed25519 JWT signing for production. The `production-multinode.toml` manifest still binds to `0.0.0.0:8080`, exposes a TCP port, and lacks a `uds_socket` value or `production_mode` enablement, so the hardening switches are not engaged.
- The manifest references `jwt_secret_path` and `ed25519_keypair_path`, but the server configuration expects either inline `jwt_secret`/`jwt_secret_file` fields and a `jwt_mode` of `eddsa` when production safeguards are active. There is also no configured JWT rotation cadence.

## Configuration Schema Drift

- `production-multinode.toml` uses keys such as `bind_address`, `artifact_dir`, and `db.url` that do not match the loader structures (`bind`, `artifacts_root`, and `database.path`). Attempting to load this manifest through `adapteros-server` or the shared config loader would fail.
- Required fields in the code (`paths.bundles_root`, `security.global_seed`) are absent from the manifest, preventing deterministic executor initialization.
- The manifest duplicates the `enable_mmap_adapters` key with conflicting cache sizes (`512` vs `2048` MB), making intent ambiguous.

## Policy & Observability Defaults

- The manifest enables the 22 policy packs, aligning with the plan, but zero-egress enforcement is declarative only; there is no evidence that the runtime scripts install the referenced PF anchor file (`/etc/pf.anchors/adapteros`).
- Telemetry paths are defined, yet the server configuration code expects structured retention settings (`bundles_root`, `TelemetryRetentionConfig`) that are missing from the manifest.

## Action Items

1. Update production manifests to use the schema exported by `adapteros-config`/`adapteros-server`, enabling `production_mode`, `uds_socket`, and Ed25519 JWT parameters.
2. Add deterministic executor inputs (`security.global_seed`, telemetry bundle paths) and remove duplicate keys.
3. Document operational steps (PF rules deployment, telemetry retention) so that declarative settings have corresponding runtime enforcement.

