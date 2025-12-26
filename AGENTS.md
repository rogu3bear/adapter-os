# AGENTS.md

Minimal guidance for deterministic builds and tests in AdapterOS.

## Build And Test Commands

```bash
# Development
make dev
make dev-no-auth
make ui-dev
make cli
./start

# Build
make build
make prepare
make metal
cargo build --release
cargo check -p <crate>

# Testing
cargo test -p <crate>
cargo test --workspace
cargo test -- --test-threads=1
cargo test -- --nocapture
make test
make test-rust
make test-ui
make test-ignored
make test-hw
make determinism-check

# Quality
cargo fmt --all
cargo clippy --workspace
make check
make fmt
make fmt-check
make clippy
```

## Determinism Rules

- Seed derivation: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`).
- Router determinism: score DESC, index ASC tie-break; Q15 denominator is 32767.0 (`crates/adapteros-lora-router/src/constants.rs`).
- No `-ffast-math` compiler flags (`Cargo.toml`).
- Set `AOS_DEBUG_DETERMINISM=1` to log seed inputs and router tie-break details.
- CI determinism gate runs `make determinism-check` and scans build artifacts for `-ffast-math`.
- OpenAPI/TypeScript clients must stay in sync; CI regenerates and diffs `docs/api/openapi.json` and `ui/src/api/generated.ts`.

## Troubleshooting

**Determinism**
1. Check seed derivation inputs.
2. Verify router sorting (score DESC, index ASC tie-break).
3. Confirm Q15 denominator = 32767.0.
4. Run `make determinism-check`.

**Build**
1. `cargo clean && cargo build`
2. Check feature flags.
3. `cargo sqlx prepare` for offline mode.
4. Verify migration signatures: `migrations/signatures.json`.

## Health Endpoints

- Liveness: `/healthz`
- Readiness: `/readyz` (canonical; no `/api/readyz` alias). `/system/ready` exposes system gate status.
