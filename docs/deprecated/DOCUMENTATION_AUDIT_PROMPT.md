# Documentation Audit Prompt

**Purpose:** Systematic evaluation of root-level documentation files to categorize, consolidate, and clean up while preserving valuable content.

**Usage:** Use this prompt with an AI assistant to evaluate each documentation file in batches.

---

## DOCUMENTATION AUDIT PROMPT

```
You are auditing documentation for the AdapterOS project. For each file I provide, analyze and respond with:

## 1. CLASSIFICATION
Categorize as ONE of:
- CORE: Essential project docs (README, CONTRIBUTING, SECURITY, LICENSE, CHANGELOG)
- ARCHITECTURE: Design decisions, patterns, system architecture (belongs in docs/)
- OPERATIONAL: Runbooks, deployment guides, troubleshooting (belongs in docs/)
- IMPLEMENTATION: Feature-specific implementation details (belongs in docs/features/ or relevant subdir)
- STATUS: Progress tracking, audit results, verification reports (potentially ephemeral)
- EPHEMERAL: Fix summaries, debugging notes, one-time task records (candidate for archive/delete)

## 2. TEMPORAL VALUE
Rate 1-5:
- 5: Evergreen - Always relevant (README, architecture docs)
- 4: Long-term - Relevant for 6+ months (guides, patterns)
- 3: Medium-term - Relevant during active development phase
- 2: Short-term - Relevant for specific task/sprint
- 1: Expired - Task complete, info superseded or stale

## 3. DUPLICATION CHECK
- Does content overlap with docs/ files?
- Could this merge into an existing doc?
- Is this a snapshot of info that exists elsewhere (CLAUDE.md, AGENTS.md)?

## 4. RECOMMENDATION
One of:
- KEEP_ROOT: Essential, belongs in project root
- MOVE_DOCS: Move to docs/ (specify subdirectory)
- MOVE_ARCHIVE: Move to docs/archive/ (valuable history but not active reference)
- MERGE: Consolidate into [specify target doc]
- DELETE: Truly ephemeral, no future value (explain why safe to delete)

## 5. CONFIDENCE
Rate your recommendation confidence: HIGH / MEDIUM / LOW

Respond in this exact format for batch processing.
```

---

## Execution Strategy

### Phase 1: Triage by Pattern

Files matching these patterns are likely ephemeral:
- `*_FIXES_*.md`, `*_FIX_*.md` - Post-implementation summaries
- `*_SUMMARY.md`, `*_REPORT.md` - One-time analysis outputs
- `*_CHECKLIST.md` - Completed task tracking
- `*_IMPLEMENTATION.md` - Feature completion records

Files likely essential:
- `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `LICENSE*`, `CHANGELOG.md`
- `CLAUDE.md`, `AGENTS.md` - AI assistant guidance
- `QUICKSTART*.md` - Onboarding
- `PRD.md` - Product requirements

### Phase 2: Batch Review

Group files by prefix pattern and review in batches:
1. `AUTH_*` files (4 files)
2. `*_FIX*` files (8+ files)
3. `*_IMPLEMENTATION*` files (6+ files)
4. `*_SUMMARY*` files (5+ files)
5. `BENCHMARK*`, `BUILD*`, `TEST*` files
6. Integration guides (`MLX_*`, `COREML_*`, etc.)

### Phase 3: Target Directories

Based on existing `docs/` structure:
- `docs/` - General documentation
- `docs/archive/` - Historical records (already exists)
- `docs/features/` - Feature-specific docs
- `docs/internal/` - Internal implementation details
- `docs/prd/` - Product requirement docs

---

## Safety Guardrails

1. **Never delete without review** - Archive first, delete after 30 days if unneeded
2. **Preserve git history** - Use `git mv` for moves
3. **Check for references** - Search codebase for links before moving/deleting
4. **Batch by confidence** - Handle HIGH confidence actions first

---

## Root Files Inventory

Current root markdown files (95 total):

**Likely Keep in Root:**
- README.md, CONTRIBUTING.md, SECURITY.md, LICENSE*, CHANGELOG.md
- CLAUDE.md, AGENTS.md, CITATIONS.md
- QUICKSTART.md, QUICKSTART_GPU_TRAINING.md, PRD.md

**Likely Move to docs/:**
- BENCHMARK_GUIDE.md, BENCHMARK_RESULTS.md
- ERROR_HANDLING_PATTERNS.md, ERROR_REFERENCE.md
- MLX_*.md files (6 files)
- DEMO_GUIDE.md

**Likely Archive or Delete:**
- *_FIXES_*.md, *_FIX_*.md files
- *_CHECKLIST.md (completed checklists)
- *_ANALYSIS.md, *_AUDIT.md (one-time analyses)
- *_SUMMARY.md (implementation summaries)

---

**Last Updated:** 2025-01-27
**Maintained by:** Documentation Team

