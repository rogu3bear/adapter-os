# Phase 02-02 Summary: CI ASAN + Concurrent Hot-Swap Stress

## Scope Executed
- `.github/workflows/ci.yml`
- `crates/adapteros-lora-mlx-ffi/tests/error_handling_tests.rs`

## What Changed
- Added push-triggered ASAN CI lane in existing workflow (no new workflow file):
  - Job: `ffi-asan`
  - Uses nightly toolchain
  - Sets `RUSTFLAGS="-Zsanitizer=address"`
  - Runs focused FFI stress selector:
    - `cargo +nightly test -p adapteros-lora-mlx-ffi --test error_handling_tests -- concurrency_error_tests::test_concurrent_hotswap_under_inference_load --nocapture`
- Extended existing `concurrency_error_tests` module with one targeted stress test:
  - `test_concurrent_hotswap_under_inference_load`
  - Overlaps inference loop (`run_step`) with concurrent adapter `load_adapter_runtime`/`unload_adapter_runtime` churn.

## Verification Evidence
1. CI wiring present
- Command:
  - `rg -n "ffi-asan|sanitizer=address|concurrent_hotswap_under_inference_load|if: github.event_name == 'push'" .github/workflows/ci.yml`
- Outcome:
  - Found job and sanitizer wiring:
    - `ffi-asan`
    - `if: github.event_name == 'push'`
    - `RUSTFLAGS: "-Zsanitizer=address"`
    - focused stress selector command present.

2. Targeted stress test passes locally
- Command:
  - `cargo test -p adapteros-lora-mlx-ffi --test error_handling_tests concurrency_error_tests::test_concurrent_hotswap_under_inference_load -- --nocapture`
- Outcome:
  - Passed:
    - `running 1 test`
    - `... test_concurrent_hotswap_under_inference_load ... ok`
    - `test result: ok. 1 passed; 0 failed`.

3. Local ASAN dry-run of CI command
- Command:
  - `RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -p adapteros-lora-mlx-ffi --test error_handling_tests -- concurrency_error_tests::test_concurrent_hotswap_under_inference_load --nocapture`
- Outcome:
  - Failed on local macOS due sanitizer runtime injection requirement:
    - `Interceptors are not working ... DYLD_INSERT_LIBRARIES=...librustc-nightly_rt.asan.dylib`
  - This does not invalidate Linux CI wiring; it indicates host-specific ASAN runtime setup is required for local macOS execution.

## Residual Risk
- ASAN lane was added as blocking in workflow job semantics, but merge-gate “required check” policy still depends on repository branch protection settings (manual governance step).
- Local ASAN validation is environment-limited on this macOS host without `DYLD_INSERT_LIBRARIES`; authoritative ASAN validation is expected in Linux CI.
