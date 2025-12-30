# AdapterOS Crate Architecture

## High-Level Architecture

```mermaid
flowchart TB
    subgraph Entry["Entry Points (Binaries)"]
        server[adapteros-server]
        cli[adapteros-cli<br/>aosctl]
        worker_bin[aos-worker]
    end

    subgraph API["API Layer"]
        server_api[adapteros-server-api]
    end

    subgraph Orchestration["Orchestration Layer"]
        orchestrator[adapteros-orchestrator]
        lora_worker[adapteros-lora-worker]
        lora_lifecycle[adapteros-lora-lifecycle]
    end

    subgraph Business["Business Logic"]
        codegraph[adapteros-codegraph]
        federation[adapteros-federation]
        manifest[adapteros-manifest]
        registry[adapteros-registry]
        model_hub[adapteros-model-hub]
        ingest_docs[adapteros-ingest-docs]
    end

    subgraph Policy["Policy & Config"]
        policy[adapteros-policy]
        config[adapteros-config]
        config_types[adapteros-config-types]
        det_exec[adapteros-deterministic-exec]
    end

    subgraph Core["Core Domain"]
        core[adapteros-core]
        types[adapteros-types]
        api_types[adapteros-api-types]
    end

    subgraph Data["Data Layer"]
        db[adapteros-db]
        storage[adapteros-storage]
        normalization[adapteros-normalization]
    end

    subgraph Infra["Infrastructure"]
        crypto[adapteros-crypto]
        telemetry[adapteros-telemetry]
        aos[adapteros-aos]
        platform[adapteros-platform]
        verify[adapteros-verify]
    end

    subgraph ML["ML Backends"]
        kernel_api[adapteros-lora-kernel-api]
        kernel_coreml[adapteros-lora-kernel-coreml]
        kernel_mtl[adapteros-lora-kernel-mtl]
        lora_router[adapteros-lora-router]
        lora_rag[adapteros-lora-rag]
        mlx_ffi[adapteros-lora-mlx-ffi]
    end

    %% Entry point dependencies
    server --> server_api
    server --> orchestrator
    server --> lora_worker
    cli --> orchestrator
    cli --> lora_worker
    worker_bin --> lora_worker

    %% API layer
    server_api --> orchestrator
    server_api --> lora_worker
    server_api --> db
    server_api --> core

    %% Orchestration
    orchestrator --> codegraph
    orchestrator --> lora_worker
    orchestrator --> db
    orchestrator --> storage
    lora_worker --> kernel_api
    lora_worker --> lora_router
    lora_worker --> lora_rag

    %% Business logic
    codegraph --> core
    federation --> core
    manifest --> core
    registry --> core
    model_hub --> core

    %% Policy
    policy --> core
    config --> config_types
    det_exec --> core

    %% Core layer
    api_types --> core
    api_types --> types
    core --> types

    %% Data layer
    db --> core
    db --> storage
    db --> normalization
    storage --> core
    storage --> crypto
    normalization --> core

    %% Infrastructure
    aos --> core
    platform --> core
    verify --> core
    telemetry --> core

    %% ML backends
    kernel_coreml --> kernel_api
    kernel_mtl --> kernel_api
    mlx_ffi --> kernel_api
    kernel_api --> core
    lora_router --> kernel_api
    lora_router --> core
    lora_rag --> core
```

## ML Training Pipeline

```mermaid
flowchart LR
    subgraph Input["Data Input"]
        dataset[(Training Dataset)]
        codebase[Codebase Files]
    end

    subgraph Ingestion["Ingestion"]
        ingest[adapteros-ingest-docs]
        codegraph[adapteros-codegraph]
    end

    subgraph Orchestration["Training Orchestration"]
        orchestrator[adapteros-orchestrator]
        lora_worker[adapteros-lora-worker]
        lora_lifecycle[adapteros-lora-lifecycle]
    end

    subgraph Routing["Adapter Routing"]
        lora_router[adapteros-lora-router]
        lora_rag[adapteros-lora-rag]
    end

    subgraph Backends["ML Backends"]
        kernel_api[lora-kernel-api<br/>Abstract Interface]
        kernel_coreml[lora-kernel-coreml<br/>CoreML + ANE]
        kernel_mtl[lora-kernel-mtl<br/>Metal GPU]
        mlx_ffi[lora-mlx-ffi<br/>MLX FFI]
    end

    subgraph Output["Output"]
        aos_format[.aos Adapter Bundle]
        db[(adapteros-db)]
    end

    dataset --> ingest
    codebase --> codegraph
    ingest --> orchestrator
    codegraph --> orchestrator
    orchestrator --> lora_worker
    lora_worker --> lora_lifecycle
    lora_worker --> lora_router
    lora_router --> lora_rag
    lora_router --> kernel_api
    kernel_api --> kernel_coreml
    kernel_api --> kernel_mtl
    kernel_api --> mlx_ffi
    kernel_coreml --> aos_format
    kernel_mtl --> aos_format
    mlx_ffi --> aos_format
    aos_format --> db
```

## Server Request Flow

```mermaid
sequenceDiagram
    participant Client
    participant Server as adapteros-server
    participant API as server-api
    participant Router as lora-router
    participant Worker as lora-worker
    participant Backend as kernel-coreml/mtl
    participant DB as adapteros-db

    Client->>Server: HTTP Request
    Server->>API: Route to handler
    API->>DB: Load adapter metadata
    DB-->>API: Adapter info
    API->>Router: Select best adapter
    Router->>Worker: Execute inference
    Worker->>Backend: Run on hardware
    Backend-->>Worker: Result
    Worker-->>API: Response
    API-->>Server: JSON response
    Server-->>Client: HTTP Response
```

## Dependency Layers

```mermaid
flowchart TB
    subgraph L1["Layer 1: Foundation (No internal deps)"]
        types[adapteros-types]
    end

    subgraph L2["Layer 2: Core Domain"]
        core[adapteros-core]
        crypto[adapteros-crypto]
        telemetry_types[adapteros-telemetry-types]
    end

    subgraph L3["Layer 3: Infrastructure"]
        telemetry[adapteros-telemetry]
        platform[adapteros-platform]
        aos[adapteros-aos]
        storage[adapteros-storage]
        kernel_api[lora-kernel-api]
    end

    subgraph L4["Layer 4: Data & ML"]
        db[adapteros-db]
        kernel_coreml[lora-kernel-coreml]
        kernel_mtl[lora-kernel-mtl]
        lora_router[lora-router]
        normalization[adapteros-normalization]
    end

    subgraph L5["Layer 5: Business Logic"]
        codegraph[adapteros-codegraph]
        manifest[adapteros-manifest]
        registry[adapteros-registry]
        policy[adapteros-policy]
        config[adapteros-config]
        lora_rag[lora-rag]
        lora_worker[lora-worker]
    end

    subgraph L6["Layer 6: Orchestration"]
        orchestrator[adapteros-orchestrator]
        lora_lifecycle[lora-lifecycle]
        federation[adapteros-federation]
    end

    subgraph L7["Layer 7: API"]
        server_api[adapteros-server-api]
        api_types[adapteros-api-types]
    end

    subgraph L8["Layer 8: Entry Points"]
        server[adapteros-server]
        cli[adapteros-cli]
        worker_bin[aos-worker]
    end

    L1 --> L2
    L2 --> L3
    L3 --> L4
    L4 --> L5
    L5 --> L6
    L6 --> L7
    L7 --> L8
```

## Feature-Gated Backends

```mermaid
flowchart TB
    subgraph Features["Cargo Features"]
        default["default = deterministic-only + coreml-backend"]
        coreml_feat[coreml-backend]
        metal_feat[metal-backend]
        mlx_feat[mlx-backend]
        multi_feat[multi-backend]
    end

    subgraph Backends["Backend Crates"]
        kernel_api[lora-kernel-api<br/>Always enabled]
        kernel_coreml[lora-kernel-coreml<br/>macOS only]
        kernel_mtl[lora-kernel-mtl<br/>macOS only]
        mlx_ffi[lora-mlx-ffi<br/>Optional]
    end

    subgraph Hardware["Hardware Targets"]
        ane[Apple Neural Engine]
        gpu[Metal GPU]
        cpu[CPU Fallback]
    end

    default --> coreml_feat
    coreml_feat --> kernel_coreml
    metal_feat --> kernel_mtl
    mlx_feat --> mlx_ffi
    multi_feat --> mlx_feat

    kernel_coreml --> ane
    kernel_mtl --> gpu
    mlx_ffi --> cpu
    kernel_mtl --> cpu
```

## Hub Crates (Most Dependencies)

```mermaid
flowchart TB
    subgraph Hubs["Hub Crates"]
        server_api[adapteros-server-api<br/>36+ deps]
        server[adapteros-server<br/>25+ deps]
        lora_worker[lora-worker<br/>20+ deps]
        orchestrator[orchestrator<br/>18+ deps]
        cli[adapteros-cli<br/>25+ deps]
    end

    subgraph Foundation["Foundation Crates (Few deps)"]
        types[types<br/>0 internal deps]
        core[core<br/>1 dep: types]
        kernel_api[kernel-api<br/>1 dep: core]
        crypto[crypto<br/>0 internal deps]
    end

    server_api --> server
    server_api --> orchestrator
    server_api --> lora_worker
    cli --> orchestrator
    cli --> lora_worker

    server_api -.-> core
    orchestrator -.-> core
    lora_worker -.-> core
    lora_worker -.-> kernel_api
    core -.-> types
    kernel_api -.-> core
```

## Data Flow: Training to Inference

```mermaid
flowchart LR
    subgraph Training["Training Phase"]
        direction TB
        raw[Raw Data/Code]
        ingest[Ingestion<br/>codegraph + ingest-docs]
        dataset[(Training Dataset)]
        train[Training Job<br/>lora-worker]
        adapter[Trained Adapter<br/>.aos bundle]
    end

    subgraph Storage["Storage"]
        direction TB
        db[(adapteros-db<br/>SQLite)]
        storage[(adapteros-storage<br/>KV + Tantivy)]
        fs[File System<br/>Adapter Files]
    end

    subgraph Inference["Inference Phase"]
        direction TB
        request[User Request]
        router[lora-router<br/>Select Adapter]
        load[Load Adapter]
        backend[ML Backend<br/>CoreML/Metal]
        response[Response]
    end

    raw --> ingest
    ingest --> dataset
    dataset --> train
    train --> adapter
    adapter --> fs
    adapter --> db

    request --> router
    db --> router
    router --> load
    fs --> load
    load --> backend
    backend --> response
```

## Crate Categories

| Category | Crates | Purpose |
|----------|--------|---------|
| **Entry Points** | `server`, `cli`, `aos-worker` | Binary executables |
| **API** | `server-api`, `api-types` | HTTP handlers, types |
| **Core** | `core`, `types` | Domain models, validation |
| **Data** | `db`, `storage`, `normalization` | Persistence layer |
| **ML** | `lora-worker`, `lora-router`, `lora-rag` | Training orchestration |
| **Backends** | `kernel-api`, `kernel-coreml`, `kernel-mtl`, `mlx-ffi` | Hardware execution |
| **Infra** | `crypto`, `telemetry`, `platform`, `aos` | Cross-cutting concerns |
| **Business** | `orchestrator`, `codegraph`, `manifest`, `registry` | Domain logic |
| **Policy** | `policy`, `config`, `deterministic-exec` | Rules & configuration |
