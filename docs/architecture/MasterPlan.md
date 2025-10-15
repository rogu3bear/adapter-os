## AdapterOS Application Layers

### 1. **Client Layer**
- **Control Plane UI**: Web interface for management
- **CLI Tools**: `aosctl` command-line interface
- **API Clients**: External integrations

### 2. **API Gateway Layer**
- **Unix Domain Socket**: Primary communication channel (no TCP)
- **Authentication**: Tenant authentication
- **Rate Limiter**: Request throttling

### 3. **AdapterOS Runtime Layer**

#### **Core Services**
- **Adapter Router**: Top-K selection with Q15 quantized gates
- **Policy Engine**: Enforces 20 policy packs
- **Evidence Tracker**: Citation manager for grounded responses

#### **Inference Engine**
- **Base LLM**: Foundation model (Qwen2.5-7B-Instruct)
- **Adapter Loader**: LoRA manager and lifecycle
- **Metal Kernels**: Fused operations on Apple Silicon

#### **Data Services**
- **RAG Engine**: Vector search for evidence retrieval
- **Response Cache**: Deduplication and caching
- **Memory Manager**: Eviction controller with ≥15% headroom

#### **Observability**
- **Telemetry Logger**: Event capture with canonical JSON
- **Trace Builder**: Audit trail for deterministic replay
- **Metrics Collector**: Performance monitoring

### 4. **Storage Layer**
- **PostgreSQL**: Registry and state management
- **pgvector**: Embeddings storage
- **Bundle Store**: Telemetry archives
- **Artifact Store**: Signed bundles with BLAKE3 hashing

### 5. **Control Plane Layer**
- **Adapter Registry**: Adapter metadata and ACLs
- **Plan Manager**: CPID lifecycle management
- **Promotion Service**: CAB gates for deployment

## Five-Tier Adapter Hierarchy

### **Layer 5: Ephemeral** (Per-Directory-Change, TTL-bound)
- **Purpose**: Fresh symbols, recent directory changes
- **Rank**: 4-8, TTL 24-72h
- **Example**: `directory_change_abc123`

### **Layer 4: Directory-Specific** (Tenant-specific, Path-bound)
- **Purpose**: Internal APIs, conventions, directory style
- **Rank**: 16-32
- **Example**: `directory_myproject_v3`

### **Layer 3: Frameworks** (Type-specific, Stack-bound)
- **Purpose**: Framework APIs, idioms, gotchas
- **Rank**: 8-16
- **Examples**: `framework_django_v1`, `framework_react_v2`

### **Layer 2: Code** (Domain-general Coding Knowledge)
- **Purpose**: Language reasoning, patterns, refactoring
- **Rank**: 16
- **Example**: `code_lang_v1`

### **Layer 1: Base** (General Language Model)
- **Purpose**: Foundational language understanding
- **Model**: Qwen2.5-7B-Instruct (int4)
- **Size**: ~4-5 GB

## Domain Adapter Layer

- **TextAdapter**: Tokenization, LoRA merge, canonical formatting
- **VisionAdapter**: Image normalization, conv pipeline, quantization
- **TelemetryAdapter**: Signal normalization, filtering, anomaly detection

This structure supports deterministic execution, evidence-grounded responses, and multi-tenant isolation with policy enforcement across all layers.

Reviewing the UI architecture and implementation.


[5 tools called]

## AdapterOS UI Architecture

### 1. **Web Control Plane UI** (`ui/`)
- **React 18** + **TypeScript**
- **Vite** for dev/build
- **Tailwind CSS** + **shadcn/ui**
- **Lucide React** icons

**Features:**
- JWT auth with role-based access
- Real-time metrics and health
- Multi-tenant management
- Adapter lifecycle management
- Policy configuration
- Code intelligence and repository analysis
- Training monitoring and orchestration
- Telemetry bundle export
- Inference playground
- Audit dashboard and compliance

**Components:**
- `Dashboard.tsx` - System overview and metrics
- `Tenants.tsx` - Multi-tenant management
- `Nodes.tsx` - Compute infrastructure monitoring
- `Adapters.tsx` - Adapter lifecycle management
- `Plans.tsx` - Execution plan compilation
- `Promotion.tsx` - Control plane promotion gates
- `Telemetry.tsx` - Event bundle management
- `Policies.tsx` - Security policy configuration
- `CodeIntelligence.tsx` - Repository analysis
- `InferencePlayground.tsx` - Interactive inference testing
- `RouterConfigPage.tsx` - Router configuration
- `GitIntegrationPage.tsx` - Git repository integration
- `AuditDashboard.tsx` - Compliance and audit trails
- `AlertsPage.tsx` - System alerts and notifications
- `AdvancedProcessControl.tsx` - Process management
- `ProcessInsights.tsx` - Process analytics

### 2. **macOS Menu Bar App** (`menu-bar-app/`)
- **SwiftUI** native macOS app
- Zero network calls (reads local JSON)
- Native system metrics via IOKit
- 5-second polling
- Offline operation

**Features:**
- Status monitoring (CPU, GPU, RAM)
- Deterministic mode indicator
- Adapter count display
- Worker status
- Uptime tracking
- Quick log access
- Keyboard shortcuts (Cmd+L for logs)

**Status Icons:**
- `⚡︎` - Normal operation, deterministic mode
- `⚡︎/` - Non-deterministic mode or offline
- `[FIRE]` - High CPU load (>70%)

### 3. **API Integration Layer**
- Centralized API client (`ui/src/api/client.ts`)
- TypeScript types matching server API
- JWT token management
- Error handling
- Environment-based API URLs

**API Endpoints:**
- Authentication (`/api/auth/login`, `/api/auth/logout`)
- Tenant management (`/api/tenants`)
- Adapter operations (`/api/adapters`)
- System metrics (`/api/metrics`)
- Telemetry (`/api/telemetry`)
- Policy management (`/api/policies`)

### 4. **Deployment Architecture**
- Static build embedded in Rust server binary via `rust-embed`
- Served at root path (`/`) with APIs at `/api/*`
- No separate web server
- Unix domain socket communication
- Zero network egress during serving

### 5. **UI Development Workflow**
```bash
# Development
pnpm install
pnpm dev          # Runs on http://localhost:3200

# Production build
pnpm build        # Outputs to ../crates/mplora-server/static/

# From project root
make ui           # Build UI
make ui-dev       # Start dev server
```

### 6. **Design System**
- **shadcn/ui** components (Radix UI primitives)
- **Tailwind CSS** utility classes
- Consistent design tokens
- Dark/light mode support
- Responsive design
- Accessibility compliance

### 7. **Security Features**
- JWT-based authentication
- Role-based access control (Admin, Operator, SRE)
- Multi-tenant isolation
- Policy enforcement UI
- Audit trail visualization
- Compliance dashboard

The UI supports deterministic execution, evidence-grounded responses, and multi-tenant isolation. The web interface handles management tasks; the menu bar app provides lightweight monitoring.
# AdapterOS Master Plan

## Overview
AdapterOS is a deterministic inference runtime and control plane for managing LoRA adapters, policies, and telemetry in air-gapped or regulated environments.  
It enforces **reproducibility**, **auditability**, and **multi-tenant isolation** from the CLI to the GPU kernel.

---

## 1. Application Layers

### **Client Layer**
Interfaces through which operators, developers, and integrations interact with the system.

- **Web Control Plane UI** — Management console for adapters, tenants, telemetry, and audits  
- **CLI Tools** — `aosctl` for local operations, replay, diagnostics  
- **API Clients** — External automation or partner systems, authenticated via short-lived tokens  

### **API Gateway Layer**
Mediates all client communication.

- **Transport** — Unix Domain Socket (no TCP, zero egress)  
- **Authentication** — Tenant JWTs signed with Ed25519 keys  
- **Rate Limiter** — Token bucket per tenant; deterministic queuing to preserve ordering  
- **Replay Endpoint** — `/api/replay/{bundle_id}` for deterministic state reconstruction  

### **Runtime Layer**

#### **Core Services**
- **Adapter Router** — Selects top-K adapters using Q15 quantized gating; merges LoRAs on-the-fly  
- **Policy Engine** — Enforces 20 system policy packs (Egress, Determinism, Telemetry, Compliance, etc.)  
- **Evidence Tracker** — Collects citations and hashes input/output pairs for audit reproducibility  
- **Concurrency Model** — Tokio runtime pinned to deterministic worker threads; no work-stealing  

#### **Inference Engine**
- **Base LLM** — `Qwen2.5-7B-Instruct` (int4, Metal kernels)  
- **Adapter Loader** — Loads LoRA deltas from registry, verifies BLAKE3 signatures  
- **Kernel Pipeline** — Fused Metal operations (matmul, layernorm, softmax) for identical results across runs  

#### **Data Services**
- **RAG Engine** — Deterministic vector search with `pgvector` backend  
- **Response Cache** — BLAKE3-keyed cache for identical queries  
- **Memory Manager** — Tracks adapter residency; evicts below 15% headroom  

#### **Observability**
- **Telemetry Logger** — Canonical JSON event streams  
- **Trace Builder** — Bundles runtime logs, policies, and adapter hashes for replay  
- **Metrics Collector** — Prometheus-style metrics via UDS; no network export  

### **Storage Layer**
Persistent components for state and evidence.

- **PostgreSQL** — Registry and adapter metadata  
- **pgvector** — Embeddings and semantic cache  
- **Bundle Store** — Immutable telemetry archives  
- **Artifact Store** — Signed model and adapter bundles (BLAKE3 + Ed25519 verification)  

### **Control Plane Layer**
Higher-level governance and lifecycle management.

- **Adapter Registry** — Versioned metadata and access control lists  
- **Plan Manager** — Compiles control-plane IDs (CPIDs), maintains deterministic version trees  
- **Promotion Service** — CAB-style promotion:  
  1. Validate hashes  
  2. Re-run replay test bundle  
  3. Record approval signature  
  4. Promote adapter to production  

---

## 2. Adapter Hierarchy

| Layer | Scope | Purpose | Rank | Example |
|-------|--------|----------|-------|----------|
| **5. Ephemeral** | Per-directory or commit change | Fresh symbols, local modifications | 4–8 | `directory_change_abc123` |
| **4. Directory** | Tenant/path-bound | Directory-specific conventions | 8–16 | `directory_myproject_v3` |
| **3. Framework** | Framework-level logic | API idioms and patterns | 16–24 | `framework_react_v2` |
| **2. Code** | Language-level | Refactoring, idioms, common patterns | 24–32 | `code_rust_v1` |
| **1. Base** | Foundation model | General language understanding | n/a | `Qwen2.5-7B-Instruct` |

Adapters can be merged dynamically during inference based on ranked priority and context fingerprint.

---

## 3. Domain Adapter Layer

- **TextAdapter** — Tokenization, LoRA merge, canonical formatting  
- **VisionAdapter** — Image normalization and quantized convolution pipeline  
- **TelemetryAdapter** — Signal normalization, anomaly detection, and deterministic filtering  

Each adapter layer communicates via **Unified Memory Access (UMA)** for Apple Silicon: no copy, no drift.

---

## 4. Concurrency and Determinism
- Worker pool pinned to N physical cores  
- Deterministic task scheduler (FIFO per-tenant)  
- Floating-point tolerance checks per kernel  
- Periodic drift audits comparing replay bundles  

---

## 5. Replay and Evidence System
- Each inference produces a **Trace Bundle**: inputs, adapter versions, policy set, hashes  
- Bundles are exportable for verification (`aosctl replay bundle.json`)  
- Deterministic replay ensures byte-identical results across sessions  

---

## 6. UI Architecture

### **Web Control Plane** (`ui/`)
React 18 + TypeScript + Tailwind + shadcn/ui.  
Manages system state, adapter lifecycle, tenants, telemetry, and audits.  
Build embedded via `rust-embed` — served locally with zero network egress.

Key components:  
`Dashboard.tsx`, `Adapters.tsx`, `Telemetry.tsx`, `AuditDashboard.tsx`, `CodeIntelligence.tsx`, etc.

### **macOS Menu Bar App** (`menu-bar-app/`)
SwiftUI native app reading local JSON status.  
Shows CPU/GPU metrics, adapter count, deterministic indicator, and logs.  

Status icons:  
`[LIGHTNING]` deterministic, `[LIGHTNING]/` non-deterministic/offline, `[FIRE]` high load.

---

## 7. Security and Policy
- Ed25519-signed JWTs  
- Role-based ACLs (Admin, Operator, SRE)  
- No network egress or telemetry leak  
- Compliance dashboard for CMMC/ITAR alignment  

---

## 8. Deployment Workflow
- `make ui` → Builds static UI  
- `cargo build --release` → Embeds assets  
- `aosctl serve` → Launches runtime via Unix socket  
- Configuration via `adapteros.toml`  

---

## 9. Future Extensions
- **Federated Adapters** — Cross-tenant synchronization via signed bundles  
- **Metal MLX Support** — Full fused attention kernels  
- **Replay Studio** — GUI for replay and trace comparison  
- **Auto-Promotion** — Continuous deterministic validation loop  

---

## 10. Determinism Manifesto
AdapterOS operates on three non-negotiables:  
1. **Determinism** — same input, same output, always.  
2. **Auditability** — every byte can be replayed.  
3. **Isolation** — zero egress, zero ambiguity.