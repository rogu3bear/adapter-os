# RAG PgVector Backend

This document explains how to enable and operate the PostgreSQL pgvector backend for the RAG subsystem while preserving the default in-memory behavior.

## Overview

- Backend selection is controlled by a compile-time feature flag.
- Default builds use in-memory per-tenant indices with a synchronous `RagSystem` API.
- When the `rag-pgvector` feature is enabled, `RagSystem` can be constructed from a PostgreSQL-backed `PgVectorIndex`.

## Build

```bash
# Default (in-memory)
cargo build -p adapteros-cli

# With PostgreSQL pgvector backend
cargo build -p adapteros-cli --features rag-pgvector
```

## Environment

- `DATABASE_URL`: PostgreSQL connection string (e.g., `postgresql://aos:aos@localhost/adapteros`).
  - If unset, `adapteros_db::postgres::PostgresDb::connect_env()` defaults to `postgresql://aos:aos@localhost/adapteros`.
- `RAG_EMBED_DIM`: Embedding dimension for vector column. Defaults to `3584`.
- `AOS_INSECURE_SKIP_EGRESS`: Set to `1/true/yes` to skip PF preflight checks in development.

## CLI Serve (feature-gated init)

When `rag-pgvector` is enabled, the CLI connects to PostgreSQL, runs migrations, and initializes the RAG system with a pgvector index. Otherwise, it uses the in-memory backend.

```bash
# macOS dev (skip PF preflight) with Postgres RAG
AOS_INSECURE_SKIP_EGRESS=1 \
DATABASE_URL=postgresql://aos:aos@localhost/adapteros \
RAG_EMBED_DIM=3584 \
 cargo run -p adapteros-cli --features rag-pgvector -- \
  serve --tenant default --plan <plan> --socket /tmp/aos.sock

If your policy enables open-book (evidence) mode, serve refuses to start without a RAG backend. Ensure pgvector is reachable or create a local index under `./var/indices/<tenant>`.
```

Relevant code:
- CLI pg init: `crates/adapteros-cli/src/commands/serve.rs`
- PgVectorIndex: `crates/adapteros-lora-rag/src/pgvector.rs`
- RagSystem backend switch: `crates/adapteros-lora-rag/src/lib.rs`
- PostgreSQL migrations: `migrations_postgres/`

## Migrations

Migrations are applied by the control-plane DB helper (`PostgresDb::migrate()`):

- Schema (tables, indices): `migrations_postgres/0001_init_pg.sql`
- pgvector index: `migrations_postgres/0002_pgvector.sql`

Ensure the `vector` extension exists:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

## Deterministic Retrieval

The pgvector path orders results with stable tie-breaking:

```sql
ORDER BY score DESC, doc_id ASC
```

Implemented in `PgVectorIndex::retrieve_postgres` so that retrieval is deterministic across runs.

## Docker Compose (Postgres + pgvector)

Use the provided compose file to run a local Postgres with pgvector:

```bash
docker compose -f scripts/docker-compose.postgres.yml up -d

export DATABASE_URL=postgresql://aos:aos@localhost:5432/adapteros
cargo run -p adapteros-cli --features rag-pgvector -- serve --tenant default --plan <plan>
```

## Notes

- The `RagSystem` API remains synchronous. Internally, pg operations are awaited without changing caller signatures.
- For production, keep PF preflight enabled and avoid the insecure bypass. Dev-only skip is available via env or hidden CLI flag.
