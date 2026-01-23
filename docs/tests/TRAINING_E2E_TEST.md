# Golden Path E2E Harness

This harness makes it possible to exercise the full “upload → train → package → infer” stack by wiring tests to a real worker process. It supports two modes:

- **spawn** – run `aos_worker` locally with an ephemeral socket and temp roots.
- **external** – attach to an already-running worker via its UDS path.

## Environment Contract

Set the following test-only variables before running the harness-driven tests:

- `AOS_TEST_E2E_ENABLE=1` – opt in; otherwise tests skip with a message.
- `AOS_TEST_WORKER_MODE=spawn|external` – select worker strategy.
- `AOS_TEST_MODEL_ID` – ID to seed into the DB for the base model.
- `AOS_TEST_MODEL_DIR` – path to the base model directory.
- `AOS_TEST_TOKENIZER_DIR` – optional tokenizer path (forwarded to the worker).
- `AOS_TEST_WORKER_UDS_PATH` – required when `worker_mode=external`.
- `AOS_TEST_WORKER_BACKEND` – optional backend hint (`mlx|metal|coreml|cpu|auto`) for worker registration/spawn.

Optional helpers:

- `AOS_TEST_WORKER_MANIFEST` – manifest file for the spawned worker (falls back to `manifests/reference.yaml`).
- `AOS_TEST_WORKER_BIN` – explicit path to the `aos_worker` binary (otherwise tries `CARGO_BIN_EXE_aos_worker`, `target/debug`, then `target/release`).
- `AOS_MODEL_CACHE_MAX_MB` – cache budget for the spawned worker (defaults to `1024` if unset).

Legacy fallbacks (tests only):

- `AOS_E2E_HARNESS=1` is accepted in place of `AOS_TEST_E2E_ENABLE=1`.
- `AOS_E2E_MODEL_PATH` or `AOS_TEST_MODEL_PATH` are accepted in place of `AOS_TEST_MODEL_DIR`.
- `AOS_E2E_UDS` is accepted in place of `AOS_TEST_WORKER_UDS_PATH`.
- `AOS_E2E_BACKEND` or `AOS_E2E_TRAINING_BACKEND` are accepted in place of `AOS_TEST_WORKER_BACKEND`.

## What the Harness Sets Up

- Ephemeral roots under `var/tmp/e2e-harness/<uuid>/` for artifacts, adapters, datasets, plans, and documents.
- An in-memory SQLite DB with the base model registered at `AOS_TEST_MODEL_ID` and its `model_path` pointing to `AOS_TEST_MODEL_DIR`.
- A unique worker socket path at `var/tmp/e2e-harness/run/<uuid>/worker.sock` (or the provided external path).
- `AOS_WORKER_SOCKET` is pointed at that socket so control-plane clients use the same UDS.
- `AOS_DEV_NO_AUTH=1` is set in debug builds so tests can call endpoints without a token.
- Spawn mode starts `aos_worker` with `AOS_DEV_NO_AUTH=1`, forwards model/tokenizer paths, and captures stdout/stderr on failures. All waits use bounded timeouts to avoid hangs.

## External Mode Readiness

When `worker_mode=external`, the harness only verifies that the configured UDS socket is connectable within a short timeout. If it cannot connect, the test reports the reason and exits.

## Running the Golden Path

After exporting the env vars above, run:

```bash
cargo test -p adapteros-server-api --test golden_path_api_e2e -- --nocapture
```

Notes:
- The golden path includes streaming inference and prints recent job progress on failures.
- The chunked upload full-loop test lives in the same binary. To run just that test:

```bash
cargo test -p adapteros-server-api --test golden_path_api_e2e test_chunked_upload_full_loop_e2e_harness -- --nocapture
```

## Running the Smoke Test

After exporting the env vars above, run:

```bash
cargo test -p adapteros-server-api --test e2e_harness_smoke -- --nocapture
```

Expected outcomes:

- If required env vars are missing, the test prints a skip reason and exits.
- In spawn mode, the worker socket must appear; otherwise the captured worker output is printed.
- In external mode, the UDS path must be reachable; otherwise the test reports the connection error.
