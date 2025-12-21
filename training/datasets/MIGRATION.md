# Dataset Taxonomy Migration Guide

**Date:** 2025-11-18
**Migration:** Legacy `codebase/` → Organized 9-category taxonomy

---

## What Changed

### Before
```
training/datasets/
  codebase/
    data_science_ml_patterns/
    advanced_testing_patterns/
    cloud_devops_patterns/
```

### After
```
training/datasets/
  behaviors/          # Adapter lifecycle & runtime
  routing/            # K-sparse router decisions
  stacks/             # Adapter stack composition
  replay/             # Deterministic replay
  determinism/        # Guardrail enforcement
  metrics/            # Telemetry & observability
  cli_contract/       # aosctl command patterns
  code_ingest/        # Document/code parsing
    codebase/         # (moved from root)
  docs_derived/       # AGENTS.md patterns
```

---

## Migration Steps

### For Existing Datasets

1. **Identify Category**
   - Code examples → `code_ingest/`
   - Router patterns → `routing/`
   - Lifecycle examples → `behaviors/`
   - CLI commands → `cli_contract/`
   - Policy patterns → `docs_derived/`

2. **Move Dataset**
   ```bash
   mv training/datasets/my_dataset training/datasets/<category>/my_dataset
   ```

3. **Update manifest.json**
   ```json
   {
     "category": "<new_category>",
     "migrated_from": "codebase",
     "migration_date": "2025-11-18"
   }
   ```

4. **Validate**
   ```bash
   cargo run -p adapteros-cli -- dataset validate training/datasets/<category>/my_dataset
   ```

### For New Datasets

1. **Choose Category** (see README.md)
2. **Create Subdirectory**
   ```bash
   mkdir -p training/datasets/<category>/my_dataset
   ```
3. **Add Files**
   - `*.positive.jsonl` - Positive examples
   - `*.negative.jsonl` - Negative examples (optional)
   - `manifest.json` - Required metadata
   - `README.md` - Dataset documentation

4. **Follow Schema** (see category README)

---

## Quality Thresholds

| Category | Min Examples | Min Relevance | Min Confidence |
|----------|--------------|---------------|----------------|
| behaviors | 500 | 0.85 | 0.90 |
| routing | 1000 | 0.90 | 0.95 |
| stacks | 300 | 0.85 | 0.90 |
| replay | 200 | 0.95 | 0.95 |
| determinism | 500 | 0.95 | 0.95 |
| metrics | 300 | 0.80 | 0.85 |
| cli_contract | 200 | 0.90 | 0.90 |
| code_ingest | 1000 | 0.85 | 0.90 |
| docs_derived | 500 | 0.90 | 0.90 |

---

## Breaking Changes

### Database Schema
- **NONE** - This is a filesystem reorganization only
- `training_datasets` table unchanged
- `dataset_files` table unchanged

### API Endpoints
- **NONE** - Training orchestrator unchanged
- `POST /api/training/datasets` - Path handling unchanged

### CLI Commands
- **NONE** - `aosctl dataset` commands unchanged
- Paths still relative to `training/datasets/`

---

## Rollback

If needed, restore original structure:

```bash
# 1. Move datasets back
mv training/datasets/code_ingest/codebase training/datasets/codebase

# 2. Remove new directories
rm -rf training/datasets/{behaviors,routing,stacks,replay,determinism,metrics,cli_contract,code_ingest,docs_derived}

# 3. Restore from git
git checkout training/datasets/
```

---

## Benefits

1. **Clarity:** Purpose-driven categorization
2. **Discoverability:** Category READMEs with examples
3. **Quality:** Per-category thresholds
4. **Separation:** Router data isolated from lifecycle data
5. **Scalability:** 9 focused categories vs 1 catch-all

---

## Questions?

See `training/datasets/README.md` or contact JKCA.
