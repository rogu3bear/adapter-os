# AdapterOS Architecture Documentation Index

Complete index of all architecture documentation with quick navigation.

---

## 📚 Documentation Overview

| Document | Type | Status | Purpose |
|----------|------|--------|---------|
| **[Concepts](CONCEPTS.md)** | Mental Model | ✅ Current | **START HERE** - Unified mental model and glossary |
| **[Precision Diagrams](architecture/precision-diagrams.md)** | Visual | ✅ Verified | Code-verified architecture diagrams |
| **[Diagram Reference](DIAGRAM_REFERENCE.md)** | Index | ✅ Current | Quick lookup guide |
| **[Diagram Summary](architecture/DIAGRAM_SUMMARY.md)** | Reference | ✅ Current | Diagram overview and metrics |
| **[MasterPlan](architecture/MasterPlan.md)** | Design | ✅ Current | Complete system design |
| **[System Architecture](architecture.md)** | Overview | ✅ Current | High-level architecture |
| **[Control Plane](control-plane.md)** | API | ✅ Current | API documentation |
| **[Database Schema](database-schema/README.md)** | Data | ✅ Current | Database design |

---

## 🎯 Start Here (Recommended Paths)

### Path 0: First-Time User (Recommended) 🌟

**Goal**: Understand what AdapterOS is and how entities relate

1. **[Concepts](CONCEPTS.md)** - **START HERE** - Mental model and glossary
   - What is a Tenant, Adapter, Stack, Router?
   - How do entities relate?
   - Key workflows explained

**Time**: 30 minutes
**Outcome**: Conceptual understanding of the system

### Path 1: Visual Learner ⭐

**Goal**: Understand AdapterOS through diagrams

1. **[Concepts](CONCEPTS.md)** - Mental model (prerequisite)

2. **[Precision Diagrams](architecture/precision-diagrams.md)** - Complete visual guide
   - System architecture
   - Request flow
   - Component details

3. **[Database ERD](database-schema/schema-diagram.md)** - Data model

4. **[Workflow Diagrams](database-schema/workflows/)** - Operational flows

**Time**: 2-3 hours
**Outcome**: Comprehensive visual understanding

### Path 2: Code-First Developer

**Goal**: Understand implementation through code

1. **[Concepts](CONCEPTS.md)** - Mental model (prerequisite)

2. **[CLAUDE.md](../CLAUDE.md)** - Developer guide with code examples

3. **[Precision Diagrams](architecture/precision-diagrams.md)** - Verify understanding
   - Cross-reference with code paths
   - Check line numbers

4. **[API Documentation](api.md)** - Endpoint reference

**Time**: 3-4 hours
**Outcome**: Code-level understanding with visual context

### Path 3: Operations & SRE

**Goal**: Operational knowledge for running AdapterOS

1. **[Control Plane](control-plane.md)** - API and operations
   
2. **[Monitoring Flow](database-schema/workflows/monitoring-flow.md)** - Metrics
   
3. **[Incident Response](database-schema/workflows/incident-response.md)** - Troubleshooting
   
4. **[Promotion Pipeline](database-schema/workflows/promotion-pipeline.md)** - Deployments

**Time**: 2-3 hours  
**Outcome**: Operational readiness

### Path 4: Security & Compliance

**Goal**: Understand security model and compliance

1. **[Security Compliance](database-schema/workflows/security-compliance.md)** - Verification
   
2. **[Precision Diagrams § 1](architecture/precision-diagrams.md#1-system-architecture)** - Isolation model
   

3. **[Policy Rulesets](../CLAUDE.md)** - 22 policy packs

3. **[Policy Rulesets](../CLAUDE.md)** - 20 policy packs
>

**Time**: 2-3 hours  
**Outcome**: Security and compliance understanding

---

## 📊 Diagram Catalog

### System Diagrams (8)

| # | Name | Location | Type | Verified |
|---|------|----------|------|----------|
| 1 | System Architecture | `precision-diagrams.md` § 1 | Component Graph | ✅ |
| 2 | Inference Pipeline Flow | `precision-diagrams.md` § 2 | Sequence | ✅ |
| 3 | Router Scoring & Selection | `precision-diagrams.md` § 3 | Flowchart | ✅ |
| 4 | Router Feature Weighting | `precision-diagrams.md` § 4 | Component Graph | ✅ |
| 5 | Memory Management System | `precision-diagrams.md` § 5 | Component Graph | ✅ |
| 6 | Memory Eviction Tree | `precision-diagrams.md` § 6 | Flowchart | ✅ |
| 7 | API Stack Architecture | `precision-diagrams.md` § 7 | Component Graph | ✅ |
| 8 | Worker Architecture | `precision-diagrams.md` § 8 | Component Graph | ✅ |

### Database Diagrams (10)

| # | Name | Location | Type | Verified |
|---|------|----------|------|----------|
| 1 | Complete ERD | `database-schema/schema-diagram.md` | ERD | ✅ |
| 2 | Promotion Pipeline | `workflows/promotion-pipeline.md` | Flowchart + Sequence | ✅ |
| 3 | Monitoring Flow | `workflows/monitoring-flow.md` | Graph + Sequence | ✅ |
| 4 | Security Compliance | `workflows/security-compliance.md` | Sequence + Flowchart | ✅ |
| 5 | Git Repository | `workflows/git-repository-workflow.md` | Flowchart | ✅ |
| 6 | Adapter Lifecycle | `workflows/adapter-lifecycle.md` | Sequence + State | ✅ |
| 7 | Incident Response | `workflows/incident-response.md` | Sequence | ✅ |
| 8 | Performance Dashboard | `workflows/performance-dashboard.md` | Graph | ✅ |
| 9 | Code Intelligence | `workflows/code-intelligence.md` | Flowchart | ✅ |
| 10 | Replication Distribution | `workflows/replication-distribution.md` | Graph | ✅ |

### Legacy Diagrams (12)


Located in `runtime-diagrams.md` - Archived (not maintained)

Located in `runtime-diagrams.md` - ⚠️ May be outdated
>

---

## 🔍 Search Index

### By Keyword

**Adapter**: §3, §4, §5, §6, workflows/adapter-lifecycle.md  
**API**: §7, control-plane.md  
**Circuit Breaker**: §8  
**Database**: database-schema/  
**Deterministic**: §1, MasterPlan.md  
**Eviction**: §6, workflows/adapter-lifecycle.md  
**Git**: §1, workflows/git-repository-workflow.md  
**Inference**: §2, §8  
**Memory**: §5, §6  
**Monitoring**: workflows/monitoring-flow.md  
**Policy**: §1, §2, §8  
**Promotion**: workflows/promotion-pipeline.md  
**Router**: §3, §4  
**Safety**: §8  
**Security**: workflows/security-compliance.md  
**Telemetry**: §1, §2  
**UDS**: §8  
**Worker**: §8  

### By File Type

**Mermaid Diagrams**: 26 total
- Precision: 8
- Database: 10
- Legacy: 8

**Markdown Docs**: 20+ files
- Architecture: 4 files
- Workflows: 9 files
- Database schema: 2 files
- Code intelligence: 20 files

**Code References**: 50+ crates
- See `Cargo.toml` for complete list

---

## 🎓 Learning Paths

### Beginner (0-1 week experience)

**Week 1 - System Overview**:
- Day 1: Read `CONCEPTS.md` + `QUICKSTART.md`
- Day 2: Study `precision-diagrams.md` § 1-2
- Day 3: Review `database-schema/schema-diagram.md`
- Day 4: Read `control-plane.md`
- Day 5: Practice with `CLAUDE.md` examples

### Intermediate (1-4 weeks experience)

**Component Deep Dives**:
- Router: `precision-diagrams.md` § 3-4
- Memory: `precision-diagrams.md` § 5-6
- API: `precision-diagrams.md` § 7
- Worker: `precision-diagrams.md` § 8
- Workflows: All in `database-schema/workflows/`

### Advanced (1+ months experience)

**System Mastery**:

- Study all 22 policy packs

- Study all 20 policy packs
>
- Review all workflow diagrams
- Read code intelligence docs
- Understand deterministic execution
- Master promotion pipeline

---

## 📋 Checklists

### Understanding Inference

- [ ] Read inference pipeline flow (§ 2)
- [ ] Understand router algorithm (§ 3)
- [ ] Learn feature weights (§ 4)
- [ ] Study safety mechanisms (§ 8)
- [ ] Review policy enforcement
- [ ] Trace request in code

### Understanding Memory

- [ ] Study memory system (§ 5)
- [ ] Learn eviction tree (§ 6)
- [ ] Review adapter states
- [ ] Understand pressure levels
- [ ] Check watchdog components
- [ ] Trace eviction in code

### Understanding API

- [ ] Review API stack (§ 7)
- [ ] Study authentication flow
- [ ] Learn all route categories
- [ ] Understand middleware stack
- [ ] Check handler implementations
- [ ] Test endpoints with Swagger

### Understanding Database

- [ ] Study ERD
- [ ] Review all tables
- [ ] Understand relationships
- [ ] Check migration files
- [ ] Learn workflow sequences
- [ ] Practice SQL queries

---

## 🔗 Cross-References

### Diagrams → Code

| Diagram Component | Code Location |
|-------------------|---------------|
| System Architecture | `Cargo.toml` (crates list) |
| Inference Pipeline | `crates/adapteros-lora-worker/src/inference_pipeline.rs` |
| Router | `crates/adapteros-lora-router/src/lib.rs` |
| Memory Watchdog | `crates/adapteros-memory/src/watchdog.rs` |
| API Routes | `crates/adapteros-server-api/src/routes.rs` |
| Worker | `crates/adapteros-lora-worker/src/lib.rs` |
| UDS Server | `crates/adapteros-lora-worker/src/uds_server.rs` |
| Database | `crates/adapteros-db/src/lib.rs` |

### Code → Diagrams

| Code File | Relevant Diagrams |
|-----------|-------------------|
| `inference_pipeline.rs` | § 2 (Inference Flow) |
| `lora-router/lib.rs` | § 3 (Router Scoring), § 4 (Feature Weights) |
| `adapteros-memory/` | § 5 (Memory System), § 6 (Eviction) |
| `server-api/routes.rs` | § 7 (API Stack) |
| `lora-worker/lib.rs` | § 8 (Worker Architecture) |
| `migrations/*.sql` | Database Schema ERD |

### Concepts → Documentation

| Concept | Primary Docs | Supporting Diagrams |
|---------|-------------|---------------------|
| K-Sparse Routing | CLAUDE.md | § 3, § 4 |
| Determinism | determinism-audit.md | § 1, § 2 |
| Policy Enforcement | POLICIES.md | § 1, § 8 |
| Memory Management | (multiple) | § 5, § 6 |
| Multi-tenant Isolation | architecture.md | § 1, workflows/security-compliance.md |
| Evidence Grounding | EVIDENCE_RETRIEVAL.md | § 2 |
| Promotion Gates | (multiple) | workflows/promotion-pipeline.md |

---

## 🎯 Use Case Matrix

| Task | Documentation | Diagrams | Code |
|------|---------------|----------|------|
| **Add new API endpoint** | control-plane.md | § 7 | routes.rs, handlers.rs |
| **Debug inference latency** | (troubleshooting) | § 2, § 3 | inference_pipeline.rs |
| **Tune router weights** | CLAUDE.md | § 3, § 4 | lora-router/lib.rs |
| **Handle memory pressure** | (troubleshooting) | § 5, § 6 | adapteros-memory/ |
| **Deploy new CP** | control-plane.md | workflows/promotion-pipeline.md | server/main.rs |
| **Add database table** | database-schema/README.md | schema-diagram.md | migrations/ |
| **Integrate git repo** | code-intelligence/ | workflows/git-repository.md | adapteros-git/ |
| **Monitor system health** | system-metrics.md | workflows/monitoring-flow.md | system-metrics/ |

---

## 🏆 Best Practices

### When Reading Diagrams

1. **Start broad, go deep**: System architecture → Component details
2. **Cross-reference code**: Use file paths and line numbers
3. **Follow data flow**: Trace requests through system
4. **Understand states**: Learn state machines (adapter lifecycle, circuit breaker)
5. **Check thresholds**: Note numeric values for configuration

### When Creating Diagrams

1. **Verify against code**: Every component, every relationship
2. **Include references**: File paths, line numbers, method names
3. **Use consistent naming**: Match codebase exactly
4. **Add metadata**: Last updated, verification status
5. **Test rendering**: Preview in markdown viewer

### When Updating Diagrams

1. **Check all references**: Ensure code still matches
2. **Update timestamps**: Mark last updated date
3. **Verify names**: Crate names, file paths, methods
4. **Test changes**: Render and review
5. **Document changes**: Note what was updated and why

---

## 📈 Documentation Metrics

### Coverage

**System Components**: 44/44 crates documented ✅  
**API Endpoints**: 60+ endpoints documented ✅  
**Database Tables**: 30+ tables documented ✅  
**Workflows**: 9 workflows documented ✅  
**Code References**: 50+ file paths included ✅  

### Quality

**Code Verification**: 17/26 diagrams verified ✅  
**Line Number References**: 8/8 precision diagrams ✅  
**Up-to-date**: Last reviewed 2025-01-14 ✅  
**Maintenance**: Active ✅  

### Completeness

| Area | Coverage | Status |
|------|----------|--------|
| System Architecture | 100% | ✅ Complete |
| Inference Pipeline | 100% | ✅ Complete |
| Router Algorithm | 100% | ✅ Complete |
| Memory Management | 100% | ✅ Complete |
| API Stack | 100% | ✅ Complete |
| Worker Architecture | 100% | ✅ Complete |
| Database Schema | 100% | ✅ Complete |
| Workflows | 100% | ✅ Complete |

---

## 🔄 Update Schedule

### Continuous (With Code Changes)

- [ ] New crates added → Update system architecture
- [ ] API routes changed → Update API stack
- [ ] Database migrations → Update schema ERD
- [ ] Config changes → Update relevant diagrams

### Regular Reviews

**Monthly**:
- [ ] Verify all file path references
- [ ] Check line number accuracy

- [ ] Confirm archival status of legacy diagrams

- [ ] Update outdated diagrams
>
- [ ] Add missing components

**Quarterly**:
- [ ] Full diagram verification against code
- [ ] Update version numbers

- [ ] Review legacy set (keep archived reference only)

- [ ] Review and prune legacy diagrams
>
- [ ] Metrics and quality check

**Per Release**:
- [ ] Update all "Last Updated" timestamps
- [ ] Verify all code references
- [ ] Generate diagram exports
- [ ] Update documentation version

---

## 🛠️ Tools & Resources

### Viewing Diagrams

**Local Development**:
```bash
# VS Code with Mermaid extension
code docs/architecture/precision-diagrams.md

# Cursor (native support)
cursor docs/architecture/precision-diagrams.md

# Browser preview
open https://mermaid.live/
```

### Exporting Diagrams

```bash
# Install mermaid-cli
npm install -g @mermaid-js/mermaid-cli

# Export to PNG
mmdc -i docs/architecture/precision-diagrams.md -o exports/diagrams.png

# Export to SVG
mmdc -i docs/architecture/precision-diagrams.md -o exports/diagrams.svg -t dark

# Batch export all diagrams
./scripts/export_diagrams.sh
```

### Validating Diagrams

```bash
# Check syntax
mmdc -i docs/architecture/precision-diagrams.md --dry-run

# Validate all markdown files
./scripts/validate_diagrams.sh


# Crate Naming (Updated 2025-01-15)
# All crates use "adapteros-*" prefix (updated from legacy "mplora-*")

# Check for outdated crate names
>
grep -r "mplora-" docs/architecture/precision-diagrams.md
# Should return no results
```

---

## 📞 Support

### Questions About Diagrams

**General questions**: Check [DIAGRAM_REFERENCE.md](DIAGRAM_REFERENCE.md) FAQ  
**Specific component**: Use quick lookup in DIAGRAM_REFERENCE.md  
**Outdated diagram**: Create issue or PR with corrections  
**Missing diagram**: Suggest in discussions  

### Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution guidelines.

**Diagram Contributions**:
1. Follow Mermaid.js syntax
2. Verify against code
3. Include metadata (last updated, code refs)
4. Test rendering
5. Update indexes
6. Submit PR

---

## 🎓 Educational Resources

### Internal Docs

- **[QUICKSTART.md](QUICKSTART.md)** - 10-minute getting started
- **[CLAUDE.md](../CLAUDE.md)** - Developer guide
- **[CONTRIBUTING.md](../CONTRIBUTING.md)** - Contribution guide
- **[Code Intelligence Docs](code-intelligence/)** - 20 detailed docs

### External References

- **[Mermaid Documentation](https://mermaid.js.org/)**
- **[Axum Documentation](https://docs.rs/axum/)** - Web framework
- **[SQLx Documentation](https://docs.rs/sqlx/)** - Database library
- **[Metal Documentation](https://developer.apple.com/metal/)** - GPU programming

---

## 📝 Document Conventions

### Naming

- **Diagram files**: `kebab-case.md`
- **Sections**: `## Title Case`
- **Component names**: Match codebase exactly
- **File paths**: Relative to workspace root

### Structure

```markdown
# Diagram Title

**Purpose**: Brief description

**Code References**:
- `crates/path/to/file.rs:123` - Specific reference

## Diagram

```mermaid
[diagram code]
```

## Explanation

Detailed explanation of the diagram.
```

### Metadata

Required in all diagrams:
- **Purpose**: What the diagram shows
- **Code References**: Exact file paths
- **Verification Status**: ✅ Verified or ⚠️ Unverified
- **Last Updated**: ISO date

---

## 🌟 Highlights

### Most Useful Diagrams

1. **[System Architecture](architecture/precision-diagrams.md#1-system-architecture)** - Understanding the whole system
2. **[Inference Pipeline](architecture/precision-diagrams.md#2-inference-pipeline-flow)** - How inference works
3. **[Router Scoring](architecture/precision-diagrams.md#3-router-scoring--selection)** - Router algorithm
4. **[API Stack](architecture/precision-diagrams.md#7-api-stack-architecture)** - All API routes
5. **[Database ERD](database-schema/schema-diagram.md)** - Complete data model

### Most Referenced

1. System Architecture - Referenced by all docs
2. Inference Pipeline - Core functionality
3. Database ERD - Data model foundation
4. Router Scoring - Algorithm understanding
5. Memory System - Resource management

---

## 📚 Complete File Listing

```
docs/
├── architecture/
│   ├── precision-diagrams.md         [8 diagrams, code-verified]
│   ├── DIAGRAM_SUMMARY.md            [Metrics and overview]
│   ├── README.md                     [Directory index]
│   └── MasterPlan.md                 [System design]
│
├── database-schema/
│   ├── schema-diagram.md             [Complete ERD]
│   ├── README.md                     [Schema overview]
│   ├── MAINTENANCE.md                [Maintenance guide]
│   ├── VALIDATION.md                 [Validation procedures]
│   └── workflows/
│       ├── promotion-pipeline.md     [CP promotion]
│       ├── monitoring-flow.md        [Metrics]
│       ├── security-compliance.md    [Security]
│       ├── git-repository-workflow.md[Git integration]
│       ├── adapter-lifecycle.md      [Adapter states]
│       ├── incident-response.md      [Troubleshooting]
│       ├── performance-dashboard.md  [Performance]
│       ├── code-intelligence.md      [Code analysis]
│       └── replication-distribution.md[Multi-node]
│
├── code-intelligence/
│   └── [20 detailed specification files]
│
├── DIAGRAM_REFERENCE.md              [Quick lookup guide]
├── ARCHITECTURE_INDEX.md             [This file]
├── architecture.md                   [Main architecture doc]
├── control-plane.md                  [API documentation]
├── api.md                            [OpenAPI spec]
├── runtime-diagrams.md               [Legacy diagrams]
└── [Additional documentation files]
```

---

## 🚦 Quick Status

| Category | Status | Last Updated |
|----------|--------|--------------|
| Precision Diagrams | ✅ Current | 2025-01-14 |
| Database Diagrams | ✅ Current | 2025-01-14 |
| Workflow Diagrams | ✅ Current | 2025-01-14 |
| API Documentation | ✅ Current | 2025-01-14 |
| Code References | ✅ Current | 2025-01-14 |

| Legacy Diagrams | ✅ Removed | 2025-01-15 (cleaned up) |

| Legacy Diagrams | ⚠️ Outdated | 2025-10-09 |
>

---

**Total Documentation Files**: 50+  
**Total Diagrams**: 26  
**Code-Verified Diagrams**: 17  
**Total Tables Documented**: 30+  
**Total API Endpoints**: 60+  
**Coverage**: 100% of system components  

---

**Last Updated**: 2025-01-14  
**Maintained By**: AdapterOS Team  
**License**: MIT OR Apache-2.0



>
