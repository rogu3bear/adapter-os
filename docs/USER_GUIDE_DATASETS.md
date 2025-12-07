# AdapterOS Dataset User Guide

Complete guide for creating, uploading, managing, and using datasets for training custom adapters.

---

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [File Formats](#file-formats)
4. [Uploading Files](#uploading-files)
5. [Using Datasets for Training](#using-datasets-for-training)
6. [CLI Usage Examples](#cli-usage-examples)
7. [API Reference](#api-reference)
8. [Dataset Management](#dataset-management)
9. [Troubleshooting](#troubleshooting)
10. [Best Practices](#best-practices)

---

## Overview

AdapterOS datasets enable you to train custom adapters on your own data. A dataset is a collection of structured training examples that define how your adapter should behave.

### Key Concepts

**Dataset**: A collection of training files organized with metadata, validated for correctness.

**Training Example**: A single input-output pair (or more complex structures) that teaches the adapter.

**File**: Individual training data files within a dataset (JSONL, JSON, text).

**Validation**: Automated checking that dataset files are well-formed and match the expected format.

### Dataset Lifecycle

```
Create Dataset → Upload Files → Validate → Store Metadata → Use in Training → Monitor
```

### Storage and Limits

- **Max file size**: 100 MB per file
- **Max total upload**: 500 MB per dataset
- **Storage path**: `var/datasets/{dataset_id}/files/`
- **Retention**: Indefinite (until explicitly deleted)

---

## Quick Start

### 5-Minute Dataset Setup

**Step 1: Prepare Training Data**

Create a simple JSONL file (`training_data.jsonl`):

```jsonl
{"input": "Explain Python", "target": "Python is a programming language known for simplicity and readability."}
{"input": "What is Rust?", "target": "Rust is a systems programming language with memory safety guarantees."}
{"input": "Tell me about Go", "target": "Go is a compiled language designed for concurrent programming and fast execution."}
```

**Step 2: Upload Dataset via API**

```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=language-intro" \
  -F "description=Intro to programming languages" \
  -F "format=jsonl" \
  -F "file=@training_data.jsonl"
```

**Response:**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "name": "language-intro",
  "description": "Intro to programming languages",
  "file_count": 1,
  "total_size_bytes": 245,
  "format": "jsonl",
  "hash": "b3:a1b2c3d4e5f6...",
  "created_at": "2025-01-15T10:30:00Z"
}
```

**Step 3: Validate Dataset**

```bash
curl -X POST http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c/validate
```

**Step 4: Use in Training**

```bash
./target/release/aosctl train \
  --dataset-id 01bx5zzkbk4st8hmq69g82np7c \
  --output adapters/my-custom.aos \
  --rank 16
```

Done! Your dataset is ready.

---

## File Formats

### JSONL Format (Recommended)

JSON Lines format - one complete JSON object per line.

**Best for**: Training examples with structured input-output pairs.

**Example:**

```jsonl
{"input": "What is AI?", "target": "Artificial Intelligence is the field of computer science focused on creating intelligent machines.", "metadata": {"source": "wiki", "category": "AI"}}
{"input": "Explain ML", "target": "Machine Learning is a subset of AI focused on learning from data.", "metadata": {"source": "textbook", "category": "ML"}}
{"input": "What is DL?", "target": "Deep Learning uses neural networks with multiple layers to learn complex patterns.", "metadata": {"source": "paper", "category": "DL"}}
```

**Structure:**

```json
{
  "input": "string - the input prompt or context",
  "target": "string - the expected output or completion",
  "metadata": {
    "source": "optional context about the example",
    "category": "optional category/tag",
    "weight": 1.0,
    "lang": "optional language tag"
  },
  "weight": 1.0
}
```

**Validation Requirements:**
- Each line must be valid JSON
- `input` and `target` fields are required
- Fields are case-sensitive
- Empty lines are ignored
- Maximum 10,000 examples per file recommended

### JSON Format

Array of objects - useful for hierarchical or nested data.

**Example:**

```json
{
  "version": "1.0",
  "examples": [
    {
      "input": "What is Python?",
      "target": "Python is a high-level programming language.",
      "tags": ["programming", "python"]
    },
    {
      "input": "What is JavaScript?",
      "target": "JavaScript is a programming language primarily used in web browsers.",
      "tags": ["programming", "javascript"]
    }
  ]
}
```

**Validation Requirements:**
- Valid JSON array or object
- If array, each element should have `input` and `target`
- If single object with `examples` key, it should contain array

### Plain Text Format

Simple text files for unstructured data.

**Example:**

```
This is the first training example about machine learning concepts.
It should cover neural networks, backpropagation, and gradient descent.

This is the second example about deep learning frameworks.
TensorFlow and PyTorch are popular choices for building neural networks.

This is another example about reinforcement learning applications.
It's used in game playing, robotics, and autonomous systems.
```

**How it's used:**
- Each line or paragraph becomes an example
- Best for simple, unstructured data
- Limited metadata support

### Dataset Manifest

Metadata file describing the entire dataset (optional but recommended).

**Example (`manifest.json`):**

```json
{
  "name": "python_basics",
  "version": "1.0.0",
  "description": "Training dataset for Python programming concepts",
  "category": "programming",
  "scope": "user",
  "tier": "training",
  "rank": 16,
  "alpha": 8.0,
  "target_modules": ["up_proj", "down_proj"],
  "entries": [
    {
      "path": "examples.jsonl",
      "format": "jsonl",
      "weight": 1.0,
      "role": "training",
      "notes": "500 examples covering Python fundamentals"
    }
  ],
  "provenance": {
    "created_by": "user@example.com",
    "created_at": "2025-01-15T10:00:00Z",
    "source": "internal training program"
  },
  "evaluation_gates": [
    "Examples > 100",
    "Average example length > 10 tokens",
    "No invalid JSON lines"
  ]
}
```

---

## Uploading Files

### Method 1: REST API (Recommended)

Upload single or multiple files at once.

**Single File:**

```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-dataset" \
  -F "description=My training data" \
  -F "format=jsonl" \
  -F "file=@training.jsonl"
```

**Multiple Files:**

```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=multi-file-dataset" \
  -F "description=Dataset with multiple files" \
  -F "format=jsonl" \
  -F "file=@training_part1.jsonl" \
  -F "file=@training_part2.jsonl" \
  -F "file=@training_part3.jsonl"
```

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Dataset name (40 chars max) |
| `description` | string | No | Human-readable description |
| `format` | string | Yes | File format: `jsonl`, `json`, `txt` |
| `file` | binary | Yes | One or more files to upload |

**Response:**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "name": "my-dataset",
  "description": "My training data",
  "file_count": 1,
  "total_size_bytes": 2048,
  "format": "jsonl",
  "hash": "b3:a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
  "created_at": "2025-01-15T10:30:00Z"
}
```

### Method 2: CLI Upload

Use the CLI for automated uploads:

```bash
./target/release/aosctl dataset upload \
  --path training.jsonl \
  --name my-dataset \
  --description "My training data" \
  --format jsonl
```

### Method 3: Batch Upload

Upload multiple files in sequence:

```bash
for file in training_*.jsonl; do
  curl -X POST http://localhost:8080/v1/datasets/upload \
    -F "name=batch-$(date +%s)" \
    -F "format=jsonl" \
    -F "file=@$file"
done
```

### Upload Troubleshooting

**Issue: File too large**

```
Error: File exceeds maximum size of 100MB
```

**Solution:** Split your file:

```bash
# Split large file into 50MB chunks
split -b 50m training.jsonl training_part_

# Upload each chunk
for file in training_part_*; do
  echo "Uploading $file..."
  curl -X POST http://localhost:8080/v1/datasets/upload \
    -F "name=split-dataset" \
    -F "format=jsonl" \
    -F "file=@$file"
done
```

**Issue: Total upload exceeds 500MB**

**Solution:** Create separate datasets:

```bash
# Create first dataset (files 1-5)
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=dataset-batch-1" \
  -F "format=jsonl" \
  -F "file=@file1.jsonl" \
  -F "file=@file2.jsonl" \
  -F "file=@file3.jsonl"

# Create second dataset (files 6-10)
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=dataset-batch-2" \
  -F "format=jsonl" \
  -F "file=@file6.jsonl" \
  -F "file=@file7.jsonl"
```

---

## Using Datasets for Training

### Basic Training with Dataset

```bash
./target/release/aosctl train \
  --dataset-id 01bx5zzkbk4st8hmq69g82np7c \
  --output adapters/custom.aos \
  --rank 16 \
  --alpha 8 \
  --epochs 3 \
  --learning-rate 0.0001
```

### Training Configuration

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `--dataset-id` | string | - | Dataset ID to use for training |
| `--rank` | integer | 16 | LoRA rank (8, 16, 32, 64) |
| `--alpha` | integer | 8 | LoRA alpha scaling factor |
| `--epochs` | integer | 3 | Number of training epochs |
| `--learning-rate` | float | 0.0001 | Training learning rate |
| `--batch-size` | integer | 4 | Batch size per step |
| `--warmup-steps` | integer | 100 | Number of warmup steps |
| `--max-seq-length` | integer | 512 | Maximum sequence length |
| `--output` | path | - | Output .aos file path |

### Example: Detailed Training

```bash
./target/release/aosctl train \
  --dataset-id 01bx5zzkbk4st8hmq69g82np7c \
  --output var/adapters/code-assistant.aos \
  --rank 32 \
  --alpha 16 \
  --epochs 5 \
  --learning-rate 0.0005 \
  --batch-size 8 \
  --warmup-steps 500 \
  --max-seq-length 1024
```

### Training from Code

You can also train from source code instead of a prepared dataset:

```bash
./target/release/aosctl train \
  --input src/ \
  --output adapters/codebase.aos \
  --rank 24
```

This automatically:
1. Analyzes code files
2. Generates training examples from patches
3. Creates a temporary dataset
4. Trains an adapter

### Monitoring Training

Check training status:

```bash
./target/release/aosctl training status {job_id}
```

**Output:**

```
Training Job: 01bx5zzkbk4st8hmq69g82np7c
Status: in_progress
Progress: 65%
Current Epoch: 2/3
Loss: 0.245
Learning Rate: 0.00008
Tokens/second: 1250
ETA: 4m 30s
```

Cancel training:

```bash
./target/release/aosctl training cancel {job_id}
```

---

## CLI Usage Examples

### List All Datasets

```bash
./target/release/aosctl dataset list
```

**Output:**

```
Datasets:
1. language-intro (01bx5zzkbk4st8hmq69g82np7c)
   Files: 1, Size: 245 B, Format: jsonl, Status: valid
   Created: 2025-01-15 10:30:00

2. python-basics (01bx5zzkbk4st8hmq69g82np7d)
   Files: 3, Size: 2.3 MB, Format: jsonl, Status: valid
   Created: 2025-01-15 11:45:00
```

### Get Dataset Details

```bash
./target/release/aosctl dataset info {dataset_id}
```

**Output:**

```
Dataset: language-intro
ID: 01bx5zzkbk4st8hmq69g82np7c
Description: Intro to programming languages
Format: jsonl
Files: 1
  - training_data.jsonl (245 B) [hash: b3:a1b2...]
Total Size: 245 B
Validation Status: valid
Hash: b3:a1b2c3d4e5f6...
Created: 2025-01-15 10:30:00
Updated: 2025-01-15 10:30:00
```

### List Dataset Files

```bash
./target/release/aosctl dataset files {dataset_id}
```

**Output:**

```
Files in dataset (01bx5zzkbk4st8hmq69g82np7c):

1. training_data.jsonl
   Size: 245 B
   Hash: b3:a1b2c3d4e5...
   Created: 2025-01-15 10:30:00
```

### Preview Dataset Content

```bash
./target/release/aosctl dataset preview {dataset_id} --limit 5
```

**Output:**

```
Dataset Preview: language-intro
Format: jsonl
Showing 3 of 3 examples:

Example 1:
  Input: "Explain Python"
  Target: "Python is a programming language..."

Example 2:
  Input: "What is Rust?"
  Target: "Rust is a systems programming language..."

Example 3:
  Input: "Tell me about Go"
  Target: "Go is a compiled language..."
```

### Validate Dataset

```bash
./target/release/aosctl dataset validate {dataset_id}
```

**Output:**

```
Validation Results for: language-intro

Status: VALID ✓
Checked: 3 examples
Errors: 0

Format Check: PASSED
  - All lines are valid JSON
  - All required fields present
  - No malformed records

Hash Verification: PASSED
  - training_data.jsonl: b3:a1b2... ✓
```

### Delete Dataset

```bash
./target/release/aosctl dataset delete {dataset_id}
```

**Confirmation:**

```
Are you sure? This will permanently delete:
  - 1 file(s)
  - 245 B of data

Type 'yes' to confirm: yes
Dataset deleted successfully.
```

### Get Dataset Statistics

```bash
./target/release/aosctl dataset stats {dataset_id}
```

**Output:**

```
Dataset Statistics: language-intro

Examples: 3
Average Input Length: 18 tokens
Average Target Length: 45 tokens
Total Tokens: 189

Language Distribution:
  English: 100%

File Type Distribution:
  jsonl: 100%

Status: Ready for training
```

---

## API Reference

### Upload Dataset

**Endpoint:** `POST /v1/datasets/upload`

**Content-Type:** `multipart/form-data`

**Parameters:**

```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=dataset-name" \
  -F "description=Optional description" \
  -F "format=jsonl" \
  -F "file=@file1.jsonl" \
  -F "file=@file2.jsonl"
```

**Response (200 OK):**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "name": "dataset-name",
  "description": "Optional description",
  "file_count": 2,
  "total_size_bytes": 4096,
  "format": "jsonl",
  "hash": "b3:a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
  "created_at": "2025-01-15T10:30:00Z"
}
```

### List Datasets

**Endpoint:** `GET /v1/datasets`

**Query Parameters:**

```bash
curl "http://localhost:8080/v1/datasets?limit=10&offset=0&format=jsonl&validation_status=valid"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 50 | Number of datasets to return (1-100) |
| `offset` | integer | 0 | Number of datasets to skip |
| `format` | string | - | Filter by format (jsonl, json, txt) |
| `validation_status` | string | - | Filter by status (valid, invalid, pending) |

**Response (200 OK):**

```json
[
  {
    "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
    "name": "language-intro",
    "description": "Intro to programming languages",
    "file_count": 1,
    "total_size_bytes": 245,
    "format": "jsonl",
    "hash": "b3:a1b2c3d4e5f6...",
    "validation_status": "valid",
    "validation_errors": null,
    "created_by": "user",
    "created_at": "2025-01-15T10:30:00Z",
    "updated_at": "2025-01-15T10:30:00Z"
  }
]
```

### Get Dataset Details

**Endpoint:** `GET /v1/datasets/{dataset_id}`

```bash
curl http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c
```

**Response (200 OK):**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "name": "language-intro",
  "description": "Intro to programming languages",
  "file_count": 1,
  "total_size_bytes": 245,
  "format": "jsonl",
  "hash": "b3:a1b2c3d4e5f6...",
  "storage_path": "var/datasets/01bx5zzkbk4st8hmq69g82np7c",
  "validation_status": "valid",
  "validation_errors": null,
  "created_by": "system",
  "created_at": "2025-01-15T10:30:00Z",
  "updated_at": "2025-01-15T10:30:00Z"
}
```

### Get Dataset Files

**Endpoint:** `GET /v1/datasets/{dataset_id}/files`

```bash
curl http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c/files
```

**Response (200 OK):**

```json
[
  {
    "file_id": "01bx5zzkbk4st8hmq69g82np7e",
    "file_name": "training_data.jsonl",
    "file_path": "var/datasets/01bx5zzkbk4st8hmq69g82np7c/files/training_data.jsonl",
    "size_bytes": 245,
    "hash": "b3:a1b2c3d4e5...",
    "mime_type": "application/jsonl",
    "created_at": "2025-01-15T10:30:00Z"
  }
]
```

### Validate Dataset

**Endpoint:** `POST /v1/datasets/{dataset_id}/validate`

```bash
curl -X POST http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c/validate \
  -H "Content-Type: application/json" \
  -d '{"check_format": true}'
```

**Response (200 OK):**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "is_valid": true,
  "validation_status": "valid",
  "errors": null,
  "validated_at": "2025-01-15T10:35:00Z"
}
```

### Get Dataset Preview

**Endpoint:** `GET /v1/datasets/{dataset_id}/preview`

```bash
curl "http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c/preview?limit=10"
```

**Response (200 OK):**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "format": "jsonl",
  "total_examples": 3,
  "examples": [
    {
      "input": "Explain Python",
      "target": "Python is a programming language known for simplicity and readability."
    },
    {
      "input": "What is Rust?",
      "target": "Rust is a systems programming language with memory safety guarantees."
    },
    {
      "input": "Tell me about Go",
      "target": "Go is a compiled language designed for concurrent programming and fast execution."
    }
  ]
}
```

### Get Dataset Statistics

**Endpoint:** `GET /v1/datasets/{dataset_id}/statistics`

```bash
curl http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c/statistics
```

**Response (200 OK):**

```json
{
  "dataset_id": "01bx5zzkbk4st8hmq69g82np7c",
  "num_examples": 3,
  "avg_input_length": 18.33,
  "avg_target_length": 45.67,
  "language_distribution": "{\"en\": 100}",
  "file_type_distribution": "{\"jsonl\": 100}",
  "total_tokens": 189,
  "computed_at": "2025-01-15T10:35:00Z"
}
```

### Delete Dataset

**Endpoint:** `DELETE /v1/datasets/{dataset_id}`

```bash
curl -X DELETE http://localhost:8080/v1/datasets/01bx5zzkbk4st8hmq69g82np7c
```

**Response (204 No Content)**

```
(empty response)
```

---

## Dataset Management

### Organizing Datasets

**By Domain:**

```
var/datasets/
├── nlp/
│   ├── sentiment-analysis/
│   ├── question-answering/
│   └── text-generation/
├── code/
│   ├── python-examples/
│   ├── rust-patterns/
│   └── web-development/
└── domain-specific/
    ├── medical/
    └── legal/
```

**Via Naming Convention:**

```
dataset-name = {domain}-{purpose}-{version}

Examples:
- nlp-sentiment-v1
- code-python-basics-v2
- medical-note-generation-v1
```

### Versioning Datasets

Track dataset improvements:

```bash
# Upload version 1
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-dataset-v1" \
  -F "file=@training-v1.jsonl"

# After reviewing and improving
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-dataset-v2" \
  -F "file=@training-v2.jsonl"

# Later versions
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-dataset-v3" \
  -F "file=@training-v3.jsonl"
```

### Comparing Datasets

Check differences between versions:

```bash
# Export both datasets
./target/release/aosctl dataset export v1_id > dataset-v1.jsonl
./target/release/aosctl dataset export v2_id > dataset-v2.jsonl

# Compare
diff dataset-v1.jsonl dataset-v2.jsonl | head -50
```

### Dataset Metadata

Store custom metadata:

```bash
# Create manifest file
cat > dataset_metadata.json << 'EOF'
{
  "name": "my-dataset",
  "version": "1.0",
  "source": "internal-team",
  "quality_score": 95,
  "examples_count": 500,
  "validation_date": "2025-01-15",
  "notes": "High-quality examples with expert validation"
}
EOF

# Upload with manifest
# Include alongside training files
```

### Backup and Export

**Export dataset to file:**

```bash
./target/release/aosctl dataset export {dataset_id} > backup.jsonl
```

**Backup all datasets:**

```bash
mkdir -p dataset_backups
for dataset in $(./target/release/aosctl dataset list | grep -oP '(?<=\().*(?=\))'); do
  ./target/release/aosctl dataset export "$dataset" > "dataset_backups/$dataset.jsonl"
done
```

**Restore dataset:**

```bash
./target/release/aosctl dataset upload \
  --path dataset_backups/{dataset_id}.jsonl \
  --name restored-dataset \
  --format jsonl
```

---

## Troubleshooting

### Upload Issues

**Problem: "File not found" error**

```
Error: Failed to read file: No such file or directory
```

**Solution:**

```bash
# Check file exists and is readable
ls -lh training.jsonl

# Use absolute path
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "file=@$(pwd)/training.jsonl"
```

**Problem: "Invalid JSON" error on JSONL file**

```
Validation error: Line 5 is not valid JSON
```

**Solution: Validate JSONL syntax**

```bash
# Check problematic lines
python3 << 'EOF'
with open('training.jsonl', 'r') as f:
    for i, line in enumerate(f, 1):
        try:
            json.loads(line)
        except json.JSONDecodeError as e:
            print(f"Line {i}: {e}")
EOF

# Fix with jq
jq -c . broken_file.json > fixed_file.jsonl
```

**Problem: Timeout during upload**

```
Error: Request timed out after 30 seconds
```

**Solution:**

```bash
# Increase timeout and split file
timeout 120 curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "file=@small_file.jsonl"

# Or split into smaller parts
split -l 5000 training.jsonl training_part_
```

### Validation Issues

**Problem: Dataset validation hangs**

```bash
# Check dataset status
curl http://localhost:8080/v1/datasets/{id} | jq .validation_status

# Manually validate
./target/release/aosctl dataset validate {dataset_id} --timeout 60
```

**Problem: Hash mismatch after upload**

```
Error: File hash mismatch. Expected: ..., Got: ...
```

**Solution:**

```bash
# Re-upload the file
rm -rf var/datasets/{dataset_id}

./target/release/aosctl dataset upload \
  --path training.jsonl \
  --name dataset-name
```

### Training with Dataset Issues

**Problem: "Dataset not found" error**

```bash
# Verify dataset exists
curl http://localhost:8080/v1/datasets/{dataset_id}

# Check dataset ID is correct
./target/release/aosctl dataset list | grep {id_part}
```

**Problem: Training fails with invalid examples**

```
Error: Not enough training examples: 0 < 10
```

**Solution:**

```bash
# Check dataset has examples
./target/release/aosctl dataset preview {dataset_id}

# Validate dataset
./target/release/aosctl dataset validate {dataset_id}

# Check file format matches
file training.jsonl
```

**Problem: Out of memory during training**

```bash
# Reduce batch size
./target/release/aosctl train \
  --dataset-id {id} \
  --batch-size 2 \
  --max-seq-length 256

# Or use fewer epochs
./target/release/aosctl train \
  --dataset-id {id} \
  --epochs 1
```

### Dataset Size Issues

**Problem: Dataset storage is growing large**

```bash
# Check disk usage
du -sh var/datasets/

# List largest datasets
du -sh var/datasets/* | sort -rh | head -10
```

**Solution:**

```bash
# Delete unused datasets
./target/release/aosctl dataset delete {old_dataset_id}

# Or clean up old versions
for id in $(./target/release/aosctl dataset list | grep v1 | cut -d' ' -f2); do
  ./target/release/aosctl dataset delete "$id"
done
```

---

## Best Practices

### 1. Data Quality

**Ensure high-quality examples:**

```jsonl
# Good: Clear, complete examples
{"input": "What is photosynthesis?", "target": "Photosynthesis is the process by which plants convert light energy into chemical energy stored in glucose."}

# Bad: Ambiguous or incomplete
{"input": "photosynthesis", "target": "process"}

# Bad: Malformed JSON
{"input": "Example", "target": "Response}
```

**Validate before uploading:**

```bash
# Remove empty lines
grep -v '^$' training.jsonl > cleaned.jsonl

# Verify JSON validity
jq -c . cleaned.jsonl > validated.jsonl
```

### 2. Example Diversity

**Include varied examples:**

```jsonl
# Simple cases
{"input": "What is 2+2?", "target": "The answer is 4."}

# Complex cases
{"input": "Explain quantum entanglement in the context of quantum computing applications.", "target": "..."}

# Edge cases
{"input": "What happens if...?", "target": "..."}

# Different languages (if applicable)
{"input": "Qu'est-ce que...?", "target": "C'est...", "metadata": {"language": "fr"}}
```

### 3. Consistent Formatting

**Maintain consistent structure:**

```jsonl
# All examples should follow same pattern
{"input": "Q1", "target": "A1", "metadata": {"category": "cat1"}}
{"input": "Q2", "target": "A2", "metadata": {"category": "cat2"}}
{"input": "Q3", "target": "A3", "metadata": {"category": "cat1"}}

# Not mixed formats
{"input": "Q1", "answer": "A1"}
{"question": "Q2", "response": "A2"}
```

### 4. Naming Conventions

**Use descriptive dataset names:**

```bash
# Good names
- nlp-sentiment-analysis-v2
- code-python-patterns-v1
- qa-medical-domain-v3

# Bad names
- dataset1
- training_data
- new_data_20250115
```

### 5. Documentation

**Document your datasets:**

```bash
# Create README for each dataset
cat > dataset-info.txt << 'EOF'
Dataset: nlp-sentiment-analysis-v2

Purpose:
  Train sentiment analysis adapter for product reviews

Statistics:
  - 5000 examples
  - 100 KB compressed
  - Languages: English
  - Format: JSONL

Quality:
  - Validation: PASSED
  - Examples manually reviewed: 200/5000
  - Estimated accuracy baseline: 92%

Source:
  - 40% Product reviews (Amazon)
  - 30% Customer feedback (internal)
  - 30% Synthetic balanced examples

Usage:
  ./target/release/aosctl train --dataset-id {id} --rank 24
EOF
```

### 6. Version Control

**Track dataset changes:**

```bash
# Keep manifest file
cat > datasets/manifest.txt << 'EOF'
v1: Initial dataset (1000 examples)
v2: Added 2000 examples, improved balance
v3: Fixed 50 examples with incorrect labels
v4: Added multilingual examples (Spanish, French)
EOF

# Use semantic versioning
# MAJOR.MINOR.PATCH
# 1.0.0 - Initial release
# 1.1.0 - Added more examples
# 1.1.1 - Fixed typos
```

### 7. Size Optimization

**Optimize file sizes:**

```bash
# Remove unnecessary fields
jq -c 'del(.unused_field)' large_dataset.json > optimized.jsonl

# Compress metadata
jq -c 'del(.metadata.comments)' dataset.jsonl > trimmed.jsonl

# Split large files
split -l 5000 huge_dataset.jsonl dataset_

# Combine small files
cat small_*.jsonl > combined.jsonl
```

### 8. Pre-training Checklist

Before training with a dataset:

- [ ] Dataset validation passes
- [ ] Minimum 100 examples uploaded
- [ ] Example format consistent
- [ ] Input/target fields complete
- [ ] JSON is valid (no syntax errors)
- [ ] File size within limits (< 500 MB)
- [ ] Descriptive name assigned
- [ ] Documentation created
- [ ] Backup copy saved locally
- [ ] Statistics reviewed

### 9. Performance Optimization

**For better training results:**

```jsonl
# Include diverse examples
{"input": "Simple question", "target": "Simple answer"}
{"input": "Complex question with context", "target": "Detailed answer"}

# Use metadata for tracking
{"input": "...", "target": "...", "metadata": {"difficulty": "hard", "verified": true}}

# Add weights for important examples
{"input": "...", "target": "...", "weight": 2.0}
```

### 10. Integration with Training Pipeline

**Workflow example:**

```bash
#!/bin/bash

# 1. Create dataset
DATASET_ID=$(curl -s -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=my-dataset" \
  -F "format=jsonl" \
  -F "file=@training.jsonl" \
  | jq -r .dataset_id)

echo "Created dataset: $DATASET_ID"

# 2. Validate
curl -s -X POST http://localhost:8080/v1/datasets/$DATASET_ID/validate | jq .

# 3. Get statistics
curl -s http://localhost:8080/v1/datasets/$DATASET_ID/statistics | jq .

# 4. Train adapter
./target/release/aosctl train \
  --dataset-id $DATASET_ID \
  --output adapters/custom.aos \
  --rank 24 \
  --epochs 3

# 5. Test inference
./target/release/aosctl infer \
  --adapter custom \
  --prompt "Test prompt"
```

---

## Summary

Datasets are the foundation of custom adapters in AdapterOS. Follow these key steps:

1. **Prepare** well-structured training data in JSONL format
2. **Upload** using the REST API or CLI
3. **Validate** to ensure quality and format
4. **Use** for training custom adapters
5. **Monitor** training progress and results
6. **Manage** versions and maintain documentation

For more information:
- See `CLAUDE.md` for development guidelines
- Check `docs/TRAINING.md` for advanced training techniques
- Review `docs/ARCHITECTURE_INDEX.md` for system architecture

**Happy dataset creation!**
