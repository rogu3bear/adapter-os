# Contributing to adapterOS

Internal development guidelines for adapterOS.

---

## Prerequisites

- macOS 13.0+ with Apple Silicon
- Rust (stable, see `rust-toolchain.toml`)
- MLX: `brew install mlx`

---

## Build

```bash
cargo build --release --workspace
./aosctl --rebuild --help
```

---

## Test

```bash
cargo test --workspace
cargo test -p adapteros-lora-router --test determinism
cargo test -p adapteros-server-api --test replay_determinism_tests
```

---

## Quality

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

---

## Path Hygiene

- Runtime data: `var/` only (gitignored)
- Never create `var/` or `tmp/` inside crates
- Never write to `/tmp`, `/private/tmp`, `/var/tmp`
- Clean test artifacts: `find ./crates -type d -name "var" -not -path "*/target/*" -exec rm -rf {} +`

---

## Scope

- Prefer minimal diffs and existing patterns
- Before expensive actions (full-workspace tests, `cargo clean`), justify or ask
- Only edit files explicitly listed in the task
