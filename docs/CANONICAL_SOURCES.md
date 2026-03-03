# Canonical Sources

This index maps high-level claims to authoritative implementation files.
Use these sources first when validating behavior, APIs, and route ownership.

## Runtime Entrypoints

- Control-plane server: `crates/adapteros-server/src/main.rs`
- API route registration: `crates/adapteros-server-api/src/routes/mod.rs`
- UI route surface: `crates/adapteros-ui/src/lib.rs`
- Canonical runtime defaults: `crates/adapteros-core/src/defaults.rs`
- Canonical API-type defaults: `crates/adapteros-api-types/src/defaults.rs`

## Startup Contracts

- Startup wrapper: `start`
- Manifest bridge helper: `scripts/lib/manifest-reader.sh`
- Local port pane helper: `scripts/lib/ports.sh`
- Startup config validation: `scripts/check-config.sh`

## Local Gate Contracts

- Local required checks: `scripts/ci/local_required_checks.sh`
- Local release gate: `scripts/ci/local_release_gate.sh`
- Local release gate (prod wrapper): `scripts/ci/local_release_gate_prod.sh`
- Contract suite: `scripts/contracts/check_all.sh`
- Port contract guard: `scripts/contracts/check_port_contract.sh`
- Route closure artifact generator: `scripts/contracts/generate_route_closure_artifacts.py`
- Runbook evidence guard: `scripts/ci/check_runbook_drill_evidence.sh`
- Release SBOM/provenance/signing generator: `scripts/release/sbom.sh`

## Prod Cut Planning

- Prod cut spec and freeze policy: `.planning/PROD_CUT.md`
- Prod cut artifacts + evidence root: `.planning/prod-cut/`
