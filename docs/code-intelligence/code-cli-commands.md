# Code Intelligence CLI Commands

## Overview

New `aosctl` subcommands for code intelligence workflows. All commands follow existing AdapterOS patterns: tenant-scoped, with audit logging.

---

## Repository Management

### `aosctl code-init`

Register and scan a repository.

**Usage**:
```bash
aosctl code-init \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --path <REPO_PATH> \
  --commit <COMMIT_SHA> \
  --languages <LANG1>,<LANG2>
```

**Flags**:
- `--tenant` (required): Tenant ID
- `--repo` (required): Repository ID (e.g., `acme/payments`)
- `--path` (required): Local repository path
- `--commit` (optional): Commit SHA (defaults to HEAD)
- `--languages` (optional): Comma-separated list (auto-detected if omitted)
- `--skip-tests`: Don't build test map
- `--output`: Output directory for artifacts

**Example**:
```bash
aosctl code-init \
  --tenant tenant_acme \
  --repo acme/payments \
  --path /repos/acme/payments \
  --languages python,typescript
```

**Output**:
```
Registering repository acme/payments...
Scanning repository at commit abc123def456...
  [1/6] Parsing files... (142 files, 15234 LOC)
  [2/6] Building CodeGraph... (1834 symbols, 287 tests)
  [3/6] Detecting frameworks... (django 4.2, pytest 7.4)
  [4/6] Building symbol index... (1834 symbols)
  [5/6] Building vector index... (856 chunks)
  [6/6] Mapping tests... (287 tests, 1834 symbols covered)

CodeGraph stored: b3:abc123...
Symbol index stored: b3:def456...
Vector index stored: b3:ghi789...
Test map stored: b3:jkl012...

Repository registered successfully.
Job ID: scan_job_12345
```

---

### `aosctl code-update`

Update indices for a new commit (incremental).

**Usage**:
```bash
aosctl code-update \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --commit <NEW_COMMIT> \
  --parent <PARENT_COMMIT>
```

**Example**:
```bash
aosctl code-update \
  --tenant tenant_acme \
  --repo acme/payments \
  --commit def456ghi789 \
  --parent abc123def456
```

---

### `aosctl code-list`

List registered repositories.

**Usage**:
```bash
aosctl code-list --tenant <TENANT_ID>
```

**Output**:
```
Repositories for tenant_acme:

  acme/payments
    Path: /repos/acme/payments
    Languages: Python, TypeScript
    Frameworks: Django 4.2, pytest 7.4
    Latest scan: abc123def456 (2025-10-05 10:32:15)

  acme/frontend
    Path: /repos/acme/frontend
    Languages: TypeScript
    Frameworks: React 18, Next.js 14
    Latest scan: def456ghi789 (2025-10-04 15:20:00)

Total: 2 repositories
```

---

## Adapter Training

### `aosctl adapter-train`

Train a codebase adapter.

**Usage**:
```bash
aosctl adapter-train \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --commit <COMMIT_SHA> \
  --rank <RANK> \
  --output <OUTPUT_BUNDLE>
```

**Flags**:
- `--tenant` (required): Tenant ID
- `--repo` (required): Repository ID
- `--commit` (optional): Commit SHA (defaults to HEAD)
- `--rank` (optional): LoRA rank (default: 24)
- `--alpha` (optional): LoRA alpha (default: rank * 2)
- `--targets` (optional): Target modules (default: all 7)
- `--output` (required): Output bundle path
- `--dry-run`: Estimate training time without training

**Example**:
```bash
aosctl adapter-train \
  --tenant tenant_acme \
  --repo acme/payments \
  --rank 24 \
  --output adapters/codebase_acme_payments_v7.tar.zst
```

**Output**:
```
Training codebase adapter for acme/payments...
  Loading CodeGraph: b3:abc123...
  Generating training pairs... (1247 pairs from docs, patterns, PRs)
  Training LoRA (rank 24, alpha 48)...
    Epoch 1/3: loss 0.125
    Epoch 2/3: loss 0.047
    Epoch 3/3: loss 0.023
  Validating adapter...
  Packaging bundle...

Adapter trained successfully.
Bundle: adapters/codebase_acme_payments_v7.tar.zst
Hash: b3:xyz789...
Size: 112 MB
```

---

## Ephemeral Adapters

### `aosctl commit-ephemeral`

Create an ephemeral adapter for a commit.

**Usage**:
```bash
aosctl commit-ephemeral \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --commit <COMMIT_SHA> \
  --mode <MODE> \
  --ttl <HOURS>
```

**Flags**:
- `--tenant` (required): Tenant ID
- `--repo` (required): Repository ID
- `--commit` (required): Commit SHA
- `--mode` (optional): `zero_train` or `micro_lora` (default: `zero_train`)
- `--rank` (optional): LoRA rank for micro_lora (default: 4)
- `--ttl` (optional): TTL in hours (default: 72)
- `--create-cdp`: Create commit delta pack first

**Example**:
```bash
aosctl commit-ephemeral \
  --tenant tenant_acme \
  --repo acme/payments \
  --commit abc123def456 \
  --mode micro_lora \
  --ttl 72 \
  --create-cdp
```

**Output**:
```
Creating commit delta pack...
  git diff abc123def456 def456ghi789...
  Changed files: 3
  Changed symbols: 5
  Running tests... (287 passed, 2 failed)
  Running linter... (1 error, 3 warnings)
  CDP created: cdp_abc123

Training ephemeral adapter (micro_lora, rank 4)...
  Generating 42 training pairs from CDP...
  Training... (loss: 0.023)
  Packaging...

Ephemeral adapter created: commit_abc123def456
Hash: b3:uvw345...
TTL: 72h
Expires: 2025-10-08 11:05:00

Adapter will be auto-attached to worker.
```

---

### `aosctl hot-reload-ephemeral`

Attach ephemeral adapter to active worker.

**Usage**:
```bash
aosctl hot-reload-ephemeral \
  --tenant <TENANT_ID> \
  --adapter <ADAPTER_ID> \
  --cp-pointer <CP_NAME>
```

**Example**:
```bash
aosctl hot-reload-ephemeral \
  --tenant tenant_acme \
  --adapter commit_abc123def456 \
  --cp-pointer code
```

---

## Patch Operations

### `aosctl patch-propose`

Propose a code patch.

**Usage**:
```bash
aosctl patch-propose \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --commit <COMMIT_SHA> \
  --prompt "<PROMPT>" \
  --context <FILE1>,<FILE2>
```

**Flags**:
- `--tenant` (required): Tenant ID
- `--repo` (required): Repository ID
- `--commit` (required): Commit SHA
- `--prompt` (required): Natural language prompt
- `--context` (optional): Comma-separated context files
- `--targets` (optional): Target symbols (JSON file)
- `--output` (optional): Output file for patch (default: stdout)
- `--dry-run`: Simulate without actual generation

**Example**:
```bash
aosctl patch-propose \
  --tenant tenant_acme \
  --repo acme/payments \
  --commit abc123def456 \
  --prompt "Fix the failing test test_process_payment_timeout" \
  --context src/payments/processor.py,tests/test_processor.py \
  --output patch_abc123.json
```

**Output** (JSON):
```json
{
  "patch_set_id": "patch_abc123",
  "status": "proposed",
  "patches": [...],
  "rationale": "...",
  "citations": [...],
  "trace": {...}
}
```

---

### `aosctl patch-apply`

Apply a proposed patch.

**Usage**:
```bash
aosctl patch-apply \
  --tenant <TENANT_ID> \
  --patch-set <PATCH_SET_ID> \
  --dry-run
```

**Flags**:
- `--tenant` (required): Tenant ID
- `--patch-set` (required): Patch set ID
- `--dry-run` (optional): Test without applying
- `--force`: Skip confirmation prompt
- `--run-tests`: Run tests after applying
- `--run-linter`: Run linter after applying

**Example**:
```bash
aosctl patch-apply \
  --tenant tenant_acme \
  --patch-set patch_abc123 \
  --dry-run \
  --run-tests
```

**Output**:
```
Applying patch set patch_abc123 (dry run)...
  Applying to temporary worktree...
  Files modified: 1
    • src/payments/processor.py

  Running tests... (287 passed, 0 failed)
  Running linter... (0 errors, 3 warnings)

  Policy checks:
    ✓ Path allowed
    ✓ No secrets detected
    ✓ No forbidden operations
    ✓ Size within limit

Recommendation: SAFE TO APPLY

Use --no-dry-run to apply patch to repository.
```

---

## Code Audit

### `aosctl code-audit`

Run code intelligence evaluation suite.

**Usage**:
```bash
aosctl code-audit \
  --corpus <CORPUS_FILE> \
  --cpid <CPID> \
  --output-dir <DIR>
```

**Flags**:
- `--corpus` (required): Path to evaluation corpus JSON
- `--cpid` (required): Control plane ID to audit
- `--output-dir` (optional): Output directory (default: `out/audit_<cpid>`)
- `--parallel` (optional): Number of parallel evaluations (default: 4)
- `--verbose`: Show detailed output per task

**Example**:
```bash
aosctl code-audit \
  --corpus tests/corpora/code_eval_v1.json \
  --cpid cp_code_v7_abc123 \
  --output-dir out/audit_code_v7
```

**Output**:
```
Running code audit for CP: cp_code_v7_abc123
Corpus: tests/corpora/code_eval_v1.json (124 tasks)

Evaluating... [========================] 124/124 (100%)

Functional Metrics:
  Compile Success Rate:    0.97 ✓ (≥ 0.95)
  Test Pass@1:             0.83 ✓ (≥ 0.80)
  Test Pass@5:             0.92 ✓ (≥ 0.90)
  Static Analyzer Delta:   0    ✓ (≤ 0)

Groundedness Metrics:
  Attribution Recall Rate: 0.96 ✓ (≥ 0.95)
  Evidence Coverage@5:     0.78 ✓ (≥ 0.75)

Safety Metrics:
  Secret Violations:       0    ✓ (= 0)
  Forbidden Operations:    0    ✓ (= 0)

Routing Metrics:
  Framework Max Activation: 0.72 ✓ (< 0.80)
  Router Overhead:         6.2% ✓ (≤ 8%)

Performance Metrics:
  Latency p95:             1847ms ✓ (< 2000ms)

Determinism:
  Replay on node2:         PASS ✓ (zero diff)

✅ All gates passed. Safe to promote.

Results saved to: out/audit_code_v7/results.json
Trace bundles saved to: out/audit_code_v7/traces/
```

---

## Utilities

### `aosctl code-search`

Search code symbols or semantically.

**Usage**:
```bash
aosctl code-search \
  --tenant <TENANT_ID> \
  --repo <REPO_ID> \
  --query "<QUERY>" \
  --mode <MODE>
```

**Modes**:
- `symbol`: Symbol name search (FTS5)
- `semantic`: Vector search

**Example (symbol)**:
```bash
aosctl code-search \
  --tenant tenant_acme \
  --repo acme/payments \
  --query "process_payment" \
  --mode symbol
```

**Output**:
```
Found 3 symbols:

1. process_payment (Function)
   File: src/payments/processor.py:58-112
   Signature: def process_payment(amount: Decimal, currency: str) -> PaymentResult
   Score: 0.95

2. ProcessPaymentView (Class)
   File: src/payments/views.py:23-67
   Score: 0.72

3. test_process_payment_success (Function)
   File: tests/test_processor.py:15-28
   Score: 0.68
```

**Example (semantic)**:
```bash
aosctl code-search \
  --tenant tenant_acme \
  --repo acme/payments \
  --query "How do we handle payment timeouts?" \
  --mode semantic \
  --k 5
```

**Output**:
```
Top 5 relevant code chunks:

1. src/payments/processor.py:58-68 (score: 0.87)
   def process_payment(...):
       ...
       if elapsed > TIMEOUT_SECONDS:
           raise PaymentTimeoutError(...)
       ...

2. src/payments/models.py:45-55 (score: 0.72)
   class Payment(models.Model):
       ...
       timeout_seconds = models.IntegerField(default=30)
       ...

...
```

---

### `aosctl code-stats`

Show repository statistics.

**Usage**:
```bash
aosctl code-stats --tenant <TENANT_ID> --repo <REPO_ID>
```

**Output**:
```
Repository: acme/payments
Commit: abc123def456

Files:          142
Lines of Code:  15234
Languages:      Python (87%), TypeScript (13%)
Frameworks:     Django 4.2, pytest 7.4

Symbols:        1834
  Functions:    1247
  Classes:      412
  Methods:      175

Tests:          287
  Unit:         245
  Integration:  42

Coverage:       
  Symbol:       92.3% (1692/1834 covered by tests)
  File:         88.7% (126/142 have tests)
```

---

## Batch Operations

### Example: Scan multiple repos

```bash
for repo in acme/payments acme/frontend acme/api; do
  aosctl code-init \
    --tenant tenant_acme \
    --repo $repo \
    --path /repos/$repo
done
```

### Example: Create ephemerals for PR commits

```bash
git log --format=%H origin/main..HEAD | while read commit; do
  aosctl commit-ephemeral \
    --tenant tenant_acme \
    --repo acme/payments \
    --commit $commit \
    --mode zero_train \
    --ttl 24
done
```

---

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: Invalid arguments
- `3`: Authentication/authorization failure
- `4`: Policy violation
- `5`: Job failed (scan, train, etc.)
- `6`: Gate failed (audit)
