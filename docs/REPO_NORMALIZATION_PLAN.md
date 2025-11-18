# Repository Normalization Plan

**Generated:** 2025-11-17
**Purpose:** Standardize folder structure, naming conventions, and eliminate duplicate definitions
**Agent:** Repo Health & API Contracts

---

## Current State Assessment

### Directory Structure Analysis

```
adapter-os/
├── crates/              ✅ Well-organized (65+ crates)
├── ui/                  ✅ Single UI project
├── docs/                ⚠️  Multiple doc locations
├── tests/               ✅ Centralized tests
├── migrations/          ✅ Centralized migrations
├── scripts/             ✅ Build/dev scripts
├── config/              ⚠️  Also configs/ exists
├── configs/             ⚠️  Duplicate of config/
├── deprecated/          ✅ Archived code
├── adapters/            ⚠️  Example adapters (unclear purpose)
├── baselines/           ⚠️  Golden run baselines (scattered)
├── golden_runs/         ⚠️  Duplicate of baselines/
├── manifests/           ⚠️  Example manifests
├── examples/            ⚠️  Also has manifests/
├── menu-bar-app/        ⚠️  macOS app (separate project?)
├── installer/           ✅ macOS installer
├── metal/               ✅ Metal shaders
├── plan/                ⚠️  Plan files (unclear)
├── training/            ⚠️  Training data (should be in test_data?)
├── test_data/           ⚠️  Also test data
├── test_training_dir/   ⚠️  Another test data location
├── tools/               ✅ Utility tools
├── xtask/               ✅ Build tasks
├── fuzz/                ✅ Fuzzing targets
├── jkca-trainer/        ⚠️  Personal trainer (should be in crates?)
└── test-status-writer/  ⚠️  Test utility (should be in tools?)
```

### Issues Identified

#### 🔴 Critical Issues
1. **Duplicate Directories**
   - `config/` vs `configs/`
   - `baselines/` vs `golden_runs/`
   - `test_data/` vs `test_training_dir/` vs `training/`

2. **Unresolved Merge Conflicts**
   - 22 merge conflict markers in `ui/src/api/`
   - Blocks TypeScript builds

3. **Ambiguous Project Scope**
   - `menu-bar-app/` - Separate deliverable or part of main project?
   - `jkca-trainer/` - Personal or canonical?
   - `test-status-writer/` - Tool or test fixture?

#### 🟡 Warning Issues
1. **Scattered Documentation**
   - `docs/` (main)
   - `ui/docs/`
   - `crates/*/README.md`
   - No clear doc hierarchy

2. **Inconsistent Naming**
   - `adapteros-*` (Rust crates) ✅
   - `mplora-*` (legacy crates) ⚠️
   - Inconsistent hyphenation

3. **Multiple Migration Directories**
   - `/migrations/` (canonical per CLAUDE.md)
   - `/migrations_postgres/`
   - `/menu-bar-app/migrations/`
   - `/menu-bar-app/migrations_postgres/`
   - `/crates/adapteros-db/migrations/` (DEPRECATED)

---

## Normalization Plan

### Phase 1: Critical Fixes (Week 1)

#### 1.1 Resolve Merge Conflicts
```bash
# Identify all conflicts
git diff --check
grep -r "<<<<<<< HEAD" ui/src/

# Fix files
# - ui/src/api/types.ts
# - ui/src/api/client.ts

# Verify clean
git status
```

**Owner:** Any agent
**Blockers:** None
**Success Criteria:** Zero merge conflict markers

#### 1.2 Consolidate Duplicate Directories

**Config Consolidation**
```bash
# Decision: Keep configs/ (plural, matches crates/ convention)
# Move config/cron/ to configs/cron/
mv config/cron configs/cron
rm -rf config/

# Update references
grep -r "config/cron" --files-with-matches | xargs sed -i 's|config/cron|configs/cron|g'
```

**Baselines Consolidation**
```bash
# Decision: Keep test/golden_baselines/ (matches test structure)
# Move baselines/ and golden_runs/ to tests/golden_baselines/
mv baselines/* tests/golden_baselines/
mv golden_runs/* tests/golden_baselines/
rm -rf baselines/ golden_runs/

# Update references in crates
grep -r "baselines/" --files-with-matches crates/ | xargs sed -i 's|baselines/|tests/golden_baselines/|g'
grep -r "golden_runs/" --files-with-matches crates/ | xargs sed -i 's|golden_runs/|tests/golden_baselines/|g'
```

**Test Data Consolidation**
```bash
# Decision: Keep test_data/ as canonical test fixtures
# Move training data to test_data/training/
mv training/ test_data/training/
mv test_training_dir/* test_data/training/
rm -rf test_training_dir/

# Update references
find crates/ -name "*.rs" -exec sed -i 's|training/datasets|test_data/training/datasets|g' {} \;
find crates/ -name "*.rs" -exec sed -i 's|test_training_dir|test_data/training|g' {} \;
```

#### 1.3 Document Ambiguous Projects

Create `PROJECT_INVENTORY.md`:
```markdown
# AdapterOS Project Inventory

## Core Projects
- `crates/adapteros-*` - Core control plane
- `crates/adapteros-cli` - CLI tool (aosctl)
- `ui/` - React dashboard
- `crates/adapteros-server` - Main server binary

## Auxiliary Projects
- `menu-bar-app/` - macOS menu bar app (separate deliverable, optional)
- `installer/` - macOS installer (packaging only)
- `jkca-trainer/` - Internal trainer tool (dev only, not shipped)
- `test-status-writer/` - Test fixture generator (dev only)

## Deprecated
- `crates/mplora-*` - Legacy crates (to be removed)
- `deprecated/` - Archived experimental code
```

---

### Phase 2: Structural Improvements (Week 2)

#### 2.1 Flatten Redundant Folders

**Move Examples to docs/**
```bash
# Consolidate examples
mv examples/ docs/examples/
mv manifests/ docs/examples/manifests/
mv adapters/ docs/examples/adapters/

# Update README references
sed -i 's|examples/manifests|docs/examples/manifests|g' README.md
```

**Consolidate Plan Files**
```bash
# Decision: Move to test_data/plans/ (these are test fixtures)
mv plan/ test_data/plans/
```

#### 2.2 Standardize Crate Naming

**Identify Legacy Crates**
```bash
find crates/ -name "Cargo.toml" -exec grep -l "^name = \"mplora-" {} \;
```

**Expected:**
- `crates/mplora-server/` → Should be `adapteros-server-legacy`?
- `crates/mplora-codegraph-viewer/` → Should be `adapteros-codegraph-ui`?

**Action:** Document migration path or rename + update dependencies

#### 2.3 Centralize Documentation

**Proposed Structure:**
```
docs/
├── api/                 # API documentation
│   ├── openapi.json     # Generated OpenAPI spec
│   └── contracts/       # Contract maps
├── architecture/        # Existing architecture docs
├── database/            # Database schema docs
├── cli/                 # CLI reference
├── ui/                  # UI component docs
├── examples/            # Code examples (from /examples)
│   ├── adapters/        # Example adapters
│   ├── manifests/       # Example manifests
│   └── workflows/       # Workflow examples
├── internal/            # Internal design docs
└── patents/             # Patent documentation
```

**Migration:**
```bash
# Move UI docs
mv ui/docs/* docs/ui/

# Move scattered examples
# (Already done in 2.1)

# Generate API docs
cargo doc --workspace --no-deps
mv target/doc docs/api/rustdoc
```

---

### Phase 3: Naming Conventions (Week 3)

#### 3.1 Establish Naming Standards

**File Naming:**
- Rust: `snake_case.rs`
- TypeScript: `camelCase.ts` or `PascalCase.tsx` (components)
- Markdown: `SCREAMING_SNAKE_CASE.md` (top-level) or `lowercase-kebab.md` (nested)
- Config: `lowercase.toml`, `lowercase.yaml`

**Crate Naming:**
- Pattern: `adapteros-{domain}-{subdomain}`
- Examples: `adapteros-lora-router`, `adapteros-api-types`
- No `mplora-*` prefix for new crates

**Type Naming (TypeScript):**
- Interfaces: `PascalCase` + descriptive suffix
  - Requests: `*Request` (e.g., `LoginRequest`)
  - Responses: `*Response` (e.g., `LoginResponse`)
  - Data: `*` (e.g., `Adapter`, `User`)
- Enums: `PascalCase` type, lowercase variants
  - Example: `type UserRole = 'admin' | 'operator' | 'sre'`

**Type Naming (Rust):**
- Structs: `PascalCase`
- Enums: `PascalCase` type + variants
- Traits: `PascalCase` (often ends in -able, -er)

#### 3.2 Audit Violations

**Scan for Violations:**
```bash
# Find non-standard Rust files
find crates/ -name "*.rs" ! -name "*_*.rs" ! -name "lib.rs" ! -name "main.rs"

# Find non-standard TypeScript files
find ui/src/ -name "*.ts" -o -name "*.tsx" | while read f; do
  basename "$f" | grep -E '[^a-zA-Z]' && echo "$f"
done

# Find mixed-case config files
find . -maxdepth 2 -name "*.toml" -o -name "*.yaml" | grep -E '[A-Z]'
```

**Document Exceptions:**
- `README.md`, `LICENSE`, `CONTRIBUTING.md` (conventional)
- `Cargo.toml`, `package.json` (conventional)

---

### Phase 4: Duplicate Schema Elimination (Week 4)

#### 4.1 Identify Duplicate Definitions

**Router Decision Types:**
```bash
# Search for RouterDecision definitions
rg "struct RouterDecision|interface RouterDecision|type RouterDecision" --type rust --type ts
```

**Expected Locations:**
- `crates/adapteros-api-types/src/inference.rs` ✅ (canonical)
- `ui/src/api/types.ts` ✅ (DTO)
- Any other? ❌ (duplicate)

**Adapter Metadata Types:**
```bash
rg "struct AdapterMeta|interface AdapterMeta|type AdapterMeta" --type rust --type ts
```

**Telemetry Event Types:**
```bash
rg "struct TelemetryEvent|interface TelemetryEvent|type TelemetryEvent" --type rust --type ts
```

#### 4.2 Establish Single Source of Truth

**Decision Matrix:**
| Type Category | Source of Truth | Reason |
|---------------|----------------|--------|
| API Contracts | Rust `adapteros-api-types` | OpenAPI generation, type safety |
| UI-Only Types | TypeScript `ui/src/api/types.ts` | Client-specific (e.g., UI state) |
| Database Schema | SQL migrations | Authoritative source |
| Telemetry Events | Rust `adapteros-telemetry-types` | Event sourcing, immutable |

**Auto-Generation:**
```bash
# Generate TypeScript from Rust (future)
# Use openapi-typescript or similar
npx openapi-typescript docs/api/openapi.json --output ui/src/api/generated.ts
```

#### 4.3 Remove Duplicate Definitions

**Process:**
1. Identify canonical source (per matrix above)
2. Search codebase for duplicates
3. Replace with imports from canonical source
4. Delete duplicate definitions
5. Run tests to verify

**Example - RouterDecision:**
```rust
// BEFORE: Duplicate in worker crate
// crates/adapteros-lora-worker/src/types.rs
pub struct RouterDecision { ... } // ❌ DUPLICATE

// AFTER: Import from canonical
// crates/adapteros-lora-worker/src/types.rs
use adapteros_api_types::RouterDecision; // ✅ CANONICAL
```

---

## Phase 5: Linting & Enforcement (Week 5)

### 5.1 Add Pre-Commit Hooks

**`.githooks/pre-commit`:**
```bash
#!/bin/bash

# Check for merge conflict markers
if grep -r "<<<<<<< HEAD" .; then
  echo "ERROR: Merge conflict markers found!"
  exit 1
fi

# Run Rust formatter
cargo fmt --all -- --check || {
  echo "ERROR: Run 'cargo fmt --all'"
  exit 1
}

# Run Clippy
cargo clippy --workspace -- -D warnings || {
  echo "ERROR: Fix clippy warnings"
  exit 1
}

# Run TypeScript checks
cd ui && npm run lint && npm run type-check || {
  echo "ERROR: Fix TypeScript errors"
  exit 1
}
```

### 5.2 CI Enforcement

**`.github/workflows/repo-health.yml`:**
```yaml
name: Repository Health

on: [pull_request]

jobs:
  check-structure:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Check for merge conflicts
        run: |
          if grep -r "<<<<<<< HEAD" .; then
            echo "Merge conflicts found"
            exit 1
          fi

      - name: Check for duplicate directories
        run: |
          # Ensure deprecated directories don't exist
          [ ! -d "config" ] || exit 1
          [ ! -d "baselines" ] || exit 1
          [ ! -d "golden_runs" ] || exit 1

      - name: Verify naming conventions
        run: |
          # No mixed-case config files
          find . -maxdepth 2 -name "*.toml" -o -name "*.yaml" | grep -E '[A-Z]' && exit 1 || true

  schema-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Check for duplicate type definitions
        run: |
          # Count RouterDecision definitions (should be ≤2: Rust + TS)
          COUNT=$(rg "struct RouterDecision|interface RouterDecision" -c | wc -l)
          [ "$COUNT" -le 2 ] || exit 1
```

### 5.3 Documentation Standards

**Add to CONTRIBUTING.md:**
```markdown
## Directory Structure Standards

- `/crates/` - Rust crates only
- `/ui/` - React UI (single project)
- `/docs/` - All documentation
- `/tests/` - Integration tests
- `/scripts/` - Build scripts
- `/configs/` - Configuration files
- `/migrations/` - Database migrations (canonical)

**Do NOT create:**
- Duplicate directories (e.g., `config/` when `configs/` exists)
- Top-level project folders (use `crates/` or `ui/`)
- Scattered test data (use `test_data/`)

## Naming Conventions

- Crates: `adapteros-{domain}-{subdomain}`
- Rust files: `snake_case.rs`
- TypeScript files: `camelCase.ts`, `PascalCase.tsx`
- Markdown: `SCREAMING_SNAKE_CASE.md` (top-level), `kebab-case.md` (nested)
```

---

## Migration Checklist

### Week 1: Critical Fixes
- [ ] Resolve all merge conflicts in `ui/src/api/`
- [ ] Consolidate `config/` → `configs/`
- [ ] Consolidate `baselines/`, `golden_runs/` → `tests/golden_baselines/`
- [ ] Consolidate `test_data/`, `test_training_dir/`, `training/` → `test_data/`
- [ ] Create `PROJECT_INVENTORY.md`

### Week 2: Structural Improvements
- [ ] Move `examples/`, `manifests/`, `adapters/` → `docs/examples/`
- [ ] Move `plan/` → `test_data/plans/`
- [ ] Audit `mplora-*` crates, document migration path
- [ ] Centralize documentation in `docs/`

### Week 3: Naming Conventions
- [ ] Document naming standards in `CONTRIBUTING.md`
- [ ] Audit file naming violations
- [ ] Standardize TypeScript type naming (fix `UserRole` duplication)

### Week 4: Schema Deduplication
- [ ] Identify duplicate type definitions
- [ ] Establish canonical sources
- [ ] Remove duplicates, add imports
- [ ] Run tests to verify

### Week 5: Linting & Enforcement
- [ ] Add pre-commit hooks
- [ ] Add CI repo health checks
- [ ] Document standards in `CONTRIBUTING.md`

---

## Success Metrics

### Quantitative
- **Zero** merge conflict markers
- **Zero** duplicate directories
- **Zero** duplicate type definitions (within tolerance)
- **100%** file naming compliance
- **100%** pre-commit hook pass rate

### Qualitative
- Clear directory hierarchy
- Documented project inventory
- Enforced naming standards
- Automated drift detection

---

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking references during moves | High | Comprehensive grep/sed, thorough testing |
| Renaming crates breaks deps | High | Update all Cargo.toml dependencies |
| Lost git history | Medium | Use `git mv` instead of `mv` |
| CI failures after changes | Medium | Test locally before commit |
| Team confusion | Low | Clear communication, document changes |

---

## Appendix A: Directory Inventory (Current)

```
adapter-os/
├── .cargo/              # Cargo config
├── .git/                # Git metadata
├── .githooks/           # Custom git hooks
├── .github/             # GitHub Actions
├── adapters/            # ⚠️  Example adapters
├── baselines/           # ⚠️  Golden run baselines (duplicate)
├── config/              # ⚠️  Config (duplicate of configs/)
├── configs/             # Configuration files
├── crates/              # ✅ Rust crates (65+)
├── deprecated/          # ✅ Archived code
├── docs/                # ✅ Documentation
├── etc/                 # System config files
├── examples/            # ⚠️  Code examples
├── fuzz/                # ✅ Fuzzing targets
├── golden_runs/         # ⚠️  Golden runs (duplicate)
├── installer/           # ✅ macOS installer
├── jkca-trainer/        # ⚠️  Personal trainer?
├── manifests/           # ⚠️  Example manifests
├── menu-bar-app/        # ⚠️  macOS menu bar app
├── metal/               # ✅ Metal shaders
├── migrations/          # ✅ Database migrations (canonical)
├── migrations_postgres/ # ⚠️  Postgres migrations?
├── plan/                # ⚠️  Plan files
├── scripts/             # ✅ Build scripts
├── src/                 # Workspace-level source?
├── test-status-writer/  # ⚠️  Test utility
├── test_data/           # ⚠️  Test fixtures
├── test_training_dir/   # ⚠️  Test training data (duplicate)
├── tests/               # ✅ Integration tests
├── tools/               # ✅ Utility tools
├── training/            # ⚠️  Training data (duplicate)
├── ui/                  # ✅ React UI
└── xtask/               # ✅ Build tasks
```

**Legend:**
- ✅ Well-organized, keep as-is
- ⚠️  Needs consolidation or clarification

---

## Appendix B: Proposed Directory Structure

```
adapter-os/
├── .cargo/              # Cargo config
├── .git/                # Git metadata
├── .githooks/           # Git hooks (pre-commit, etc.)
├── .github/             # GitHub Actions workflows
├── crates/              # All Rust crates
│   ├── adapteros-*/     # Core crates
│   ├── tools/           # Internal tools (jkca-trainer, etc.)
│   └── deprecated/      # Legacy crates (mplora-*)
├── ui/                  # React dashboard
├── docs/                # All documentation
│   ├── api/             # API docs (OpenAPI, contracts)
│   ├── architecture/    # Architecture docs
│   ├── cli/             # CLI reference
│   ├── database/        # Database schema
│   ├── examples/        # Code examples
│   │   ├── adapters/    # Example adapters
│   │   ├── manifests/   # Example manifests
│   │   └── workflows/   # Workflow examples
│   ├── internal/        # Internal design docs
│   ├── patents/         # Patent docs
│   └── ui/              # UI component docs
├── tests/               # Integration tests
│   ├── benchmark/       # Performance tests
│   ├── e2e/             # End-to-end tests
│   ├── fixtures/        # Test fixtures
│   ├── golden_baselines/# Golden run baselines
│   └── security/        # Security tests
├── test_data/           # Test data fixtures
│   ├── adapters/        # Adapter test data
│   ├── plans/           # Plan test data
│   └── training/        # Training test data
├── scripts/             # Build and dev scripts
├── configs/             # Configuration files
│   └── cron/            # Cron configs
├── migrations/          # Database migrations (SQLite)
├── migrations_postgres/ # Postgres migrations (if needed)
├── metal/               # Metal shaders
├── installer/           # macOS installer
├── menu-bar-app/        # macOS menu bar app (optional)
├── fuzz/                # Fuzzing targets
├── xtask/               # Build tasks
├── deprecated/          # Archived projects
└── tools/               # External tools
    └── inventory/       # Inventory tools
```

---

**Document Version:** 1.0
**Status:** Draft
**Next Review:** After Phase 1 completion
**Maintainer:** Repo Health Agent
