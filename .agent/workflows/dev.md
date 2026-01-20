---
description: Development cycle - check, build, format, lint, test, fix
---

# Development Workflow

// turbo-all

## Quick Verify
```bash
cargo check --workspace
```

## Format
```bash
cargo fmt --all
```

## Lint
```bash
cargo clippy --workspace -- -D warnings
```

## Build Release
```bash
cargo build --release --workspace
ln -sf target/release/aosctl ./aosctl
```

## Test
```bash
cargo test --workspace
```

## Test Specific Crate
```bash
cargo test -p <crate-name> -- --nocapture
```

## Auto-Fix Lints
```bash
cargo clippy --workspace --fix --allow-dirty --allow-staged -- -D warnings
```

## Fresh Build (nuclear option)
```bash
cargo clean && cargo build --release --workspace
```
