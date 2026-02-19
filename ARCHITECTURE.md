# adapterOS Architecture

High-level system design for the adapterOS ML inference platform.

---

## Overview

adapterOS is a single-node ML inference platform for Apple Silicon. It orchestrates multi-LoRA inference with deterministic execution, policy enforcement, and zero network egress during serving.

---

## Layers

### Control Plane

- **adapteros-server** — HTTP API server (port 8080)
- **adapteros-server-api** — REST handlers, middleware, routes
- **adapteros-db** — SQLite with migrations, adapter registry, telemetry
- **adapteros-policy** — Policy registry and enforcement

### Inference Pipeline

- **adapteros-lora-router** — K-sparse adapter selection, Q15 quantization
- **adapteros-lora-worker** — Inference engine, policy gates, backend dispatch
- **adapteros-lora-mlx-ffi** — MLX backend (primary)
- **adapteros-lora-kernel-mtl** — Metal GPU kernels
- **adapteros-lora-kernel-coreml** — CoreML/ANE acceleration

### Storage and Lifecycle

- **Adapter registry** — Content-addressed LoRA storage under `var/adapters/`
- **Lifecycle manager** — Hot-swap, eviction, memory headroom
- **Telemetry** — Canonical event logging, audit trail

---

## Request Flow

1. HTTP request → Auth → Tenant guard → Policy → Inference core
2. Router selects top-K adapters (score DESC, stable_id ASC tie-break)
3. Worker loads model + adapters, executes via MLX/CoreML/Metal
4. Response + evidence returned; telemetry recorded

---

## Backends

| Backend | Role | Use |
|---------|------|-----|
| **MLX** | Primary | Inference, training, hot-swap adapters |
| **CoreML** | Acceleration | ANE-accelerated ops for specific layers |
| **Metal** | Kernels | Low-level GPU compute |

---

## Key Directories

- `var/` — Runtime data (gitignored)
- `configs/` — Configuration files
- `migrations/` — Database migrations
- `crates/` — Rust workspace crates

---

## See Also

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Full architecture (boot, middleware, inference flow)
- [docs/MLX_GUIDE.md](docs/MLX_GUIDE.md) — MLX backend
