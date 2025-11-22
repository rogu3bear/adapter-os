# AdapterOS Documentation

Welcome to the AdapterOS documentation. This directory contains comprehensive technical documentation for understanding, deploying, and extending the AdapterOS inference runtime.

## Quick Navigation

### 🎓 New to AdapterOS?

- **[Getting Started with Diagrams](GETTING_STARTED_WITH_DIAGRAMS.md)** ⭐ **START HERE**
  - Plain-language explanation of how everything works
  - Visual tour with real-world examples
  - No technical background required
  - Learn through diagrams and stories

### 🚀 Getting Started
- **[Quick Start Guide](QUICKSTART.md)** - Get up and running in 10 minutes
  - Backend setup and configuration
  - Web UI deployment
  - Common tasks and troubleshooting

### 🏗️ Core Architecture

#### **Visual Guides** ⭐

- **[Precision Diagrams](architecture/PRECISION-DIAGRAMS.md)** - Code-verified architecture diagrams
  - Complete system architecture with exact crate names and file paths
  - Inference pipeline flow with step-by-step code references
  - Router scoring algorithm with feature weights and Q15 quantization
  - Memory management system with watchdog and lifecycle
  - API stack with all routes, handlers, and middleware
  - Worker architecture with UDS server and safety mechanisms


- **[Branch Reconciliation Report](../BRANCH_RECONCILIATION_REPORT.md)** - Repository maintenance and verification
  - Complete branch cleanup documentation
  - Feature implementation verification
  - Deterministic reconciliation process
  - Exact source citations for all changes


>
- **[Diagram Reference Guide](DIAGRAM_REFERENCE.md)** - Quick lookup and navigation
  - Diagram locations and quick links
  - Search by topic or role
  - Diagram maintenance guidelines
  - FAQ and troubleshooting

#### **System Documentation**

- **[System Architecture](ARCHITECTURE.md)** - High-level system design and component overview
  - Worker architecture and inference pipeline
  - Router and adapter management
  - Memory management and eviction
  - Policy enforcement system

- **[Branch Reconciliation Report](../BRANCH_RECONCILIATION_REPORT.md)** - Complete branch cleanup and feature verification
  - Deterministic branch reconciliation process
  - Feature completion verification
  - Repository cleanup results
  - Exact source citations

- **[Control Plane](CONTROL-PLANE.md)** - Control plane architecture and APIs
  - Tenant management
  - Plan building and promotion
  - Telemetry and metrics collection
  - REST API reference

- **[Database Schema](database-schema/README.md)** - Database design and workflow animations
  - Static ER diagrams with comprehensive field documentation
  - Animated workflow sequences for operational processes
  - Real-time monitoring and performance visualization
  - Security and compliance workflows

### 🤖 Model Integration
- **[MLX Integration](MLX_INTEGRATION.md)** - Apple MLX framework integration
  - Model loading and conversion
  - PyO3 bindings
  - Metal kernel integration
  - Performance optimization

- **[Qwen Integration](QWEN-INTEGRATION.md)** - Qwen model support
  - Model configuration
  - Tokenizer setup
  - Chat templates
  - Fine-tuning guide

### 🔧 Advanced Topics

#### Code Intelligence
- **[Code Intelligence Overview](code-intelligence/README.md)** - Complete code analysis stack
  - Architecture and design
  - Multi-language support
  - Framework detection
  - Patch generation and validation

**Detailed Guides:**
- [Architecture](code-intelligence/code-intelligence-ARCHITECTURE.md) - System design and components
- [Tiers](code-intelligence/CODE-INTELLIGENCE-TIERS.md) - Feature tiers and capabilities
- [API Reference](code-intelligence/CODE-API-REGISTRY.md) - REST API documentation
- [CLI Commands](code-intelligence/CODE-CLI-COMMANDS.md) - Command-line interface
- [Policies](code-intelligence/CODE-POLICIES.md) - Policy configuration
- [Router Features](code-intelligence/CODE-ROUTER-FEATURES.md) - Routing integration
- [Evaluation](code-intelligence/CODE-EVALUATION.md) - Metrics and testing

#### UI Component Hierarchy
- **[UI Component Hierarchy](UI-COMPONENT-HIERARCHY.md)** - React component structure and relationships

#### Policy Engine Outline
- **[Policy Engine Outline](POLICY-ENGINE-OUTLINE.md)** - High-level policy enforcement architecture

#### Metal Kernels
- **[Metal Kernels](metal/PHASE4-METAL-KERNELS.md)** - Custom Metal GPU kernels
  - Fused attention operations
  - LoRA application kernels
  - Quantization support
  - Performance optimization

### 🔒 Safety & Security
- **[Runaway Prevention](RUNAWAY-PREVENTION.md)** - Safety mechanisms
  - Memory pressure handling
  - Router skew detection
  - Determinism enforcement
  - Incident response procedures

- **[Code Graph Specification](CODEGRAPH-SPEC.md)** - Code analysis security
  - Graph construction
  - Security boundaries
  - Validation rules

## Documentation by Audience

### For Developers
Start here if you're building on AdapterOS or contributing to the codebase:
1. [Quick Start Guide](QUICKSTART.md)
2. [System Architecture](ARCHITECTURE.md)
3. [Code Intelligence](code-intelligence/README.md)
4. See `examples/` in project root

### For Operators
Start here if you're deploying and managing AdapterOS:
1. [Quick Start Guide](QUICKSTART.md)
2. [Control Plane](CONTROL-PLANE.md)
3. [Runaway Prevention](RUNAWAY-PREVENTION.md)
4. [MLX Integration](MLX_INTEGRATION.md)

### For Researchers
Start here if you're experimenting with models and adapters:
1. [System Architecture](ARCHITECTURE.md)
2. [MLX Integration](MLX_INTEGRATION.md)
3. [Qwen Integration](QWEN-INTEGRATION.md)
4. [Metal Kernels](metal/PHASE4-METAL-KERNELS.md)

### For Security Auditors
Start here if you're evaluating AdapterOS for compliance:
1. [Runaway Prevention](RUNAWAY-PREVENTION.md)
2. [Code Graph Specification](CODEGRAPH-SPEC.md)
3. [Code Intelligence Policies](code-intelligence/CODE-POLICIES.md)
4. See policy rulesets in project workspace rules

## API Documentation

### REST API
- Control Plane API: See [control-plane.md](CONTROL-PLANE.md)
- Authentication API: See [AUTHENTICATION.md](AUTHENTICATION.md)
- Code Intelligence API: See [code-intelligence/code-api-registry.md](code-intelligence/CODE-API-REGISTRY.md)
- OpenAPI Specification: See [api.md](API.md) (auto-generated)

### Rust API
Generate and browse Rust API documentation:
```bash
cargo doc --no-deps --open
```

## Directory Structure

```
docs/
├── README.md                    # This file
├── QUICKSTART.md               # Quick start guide
├── architecture.md             # System architecture
├── control-plane.md            # Control plane docs
├── MLX_INTEGRATION.md          # MLX integration
├── qwen-integration.md         # Qwen model docs
├── runaway-prevention.md       # Safety mechanisms
├── codegraph-spec.md           # Code graph spec
├── database-schema/            # Database schema documentation
│   ├── README.md              # Schema documentation index
│   ├── schema-diagram.md       # Static ER diagram
│   ├── workflows/             # Animated workflow diagrams
│   │   ├── adapter-lifecycle.md
│   │   ├── promotion-pipeline.md
│   │   ├── monitoring-flow.md
│   │   └── ...                # Additional workflows
│   └── examples/              # Usage examples and tutorials
├── code-intelligence/          # Code intelligence docs
│   ├── README.md              # Code intelligence overview
│   ├── code-intelligence-architecture.md
│   ├── code-intelligence-tiers.md
│   ├── code-api-*.md          # API documentation
│   ├── code-cli-commands.md   # CLI reference
│   ├── code-policies.md       # Policy configuration
│   └── ...                    # Additional guides
└── metal/                      # Metal kernel docs
    └── phase4-metal-kernels.md
```

## Key Concepts

### Adapters
LoRA (Low-Rank Adaptation) modules that modify base model behavior:
- Loaded dynamically based on routing decisions
- Memory-efficient with shared base model
- Tiered by importance and activation frequency

### Router
K-sparse routing system that selects top-K adapters per token:
- Quantized gates (Q15) for efficiency
- Entropy floor to prevent collapse
- Code-aware routing with feature extraction

### Plan
Immutable deployment unit containing:
- Model configuration
- Adapter registry
- Policy rules
- Kernel hashes

### Control Point (CP)
Versioned configuration snapshot for promotion and rollback:
- Deterministic execution
- Gate-checked promotion
- Audit trail with telemetry

### Telemetry
Event logging system for observability:
- Canonical JSON format
- BLAKE3 hashing for integrity
- Bundle rotation and signing

## Implementation History

Previous implementation tracking documents have been archived to keep the main documentation clean:
- Location: `../archive/implementation-history/`
- Includes: Phase completion docs, status updates, implementation plans
- Purpose: Historical reference for development process

## Contributing to Documentation

When adding or updating documentation:
1. Follow the existing structure and format
2. Add navigation links to this README
3. Use clear headings and code examples
4. Update the API documentation when changing interfaces
5. Keep the quick start guide up to date

See [CONTRIBUTING.md](../CONTRIBUTING.md) in the project root for general contribution guidelines.

## Getting Help

- **Questions**: Check the relevant documentation section above
- **Issues**: Review troubleshooting in [QUICKSTART.md](QUICKSTART.md)
- **Examples**: See `examples/` directory in project root
- **Tests**: See `tests/` directory for usage patterns
- **API Reference**: Run `cargo doc --open`

## License

AdapterOS is dual-licensed under Apache 2.0 or MIT at your option.
See [LICENSE-APACHE](../LICENSE-APACHE) and [LICENSE-MIT](../LICENSE-MIT) for details.

---

## See Also

- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Detailed architecture patterns with diagrams
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Complete architecture documentation index
- [FEATURE_FLAGS.md](FEATURE_FLAGS.md) - Feature flag reference
- [LOCAL_BUILD.md](LOCAL_BUILD.md) - Local build guide
- [MLX_INTEGRATION.md](MLX_INTEGRATION.md) - MLX backend integration
- [COREML_INTEGRATION.md](COREML_INTEGRATION.md) - CoreML backend with ANE acceleration
- [ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md) - Multi-backend architecture decision

---

**Last Updated**: November 13, 2025
**AdapterOS Version**: alpha-v0.04-unstable
**Maintained by**: [@rogu3bear](https://github.com/rogu3bear)

