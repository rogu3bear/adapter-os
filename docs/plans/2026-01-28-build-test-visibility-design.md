# Build & Test Visibility Quick Wins

**Date:** 2026-01-28
**Status:** Implemented

## Problem

With 83 crates and a 104GB target directory, `cargo build` and `cargo test` provide minimal feedback during execution. Developers see no progress indication for minutes, making it impossible to know if the build is stuck, which crates are slow, or how much work remains.

## Solution: Quick Wins

Enable existing tooling with minimal configuration changes.

### 1. sccache (Compilation Cache)

**What:** Shared compilation cache that persists across `cargo clean`.

**Configuration:** Enabled in `.cargo/config.toml`:
```toml
[build]
rustc-wrapper = "sccache"
```

**Usage:**
- Automatic — all builds use cache
- Check stats: `sccache --show-stats`
- Clear cache: `sccache --zero-stats`

**Impact:** 30-50% faster rebuilds after clean.

### 2. Build Timing Analysis

**What:** Cargo's built-in timing report showing crate-level breakdown.

**Usage:**
```bash
cargo tb        # timed build (alias)
cargo tbr       # timed release build
# Opens: target/cargo-timings/cargo-timing.html
```

**Impact:** Identifies slow crates and parallelism bottlenecks.

### 3. cargo-nextest (Better Test Runner)

**What:** Modern test runner with progress bars, parallel execution, and clear output.

**Installation:** `cargo install cargo-nextest`

**Usage:**
```bash
cargo nt        # run all tests with progress
cargo ntf       # fail-fast with immediate output
cargo nextest run -p adapteros-core  # single crate
```

**Configuration:** `.config/nextest.toml` with:
- Fail-fast by default
- Immediate failure output
- 30s slow test warning
- JUnit XML output for CI

**Impact:** Real-time progress, faster execution, clearer output.

### 4. Progress Environment Variable

**What:** Forces cargo to always show progress bar.

**Configuration:** In `.cargo/config.toml`:
```toml
[env]
CARGO_TERM_PROGRESS_WHEN = "always"
```

## New Aliases

| Alias | Command | Purpose |
|-------|---------|---------|
| `cargo tb` | `build --workspace --timings` | Timed build |
| `cargo tbr` | `build --workspace --release --timings` | Timed release build |
| `cargo bv` | `build --workspace -v` | Verbose build |
| `cargo nt` | `nextest run --workspace` | Run tests with progress |
| `cargo ntf` | `nextest run --workspace --failure-output=immediate` | Tests with immediate failures |
| `cargo c` | `check --workspace` | Quick type check |

## Files Changed

- `.cargo/config.toml` — sccache, env vars, aliases
- `.config/nextest.toml` — nextest configuration (new)
