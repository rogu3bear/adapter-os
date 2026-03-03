# Software Stack Snapshot

## Rust workspace foundation
- `Cargo.toml` describes a Rust 2021 workspace with the control plane (`crates/adapteros-server`, `crates/adapteros-server-api`), routing/runtime (`crates/adapteros-core`, `crates/adapteros-lora-worker`, `crates/adapteros-lora-router`), telemetry (`crates/adapteros-telemetry`, `crates/adapteros-metrics-exporter`), tooling (`crates/adapteros-cli`, `crates/adapteros-aos`, `crates/adapteros-tui`), agent orchestration (`crates/adapteros-agent-spawn`), retrieval/RAG (`crates/adapteros-retrieval`, `crates/adapteros-lora-rag`), and UI (`crates/adapteros-ui`, `crates/adapteros-api-types`) members.
- Workspace features such as `multi-backend`, `coreml-backend`, and `production-macos` wire together `adapteros-lora-mlx-ffi`, `adapteros-lora-kernel-mtl`, and the CoreML/Metal kernels that are guarded by `target_os = "macos"` snippets at the bottom of `Cargo.toml`.
- `rust-toolchain.toml` locks the stable toolchain with `rustfmt` and `clippy`, so contributors build with the same formatter/linter combination.

## Control plane runtime
- The HTTP/API surface leans on `tokio`, `axum`, `tower`, `hyper`, `sqlx`, `serde`, `tracing`, `tracing-subscriber`, and the `utoipa` stack that appears in `Cargo.toml` to power the single-page API plus `/metrics` routes documented in `docs/api/openapi.json`.
- Deterministic routing relies on the guarantees spelled out in `DETERMINISM.md`—HKDF seeding, canonical serialization, and replay-able receipts—so low-level math and serialization helpers live in `crates/adapteros-deterministic-exec`, `crates/adapteros-replay`, and `crates/adapteros-trace` per the workspace list.
- `README.md` outlines the CLI flow (`./aosctl --rebuild`, `./aosctl db migrate`, `./start`), so the `adapteros-cli` crate plus the `start` script remain the primary developer entry points; `docs/DEPLOYMENT.md` explains how `./start` drives `adapteros-server` plus `aos-worker` and where the Leptos UI output lands (`crates/adapteros-server/static/`).

## Storage & persistence
- Persistence glue uses `rusqlite`, `refinery`, `sqlx` (SQLite + optional PostgreSQL), and `libsqlite3-sys` references in `Cargo.toml`; the example config under `configs/cp-auth-example.toml` shows the default `var/aos-cp.sqlite3` path and WAL mode.
- Production-grade migrations include PostgreSQL/pgvector support (`migrations/0029_pgvector_rag.sql` and `migrations/postgres/0201_adapter_version_publish_attach.sql`), so connectable storage backends range from the embedded `DbFactory` defaults to the PostgreSQL-specific pools referenced in `configs/TIMEOUTS.md` and the `AOS_DB_ACQUIRE_TIMEOUT_SECS` override env var.

## Inference & backend accelerators
- Metal/CoreML/ANE kernels surface via `crates/adapteros-lora-kernel-mtl` (macOS target dependency) and the `coreml-backend` feature, while the MLX-hosted inference loop lives in `crates/adapteros-lora-mlx-ffi` plus `crates/adapteros-lora-worker` per the workspace ordering in `Cargo.toml`.
- Model manifests in `manifests/` feed the `crates/adapteros-model-hub` downloader and worker bootstrapping so the same `adapteros-lora-kernel-api` and `adapteros-lora-quant` layers run against the vectorized adapters defined there.

## Observability & UI surface
- Prometheus-style metrics come from `crates/adapteros-telemetry/src/lib.rs`/`crates/adapteros-metrics-exporter/src/lib.rs` and are documented in `crates/adapteros-server-api/src/handlers/metrics.rs` (exposing `/metrics`).
- OpenTelemetry tracing is bootstrapped inside `crates/adapteros-server/src/otel.rs`, while the default collector port (`4317`) appears in `crates/adapteros-core/src/defaults.rs`.
- The Leptos WASM UI in `crates/adapteros-ui` is built with `Trunk.toml` and delivered through `adapteros-server/static/` as described in `docs/DEPLOYMENT.md`; `docs/api/openapi.json` embeds the Swagger contract used by both UI and API consumers.

## TODO
- [ ] Keep `docs/api/openapi.json` aligned with new `adapteros-server-api` routes so front-end and CLI clients stay in sync.
- [ ] Audit any new `migrations/` additions for both SQLite and PostgreSQL branches to keep the dual-backend story intact.
