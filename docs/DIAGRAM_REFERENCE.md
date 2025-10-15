# AdapterOS Diagram Reference

Quick reference for navigating AdapterOS architecture diagrams.

---

## Diagram Locations

### **Precision Diagrams** (Code-Verified) ⭐
**File**: `docs/architecture/precision-diagrams.md`  
**Status**: ✅ Verified against codebase  
**Last Updated**: 2025-01-14

1. **System Architecture** - Complete component graph with exact crate names
2. **Inference Pipeline Flow** - Request lifecycle with line numbers
3. **Router Scoring & Selection** - Algorithm with Q15 quantization
4. **Router Feature Weighting** - 22-dim feature vector breakdown
5. **Memory Management System** - Watchdog, lifecycle, unified memory
6. **Memory Eviction Decision Tree** - Pressure levels and eviction order
7. **API Stack Architecture** - Routes, handlers, middleware
8. **Worker Architecture** - UDS server, pipeline, safety mechanisms

### **Runtime Diagrams** (Legacy)
**File**: `docs/runtime-diagrams.md`  
**Status**: ⚠️ May contain outdated crate names  
**Note**: Use precision diagrams for current codebase

### **Database Schema Diagrams**
**File**: `docs/database-schema/schema-diagram.md`  
**Status**: ✅ Accurate  
**Content**: Complete ERD with all tables and relationships

### **Workflow Diagrams**
**Directory**: `docs/database-schema/workflows/`  
**Status**: ✅ Accurate

- `promotion-pipeline.md` - CP promotion process
- `monitoring-flow.md` - System metrics and health
- `security-compliance.md` - Artifact signing and compliance
- `git-repository-workflow.md` - Repository integration
- `adapter-lifecycle.md` - Adapter state transitions
- `incident-response.md` - Incident handling
- `performance-dashboard.md` - Performance visualization
- `code-intelligence.md` - Code analysis pipeline
- `replication-distribution.md` - Multi-node artifact sync

---

## Quick Lookup

### By Topic

**Architecture**:
- System overview → `precision-diagrams.md` § 1
- Component relationships → `precision-diagrams.md` § 1
- Deployment → `runtime-diagrams.md` § 12

**Inference**:
- Request flow → `precision-diagrams.md` § 2
- Router algorithm → `precision-diagrams.md` § 3
- Feature weights → `precision-diagrams.md` § 4

**Memory**:
- Management system → `precision-diagrams.md` § 5
- Eviction strategy → `precision-diagrams.md` § 6
- Adapter lifecycle → `workflows/adapter-lifecycle.md`

**API**:
- Stack architecture → `precision-diagrams.md` § 7
- Routes and handlers → `precision-diagrams.md` § 7
- Authentication flow → `precision-diagrams.md` § 7

**Worker**:
- Worker architecture → `precision-diagrams.md` § 8
- Safety mechanisms → `precision-diagrams.md` § 8
- UDS server → `precision-diagrams.md` § 8

**Database**:
- Schema ERD → `database-schema/schema-diagram.md`
- Table descriptions → `database-schema/README.md`

**Operations**:
- Promotion → `workflows/promotion-pipeline.md`
- Monitoring → `workflows/monitoring-flow.md`
- Incident response → `workflows/incident-response.md`
- Security → `workflows/security-compliance.md`

### By Role

**Developers**:
1. Start with `precision-diagrams.md` § 1 (System Architecture)
2. Review `precision-diagrams.md` § 2 (Inference Flow)
3. Study `precision-diagrams.md` § 8 (Worker Architecture)

**SREs**:
1. Review `workflows/monitoring-flow.md` (Metrics)
2. Study `precision-diagrams.md` § 6 (Memory Eviction)
3. Reference `workflows/incident-response.md` (Troubleshooting)

**Auditors**:
1. Review `workflows/security-compliance.md` (Compliance)
2. Study `workflows/promotion-pipeline.md` (Quality gates)
3. Reference `database-schema/schema-diagram.md` (Data model)

**Operators**:
1. Start with `precision-diagrams.md` § 7 (API Stack)
2. Review `workflows/promotion-pipeline.md` (Deployments)
3. Study `workflows/adapter-lifecycle.md` (Adapter management)

---

## Key Differences: Precision vs Legacy

### Precision Diagrams (`architecture/precision-diagrams.md`)

✅ **Accurate**:
- Crate names: `adapteros-*` (not `mplora-*`)
- Ports: 3200 (UI dev), 8080 (API)
- Database: SQLite primary, PostgreSQL optional
- File paths with line numbers
- Exact feature weights and thresholds
- All 44 workspace crates represented

### Legacy Diagrams (`runtime-diagrams.md`)

⚠️ **May be outdated**:
- Uses `mplora-*` crate names
- Shows PostgreSQL as primary
- Missing newer components (git, codegraph, deterministic-exec)
- Some port references may be incorrect

**Recommendation**: Use precision diagrams for current work. Legacy diagrams retained for historical reference.

---

## Rendering Diagrams

### VS Code
Install extension: **Markdown Preview Mermaid Support**

### GitHub
Mermaid diagrams render automatically in `.md` files.

### Local Preview
```bash
# Install mermaid-cli
npm install -g @mermaid-js/mermaid-cli

# Render to PNG
mmdc -i docs/architecture/precision-diagrams.md -o diagrams.png

# Or use mermaid.live
open https://mermaid.live/
```

### IDE Integration
- **Cursor**: Native Mermaid rendering in markdown preview
- **VS Code**: Install Mermaid extension
- **IntelliJ**: Plugin available in marketplace

---

## Diagram Maintenance

### When to Update

Update diagrams when:
- Adding new crates to workspace
- Changing API routes or endpoints
- Modifying router feature weights
- Changing memory thresholds
- Adding new safety mechanisms
- Updating database schema

### Verification Checklist

Before committing diagram updates:

- [ ] Verify crate names against `Cargo.toml`
- [ ] Check ports against `configs/cp.toml`
- [ ] Confirm file paths exist in codebase
- [ ] Test line number references
- [ ] Validate method/function names with `grep`
- [ ] Check thresholds against code constants
- [ ] Render diagram to verify syntax
- [ ] Update "Last Updated" timestamp
- [ ] Update version number if major changes

### Testing Diagrams

```bash
# Validate all diagrams render without errors
for file in docs/**/*.md; do
  if grep -q "```mermaid" "$file"; then
    echo "Checking $file"
    # Add validation logic here
  fi
done
```

---

## FAQ

### Q: Which diagram should I use for understanding the request flow?
**A**: `precision-diagrams.md` § 2 (Inference Pipeline Flow) - has exact step-by-step flow with line numbers.

### Q: Where can I see the router feature weights?
**A**: `precision-diagrams.md` § 4 (Router Feature Weighting) - shows all 22 dimensions and weights.

### Q: How do I understand memory eviction?
**A**: `precision-diagrams.md` § 6 (Memory Eviction Decision Tree) - shows pressure levels and eviction order.

### Q: What are all the API endpoints?
**A**: `precision-diagrams.md` § 7 (API Stack Architecture) - complete route listing.

### Q: Where is the database schema?
**A**: `database-schema/schema-diagram.md` - ERD with all tables and relationships.

### Q: How does promotion work?
**A**: `database-schema/workflows/promotion-pipeline.md` - complete promotion flow with quality gates.

### Q: What safety mechanisms exist in workers?
**A**: `precision-diagrams.md` § 8 (Worker Architecture) - all five safety mechanisms with thresholds.

### Q: How is memory managed?
**A**: `precision-diagrams.md` § 5 (Memory Management System) - watchdog, lifecycle, unified memory.

---

## Contributing

When adding new diagrams:

1. Create in appropriate directory:
   - System architecture → `docs/architecture/`
   - Workflows → `docs/database-schema/workflows/`
   - Feature-specific → `docs/code-intelligence/`, etc.

2. Use Mermaid.js syntax

3. Include metadata:
   ```markdown
   # Diagram Title
   
   ## Overview
   Brief description
   
   ## Code References
   - `crates/path/to/file.rs:123` - Specific reference
   
   ```mermaid
   [diagram]
   ```
   ```

4. Verify against code before committing

5. Update this reference document

---

## External Resources

- [Mermaid.js Documentation](https://mermaid.js.org/)
- [Mermaid Live Editor](https://mermaid.live/)
- [AdapterOS Architecture Guide](architecture.md)
- [AdapterOS CLAUDE.md](../../CLAUDE.md)

---

**Quick Links**:
- [Precision Diagrams](architecture/precision-diagrams.md)
- [Database Schema](database-schema/schema-diagram.md)
- [Promotion Pipeline](database-schema/workflows/promotion-pipeline.md)
- [System Architecture](architecture.md)

