# Phase 4 Execution: Control Plane Contract Convergence

## Objective
- Eliminate configuration and readiness contract ambiguity without introducing duplicate state surfaces.

## Deliverable A: Config Parity Matrix

| Field group | Server source | API source | Status |
|---|---|---|---|
| `routing.use_session_stack_for_routing` | `crates/adapteros-server/src/config.rs` (`RoutingConfig`) | `crates/adapteros-server/src/boot/api_config.rs` (`build_api_config`) | Drift: API config is still hardcoded to `false`. |
| `server.health_check_worker_timeout_ms` | `crates/adapteros-server/src/config.rs` | `crates/adapteros-server/src/boot/api_config.rs` + `crates/adapteros-server-api/src/handlers/health.rs` | Drift: fixed `5000` in API config path. |
| `server.health_check_models_timeout_ms` | `crates/adapteros-server/src/config.rs` | `crates/adapteros-server/src/boot/api_config.rs` + `crates/adapteros-server-api/src/handlers/health.rs` | Drift: fixed `15000` in API config path. |
| `metrics`, `paths`, `auth`, most `security` fields | `crates/adapteros-server/src/config.rs` | `crates/adapteros-server/src/boot/api_config.rs` and `crates/adapteros-server-api/src/state.rs` | Mostly parity; selective mapping still requires explicit contract table. |

## Deliverable B: Boot -> `readyz` Invariant Map

| Boot phase/state | Readiness check | Notes |
|---|---|---|
| DB connect/ready | `readyz` DB check in `crates/adapteros-server-api/src/handlers/health.rs` | Correctly re-validates at request time, can differ from boot-time pass. |
| Worker attach | worker-count check in `readyz` | Correctly dynamic, can expose post-boot worker loss. |
| Model-server readiness | model-seeded checks in `readyz` | Correctly dynamic; verifies runtime state, not only boot result. |
| `BootState::Ready` transition | `boot_state.is_ready()` gates in `readyz` | Shared contract exists; needs explicit mapping document to avoid future drift. |

## Deliverable C: Zero-Duplicate Status Model
- Authoritative in-memory task state: `BackgroundTaskTracker` in `crates/adapteros-server-api/src/state.rs`.
- Authoritative persisted operation timeline: `ProgressService` in `crates/adapteros-server-api/src/progress_service.rs`.
- Transport/replay channel only: `SseEventManager` in `crates/adapteros-server-api/src/sse/event_manager.rs`.

### Migration order (no duplicate storage)
1. Ensure every long-running task is registered through `BackgroundTaskSpawner` in `crates/adapteros-server/src/boot/background_tasks.rs` and recorded by tracker hooks.
2. Bridge tracker lifecycle updates into `ProgressService` events keyed by stable operation IDs.
3. Stream progress events through `SseEventManager` ring buffers only (no secondary persistence).

## Verification Run
- Ran parity scan:
`rg -n "use_session_stack_for_routing|health_check_worker_timeout_ms|health_check_models_timeout_ms|build_api_config|ready\(" crates/adapteros-server/src/config.rs crates/adapteros-server/src/boot/api_config.rs crates/adapteros-server-api/src/handlers/health.rs crates/adapteros-server-api/src/state.rs`
- Result: drift anchors confirmed in source.

- Ran targeted readiness tests:
`cargo test -p adapteros-server-api --test readyz_failure_modes`
- Result: passed (`10 passed`).
- Readiness harness alignment confirmed:
  - `crates/adapteros-server-api/tests/readyz_failure_modes.rs`
  - `advance_boot_to_ready()` includes `boot_state.worker_discovery().await` before `boot_state.ready().await`, matching strict boot transition order.

`cargo test -p adapteros-server-api --test health_readyz_timeout_tests`
- Result: passed (`31 passed`).

## Phase 4 Completion
- [x] Config drift matrix delivered.
- [x] Boot/readiness invariant map delivered.
- [x] Zero-duplicate status model and migration order delivered.
- [x] Verification gates are green after minimal readiness-test alignment.
