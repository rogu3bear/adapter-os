# AdapterOS Control Plane

## Overview

The AdapterOS Control Plane (`aos-cp`) is a Rust-based orchestration service that manages workers, configuration plans, telemetry, and policy enforcement across your AdapterOS cluster.

## Architecture

```
┌──────────────────────────────┐
│           Web UI             │
│   (future, talks to CP API)  │
└──────────────┬───────────────┘
              HTTPS / UDS
┌──────────────────────────────────────────────────────────────────┐
│                        aos-cp (Control Plane)                    │
│  axum API  ───────────────────────────────────────────┐          │
│  Jobs/Scheduler  ─────────┐                          │          │
│  Registry/DB (SQLite)     │     Telemetry indexer     │          │
│  Policy engine            │     (bundles, reports)    │          │
│  Artifact/CAS verifier    │                           │          │
└───────────────────────────┼───────────────────────────┼──────────┘
                            │                           │
                 mTLS / UDS │                           │ bundle FS
                            │                           │
               ┌────────────▼────────────┐     ┌────────▼─────────┐
               │        aos-node         │     │  Audit/Events FS │
               │  (per worker host)      │     │  NDJSON bundles  │
               │  - spawn aos-worker     │     │  reports, SBOM   │
               │  - PF preflight         │     └───────────────────┘
               │  - setuid to tenant     │
               │  - policy checks        │
               └───────────┬─────────────┘
                           │ UDS per-tenant
                    ┌──────▼──────┐
                    │ aos-worker  │
                    └─────────────┘
```

## Components

### aos-cp
Main control plane service with REST API, authentication, and orchestration.

### aos-cp-api
Shared API types, routes, handlers, authentication, and authorization (RBAC).

### aos-cp-db
Database layer using sqlx with SQLite/WAL for configuration, jobs, audits, and telemetry indexes.

See [Database Schema](database-schema/README.md) for complete schema documentation, including:
- [Promotion Pipeline](database-schema/workflows/promotion-pipeline.md) - Deployment workflows
- [Monitoring Flow](database-schema/workflows/monitoring-flow.md) - Real-time metrics
- [Security & Compliance](database-schema/workflows/security-compliance.md) - Audit trails

### aos-cp-jobs
Job orchestration for async tasks: plan builds, audits, replays, and node commands.

### aos-cp-telemetry
Telemetry indexer that watches NDJSON bundles and streams events via SSE.

### aos-node
Node agent that runs on worker hosts to spawn workers with tenant isolation.

## Security Model

### Zero Egress
- Control plane checks PF (Packet Filter) rules at startup
- Refuses to start if outbound networking is not blocked
- Enforces air-gapped operation

### Authentication
- Local user accounts with Argon2id password hashing
- JWT tokens with 8-hour expiry
- Optional mTLS for node agents

### Authorization (RBAC)
Six roles with different access levels:
- **admin**: Full access to all operations
- **operator**: Manage workers, plans, and promotions
- **sre**: Worker management and node operations
- **compliance**: Audit access, policy management
- **auditor**: Read-only audit and telemetry access
- **viewer**: Read-only access to status and reports

### Multi-tenant Isolation
- Each tenant runs under unique UID/GID
- Per-tenant UDS sockets with strict permissions
- Secure Enclave for key storage
- Envelope encryption for artifacts

## API Endpoints

### Health & Auth
- `GET /healthz` - Health check
- `GET /readyz` - Readiness check
- `POST /v1/auth/login` - Login with email/password

### Tenants
- `GET /v1/tenants` - List tenants
- `POST /v1/tenants` - Create tenant (admin only)

### Nodes
- `GET /v1/nodes` - List worker nodes
- `POST /v1/nodes/register` - Register node agent

### Models & Adapters
- `POST /v1/models/import` - Import base model
- `GET /v1/models` - List models
- `POST /v1/adapters/register` - Register adapter

### Plans & Control Points
- `POST /v1/plans/build` - Build execution plan (creates job)
- `GET /v1/plans` - List plans
- `POST /v1/cp/promote` - Promote plan to control point
- `POST /v1/cp/rollback` - Rollback to previous CP

### Workers
- `POST /v1/workers/spawn` - Spawn worker on node
- `POST /v1/workers/stop` - Stop worker
- `GET /v1/workers` - List workers with health

### Jobs
- `GET /v1/jobs` - List jobs
- `GET /v1/jobs/{id}` - Get job details
- `GET /v1/jobs/{id}/logs` - Stream job logs

### Telemetry & Audits
- `GET /v1/telemetry/bundles` - List event bundles
- `GET /v1/telemetry/stream` - SSE stream of events
- `POST /v1/audit/run` - Run audit suite
- `GET /v1/audit/results` - Get audit results

### Security
- `GET /v1/security/egress-preflight` - Check PF status
- `POST /v1/replay/verify` - Verify determinism

### OpenAPI
- `GET /swagger-ui` - Interactive API documentation

## Database Schema

### Core Tables
- `users` - Local authentication
- `tenants` - Multi-tenant boundaries
- `nodes` - Worker host registry
- `models` - Base model artifacts
- `adapters` - LoRA adapters per tenant
- `manifests` - Configuration declarations
- `plans` - Compiled execution plans
- `cp_pointers` - Active control point pointers
- `policies` - Policy packs per tenant

### Operations Tables
- `jobs` - Async task queue
- `workers` - Active worker processes
- `telemetry_bundles` - Event bundle index
- `audits` - Hallucination metrics
- `artifacts` - CAS registry with signatures
- `incidents` - Security violations

## Quick Start

### 1. Initialize

```bash
./scripts/init_cp.sh
```

### 2. Configure

Edit `configs/cp.toml`:
- Set a strong JWT secret (64 characters)
- Configure database path
- Set artifact and bundle storage paths

### 3. Run Migrations

```bash
cargo run --release --bin aos-cp -- --config configs/cp.toml --migrate-only
```

### 4. Start Control Plane

```bash
cargo run --release --bin aos-cp -- --config configs/cp.toml
```

The control plane will:
1. Check PF egress rules (fails if not blocked)
2. Connect to database
3. Run migrations
4. Start API server on port 9443

### 5. Create Admin User

Using the database directly (for initial bootstrap):

```bash
sqlite3 var/aos-cp.sqlite3
```

```sql
-- Generate a password hash first using aos-cp-api::auth::hash_password
-- Then insert:
INSERT INTO users (id, email, display_name, pw_hash, role)
VALUES ('admin-001', 'admin@example.com', 'Admin User', '$argon2id$...', 'admin');
```

### 6. Login

```bash
curl -X POST http://localhost:9443/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"your_password"}'
```

### 7. Create Tenant

```bash
curl -X POST http://localhost:9443/v1/tenants \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"demo_tenant","itar_flag":false}'
```

## Policy Enforcement

Control plane enforces policies at multiple points:

1. **Startup**: PF egress check (fail closed)
2. **Plan Build**: Manifest validation, kernel hash recording
3. **CP Promotion**: Audit gates (ARR/ECS/HLR/CR thresholds), replay verification
4. **Worker Spawn**: UID/GID isolation, socket permissions, zero egress
5. **Artifacts Import**: Signature + SBOM verification

## Promotion Gates

A control point can only be promoted if:

- **Determinism**: Replay shows zero diff
- **Hallucination Metrics**:
  - ARR ≥ 0.95 (Answer Relevance Rate)
  - ECS@5 ≥ 0.75 (Evidence Coverage Score)
  - HLR ≤ 0.03 (Hallucination Rate)
  - CR ≤ 0.01 (Conflict Rate)
- **Performance**: Budgets met, router overhead ≤ threshold
- **Compliance**: Control matrix links resolve, ITAR suite passes

## Node Agent

The `aos-node` agent runs on each worker host and:

1. Receives spawn requests from control plane (mTLS)
2. Verifies local PF rules
3. Executes `setuid`/`setgid` for tenant isolation
4. Spawns `aos-worker` process
5. Reports health back to control plane

## Development

### Run Tests

```bash
cargo test --all
```

### Check Lints

```bash
cargo clippy --all-targets --all-features
```

### Format Code

```bash
cargo fmt --all
```

### Build Release

```bash
cargo build --release --bin aos-cp
```

## Telemetry Streaming

The control plane indexes NDJSON event bundles and streams them via SSE:

```bash
curl -N http://localhost:9443/v1/telemetry/stream?tenant=demo_tenant
```

Events include:
- Router decisions (first N tokens, then sampled)
- Policy violations (100%)
- Adapter evictions (100%)
- Security incidents (100%)

## RBAC Matrix

| Endpoint | admin | operator | sre | compliance | auditor | viewer |
|----------|-------|----------|-----|------------|---------|--------|
| POST /v1/tenants | ✓ | | | | | |
| POST /v1/models/import | ✓ | ✓ | | ✓ | | |
| POST /v1/plans/build | ✓ | ✓ | | | | |
| POST /v1/cp/promote | ✓ | ✓ | | ✓ | | |
| POST /v1/workers/spawn | ✓ | ✓ | ✓ | | | |
| GET /v1/telemetry/stream | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| POST /v1/audit/run | ✓ | | | ✓ | ✓ | |

## Troubleshooting

### Control plane won't start

Check PF rules:
```bash
sudo pfctl -s info  # macOS
sudo iptables -L OUTPUT -n  # Linux
```

### Database locked

SQLite WAL mode is enabled, but check for:
- Multiple processes accessing the DB
- NFS mounts (SQLite requires local filesystem)

### JWT token expired

Tokens expire after 8 hours. Re-login to get a new token.

## Next Steps

- Implement full job execution (plan builds, audits)
- Add SSE telemetry streaming from bundle watcher
- Implement CP promotion with audit gates
- Add mTLS support for node agents
- Build Web UI for control plane

## References

- [AdapterOS Architecture](architecture.md)
- [Policy Rulesets](../README.md#policy-rulesets)
- [Phase 4 Metal Kernels](phase4-metal-kernels.md)
