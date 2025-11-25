# Training Provenance & Evidence (PRD-DATA-01)

**Status:** Implemented (v0.3-alpha)
**Policy:** cp-evidence-004
**Last Updated:** 2025-11-25

## Overview

AdapterOS implements comprehensive training provenance tracking to ensure T1 (persistent/production) adapters have documented evidence of their training data, quality validation, and lineage.

## Policy Requirements (cp-evidence-004)

### T1 Adapter Requirements

All T1 (persistent tier) adapters MUST have:

1. **Primary Dataset Specified** - `primary_dataset_id` field populated
2. **Evidence Entries** - At least one evidence entry documenting provenance
3. **Eval Dataset (Production)** - For production T1 adapters, an `eval_dataset_id` is required

### Violation Handling

- Missing primary dataset → Policy violation (non-compliant)
- Missing evidence entries → Policy violation (non-compliant)
- Missing eval dataset (production) → Warning (soft violation)

## Database Schema

### Extended `training_datasets` Table (Migration 0084)

```sql
-- Dataset type classification
ALTER TABLE training_datasets ADD COLUMN dataset_type TEXT NOT NULL DEFAULT 'training'
    CHECK(dataset_type IN ('training', 'eval', 'red_team', 'logs', 'other'));

-- Purpose and provenance
ALTER TABLE training_datasets ADD COLUMN purpose TEXT;
ALTER TABLE training_datasets ADD COLUMN source_location TEXT;
ALTER TABLE training_datasets ADD COLUMN collection_method TEXT NOT NULL DEFAULT 'manual'
    CHECK(collection_method IN ('manual', 'sync', 'api', 'pipeline', 'scrape', 'other'));
ALTER TABLE training_datasets ADD COLUMN ownership TEXT;
ALTER TABLE training_datasets ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE;
```

### Evidence Entries Table (Migration 0085)

```sql
CREATE TABLE IF NOT EXISTS evidence_entries (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT REFERENCES training_datasets(id) ON DELETE CASCADE,
    adapter_id TEXT REFERENCES adapters(id) ON DELETE CASCADE,
    evidence_type TEXT NOT NULL CHECK(evidence_type IN ('doc', 'ticket', 'commit', 'policy_approval', 'data_agreement', 'review', 'audit', 'other')),
    reference TEXT NOT NULL,  -- URL, commit SHA, ticket ID, document path
    description TEXT,
    confidence TEXT NOT NULL DEFAULT 'medium' CHECK(confidence IN ('high', 'medium', 'low')),
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT,
    CHECK (dataset_id IS NOT NULL OR adapter_id IS NOT NULL)
);
```

### Dataset-Adapter Links (Migration 0085)

```sql
CREATE TABLE IF NOT EXISTS dataset_adapter_links (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    adapter_id TEXT NOT NULL REFERENCES adapters(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL CHECK(link_type IN ('training', 'eval', 'validation', 'test')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(dataset_id, adapter_id, link_type)
);
```

### Adapters Table Extension (Migration 0086)

```sql
-- Primary dataset linkage for T1 adapters
ALTER TABLE adapters ADD COLUMN primary_dataset_id TEXT REFERENCES training_datasets(id) ON DELETE SET NULL;
ALTER TABLE adapters ADD COLUMN eval_dataset_id TEXT REFERENCES training_datasets(id) ON DELETE SET NULL;
```

## Database Operations

### Evidence Entry Management

```rust
use adapteros_db::Db;

// Create evidence entry
let entry_id = db.create_evidence_entry(
    Some("dataset-123"),  // dataset_id
    Some("adapter-456"),  // adapter_id
    "commit",            // evidence_type
    "https://github.com/org/repo/commit/abc123",  // reference
    Some("Training dataset v1.0 commit"),  // description
    "high",              // confidence
    Some("user@example.com"),  // created_by
    None,                // metadata_json
).await?;

// Get evidence for adapter
let evidence = db.get_adapter_evidence("adapter-456").await?;
println!("Adapter has {} evidence entries", evidence.len());

// Count evidence
let count = db.count_adapter_evidence("adapter-456").await?;
```

### Dataset-Adapter Links

```rust
// Create link between dataset and adapter
let link_id = db.create_dataset_adapter_link(
    "dataset-123",
    "adapter-456",
    "training"
).await?;

// Get all adapters trained with a dataset
let links = db.get_dataset_adapters("dataset-123").await?;

// Count usage
let usage_count = db.count_dataset_usage("dataset-123").await?;
println!("Dataset used to train {} adapters", usage_count);
```

## Evidence Types

| Type | Description | Example Reference |
|------|-------------|------------------|
| `doc` | Documentation, design docs | `https://docs.example.com/dataset-spec.md` |
| `ticket` | JIRA/GitHub issue tracking | `https://jira.example.com/PROJ-123` |
| `commit` | Git commit SHA | `https://github.com/org/repo/commit/abc123` |
| `policy_approval` | Policy review approval | `policy-review-2025-11-25.pdf` |
| `data_agreement` | Data usage agreement | `DUA-2025-001` |
| `review` | Code/data review | `https://github.com/org/repo/pull/456` |
| `audit` | Compliance audit report | `audit-report-Q4-2025.pdf` |
| `other` | Other evidence | Custom reference |

## Confidence Levels

- **high** - Verified, signed, or from authoritative source
- **medium** - Documented but not formally verified
- **low** - Informal or incomplete documentation

## Policy Enforcement

### Evidence Policy (cp-evidence-004)

```rust
use adapteros_policy::packs::evidence::{EvidencePolicy, EvidenceConfig};

let config = EvidenceConfig::default();
let policy = EvidencePolicy::new(config);

// Enforce policy with adapter metadata
let audit = policy.enforce(&adapter_context)?;
if !audit.violations.is_empty() {
    eprintln!("Policy violations: {:?}", audit.violations);
}
```

### Compliance API Integration

The compliance audit endpoint (`/v1/audit/compliance`) automatically checks T1 adapter evidence requirements:

```rust
// GET /v1/audit/compliance response includes:
{
  "controls": [
    {
      "control_id": "EVIDENCE-004",
      "control_name": "Training Provenance & Evidence (cp-evidence-004)",
      "status": "compliant" | "non_compliant",
      "findings": [
        "X T1 adapters missing primary dataset",
        "Y T1 adapters missing evidence entries"
      ]
    }
  ]
}
```

## Workflow: Proving Training Provenance

### Step 1: Create Dataset with Metadata

```bash
# Upload dataset with provenance metadata
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=rust-qa-v1" \
  -F "format=jsonl" \
  -F "file=@training.jsonl" \
  -F "dataset_type=training" \
  -F "purpose=Rust Q&A training corpus" \
  -F "source_location=https://github.com/org/rust-qa-corpus" \
  -F "collection_method=sync" \
  -F "ownership=ml-team@example.com"
```

### Step 2: Add Evidence Entries

```rust
// Add commit evidence
db.create_evidence_entry(
    Some("dataset-123"),
    None,
    "commit",
    "https://github.com/org/rust-qa-corpus/commit/abc123",
    Some("Dataset v1.0 release commit"),
    "high",
    Some("ml-team@example.com"),
    None,
).await?;

// Add policy approval
db.create_evidence_entry(
    Some("dataset-123"),
    None,
    "policy_approval",
    "policy-review-2025-11-25-approved",
    Some("Legal review approved dataset usage"),
    "high",
    Some("legal@example.com"),
    None,
).await?;
```

### Step 3: Train Adapter with Dataset

```bash
# Training automatically links dataset to adapter
./target/release/aosctl train \
  --dataset-id dataset-123 \
  --output adapters/rust-expert.aos \
  --rank 16 --epochs 3
```

### Step 4: Verify Compliance

```bash
# Check compliance status
curl -X GET http://localhost:8080/v1/audit/compliance
```

## UI Integration

### Datasets Tab

Displays dataset type, usage count, and evidence count:

```typescript
// ui/src/pages/Training/DatasetsTab.tsx
<TableCell>
  <Badge variant="outline">
    {dataset.dataset_type || 'training'}
  </Badge>
</TableCell>
<TableCell>
  {dataset.usage_count || 0} adapters
</TableCell>
<TableCell>
  {dataset.evidence_count || 0} evidence
</TableCell>
```

### Evidence Explorer

Filter and view evidence entries across datasets and adapters:

```typescript
// ui/src/components/EvidenceExplorer.tsx
<EvidenceExplorer
  filters={{
    dataset_id: "dataset-123",
    evidence_type: "commit",
    confidence: "high"
  }}
/>
```

## Best Practices

1. **Always Document T1 Adapters** - Before promoting to persistent tier, ensure dataset and evidence are documented
2. **Use High Confidence for Critical Evidence** - Policy approvals, audits, and signed documents should be marked as high confidence
3. **Link Eval Datasets** - For production T1 adapters, always specify an eval_dataset_id
4. **Update Evidence Regularly** - Add evidence entries as new documentation becomes available
5. **Track Ownership** - Assign clear ownership for dataset accountability

## References

- [CLAUDE.md](../CLAUDE.md) - Main developer guide
- [TRAINING_PIPELINE.md](TRAINING_PIPELINE.md) - Training workflow
- [DATABASE_REFERENCE.md](DATABASE_REFERENCE.md) - Complete schema
- [RBAC.md](RBAC.md) - Permission requirements

## Policy Testing

Run evidence policy tests:

```bash
cargo test -p adapteros-policy test_t1_adapter
```

Expected tests:
- `test_t1_adapter_without_primary_dataset_violation`
- `test_t1_adapter_without_evidence_entries_violation`
- `test_t1_adapter_compliant`
- `test_non_t1_adapter_no_evidence_requirements`
