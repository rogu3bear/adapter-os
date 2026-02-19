# ARCHITECTURE

adapterOS control plane and worker topology. Code is authoritative.

---

## Topology

```mermaid
flowchart TB
    subgraph Clients["Clients"]
        UI["Leptos WASM<br/>crates/adapteros-ui"]
        CLI["aosctl<br/>crates/adapteros-cli"]
    end

    subgraph CP["Control Plane (adapteros-server)"]
        API["Axum Router<br/>adapteros-server-api"]
        DB[(SQLite<br/>var/aos-cp.sqlite3)]
    end

    subgraph Workers["Workers"]
        W["aos-worker<br/>adapteros-lora-worker"]
    end

    subgraph Backends["Inference Backends"]
        MLX["MLX FFI<br/>adapteros-lora-mlx-ffi"]
        MTL["Metal Kernels<br/>adapteros-lora-kernel-mtl"]
        CML["CoreML ANE<br/>adapteros-lora-kernel-coreml"]
    end

    UI -->|"HTTP :8080"| API
    CLI -->|"HTTP :8080"| API
    API -->|"UDS<br/>var/run/aos/<tenant>/worker.sock"| W
    W --> MLX
    W --> MTL
    W --> CML
    API --> DB
```

**Key paths:**
- Control plane: `crates/adapteros-server/src/main.rs`
- API routes: `crates/adapteros-server-api/src/routes/mod.rs`
- Worker socket: `adapteros-core::defaults::DEFAULT_WORKER_SOCKET_PROD_ROOT` = `var/run/aos`

---

## Boot Sequence

Phases run via `StartupOrchestrator::run_phase()`. Source: `adapteros-server/src/main.rs`.

```mermaid
flowchart TD
    subgraph Phase1["Phase 1: Config"]
        P1["initialize_config()<br/>ConfigContext"]
    end

    subgraph Phase2["Phase 2: Security"]
        P2["initialize_security()<br/>security_init"]
    end

    subgraph Phase3["Phase 3: Executor"]
        P3["initialize_executor()<br/>executor_init"]
        P3b["determinism_seed<br/>manifest_hash gate"]
    end

    subgraph Phase4["Phase 4: Preflight & Invariants"]
        P4["run_preflight_checks()"]
        P4b["validate_boot_invariants()<br/>invariants"]
        P4c["check_model_server_readiness()"]
    end

    subgraph Phase5["Phase 5-6: Database"]
        P5["initialize_database()<br/>db_connect"]
        P6["run_migrations()<br/>migrations"]
        P6b["validate_post_db_invariants()"]
    end

    subgraph Phase7["Phase 7: Recovery"]
        P7["run_startup_recovery()<br/>orphaned jobs, adapters"]
    end

    subgraph Phase8["Phase 8-9: Router & Federation"]
        P8["build_api_config()<br/>router_build"]
        P9["initialize_federation()"]
        P9b["initialize_metrics()"]
    end

    subgraph Phase10["Phase 10: AppState"]
        P10["build_app_state()<br/>WorkerHealthMonitor, LifecycleManager"]
        P10b["spawn_all_background_tasks()<br/>worker_attach"]
    end

    subgraph Phase11["Phase 11-12: Bind"]
        P11["ensure_runtime_gates_ready()<br/>replay_ready"]
        P12["bind_and_serve()"]
    end

    P1 --> P2 --> P3 --> P3b --> P4 --> P4b --> P4c --> P5 --> P6 --> P6b
    P6b --> P7 --> P8 --> P9 --> P9b --> P10 --> P10b --> P11 --> P12
```

**Failure codes:** `adapteros-server-api::boot_state::failure_codes` (e.g. `SECURITY_INIT_FAILED`, `DB_CONN_FAILED`, `WORKER_ATTACH_FAILED`).

---

## Request Path: Middleware Chain

Order enforced at compile time via type-state pattern. Source: `adapteros-server-api/src/middleware/chain_builder.rs`.

```mermaid
flowchart LR
    subgraph Inbound["Request Inbound"]
        R["HTTP Request"]
    end

    subgraph Chain["ProtectedMiddlewareChain (outermost first)"]
        A["auth_middleware<br/>Claims, Principal"]
        T["tenant_route_guard_middleware<br/>TenantGuard"]
        C["csrf_middleware<br/>Double-submit"]
        X["context_middleware<br/>RequestContext"]
        P["policy_enforcement_middleware<br/>PolicyPackManager"]
        U["audit_middleware<br/>Audit logging"]
    end

    subgraph Handler["Handler"]
        H["Route handler"]
    end

    R --> A --> T --> C --> X --> P --> U --> H
```

**Type states:** `NeedsAuth` → `NeedsTenantGuard` → `NeedsCsrf` → `NeedsContext` → `NeedsPolicy` → `NeedsAudit` → `Complete`.

**Route tiers:**
- `health`: `/healthz`, `/readyz`, `/version` (no middleware)
- `public`: `/v1/auth/login`, `/v1/status`, `/metrics`, etc.
- `optional_auth`: `/v1/models/status`, `/v1/topology`
- `internal`: `/v1/workers/register`, `/v1/workers/heartbeat` (worker UID, skip tenant guard)
- `protected`: full chain above

---

## Inference Flow

End-to-end path from HTTP to tokens. Source: `adapteros-server-api/src/inference_core/core.rs`, `adapteros-server-api/src/uds_client.rs`.

```mermaid
sequenceDiagram
    participant Client
    participant Handler as infer handler
    participant Core as InferenceCore
    participant Router as K-sparse Router
    participant UDS as UdsClient
    participant Worker as aos-worker
    participant Backend as MLX/Metal

    Client->>Handler: POST /v1/infer
    Handler->>Core: route_and_infer()
    
    Note over Core: OnBeforeInference policy hook
    Core->>Core: validate_pinned_adapters_for_tenant()
    Core->>Router: select adapters (score DESC, stable_id ASC)
    Core->>Core: derive_seed(manifest_hash, "router")
    
    Core->>UDS: infer_with_phase_timings() or infer_stream()
    UDS->>Worker: POST /inference over UnixStream
    Note over UDS,Worker: var/run/aos/<tenant>/worker.sock
    
    Worker->>Backend: infer()
    Backend-->>Worker: tokens
    Worker-->>UDS: WorkerInferResponse / SSE
    UDS-->>Core: response
    
    Note over Core: OnAfterInference policy hook
    Core-->>Handler: InferResponse
    Handler-->>Client: JSON / stream
```

**Key types:**
- `InferenceCore::route_and_infer()` — main entry
- `WorkerInferRequest` / `WorkerInferResponse` — UDS payload
- `UdsClient::infer_with_phase_timings()` — sync inference
- `UdsClient::infer_stream()` — streaming

---

## AppState

Central services in `AppState`. Source: `adapteros-server-api/src/state.rs`, `adapteros-server/src/boot/app_state.rs`.

```mermaid
flowchart TB
    subgraph AppState["AppState (build_app_state)"]
        DB["Db (SQLite pool)"]
        Config["Config (Arc RwLock)"]
        Lifecycle["LifecycleManager"]
        Registry["Adapter Registry"]
        Metrics["MetricsRegistry, MetricsCollector"]
        Telemetry["TelemetryBuffer, TraceBuffer"]
        Policy["PolicyPackManager, PolicyHashWatcher"]
        Tick["GlobalTickLedger"]
        Manifest["manifest_hash: B3Hash"]
        WorkerHealth["WorkerHealthMonitor"]
        UMA["UmaPressureMonitor"]
    end

    subgraph External["External Services"]
        Fed["FederationDaemon"]
        Training["TrainingService"]
    end

    AppState --> Fed
    AppState --> Training
```

---

## Policy Packs

30 packs, enforced in order. Source: `adapteros-policy/src/registry.rs`, `PolicyId`.

```mermaid
flowchart LR
    subgraph Packs["PolicyId (1-30)"]
        P1["Egress"]
        P2["Determinism"]
        P3["Router"]
        P4["Evidence"]
        P5["Refusal"]
        P6["Numeric"]
        P7["RAG"]
        P8["Isolation"]
        P9["Telemetry"]
        P10["Retention"]
    end

    subgraph Hooks["Policy Hooks"]
        H1["OnRequestBeforeRouting"]
        H2["OnBeforeInference"]
        H3["OnAfterInference"]
    end

    Packs --> H1
    Packs --> H2
    Packs --> H3
```

**Enforcement:** `policy_enforcement_middleware` → `PolicyPackManager` → hooks. See [POLICIES.md](POLICIES.md).

---

## Crates

| Crate | Role | Key Modules |
|-------|------|-------------|
| adapteros-server | CP entry, boot | `main.rs`, `boot/` |
| adapteros-server-api | Routes, handlers, middleware | `routes/mod.rs`, `inference_core/`, `middleware/` |
| adapteros-lora-worker | Inference, backend dispatch | `lib.rs`, `uds_server.rs` |
| adapteros-lora-router | K-sparse selection | `quantization.rs` (Q15 denom 32767.0) |
| adapteros-lora-mlx-ffi | MLX backend | C++ FFI |
| adapteros-lora-kernel-mtl | Metal kernels | `metal/` |
| adapteros-db | SQLite, migrations | `migrations/` |
| adapteros-policy | Policy packs | `registry.rs`, `policy_packs.rs` |
| adapteros-core | Seed, errors, path security | `seed.rs`, `error_codes.rs`, `path_security.rs` |
| adapteros-config | Config loader | `configs/cp.toml` |
