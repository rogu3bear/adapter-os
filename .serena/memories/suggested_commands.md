# Suggested Commands for AdapterOS Development

## Build Commands

```bash
# Quick type check (workspace)
cargo c

# Build release
cargo build --release --workspace

# Build CLI and symlink
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl

# Timed build with HTML report
cargo tb

# Verbose build (shows each crate)
cargo bv
```

## Running the Server

```bash
# Start dev server (port 8080)
cargo run -p adapteros-server -- --config configs/cp.toml

# With auth disabled (development)
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml

# Full boot (backend + worker, serves UI)
./start
```

## Testing

```bash
# All workspace tests
cargo test --workspace

# Single crate tests
cargo test -p adapteros-lora-router

# Specific test with output
cargo test test_name -- --nocapture

# Using nextest (progress bar)
cargo nt

# Nextest with immediate failure output
cargo ntf

# Ignored tests (hardware-dependent)
cargo test --workspace -- --ignored

# Determinism verification
cargo test --test determinism_core_suite -- --test-threads=8
```

## Code Quality

```bash
# Format all code
cargo fmt --all

# Check formatting (CI)
cargo fmt --all --check

# Lint with warnings as errors
cargo clippy --workspace -- -D warnings
```

## CLI (aosctl)

```bash
./aosctl db migrate              # Run database migrations
./aosctl status                  # Show system status
./aosctl doctor                  # System health diagnostics
./aosctl preflight               # Pre-flight readiness check
./aosctl adapter list            # List adapters
./aosctl models list             # List registered models
./aosctl chat                    # Interactive chat
```

## UI Development (Leptos WASM)

```bash
cd crates/adapteros-ui

# Dev server with hot reload
trunk serve

# Production build (outputs to ../adapteros-server/static/)
trunk build --release

# Check WASM compilation
cargo check -p adapteros-ui --target wasm32-unknown-unknown

# Unit tests (native)
cargo test -p adapteros-ui --lib
```

## Database

```bash
# Run migrations
./aosctl db migrate

# Prepare SQLx offline mode
cargo sqlx prepare --workspace
```

## System Commands (Darwin/macOS)

```bash
# List files
ls -la

# Find files
find . -name "*.rs" -type f

# Search in files (ripgrep preferred)
rg "pattern" --type rust

# Git operations
git status
git diff
git log --oneline -10

# Process viewing
ps aux | grep adapteros

# Compilation cache stats
sccache --show-stats
```

## Metal Shaders

```bash
cd metal && bash build.sh
```

## Cleanup

```bash
# Clean crate-level test artifacts
find ./crates -type d -name "var" -not -path "*/target/*" -exec rm -rf {} +

# Clean test databases
rm -f ./var/*-test.sqlite3* ./var/*_test.sqlite3*
```
