# Integration Landscape

## Observability & telemetry
- Prometheus/OpenMetrics exporters live in `crates/adapteros-telemetry/src/lib.rs` and `crates/adapteros-metrics-exporter/src/lib.rs`; the `/metrics` handler is wired through `crates/adapteros-server-api/src/handlers/metrics.rs` and the contract published under `docs/api/openapi.json`.
- External push gateways are addressed by `scripts/metrics-bridge.sh`, which streams metrics from `AOS_METRICS_SOCKET` (default `/var/run/aos/default/metrics.sock`) to `PROMETHEUS_PUSH_GATEWAY` every `METRICS_PUSH_INTERVAL` seconds; this script also references `PUSH_GATEWAY`, `PROMETHEUS_JOB`, and health-checks for `socat` + `curl`.
- OpenTelemetry tracing is initialized inside `crates/adapteros-server/src/otel.rs` and defaults to collector_port `4317` from `crates/adapteros-core/src/defaults.rs`, so OTLP collectors and downstream observability stacks must honor that port plus `AOS_OPENTELEMETRY_ENDPOINT`-style overrides defined in the server config.

## Data & persistence
- SQLite is the default control plane store (`configs/cp-auth-example.toml` points to `var/aos-cp.sqlite3` with WAL enabled), but PostgreSQL support appears via `migrations/0029_pgvector_rag.sql`, `migrations/postgres/0201_adapter_version_publish_attach.sql`, and the `DbFactory` defaults referenced from `configs/TIMEOUTS.md`. Any deployment switching to PostgreSQL must satisfy `AOS_DB_ACQUIRE_TIMEOUT_SECS`/`DB_ACQUIRE_TIMEOUT` expectations documented in `configs/TIMEOUTS.md` (idle/acquire timeouts).
- Model artifacts are listed in `manifests/` (e.g., `qwen7b-mlx.yaml`, `llama3.2-3b-instruct-4bit.yaml`); `crates/adapteros-model-hub` and the worker boot path (`crates/adapteros-aos`, `aos-worker`) consume these manifests, so upstream storage (object/store location) must match the manifest `model_url` entries.

## Security & zero-egress
- `configs/cp-auth-example.toml` / `configs/mlx-production.toml` enable `security.require_pf_deny`, which `crates/adapteros-server-api/src/handlers/settings.rs` surfaces as `require_pf_deny` for UI control; `scripts/deploy-production.sh` double-checks PF activation via `pfctl -s info` before allowing zero-egress mode, and `configs/production-multinode.toml` mirrors the same requirement.
- Packet-filter rules are expected at `/etc/pf.anchors/adapteros` per the air-gap plan described in `docs/DEPLOYMENT.md` and `training/synthesis_model/data/train.jsonl` (air-gap airg). Deployments must call `pfctl -f /etc/pf.conf` + `pfctl -a adapteros ...` to enforce `require_pf_deny=true`.

## External runtime dependencies
- Apple Silicon accelerators rely on the Metal/CoreML stack: `crates/adapteros-lora-kernel-mtl` (macOS target dependency) and the workspace feature `coreml-backend` in `Cargo.toml`. Deployments on Apple hardware also require Homebrew-installed `mlx` (comment in `README.md`/`Cargo.toml`) and `metal` command-line tools (`xcrun`), which the `docs/DEPLOYMENT.md` procedure and `training/synthesis_model` notes recommend verifying (`metal/build.sh`, `xcrun --version`).
- The lightweight `start` orchestrator (`./start` script in `docs/DEPLOYMENT.md`) launches `adapteros-server` + `aos-worker` with `scripts/lib/env-loader.sh` and the `monitoring/` stack; any external service (PostgreSQL, Prometheus, OTLP collector, PF) must be reachable from the host where `./start` runs.

## TODO
- [ ] List any third-party KMS/Vault connections if this repo begins wiring HashiCorp secrets (default port `8200` is declared in `crates/adapteros-core/src/defaults.rs` but not yet configured).
- [ ] Track additional downstream dashboards (Grafana provisioning files in `monitoring/grafana/provisioning/`) to keep alert rules / datasource credentials in sync with deployed Prometheus endpoints.
