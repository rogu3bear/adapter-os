# AdapterOS Route Map Diagram

**Copyright:** 2025 JKCA / James KC Auchterlonie. All rights reserved.

This document provides visual diagrams of the route structure and data flow in AdapterOS.

---

## Frontend Route Hierarchy

```mermaid
graph TD
    subgraph Root
        LOGIN["/login"]
        ROOT["/"] --> DASH
    end

    subgraph Home["Home Group"]
        DASH["/dashboard"]
        MGMT["/management"]
        WORKFLOW["/workflow"]
        PERSONAS["/personas"]
    end

    subgraph MLPipeline["ML Pipeline Group"]
        TRAINER["/trainer"]
        TRAINING["/training"]
        TRAINING --> JOBS["/training/jobs"]
        JOBS --> JOBDETAIL["/training/jobs/:jobId"]
        TRAINING --> DATASETS["/training/datasets"]
        TRAINING --> TEMPLATES["/training/templates"]
        TESTING["/testing"]
        GOLDEN["/golden"]
        PROMOTION["/promotion"]
        ADAPTERS["/adapters"]
        ADAPTERS --> ADAPTERNEW["/adapters/new"]
        ADAPTERS --> ADAPTERDETAIL["/adapters/:adapterId"]
        ADAPTERDETAIL --> ACTIVATIONS["/adapters/:adapterId/activations"]
        ADAPTERDETAIL --> LINEAGE["/adapters/:adapterId/lineage"]
        ADAPTERDETAIL --> MANIFEST["/adapters/:adapterId/manifest"]
    end

    subgraph Monitoring["Monitoring Group"]
        METRICS["/metrics"]
        MONITORING["/monitoring"]
        ROUTING["/routing"]
    end

    subgraph System["System Group"]
        SYSOVERVIEW["/system"]
        SYSOVERVIEW --> NODES["/system/nodes"]
        SYSOVERVIEW --> WORKERS["/system/workers"]
        SYSOVERVIEW --> MEMORY["/system/memory"]
        SYSOVERVIEW --> SYSMETRICS["/system/metrics"]
    end

    subgraph Operations["Operations Group"]
        INFERENCE["/inference"]
        TELEMETRY["/telemetry"]
        REPLAY["/replay"]
    end

    subgraph Security["Security Group"]
        POLICIES["/security/policies"]
        AUDIT["/security/audit"]
        COMPLIANCE["/security/compliance"]
    end

    subgraph Admin["Administration Group"]
        ADMINPAGE["/admin"]
        ADMINPAGE --> TENANTS["/admin/tenants"]
        TENANTS --> TENANTDETAIL["/admin/tenants/:tenantId"]
        ADMINPAGE --> STACKS["/admin/stacks"]
        ADMINPAGE --> PLUGINS["/admin/plugins"]
        ADMINPAGE --> SETTINGS["/admin/settings"]
        REPORTS["/reports"]
    end
```

---

## API Endpoint Categories

```mermaid
graph LR
    subgraph Public["Public (No Auth)"]
        H1["/healthz"]
        H2["/healthz/all"]
        H3["/readyz"]
        H4["/v1/auth/login"]
        H5["/v1/meta"]
    end

    subgraph Auth["Authentication"]
        A1["/v1/auth/logout"]
        A2["/v1/auth/me"]
        A3["/v1/auth/refresh"]
        A4["/v1/auth/sessions"]
    end

    subgraph Core["Core Resources"]
        C1["/v1/adapters/*"]
        C2["/v1/adapter-stacks/*"]
        C3["/v1/tenants/*"]
        C4["/v1/training/*"]
        C5["/v1/datasets/*"]
    end

    subgraph Ops["Operations"]
        O1["/v1/infer"]
        O2["/v1/infer/stream"]
        O3["/v1/routing/*"]
        O4["/v1/golden/*"]
    end

    subgraph Infra["Infrastructure"]
        I1["/v1/nodes/*"]
        I2["/v1/workers/*"]
        I3["/v1/services/*"]
        I4["/v1/metrics/*"]
    end

    subgraph Audit["Audit & Compliance"]
        AU1["/v1/audit/*"]
        AU2["/v1/policies/*"]
        AU3["/v1/telemetry/*"]
    end
```

---

## Authentication Flow

```mermaid
sequenceDiagram
    participant U as User
    participant F as Frontend
    participant B as Backend
    participant DB as Database

    U->>F: Enter credentials
    F->>B: POST /v1/auth/login
    B->>DB: Validate user
    DB-->>B: User found
    B->>B: Generate JWT (Ed25519)
    B-->>F: Set httpOnly cookie
    F-->>U: Redirect to /dashboard

    Note over F,B: Subsequent requests

    U->>F: Navigate to /adapters
    F->>B: GET /v1/adapters (with cookie)
    B->>B: Validate JWT from cookie
    B->>B: Extract Claims
    B->>B: Check permissions
    B->>DB: Query adapters
    DB-->>B: Adapter list
    B-->>F: JSON response
    F-->>U: Render adapter list
```

---

## Inference Data Flow

```mermaid
sequenceDiagram
    participant U as User
    participant F as Frontend
    participant API as API Server
    participant Router as LoRA Router
    participant Backend as MLX Backend
    participant GPU as GPU/ANE

    U->>F: Submit prompt
    F->>API: POST /v1/infer/stream
    API->>Router: Route request
    Router->>Router: K-sparse selection
    Router-->>API: Selected adapters
    API->>Backend: Load adapters
    Backend->>GPU: Execute inference

    loop Token generation
        GPU-->>Backend: Next token
        Backend-->>API: Token
        API-->>F: SSE: token chunk
        F-->>U: Display token
    end

    API-->>F: SSE: [DONE]
    F-->>U: Complete response
```

---

## Adapter Lifecycle State Machine

```mermaid
stateDiagram-v2
    [*] --> Unloaded: Register
    Unloaded --> Cold: First access
    Cold --> Warm: Load request
    Warm --> Hot: High activation %
    Hot --> Resident: Pin

    Resident --> Hot: Unpin
    Hot --> Warm: Low activation
    Warm --> Cold: Timeout
    Cold --> Unloaded: Memory pressure

    Unloaded --> [*]: Delete
```

---

## Training Pipeline Flow

```mermaid
flowchart TD
    subgraph Upload["Dataset Upload"]
        U1[Upload file] --> U2[Validate format]
        U2 --> U3[Store in DB]
    end

    subgraph Training["Training Process"]
        T1[Start job] --> T2[Load dataset]
        T2 --> T3[Configure trainer]
        T3 --> T4[Training loop]
        T4 --> T5{Complete?}
        T5 -->|No| T4
        T5 -->|Yes| T6[Save weights]
    end

    subgraph Packaging["Adapter Packaging"]
        P1[Create manifest] --> P2[Package .aos]
        P2 --> P3[Register adapter]
    end

    U3 --> T1
    T6 --> P1
```

---

## Page-to-API Mapping (Visual)

```mermaid
graph TB
    subgraph Pages["Frontend Pages"]
        PD[Dashboard]
        PA[Adapters]
        PT[Training]
        PI[Inference]
        PM[Metrics]
        PS[System]
    end

    subgraph APIs["Backend APIs"]
        A1["/healthz/all"]
        A2["/v1/adapters"]
        A3["/v1/metrics/*"]
        A4["/v1/training/*"]
        A5["/v1/datasets/*"]
        A6["/v1/infer/*"]
        A7["/v1/nodes/*"]
        A8["/v1/workers/*"]
        A9["/v1/system/*"]
    end

    PD --> A1
    PD --> A2
    PD --> A3

    PA --> A2

    PT --> A4
    PT --> A5

    PI --> A6

    PM --> A3

    PS --> A7
    PS --> A8
    PS --> A9
```

---

## SSE Streaming Architecture

```mermaid
flowchart LR
    subgraph Client["Frontend"]
        EventSource[EventSource API]
    end

    subgraph Server["Backend"]
        SSE[SSE Handler]
        Metrics[Metrics Collector]
        Training[Training Monitor]
        Adapters[Adapter State]
    end

    Metrics --> SSE
    Training --> SSE
    Adapters --> SSE
    SSE --> |"data: {...}\n\n"| EventSource

    subgraph Endpoints["SSE Endpoints"]
        E1["/v1/streams/training"]
        E2["/v1/stream/metrics"]
        E3["/v1/stream/adapters"]
        E4["/v1/infer/stream"]
    end
```

---

## RBAC Permission Flow

```mermaid
flowchart TD
    Request[API Request] --> Auth{Has valid JWT?}
    Auth -->|No| Reject1[401 Unauthorized]
    Auth -->|Yes| Extract[Extract Claims]

    Extract --> RoleCheck{Required role?}
    RoleCheck -->|Yes| HasRole{User has role?}
    HasRole -->|No| Reject2[403 Forbidden]
    HasRole -->|Yes| PermCheck
    RoleCheck -->|No| PermCheck

    PermCheck{Required permission?}
    PermCheck -->|Yes| HasPerm{User has permission?}
    HasPerm -->|No| Reject3[403 Forbidden]
    HasPerm -->|Yes| Execute
    PermCheck -->|No| Execute

    Execute[Execute Handler]
    Execute --> AuditLog[Log to audit_logs]
```

---

## Middleware Stack

```mermaid
flowchart TB
    Request[Incoming Request]

    subgraph Middleware["Middleware Stack (applied in reverse)"]
        M1[Client IP Extraction]
        M2[Security Headers]
        M3[Request Size Limit]
        M4[Rate Limiting]
        M5[CORS]
        M6[Trace Layer]
        M7[Auth Middleware]
    end

    Request --> M1
    M1 --> M2
    M2 --> M3
    M3 --> M4
    M4 --> M5
    M5 --> M6
    M6 --> M7
    M7 --> Handler[Route Handler]

    Handler --> Response[Response]
```

---

## Notes

1. All diagrams use Mermaid syntax for rendering
2. View these diagrams in any Mermaid-compatible viewer
3. GitHub and VS Code render Mermaid natively
4. For interactive viewing, use https://mermaid.live
