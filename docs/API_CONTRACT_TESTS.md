# API Contract Test Target

`adapteros-server-api` exposes an integration-test target named `api_contracts`.

## Compile-Only Verification

Run:

```bash
cargo test -p adapteros-server-api --test api_contracts --no-run
```

## Current Implementation Mapping

- Test target file: `crates/adapteros-server-api/tests/api_contracts.rs`
- Backing implementation: `crates/adapteros-server-api/tests/contract_snapshots.rs`
- Snapshot fixtures: `crates/adapteros-server-api/tests/contracts/`

`api_contracts.rs` is a thin compatibility entrypoint that includes the existing
contract snapshot suite so callers can use the documented test target name.
