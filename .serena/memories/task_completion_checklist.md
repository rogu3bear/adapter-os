# Task Completion Checklist

When completing a coding task in AdapterOS, run through this checklist:

## 1. Code Quality

- [ ] Run `cargo fmt --all` to format code
- [ ] Run `cargo clippy --workspace -- -D warnings` to check lints
- [ ] Run `cargo c` (quick type check) to verify compilation

## 2. Testing

- [ ] Run `cargo test -p <affected-crate>` for changed crates
- [ ] Run `cargo test --workspace` if changes span multiple crates
- [ ] For determinism-critical changes: `cargo test --test determinism_core_suite`

## 3. UI Changes (if applicable)

- [ ] Run `cargo check -p adapteros-ui --target wasm32-unknown-unknown` to verify WASM compiles
- [ ] Run `trunk serve` and test in browser
- [ ] Run `cargo test -p adapteros-ui --lib` for unit tests

## 4. Database Changes (if applicable)

- [ ] Run `./aosctl db migrate` to apply migrations
- [ ] Run `cargo sqlx prepare --workspace` for offline mode

## 5. Determinism Rules (for inference/training changes)

- [ ] Verify seed derivation uses HKDF-SHA256
- [ ] Check router tie-breaking: score DESC, stable_id ASC
- [ ] Confirm no `-ffast-math` flags
- [ ] Test with `AOS_DEBUG_DETERMINISM=1` if debugging

## 6. File Hygiene

- [ ] No files created outside `./var/` for runtime data
- [ ] Clean up test artifacts: `rm -f ./var/*-test.sqlite3*`
- [ ] No new crate-level `var/` directories

## 7. Documentation (if API changes)

- [ ] Update doc comments for public APIs
- [ ] Update CLAUDE.md if new commands/patterns introduced

## 8. Security Review

- [ ] No command injection vulnerabilities
- [ ] No credentials in code
- [ ] Validate at system boundaries only

## Quick Pre-Commit Commands

```bash
# Format and lint
cargo fmt --all && cargo clippy --workspace -- -D warnings

# Type check
cargo c

# Run tests
cargo test --workspace
```
