# AdapterOS Training Datasets

This directory contains structured training datasets organized by the AdapterOS five-tier hierarchy.

## Dataset Categories

### 1. Base Datasets (`base/`)
**Layer 2: Code (Domain-General)**
- **Purpose**: Generic coding knowledge across languages
- **Category**: `code`
- **Scope**: `global`
- **Tier**: `persistent`
- **Rank**: 16, **Alpha**: 32.0
- **Target Modules**: All 7 linear layers

**Current Datasets:**
- `base/code/adapteros/` - Core AdapterOS patterns and policies

### 2. Framework Datasets (`framework/`)
**Layer 3: Frameworks (Type-Specific)**
- **Purpose**: Stack-specific APIs, idioms, and conventions
- **Category**: `framework`
- **Scope**: `global` (IT-controlled) or `tenant` (custom)
- **Tier**: `persistent`
- **Rank**: 12, **Alpha**: 24.0
- **Target Modules**: q/k/v/o projections

**Current Datasets:**
- `framework/rust/` - Rust ecosystem patterns
- `framework/python/` - Python ecosystem patterns
- `framework/typescript/` - TypeScript ecosystem patterns

### 3. Codebase Datasets (`codebase/`)
**Layer 4: Directory-Specific (Tenant-Specific)**
- **Purpose**: Internal APIs, conventions, directory style
- **Category**: `codebase`
- **Scope**: `tenant`
- **Tier**: `persistent`
- **Rank**: 8, **Alpha**: 16.0
- **Target Modules**: gate/up/down projections

**Current Datasets:**
- `codebase/acme_payments/` - ACME Payments tenant patterns

### 4. Ephemeral Datasets (`ephemeral/`)
**Layer 5: Ephemeral (Per-Directory-Change)**
- **Purpose**: Fresh symbols, recent directory changes
- **Category**: `ephemeral`
- **Scope**: `commit`
- **Tier**: `ephemeral`
- **Rank**: 4, **Alpha**: 8.0
- **TTL**: 24-72 hours
- **Target Modules**: gate/up projections

**Current Datasets:**
- `ephemeral/commit_abc123/` - Example ephemeral commit dataset

## Dataset Structure

Each dataset follows this structure:

```
dataset_name/
├── manifest.json              # Dataset definition and metadata
├── positive-examples.jsonl    # Positive training examples (weight +1.0)
├── negative-examples.jsonl    # Negative/guardrail examples (weight -1.0)
└── README.md                  # Dataset-specific documentation
```

## Manifest Schema

Each `manifest.json` includes:

```json
{
  "name": "dataset_name",
  "description": "Human-readable description",
  "version": "1.0.0",
  "category": "code|framework|codebase|ephemeral",
  "scope": "global|tenant|repo|commit",
  "tier": "persistent|ephemeral",
  "rank": 4-16,
  "alpha": 8.0-32.0,
  "framework_id": "rust|python|typescript",
  "framework_version": "version_string",
  "tenant_id": "tenant_name",
  "repo_id": "owner/repo",
  "commit_sha": "git_commit_hash",
  "ttl": 259200,
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
  "entries": [
    {
      "path": "positive-examples.jsonl",
      "format": "jsonl",
      "weight": 1.0,
      "role": "positive",
      "notes": "Description of positive examples"
    },
    {
      "path": "negative-examples.jsonl",
      "format": "jsonl",
      "weight": -1.0,
      "role": "negative",
      "notes": "Description of negative examples"
    }
  ],
  "provenance": {
    "masterplan_sections": ["docs/architecture/MasterPlan.md#..."],
    "created_by": "creator_name",
    "created_at": "2025-01-15T00:00:00Z",
    "last_reviewed_at": "2025-01-15T00:00:00Z",
    "review_notes": "Review notes"
  },
  "evaluation_gates": [
    "Evaluation criteria 1",
    "Evaluation criteria 2"
  ],
  "acl": ["tenant_name"],
  "intent": "optional_intent_description"
}
```

## Training Examples Format

Each JSONL file contains one training example per line:

```json
{"input": [1, 2, 3, 4], "target": [5, 6, 7, 8], "metadata": {"source": "example"}, "weight": 1.0}
```

Where:
- `input`: Input token sequence (Vec<u32>)
- `target`: Target token sequence (Vec<u32>)
- `metadata`: Optional metadata object
- `weight`: Example weight (+1.0 for positive, -1.0 for negative)

## Usage

### Training Base Adapter
```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --output-dir adapters/ \
  --adapter-id code_lang_v1
```

### Training Framework Adapter
```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/framework/rust/manifest.json \
  --output-dir adapters/ \
  --adapter-id rust_framework_v1
```

### Training Codebase Adapter
```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/codebase/acme_payments/manifest.json \
  --output-dir adapters/ \
  --adapter-id acme_payments_v1
```

### Training Ephemeral Adapter
```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/ephemeral/commit_abc123/manifest.json \
  --output-dir adapters/ \
  --adapter-id commit_abc123
```

## Adding New Datasets

1. **Create directory structure**:
   ```bash
   mkdir -p training/datasets/{category}/{subcategory}/{dataset_name}
   ```

2. **Create manifest.json** following the schema above

3. **Add training examples**:
   - Create `positive-examples.jsonl` with weight +1.0
   - Create `negative-examples.jsonl` with weight -1.0

4. **Train adapter**:
   ```bash
   cargo xtask train-base-adapter --manifest path/to/manifest.json
   ```

5. **Register adapter**:
   ```bash
   aosctl adapters register --path adapters/adapter_name/
   ```

## Best Practices

- **Versioning**: Bump version in manifest when updating datasets
- **Provenance**: Always include MasterPlan section references
- **Evaluation**: Define clear evaluation gates for each dataset
- **Security**: Use ACLs for tenant-specific datasets
- **TTL**: Set appropriate TTL for ephemeral datasets
- **Documentation**: Include README.md for complex datasets

## Integration with .aos Files

Datasets can be packaged into .aos files:

```bash
# Train and package as .aos
cargo xtask train-base-adapter \
  --manifest training/datasets/framework/rust/manifest.json \
  --output-format aos \
  --output-dir adapters/

# Migrate existing dataset to .aos
aosctl migrate adapter \
  --source adapters/rust_framework_v1/ \
  --output adapters/rust_framework_v1.aos
```

This creates portable, self-contained adapter files with embedded training data.
