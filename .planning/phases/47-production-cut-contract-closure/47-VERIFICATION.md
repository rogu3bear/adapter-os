# Phase 47 Verification

## Commands

```bash
./start preflight
./aosctl --rebuild --help
./aosctl rebuild --help
bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'
bash /Users/star/.codex/skills/gsd-codex-artifacts/scripts/run_health.sh --cwd /Users/star/Dev/adapter-os
```

## Expected Results

1. Preflight rejects invalid model path before backend launch.
2. `aosctl` rebuild path is resilient under missing/invalid `DATABASE_URL`.
3. Governance preflight behavior matches enforced default lane.
4. Planning health no longer reports phase-47 directory mismatch (`W006`).

