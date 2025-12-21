# println!/eprintln! Usage Audit

Status: Low priority (monitoring)

Summary:
- Core library crates use `tracing` for runtime logging; no `println!/eprintln!` outside tests.
- Remaining stdout/stderr usage is in binaries (CLI/tools/daemons), build scripts, tests/examples/benches, and docs.
- A few `#[cfg(test)]` modules inside `src/` print diagnostics.

Locations (non-production runtime output):
- Binaries/daemons: `crates/adapteros-server/src`, `crates/adapteros-cli/src`,
  `crates/adapteros-lint/src/main.rs`, `crates/sign-migrations/src/main.rs`,
  `crates/adapteros-codegraph/src/bin/codegraph.rs`,
  `crates/adapteros-lora-worker/src/bin/residency_harness.rs`
- Test/support crates: `crates/adapteros-testing`
- Build scripts: `crates/*/build.rs`
- Tests/examples/benches: `crates/**/tests`, `crates/**/examples`, `crates/**/benches`,
  `examples/`, `scripts/`
- Docs: `docs/`

Notes:
- Build scripts use `println!` for Cargo directives and `cargo:warning` output.
- CLI/tools use stdout/stderr intentionally for user-facing output.
