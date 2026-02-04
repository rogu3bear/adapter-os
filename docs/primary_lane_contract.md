# primary lane contract

Checklist and acceptance criteria for the primary lane.

## Contract checklist
- [x] Auth to Chat: `/login` -> `/chat` with input ready (`data-testid="chat-input"`).
  Where: `crates/adapteros-ui/src/pages/chat.rs`
  Test: `tests/playwright/ui/primary.lane.spec.ts`
- [x] Inference (streaming + non-streaming as available) completes with response.
  Where: `crates/adapteros-ui/src/signals/chat.rs`
  Test: `tests/playwright/ui/primary.lane.spec.ts`
- [x] Run detail view shows provenance summary and receipt verification.
  Where: `crates/adapteros-ui/src/pages/flight_recorder.rs`, `crates/adapteros-ui/src/components/trace_viewer.rs`
  Test: `tests/playwright/ui/primary.lane.spec.ts`
- [x] Token decisions view supports safe paging with render cap and incremental load.
  Where: `crates/adapteros-ui/src/components/trace_viewer.rs`
  Test: `tests/playwright/ui/primary.lane.spec.ts`
- [x] Model readiness path is available when inference is blocked.
  Where: `crates/adapteros-ui/src/components/inference_guidance.rs`, `crates/adapteros-ui/src/pages/chat.rs`
  Test: `tests/playwright/ui/primary.lane.spec.ts`

## perf notes
- Metrics used (behind `perf_logging_enabled()`):
  - streaming time to first token
  - non-streaming completion time
  - run detail load time (API + render)
  - token decisions paging latency
- Improvements:
  - Added run detail render-ready timing to complement trace load time logs.
  - UI trace detail endpoint defaults to bounded token decision paging (limit 200); UI uses paged load with "Show more".
- Deferred:
  - Backend receipt verification caching review (current UI uses stored parity flag; no recompute on read).

## hang audit
- MLX activation function tests hung on Metal command buffer completion: serialize tests with a shared lock.
  Where: `crates/adapteros-lora-mlx-ffi/src/lib.rs`, `crates/adapteros-lora-mlx-ffi/tests/activation_functions_tests.rs`

## test mapping table
- Auth to Chat -> `tests/playwright/ui/primary.lane.spec.ts` -> login + chat input ready
- Inference streaming -> `tests/playwright/ui/primary.lane.spec.ts` -> response rendered
- Run detail provenance + receipt -> `tests/playwright/ui/primary.lane.spec.ts` -> provenance summary + receipt status visible
- Token decisions paging -> `tests/playwright/ui/primary.lane.spec.ts` -> expand + show more + row count
- Model readiness guardrail -> `tests/playwright/ui/primary.lane.spec.ts` -> inference not ready banner + error
