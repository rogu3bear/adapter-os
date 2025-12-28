# SQLx Offline-First Workflow

SQLx compile-time query checks rely on a committed cache so builds can run with `SQLX_OFFLINE=1`.

## Prereqs

- `cargo sqlx` available:
```bash
cargo install sqlx-cli --version 0.8.2 --no-default-features --features sqlite
```

## Minimal recipe (SQLite, no server needed)

1) Start DB (file-based):
```bash
export DATABASE_URL="sqlite://./var/sqlx-dev.sqlite3"
```

2) Run migrations:
```bash
cargo sqlx migrate run
```

3) Prepare cache:
```bash
SQLX_OFFLINE_DIR=crates/adapteros-db/.sqlx \
  cargo sqlx prepare --workspace -- \
  --package adapteros-db \
  --package adapteros-server-api \
  --package adapteros-server
```

4) Verify offline build:
```bash
SQLX_OFFLINE=1 SQLX_OFFLINE_DIR=crates/adapteros-db/.sqlx \
  cargo check -p adapteros-server-api
```

Or run the scripted flow:
```bash
./scripts/sqlx_prepare.sh
```

## When to update the cache

- Any change to SQLx query macros (`query!`, `query_as!`, `query_scalar!`)
- Any migration that changes schema
- Any backend switch (SQLite vs Postgres) since caches are backend-specific

Commit updates in `crates/adapteros-db/.sqlx/`.

## Postgres or other backend

- Set `DATABASE_URL` to your local instance.
- Run migrations for that backend, then run the same prepare + offline check steps.
- Do not commit personal DB URLs.

## Avoid

- Do not replace `query!` with `query` just to “get it compiling.”
- Do not disable SQLx compile-time checks permanently.
- Do not bake personal DB URLs into code or docs.
