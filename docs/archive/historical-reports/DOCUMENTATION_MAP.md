# AdapterOS Documentation Map

Visual guide to navigating all AdapterOS documentation.

---

## 🗺️ Documentation Landscape

```
                        START HERE
                            ↓
        ┌───────────────────────────────────────┐
        │  GETTING_STARTED_WITH_DIAGRAMS.md    │
        │  (Beginner-friendly visual guide)     │
        └───────────────────────────────────────┘
                            ↓
              ┌─────────────┼─────────────┐
              ↓             ↓             ↓
        ┌──────────┐  ┌──────────┐  ┌──────────┐
        │ QUICK    │  │ VISUAL   │  │ DEEP     │
        │ START    │  │ LEARNER  │  │ DIVE     │
        └──────────┘  └──────────┘  └──────────┘
              ↓             ↓             ↓
```

---

## 🎯 Choose Your Path

### 👶 Complete Beginner

**You are**: New to AdapterOS, may not be technical

**Start**: [Getting Started with Diagrams](GETTING_STARTED_WITH_DIAGRAMS.md)

**Then**:
1. [Quick Start](QUICKSTART.md) - Get it running
2. [Architecture Overview](ARCHITECTURE.md) - High-level concepts
3. [Web UI Tour](http://localhost:3200) - Try it yourself

**Time**: 1-2 hours  
**Goal**: Understand what AdapterOS does and see it working

---

### 🚀 Quick Start (Already Technical)

**You are**: Developer who wants to get started fast

**Start**: [Quick Start Guide](QUICKSTART.md)

**Then**:
1. [CLAUDE.md](../CLAUDE.md) - Developer guide
2. [Control Plane API](CONTROL-PLANE.md) - API reference
3. [Precision Diagrams](architecture/PRECISION-DIAGRAMS.md) - Architecture

**Time**: 30 minutes  
**Goal**: Running system, ready to develop

---

### 📊 Visual Architecture Deep Dive

**You are**: Want comprehensive understanding through diagrams

**Start**: [Precision Diagrams](architecture/PRECISION-DIAGRAMS.md)

**Path**:
```
1. System Architecture (§1)
   ↓
2. Inference Flow (§2)
   ↓
3. Router Details (§3, §4)
   ↓
4. Memory System (§5, §6)
   ↓
5. API & Worker (§7, §8)
   ↓
6. Database Schema
   ↓
7. All Workflows
```

**Time**: 3-4 hours  
**Goal**: Complete visual mental model

---

### 💻 Code-First Developer

**You are**: Want to read code and understand implementation

**Start**: [CLAUDE.md](../CLAUDE.md)

**Path**:
```
1. CLAUDE.md (Developer guide)
   ↓
2. Browse crates/ directory
   ↓
3. Cross-reference with Precision Diagrams
   ↓
4. Read tests in tests/
   ↓
5. Study specific components
```

**Time**: 4-6 hours  
**Goal**: Contributor-level code knowledge

---

### 🏗️ System Operations (SRE)

**You are**: Need to deploy, monitor, and maintain AdapterOS

**Start**: [Control Plane](CONTROL-PLANE.md)

**Path**:
```
1. Control Plane API
   ↓
2. Monitoring Flow Workflow
   ↓
3. Promotion Pipeline
   ↓
4. Incident Response
   ↓
5. Performance Dashboard
```

**Time**: 2-3 hours  
**Goal**: Operational readiness

---

### 🔒 Security & Compliance

**You are**: Auditor or security engineer

**Start**: [Security Compliance Workflow](database-schema/workflows/SECURITY-COMPLIANCE.md)

**Path**:
```
1. Security Compliance Workflow
   ↓
2. Policy Packs (CLAUDE.md)
   ↓
3. Promotion Pipeline (Quality Gates)
   ↓
4. Database Schema (Data model)
   ↓
5. Isolation Model (Precision Diagrams §1)
```

**Time**: 2-3 hours  
**Goal**: Security and compliance understanding

---

## 📚 Complete Documentation Inventory

### Entry Points (Start Here)

| Document | Audience | Time | Purpose |
|----------|----------|------|---------|
| **[Getting Started](GETTING_STARTED_WITH_DIAGRAMS.md)** | Everyone | 1h | Plain-language intro |
| **[Quick Start](QUICKSTART.md)** | Developers | 10min | Get running |
| **[CLAUDE.md](../CLAUDE.md)** | Contributors | 30min | Code guide |

### Architecture Documentation

| Document | Type | Audience | Content |
|----------|------|----------|---------|
| **[Precision Diagrams](architecture/PRECISION-DIAGRAMS.md)** | Visual | All | 8 code-verified diagrams |
| **[MasterPlan](architecture/MASTERPLAN.md)** | Design | Architects | Complete system design |
| **[Architecture](ARCHITECTURE.md)** | Overview | All | High-level concepts |
| **[Diagram Reference](DIAGRAM_REFERENCE.md)** | Index | All | Quick lookup |
| **[Diagram Summary](architecture/DIAGRAM_SUMMARY.md)** | Reference | All | Metrics & overview |

### API & Operations

| Document | Audience | Content |
|----------|----------|---------|
| **[Control Plane](CONTROL-PLANE.md)** | Operators, Developers | API docs, operations |
| **[API Spec](API.md)** | Developers | OpenAPI specification |
| **[Swagger UI](http://localhost:8080/swagger-ui)** | Developers | Interactive API |

### Database

| Document | Audience | Content |
|----------|----------|---------|
| **[Schema Diagram](database-schema/SCHEMA-DIAGRAM.md)** | All | Complete ERD |
| **[Database README](database-schema/README.md)** | All | Schema overview |
| **[Workflows](database-schema/workflows/)** | Operators | 9 workflow diagrams |

### Workflows (database-schema/workflows/)

| Workflow | Purpose | Audience |
|----------|---------|----------|
| **promotion-pipeline.md** | CP promotion | Operators |
| **monitoring-flow.md** | System metrics | SREs |
| **security-compliance.md** | Security verification | Auditors |
| **git-repository-workflow.md** | Git integration | Developers |
| **adapter-lifecycle.md** | State management | All |
| **incident-response.md** | Troubleshooting | SREs |
| **performance-dashboard.md** | Performance viz | SREs |
| **code-intelligence.md** | Code analysis | Developers |
| **replication-distribution.md** | Multi-node | Operators |

### Code Intelligence

| Directory | Files | Audience |
|-----------|-------|----------|
| **[code-intelligence/](code-intelligence/)** | 20 specs | Developers |

### Indexes & References

| Document | Purpose |
|----------|---------|
| **[Architecture Index](ARCHITECTURE_INDEX.md)** | Master index |
| **[Diagram Reference](DIAGRAM_REFERENCE.md)** | Quick lookup |
| **[Documentation Map](DOCUMENTATION_MAP.md)** | This file |

---

## 🎓 Learning Progression

### Week 1: Foundations

**Monday**: Getting Started + Quick Start (2 hours)
- Read beginner guide
- Get system running
- Try Web UI

**Tuesday**: System Architecture (2 hours)
- Study Precision Diagrams §1
- Understand component layout
- Learn data flow

**Wednesday**: Inference Pipeline (2 hours)
- Study Precision Diagrams §2
- Trace a request
- Understand steps

**Thursday**: Router & Memory (2 hours)
- Study Precision Diagrams §3-6
- Learn adapter selection
- Understand memory management

**Friday**: API & Workflows (2 hours)
- Study Precision Diagrams §7-8
- Review workflow diagrams
- Try API calls

### Week 2: Deep Dive

**Focus Areas** (choose based on role):
- **Developers**: Code intelligence, API, CLAUDE.md
- **SREs**: Monitoring, incidents, promotions
- **Architects**: MasterPlan, all workflows
- **Security**: Compliance, policies, isolation

### Week 3: Mastery

**Activities**:
- Read source code in `crates/`
- Contribute improvements
- Build features
- Help others learn

---

## 🔍 Find What You Need

### By Question

| Question | Document |
|----------|----------|
| "How does it work?" | [Getting Started](GETTING_STARTED_WITH_DIAGRAMS.md) |
| "How do I install it?" | [Quick Start](QUICKSTART.md) |
| "How does inference work?" | [Precision Diagrams §2](architecture/precision-diagrams.md#2-inference-pipeline-flow) |
| "How are adapters selected?" | [Precision Diagrams §3](architecture/precision-diagrams.md#3-router-scoring--selection) |
| "How is memory managed?" | [Precision Diagrams §5](architecture/precision-diagrams.md#5-memory-management-system) |
| "What's the database schema?" | [Schema Diagram](database-schema/SCHEMA-DIAGRAM.md) |
| "How do promotions work?" | [Promotion Pipeline](database-schema/workflows/PROMOTION-PIPELINE.md) |
| "What API endpoints exist?" | [Control Plane](CONTROL-PLANE.md) or [Swagger](http://localhost:8080/swagger-ui) |
| "How do I contribute?" | [CONTRIBUTING.md](../CONTRIBUTING.md) + [CLAUDE.md](../CLAUDE.md) |

### By Component

| Component | Documentation |
|-----------|---------------|
| **Router** | Precision Diagrams §3-4, CLAUDE.md |
| **Memory** | Precision Diagrams §5-6, Adapter Lifecycle |
| **API** | Precision Diagrams §7, Control Plane |
| **Worker** | Precision Diagrams §8, CLAUDE.md |
| **Database** | Schema Diagram, Database README |
| **Git** | Git Workflow, Code Intelligence |
| **Policy** | CLAUDE.md (20 policy packs) |
| **Telemetry** | Precision Diagrams §1-2, Monitoring Flow |

### By File Type

**Visual Diagrams**: 
- [Precision Diagrams](architecture/PRECISION-DIAGRAMS.md) (8)
- [Database Workflows](database-schema/workflows/) (9)
- [Schema ERD](database-schema/SCHEMA-DIAGRAM.md) (1)

**Text Guides**:
- [Getting Started](GETTING_STARTED_WITH_DIAGRAMS.md)
- [Quick Start](QUICKSTART.md)
- [CLAUDE.md](../CLAUDE.md)

**Reference Docs**:
- [Architecture Index](ARCHITECTURE_INDEX.md)
- [Diagram Reference](DIAGRAM_REFERENCE.md)
- [Control Plane](CONTROL-PLANE.md)

**Code Specs**:
- [Code Intelligence](code-intelligence/) (20 files)

---

## 🎯 Reading Order Recommendations

### Scenario 1: "I want to understand how it works"

```
1. Getting Started with Diagrams (1 hour)
2. Precision Diagrams §1-2 (30 min)
3. MasterPlan (1 hour)
4. Selected workflows (1 hour)

Total: 3.5 hours
```

### Scenario 2: "I need to use the API"

```
1. Quick Start (10 min)
2. Control Plane API docs (30 min)
3. Swagger UI exploration (20 min)
4. Try example requests (30 min)

Total: 1.5 hours
```

### Scenario 3: "I want to contribute code"

```
1. Quick Start (10 min)
2. CLAUDE.md (1 hour)
3. Precision Diagrams (2 hours)
4. Code Intelligence docs (1 hour)
5. Read relevant crates (2 hours)

Total: 6 hours
```

### Scenario 4: "I'm auditing for security"

```
1. Getting Started (1 hour)
2. Security Compliance Workflow (30 min)
3. Policy Packs in CLAUDE.md (1 hour)
4. Isolation model in Precision Diagrams (30 min)
5. Database schema review (1 hour)

Total: 4 hours
```

### Scenario 5: "I need to deploy to production"

```
1. Quick Start (10 min)
2. Control Plane (1 hour)
3. Promotion Pipeline (30 min)
4. Monitoring Flow (30 min)
5. Incident Response (30 min)
6. Practice deployment (2 hours)

Total: 5 hours
```

---

## 📊 Documentation Statistics

**Total Files**: 50+ documentation files  
**Total Diagrams**: 26 (18 code-verified)  
**Total Workflows**: 9 animated sequences  
**Code References**: 50+ file paths with line numbers  
**API Endpoints**: 60+ documented  
**Database Tables**: 30+ documented  
**Coverage**: 100% of system components  

---

## 🚀 Quick Links

### Most Important

- 🎓 **[START: Getting Started](GETTING_STARTED_WITH_DIAGRAMS.md)**
- 📊 **[Precision Diagrams](architecture/PRECISION-DIAGRAMS.md)**
- 🚀 **[Quick Start](QUICKSTART.md)**
- 💻 **[Developer Guide (CLAUDE.md)](../CLAUDE.md)**

### Most Useful

- 🔍 **[Diagram Reference](DIAGRAM_REFERENCE.md)** - Quick lookup
- 📖 **[Architecture Index](ARCHITECTURE_INDEX.md)** - Complete index
- 🗄️ **[Database Schema](database-schema/SCHEMA-DIAGRAM.md)** - Data model
- 🔄 **[Workflows](database-schema/workflows/)** - Operations

### Most Detailed

- 📐 **[MasterPlan](architecture/MASTERPLAN.md)** - Complete design
- 📝 **[Code Intelligence](code-intelligence/)** - 20 specs
- 🎯 **[Control Plane](CONTROL-PLANE.md)** - API reference
- 🔬 **[Diagram Summary](architecture/DIAGRAM_SUMMARY.md)** - All metrics

---

## 🆘 Help & Support

**Lost?** Start at [Getting Started with Diagrams](GETTING_STARTED_WITH_DIAGRAMS.md)

**Quick question?** Check [Diagram Reference](DIAGRAM_REFERENCE.md) FAQ

**API question?** Try [Swagger UI](http://localhost:8080/swagger-ui)

**Found an issue?** Create a [GitHub Issue](https://github.com/your-repo/issues)

**Want to contribute?** Read [CONTRIBUTING.md](../CONTRIBUTING.md)

---

## ✨ Tips for Success

### Reading Tips

1. **Start visual**: Begin with diagrams, then read text
2. **Cross-reference**: Compare diagrams with code
3. **Take breaks**: Architecture is dense, process in chunks
4. **Ask questions**: Create issues for unclear parts
5. **Try it**: Run the system while reading docs

### Learning Tips

1. **Follow a path**: Don't jump randomly between docs
2. **Build mental model**: Draw your own diagrams
3. **Test understanding**: Try to explain to someone else
4. **Write code**: Best way to learn is to do
5. **Join community**: Learn from others

### Contributing Tips

1. **Understand first**: Read relevant docs thoroughly
2. **Check accuracy**: Verify all claims against code
3. **Be specific**: Include file paths and line numbers
4. **Test changes**: Render diagrams before submitting
5. **Update indexes**: Keep cross-references current

---

## 🎉 You're Ready!

Pick your starting point above and dive in. The documentation is comprehensive but approachable - start with [Getting Started](GETTING_STARTED_WITH_DIAGRAMS.md) if unsure.

Happy learning! 🚀

---

**Last Updated**: 2025-01-14  
**Version**: 2.0  
**Maintained By**: AdapterOS Documentation Team

