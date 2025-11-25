# AI Slop Detector

Lightweight helper to surface high-priority issues (generic errors, determinism) and optional advisory signals. Scoped to `crates/` and excludes `tests/`, `benches/`, `examples/`, `fixtures/`, and `mocks/`.

## Usage

```bash
# Default: only high-priority checks affect exit code
bash ai_slop_detector.sh

# Run authoritative checks too (preferred in CI)
RUN_ADAPTEROS_LINT=1 RUN_MAKE_DUP=1 bash ai_slop_detector.sh
```

## Interpretation

- **Fail conditions:** Any high-priority finding (>0) for `AosError` usage or determinism patterns.
- **Informational only:** Duplication/boilerplate/domain-context/TODO heuristics; use `adapteros-lint` and `make dup` for authoritative duplication/pattern enforcement.
- **Scope:** Production code under `crates/`; ignores examples/fixtures/tests/benches/mocks.

## Best Practices

- Fix or explicitly exclude generic error usages; production paths should return `AosError`.
- Keep determinism clean (use `spawn_deterministic`/HKDF per AGENTS.md where required).
- Treat heuristic sections as advisory; rely on `adapteros-lint` and `make dup` for actionability.
