# AdapterOS Architecture Documentation

Comprehensive architecture documentation for AdapterOS.

---

## Quick Start

New to AdapterOS architecture? Start here:

1. **[Precision Diagrams](precision-diagrams.md)** - Visual architecture guide (⭐ recommended)
2. **[MasterPlan](MasterPlan.md)** - Complete system design
3. **[Main Architecture Doc](../architecture.md)** - High-level overview

---

## Documentation Hierarchy

### **Visual Guides** (Start Here)

#### **[Precision Diagrams](precision-diagrams.md)** ⭐
Code-verified Mermaid diagrams with exact component names, file paths, and line numbers.

**Contents**:
1. System Architecture - Complete component graph
2. Inference Pipeline Flow - Request lifecycle
3. Router Scoring & Selection - Algorithm details
4. Router Feature Weighting - 22-dim feature vector
5. Memory Management System - Watchdog + lifecycle
6. Memory Eviction Decision Tree - Pressure handling
7. API Stack Architecture - Routes and handlers
8. Worker Architecture - UDS server + safety

**Use Cases**:
- Understanding system components
- Following request flow
- Debugging inference issues
- Learning router algorithm
- Troubleshooting memory pressure

---

### **System Design**

#### **[MasterPlan](MasterPlan.md)**
Complete system design document covering all layers, components, and interactions.

**Contents**:
- Application layers (Client → API → Runtime → Storage → Control Plane)
- Five-tier adapter hierarchy
- Data flow and request lifecycle
- Security model and isolation
- Performance optimization strategies
- Production deployment architecture

**Use Cases**:
- System design review
- Architecture decision making
- Team onboarding
- Stakeholder presentations

---

### **Database Architecture**

#### **[Database Schema Diagrams](../database-schema/)**

Complete database documentation with ERD and workflow animations.

**Key Files**:
- `schema-diagram.md` - Complete ERD with all tables
- `README.md` - Schema overview and guidelines
- `workflows/` - Operational workflow animations

**Workflows**:
- `promotion-pipeline.md` - CP promotion process
- `monitoring-flow.md` - System metrics
- `security-compliance.md` - Artifact signing
- `adapter-lifecycle.md` - State transitions
- `git-repository-workflow.md` - Git integration
- `incident-response.md` - Incident handling
- `performance-dashboard.md` - Performance viz
- `code-intelligence.md` - Code analysis

**Use Cases**:
- Understanding data model
- Writing database queries
- Designing new features
- Debugging data issues

---

## By Component

### **Inference Engine**
- [Precision Diagrams § 2](precision-diagrams.md#2-inference-pipeline-flow) - Complete inference flow
- [Worker Architecture § 8](precision-diagrams.md#8-worker-architecture) - Worker internals
- [Router Scoring § 3](precision-diagrams.md#3-router-scoring--selection) - Router algorithm

**Code**:
- `crates/adapteros-lora-worker/src/inference_pipeline.rs`
- `crates/adapteros-lora-router/src/lib.rs`
- `crates/adapteros-lora-kernel-mtl/`

### **Router System**
- [Router Scoring § 3](precision-diagrams.md#3-router-scoring--selection) - Algorithm
- [Feature Weighting § 4](precision-diagrams.md#4-router-feature-weighting) - Feature breakdown

**Code**:
- `crates/adapteros-lora-router/src/lib.rs`
- `crates/adapteros-lora-router/src/features.rs`
- `crates/adapteros-lora-router/src/calibration.rs`

### **Memory Management**
- [Memory System § 5](precision-diagrams.md#5-memory-management-system) - Components
- [Eviction Tree § 6](precision-diagrams.md#6-memory-eviction-decision-tree) - Eviction logic
- [Adapter Lifecycle](../database-schema/workflows/adapter-lifecycle.md) - State transitions

**Code**:
- `crates/adapteros-memory/src/watchdog.rs`
- `crates/adapteros-memory/src/unified_memory.rs`
- `crates/adapteros-lora-lifecycle/src/lib.rs`

### **API Layer**
- [API Stack § 7](precision-diagrams.md#7-api-stack-architecture) - Routes and handlers
- [Control Plane](../control-plane.md) - API documentation

**Code**:
- `crates/adapteros-server/src/main.rs`
- `crates/adapteros-server-api/src/routes.rs`
- `crates/adapteros-server-api/src/handlers.rs`

### **Database**
- [Schema Diagram](../database-schema/schema-diagram.md) - Complete ERD
- [Database README](../database-schema/README.md) - Overview

**Code**:
- `crates/adapteros-db/src/lib.rs`
- `migrations/*.sql`

### **Policy Engine**
- [System Architecture § 1](precision-diagrams.md#1-system-architecture) - Policy integration
<<<<<<< HEAD
- [MasterPlan](MasterPlan.md) - 22 policy packs
=======
- [MasterPlan](MasterPlan.md) - 20 policy packs
>>>>>>> integration-branch

**Code**:
- `crates/adapteros-policy/src/`
- `.cursor/rules/global.mdc` - Policy pack definitions

### **Code Intelligence**
- [System Architecture § 1](precision-diagrams.md#1-system-architecture) - Git + CodeGraph
- [Git Workflow](../database-schema/workflows/git-repository-workflow.md)
- [Code Intelligence](../database-schema/workflows/code-intelligence.md)

**Code**:
- `crates/adapteros-git/src/subsystem.rs`
- `crates/adapteros-codegraph/src/lib.rs`

---

## Diagram Types

### **Mermaid Diagrams**

All diagrams use Mermaid.js syntax for:
- Flowcharts (`flowchart TD/LR`)
- Sequence diagrams (`sequenceDiagram`)
- State diagrams (`stateDiagram-v2`)
- Entity relationship diagrams (`erDiagram`)
- Graphs (`graph TB/LR`)

**Advantages**:
- Renders in GitHub, VS Code, Cursor
- Version controlled as text
- Easy to update and maintain
- No external image files

### **ASCII Diagrams**

Simple text diagrams in code comments:
```
Request → API → Worker → Router → Kernels → Response
```

**Use for**:
- Code comments
- Quick sketches
- Simple flows

---

## Update History

| Date | Version | Changes | Updated By |
|------|---------|---------|------------|
| 2025-01-14 | 2.0.0 | Created precision diagrams with code verification | System |
| 2025-10-09 | 1.0.0 | Initial runtime diagrams | System |

---

## Feedback & Improvements

To suggest diagram improvements:

1. Verify against current code
2. Create issue with:
   - Diagram reference
   - Inaccuracy description
   - Proposed correction
   - Code evidence

Or submit PR with updated diagrams following the [contribution guidelines](../../CONTRIBUTING.md).

---

## License

Documentation is dual-licensed under MIT OR Apache-2.0, consistent with the AdapterOS project.

