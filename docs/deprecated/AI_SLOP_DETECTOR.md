# AI Slop Detector

Lightweight helper to surface high-priority issues (error typing, determinism) and advisory signals. Scoped to `crates/` and excludes `tests/`, `benches/`, `examples/`, `fixtures/`, and `mocks/`.

## Usage

```bash
# Default: runs high-priority checks and `make dup` (duplication gate)
bash ai_slop_detector.sh

# Skip duplication gate if needed
RUN_MAKE_DUP=0 bash ai_slop_detector.sh

# Run authoritative lint too (preferred in CI)
RUN_ADAPTEROS_LINT=1 bash ai_slop_detector.sh
```

## Interpretation

- **Fail conditions:** Any high-priority finding (>0) for error typing (`AosError` required) or determinism violations (non-deterministic spawns/RNG in deterministic paths).
- **Blocking duplication gate:** `make dup` runs by default; set `RUN_MAKE_DUP=0` to disable.
- **Informational only:** Heuristic duplication/boilerplate/TODO counts; rely on `adapteros-lint` and `make dup` for authoritative duplication/pattern enforcement.
- **Scope:** Production code under `crates/`; ignores examples/fixtures/tests/benches/mocks.

## Best Practices

- Fix or explicitly exclude generic error usages; production paths should return `AosError`.
- Keep determinism clean: use `spawn_deterministic` in deterministic contexts and HKDF-derived seeds instead of `tokio::spawn`, `std::thread::spawn`, `rand::thread_rng`, or `StdRng::from_entropy`.
- Treat heuristic sections as advisory; rely on `adapteros-lint` and `make dup` for actionability.
