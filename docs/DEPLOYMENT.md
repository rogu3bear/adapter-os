# AdapterOS Production Deployment Guide

## Prerequisites
- macOS 13.0+ with Apple Silicon (M1+)
- Rust 1.75+: `rustup install stable`
- ≥16GB RAM, Postgres 15+ for prod (SQLite for dev)
- MLX: `pip install mlx` (optional, experimental【@Docs README.md §103】)

## Database Setup
### Development (SQLite)
Default: `var/aos.db`. Init: `./target/release/aosctl init-tenant --id default`.

### Production (Postgres + pgvector for RAG)
1. Install Postgres/pgvector: Use docker-compose【@Docs rag-pgvector.md §docker】:
   ```
   version: '3.8'
   services:
     postgres:
       image: pgvector/pgvector:pg15
       environment:
         POSTGRES_DB: adapteros
         POSTGRES_USER: aos
         POSTGRES_PASSWORD: aos
       ports:
         - "5432:5432"
   ```
2. Set `DATABASE_URL=postgresql://aos:aos@localhost/adapteros`.
3. Run migrations: `cargo run --bin adapteros-db -- migrate up --database-url $DATABASE_URL`【@Docs database-schema/migrations.md §10】.
4. RAG: Enable `--features rag-pgvector`, set `RAG_EMBED_DIM=3584`【@Docs README.md §190】. Index auto-creates on startup.

Rollback: `migrate down` with step count【@Docs database-schema/migrations.md §30】.

## Build and Run
1. `cargo build --release --features rag-pgvector` (for prod backend【@Docs README.md §103】).
2. Init: `./target/release/aosctl init-tenant --id prod`.
3. Import model: `./target/release/aosctl import-model --name qwen2.5-7b --weights models/...`【@Docs QUICKSTART.md §110】.
4. Serve: `./target/release/aosctl serve --plan prod-plan --config configs/prod.toml` (port 8080, Unix sockets【@Docs control-plane.md §15】).
5. API: `curl -H 'Authorization: Bearer $TOKEN' http://localhost:8080/v1/healthz`.

Multi-node: Set `AOS_FEDERATION_MODE=cluster`, use `adapteros-federation` for sync【@Docs federation.md §1】. Leader election via DB.

## Monitoring and Observability
- Telemetry: Logs to JSON/Merkle trees in `var/telemetry/`【@Docs telemetry.md §1】. Export to Prometheus: Enable in config, hit `/metrics`【@Docs README.md §431】.
- Prometheus: Scrape /metrics every 15s (counters: inference_total, errors_total【@crates/adapteros-metrics-exporter/src/lib.rs】). Alert queries: rate(adapteros_errors_total[5m]) > 10 (anomaly【@prometheus docs§alerting】). Grafana dashboard: Import json for AdapterOS metrics.
- Policies: Enforce 20 packs on startup【@Docs POLICIES.md §1】; audit via `/v1/audit/compliance`.
- Drift: Environment fingerprinting auto-baselines【@Docs determinism.md §16】.

## Configuration
TOML precedence: CLI > env > file【@Docs CONFIG_PRECEDENCE.md §1】. Freeze for determinism: `aosctl freeze-config`【@Docs CONFIG_PRECEDENCE.md §freeze】.

## Edge Cases and Troubleshooting
- Migrations Rollback: `migrate down 1` for last step【@Docs database-schema/migrations.md §30】.
- Zero-Egress: Verify with `tcpdump` (no outbound【@Docs runaway-prevention.md §egress】).
- Low RAM: Auto-evict ephemeral adapters (≥15% headroom【@Docs README.md §406】).
- RAG Fail: If pgvector down, fallback in-memory (less scalable【@Docs rag-pgvector.md §189】).

For full API: Run `cargo doc --open`【@Docs README.md §451】. Questions? See @Docs architecture.md.
