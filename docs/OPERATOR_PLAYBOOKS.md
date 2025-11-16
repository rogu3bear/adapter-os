# AdapterOS Operator Playbooks

**Purpose:** Step-by-step operational procedures with concrete commands
**Audience:** System operators, DevOps engineers, SREs
**Last Updated:** 2025-01-16

---

## Table of Contents

- [Playbook 1: Onboarding a New Tenant](#playbook-1-onboarding-a-new-tenant)
- [Playbook 2: Rolling Back a Bad Adapter](#playbook-2-rolling-back-a-bad-adapter)
- [Playbook 3: Hot-Swapping Adapters in Production](#playbook-3-hot-swapping-adapters-in-production)
- [Playbook 4: Creating Training Datasets from Documents](#playbook-4-creating-training-datasets-from-documents)
- [Playbook 5: Training and Deploying a New Adapter](#playbook-5-training-and-deploying-a-new-adapter)
- [Playbook 6: Verifying Determinism Across Cluster](#playbook-6-verifying-determinism-across-cluster)
- [Playbook 7: Responding to Memory Pressure](#playbook-7-responding-to-memory-pressure)
- [Playbook 8: Drift Detection and Baseline Management](#playbook-8-drift-detection-and-baseline-management)
- [Playbook 9: Incident Response - Adapter Failures](#playbook-9-incident-response---adapter-failures)
- [Playbook 10: Telemetry Audit Trail Verification](#playbook-10-telemetry-audit-trail-verification)

---

## Playbook 1: Onboarding a New Tenant

**Objective:** Create a new tenant with proper isolation and initial adapter configuration

**When to use:**
- New customer/team onboarding
- Creating isolated development environments
- Setting up staging/production tenants

**Prerequisites:**
- AdapterOS installed and running
- Admin access to `aosctl` CLI
- Database initialized

### Steps

#### 1.1 Create the Tenant

```bash
# Create tenant with unique UID/GID for isolation
aosctl init-tenant \
  --id tenant_acme \
  --uid 5000 \
  --gid 5000
```

**Expected output:**
```
✓ Tenant 'tenant_acme' initialized
  UID: 5000
  GID: 5000
  Registry path: /var/aos/tenants/tenant_acme/registry.db
```

**Verify:**
```bash
# Check tenant exists in database
sqlite3 var/aos.db "SELECT * FROM tenants WHERE tenant_id = 'tenant_acme';"
```

#### 1.2 Configure Tenant Quota (API)

⚠️ **WARNING:** Tenant quota API not yet implemented. Skip this step for now.

```bash
# FUTURE: Set memory quota for tenant (requires server API)
# This endpoint does not exist yet - see PLAYBOOK_VERIFICATION.md
# curl -X POST http://localhost:8080/v1/tenants/tenant_acme/quota \
#   -H "Content-Type: application/json" \
#   -d '{
#     "max_vram_mb": 8192,
#     "max_adapters": 10,
#     "max_resident_adapters": 3
#   }'

# Workaround: Quotas managed via configuration file for now
```

#### 1.3 Register Initial Adapters

```bash
# Register production adapters for tenant
aosctl register-adapter \
  code_review \
  b3:a1b2c3d4e5... \
  --tier persistent \
  --rank 16

aosctl register-adapter \
  security_audit \
  b3:f6g7h8i9j0... \
  --tier persistent \
  --rank 12
```

#### 1.4 Create Adapter Stack

```bash
# Create reusable adapter stack via API
curl -X POST http://localhost:8080/v1/adapter-stacks \
  -H "Content-Type: application/json" \
  -d '{
    "name": "acme-code-review",
    "description": "Code review and security audit stack",
    "adapter_ids": ["code_review", "security_audit"],
    "workflow_type": "UpstreamDownstream"
  }'

# Response: {"id": "stack-abc123", "name": "acme-code-review", ...}
```

#### 1.5 Pin Critical Adapters

```bash
# Pin adapters to prevent eviction
aosctl pin-adapter \
  --tenant tenant_acme \
  --adapter code_review \
  --reason "Production critical - always resident"

aosctl pin-adapter \
  --tenant tenant_acme \
  --adapter security_audit \
  --reason "Production critical - always resident"
```

#### 1.6 Verify Configuration

```bash
# List tenant adapters
aosctl list-adapters --tier persistent --json

# List pinned adapters (⚠️ workaround - no dedicated command yet)
aosctl adapter list --tenant tenant_acme --json | jq '.[] | select(.pinned == true)'

# Check adapter info
aosctl adapter-info code_review
```

**Success criteria:**
- ✅ Tenant exists in database
- ✅ UID/GID mapping correct
- ✅ Adapters registered
- ✅ Adapter stack created
- ✅ Critical adapters pinned

---

## Playbook 2: Rolling Back a Bad Adapter

**Objective:** Safely revert to a previous adapter version after deployment issues

**When to use:**
- Adapter causing inference errors
- Performance degradation detected
- Policy violations observed

**Prerequisites:**
- Access to telemetry logs
- Previous adapter hash known
- Hot-swap capability enabled

### Steps

#### 2.1 Identify the Problem

```bash
# Check recent telemetry for errors
aosctl telemetry-show \
  --bundle-dir ./var/telemetry \
  --filter "event_type=inference_error" \
  --since "1 hour ago"

# Check adapter activation percentages
aosctl adapter list --tenant tenant_acme --json | \
  jq '.adapters[] | select(.activation_pct > 0) | {id, activation_pct, last_error}'
```

#### 2.2 Find Previous Working Version

```bash
# Query telemetry for last known good hash
aosctl telemetry-show \
  --bundle-dir ./var/telemetry \
  --filter "adapter_id=code_review,event_type=adapter_loaded" \
  --json | jq -r '.[].adapter_hash' | sort | uniq

# Example output:
# b3:old_hash_abc123...  (last known good)
# b3:new_hash_def456...  (current broken version)
```

#### 2.3 Verify Rollback Target

```bash
# Check if old version exists in CAS
ls -la var/cas/b3/old_hash_abc123*

# Verify adapter integrity
aosctl verify --hash b3:old_hash_abc123... --cas-root ./var/cas
```

#### 2.4 Re-register Old Version

```bash
# Register previous version with new ID
aosctl register-adapter \
  code_review_rollback \
  b3:old_hash_abc123... \
  --tier persistent \
  --rank 16
```

#### 2.5 Perform Hot-Swap

```bash
# Dry-run first to check VRAM impact
aosctl adapter-swap \
  --tenant tenant_acme \
  --add code_review_rollback \
  --remove code_review \
  --socket /var/run/aos/aos.sock

# Expected output:
# Dry-run: VRAM delta: +0 MB, 1 adapter swapped
# No errors detected

# Commit the swap
aosctl adapter-swap \
  --tenant tenant_acme \
  --add code_review_rollback \
  --remove code_review \
  --commit \
  --socket /var/run/aos/aos.sock
```

#### 2.6 Verify Rollback

```bash
# Check active adapters
curl http://localhost:8080/api/adapters | jq '.adapters[] | select(.state == "Hot")'

# Run test inference
aosctl infer \
  --prompt "Review this code: def foo(): pass" \
  --adapter-stack acme-code-review \
  --tenant tenant_acme

# Monitor telemetry for errors
tail -f var/telemetry/events.jsonl | grep "inference_error"
```

#### 2.7 Update Adapter Stack

```bash
# Update stack to use rollback adapter
sqlite3 var/aos.db <<EOF
UPDATE adapter_stacks
SET adapter_ids_json = json_replace(
  adapter_ids_json,
  '$[0]',
  'code_review_rollback'
)
WHERE name = 'acme-code-review';
EOF
```

#### 2.8 Document Incident

```bash
# Create incident report
cat > incidents/$(date +%Y%m%d)_adapter_rollback.md <<EOF
# Adapter Rollback Incident

**Date:** $(date)
**Tenant:** tenant_acme
**Adapter:** code_review
**Root Cause:** [TBD - add root cause analysis]

## Timeline
- Detection: $(date -d '1 hour ago')
- Rollback: $(date)

## Actions Taken
1. Identified bad adapter: b3:new_hash_def456...
2. Rolled back to: b3:old_hash_abc123...
3. Hot-swapped in production
4. Verified inference working

## Prevention
- Add integration test for this failure mode
- Implement canary deployment for adapters
EOF
```

**Success criteria:**
- ✅ Old adapter version loaded
- ✅ Inference errors stopped
- ✅ Telemetry shows no new errors
- ✅ Adapter stack updated
- ✅ Incident documented

---

## Playbook 3: Hot-Swapping Adapters in Production

**Objective:** Replace adapters without downtime

**When to use:**
- Deploying new adapter versions
- A/B testing adapters
- Emergency patches

**Prerequisites:**
- Worker running with hot-swap enabled
- New adapter registered in registry
- VRAM capacity checked

### Steps

#### 3.1 Pre-Swap Verification

```bash
# Check current VRAM usage
curl http://localhost:8080/api/memory/usage | jq '.vram_used_mb, .vram_total_mb, .headroom_pct'

# Expected output:
# 12800  (used)
# 16384  (total)
# 21.8   (headroom %)

# Verify new adapter exists
aosctl adapter-info new_adapter

# Check new adapter VRAM requirements
# Should be in adapter metadata
```

#### 3.2 Calculate VRAM Impact

```bash
# Estimate swap impact
# Assume: old_adapter = 512 MB, new_adapter = 768 MB
# Delta = +256 MB

# Check if sufficient headroom
# Current headroom: 3584 MB (21.8%)
# After swap: 3328 MB (20.3%)
# Still above 15% minimum ✓
```

#### 3.3 Dry-Run Swap

```bash
# Test swap without committing
aosctl adapter-swap \
  --tenant tenant_acme \
  --add new_adapter \
  --remove old_adapter \
  --timeout 5000 \
  --socket /var/run/aos/aos.sock

# Expected output:
# Dry-run successful
# VRAM delta: +256 MB
# Adapters to add: 1
# Adapters to remove: 1
# Estimated time: <100ms
```

#### 3.4 Commit Swap

```bash
# Execute swap atomically
aosctl adapter-swap \
  --tenant tenant_acme \
  --add new_adapter \
  --remove old_adapter \
  --commit \
  --timeout 5000 \
  --socket /var/run/aos/aos.sock
```

#### 3.5 Verify Swap

```bash
# Check adapter table
curl http://localhost:8080/api/adapters | jq '.adapters[] | {id, state, vram_mb}'

# Expected:
# {
#   "id": "new_adapter",
#   "state": "Hot",
#   "vram_mb": 768
# }

# Run test inference
aosctl infer \
  --prompt "Test prompt" \
  --adapter new_adapter \
  --tenant tenant_acme
```

#### 3.6 Monitor Post-Swap

```bash
# Watch telemetry for 5 minutes
timeout 300 tail -f var/telemetry/events.jsonl | \
  jq 'select(.adapter_id == "new_adapter")'

# Check for errors
grep "new_adapter" var/telemetry/events.jsonl | \
  jq 'select(.event_type == "inference_error")'
```

#### 3.7 Rollback if Needed

```bash
# If issues detected, rollback immediately
aosctl adapter-swap \
  --tenant tenant_acme \
  --add old_adapter \
  --remove new_adapter \
  --commit \
  --socket /var/run/aos/aos.sock
```

**Success criteria:**
- ✅ Swap completed in <5 seconds
- ✅ No inference interruptions
- ✅ VRAM headroom maintained >15%
- ✅ New adapter serving traffic
- ✅ No telemetry errors

---

## Playbook 4: Creating Training Datasets from Documents

**Objective:** Ingest documents and generate training data for adapter training

**When to use:**
- Building domain-specific adapters
- RAG pipeline setup
- Knowledge base integration

**Prerequisites:**
- Documents in PDF/Markdown format
- Tokenizer available
- Storage space for datasets

### Steps

#### 4.1 Organize Source Documents

```bash
# Create document directory structure
mkdir -p datasets/acme-api-docs/{pdfs,markdown}

# Copy source documents
cp ~/docs/api/*.pdf datasets/acme-api-docs/pdfs/
cp ~/docs/guides/*.md datasets/acme-api-docs/markdown/
```

#### 4.2 Ingest Documents via CLI

```bash
# Ingest PDF documents and generate training data
aosctl ingest-docs \
  datasets/acme-api-docs/pdfs/api-reference.pdf \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --chunk-tokens 512 \
  --overlap-tokens 64 \
  --generate-training \
  --training-output datasets/acme-api-docs/training/api-reference.jsonl

# Ingest Markdown and generate training data
aosctl ingest-docs \
  datasets/acme-api-docs/markdown/user-guide.md \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --chunk-tokens 512 \
  --overlap-tokens 64 \
  --generate-training \
  --training-output datasets/acme-api-docs/training/user-guide.jsonl
```

#### 4.3 Combine Training Examples (Optional)

⚠️ **NOTE:** Training data already generated in Step 4.2. This step only needed if combining multiple files.

```bash
# Combine multiple training files (optional)
cat datasets/acme-api-docs/training/*.jsonl > datasets/acme-api-docs/training-combined.jsonl

# Or use individual files separately for training
```

#### 4.4 Create Dataset Record in Database

```bash
# Register dataset (via API)
curl -X POST http://localhost:8080/v1/training/datasets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "acme-api-docs-v1",
    "description": "ACME API documentation training data",
    "document_paths": [
      "datasets/acme-api-docs/pdfs/api-reference.pdf",
      "datasets/acme-api-docs/markdown/user-guide.md"
    ],
    "training_config": {
      "strategy": "identity",
      "max_seq_length": 512,
      "add_special_tokens": true
    },
    "created_by": "operator"
  }'

# Response includes dataset_id
# {"dataset_id": "dataset-abc123", "num_examples": 1247, "hash_b3": "b3:..."}
```

#### 4.5 Validate Dataset

```bash
# Check dataset statistics
curl http://localhost:8080/v1/training/datasets/dataset-abc123 | jq

# Expected output:
# {
#   "dataset_id": "dataset-abc123",
#   "name": "acme-api-docs-v1",
#   "num_examples": 1247,
#   "total_tokens": 638976,
#   "validation_status": "valid",
#   "hash_b3": "b3:..."
# }

# Verify dataset file
aosctl verify \
  --hash b3:... \
  --cas-root ./var/cas
```

#### 4.6 Inspect Dataset Samples

```bash
# View first 5 examples
head -5 datasets/acme-api-docs/training.jsonl | jq

# Check token distribution
cat datasets/acme-api-docs/training.jsonl | \
  jq -r '.input | length' | \
  awk '{sum+=$1; sumsq+=$1*$1} END {print "Mean:", sum/NR, "StdDev:", sqrt(sumsq/NR - (sum/NR)^2)}'
```

**Success criteria:**
- ✅ All documents ingested
- ✅ Training examples generated
- ✅ Dataset registered in database
- ✅ Validation status: valid
- ✅ BLAKE3 hash computed

---

## Playbook 5: Training and Deploying a New Adapter

**Objective:** Train a LoRA adapter from dataset and deploy to production

**When to use:**
- After creating training dataset
- Fine-tuning for new domain
- Improving existing adapter

**Prerequisites:**
- Training dataset created (Playbook 4)
- Worker with Metal backend
- VRAM capacity for training

### Steps

#### 5.1 Select Training Template

```bash
# List available templates
curl http://localhost:8080/v1/training/templates | jq

# Example output:
# [
#   {
#     "id": "general-code",
#     "name": "General Code Adapter",
#     "config": {
#       "rank": 16,
#       "alpha": 32,
#       "targets": ["q_proj", "k_proj", "v_proj", "o_proj"],
#       "epochs": 3,
#       "learning_rate": 0.0001
#     }
#   }
# ]
```

#### 5.2 Start Training Job

```bash
# Create training job
curl -X POST http://localhost:8080/v1/training/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "adapter_id": "acme-api-adapter-v1",
    "template_id": "general-code",
    "dataset_id": "dataset-abc123",
    "config": {
      "rank": 16,
      "alpha": 32,
      "epochs": 3,
      "learning_rate": 0.0001,
      "batch_size": 8
    }
  }'

# Response: {"job_id": "job-xyz789", "status": "Pending"}
```

#### 5.3 Monitor Training Progress

```bash
# Poll job status
watch -n 5 'curl -s http://localhost:8080/v1/training/jobs/job-xyz789 | jq'

# Expected output (updating):
# {
#   "job_id": "job-xyz789",
#   "status": "Running",
#   "progress_pct": 45.2,
#   "current_loss": 0.342,
#   "epoch": 2,
#   "tokens_per_sec": 1250
# }

# Watch telemetry logs
tail -f var/telemetry/events.jsonl | \
  jq 'select(.job_id == "job-xyz789")'
```

#### 5.4 Verify Training Completion

```bash
# Check final status
curl http://localhost:8080/v1/training/jobs/job-xyz789 | jq

# Expected:
# {
#   "status": "Completed",
#   "progress_pct": 100.0,
#   "final_loss": 0.089,
#   "trained_adapter_id": "acme-api-adapter-v1",
#   "adapter_hash": "b3:trained_hash...",
#   "completed_at": "2025-01-16T12:34:56Z"
# }
```

#### 5.5 Register Trained Adapter

```bash
# Adapter is automatically registered after training
# Verify registration
aosctl adapter-info acme-api-adapter-v1

# Expected output:
# Adapter ID: acme-api-adapter-v1
# Hash: b3:trained_hash...
# Rank: 16
# Tier: persistent
# State: Unloaded
# Training job: job-xyz789
```

#### 5.6 Test Adapter

```bash
# Run test inference
aosctl infer \
  --prompt "Explain the /api/users endpoint" \
  --adapter acme-api-adapter-v1 \
  --tenant tenant_acme \
  --max-tokens 256

# Check for hallucination/quality
# Expected: Should reference API docs content
```

#### 5.7 Deploy to Production

```bash
# Load adapter into lifecycle
aosctl adapter load \
  --adapter acme-api-adapter-v1 \
  --tenant tenant_acme

# Add to adapter stack
curl -X POST http://localhost:8080/v1/adapter-stacks \
  -H "Content-Type: application/json" \
  -d '{
    "name": "acme-api-qa",
    "description": "API Q&A with trained adapter",
    "adapter_ids": ["acme-api-adapter-v1"],
    "workflow_type": "Sequential"
  }'

# Hot-swap into production (if replacing existing)
aosctl adapter-swap \
  --tenant tenant_acme \
  --add acme-api-adapter-v1 \
  --remove old-api-adapter \
  --commit \
  --socket /var/run/aos/aos.sock
```

#### 5.8 Monitor Deployment

```bash
# Watch activation percentages
watch -n 10 'curl -s http://localhost:8080/api/adapters | \
  jq ".adapters[] | select(.id == \"acme-api-adapter-v1\") | {state, activation_pct}"'

# Check inference quality
# Run sample queries and verify responses
aosctl infer \
  --prompt "What authentication methods does the API support?" \
  --adapter-stack acme-api-qa \
  --tenant tenant_acme
```

**Success criteria:**
- ✅ Training job completed
- ✅ Final loss <0.1
- ✅ Adapter registered
- ✅ Test inference successful
- ✅ Deployed to production
- ✅ Activation percentage increasing

---

## Playbook 6: Verifying Determinism Across Cluster

**Objective:** Ensure all cluster nodes produce identical outputs

**When to use:**
- After cluster deployment
- Before production traffic
- Regular compliance audits
- After kernel updates

**Prerequisites:**
- Multi-node cluster deployed
- Same base model on all nodes
- Same adapter hashes on all nodes

### Steps

#### 6.1 Verify Environment Fingerprints

```bash
# Check fingerprint on each node
for node in node1 node2 node3; do
  ssh $node "cd /opt/adapteros && aosctl drift-check --json" > ${node}_fingerprint.json
done

# Compare fingerprints
diff -u node1_fingerprint.json node2_fingerprint.json
diff -u node2_fingerprint.json node3_fingerprint.json

# Expected: No differences (deterministic environment)
```

#### 6.2 Verify Adapter Hashes

```bash
# Check adapter registry on each node
for node in node1 node2 node3; do
  ssh $node "cd /opt/adapteros && aosctl list-adapters --json" > ${node}_adapters.json
done

# Compare hashes
jq -r '.adapters[] | "\(.id): \(.hash)"' node1_adapters.json | sort > hashes1.txt
jq -r '.adapters[] | "\(.id): \(.hash)"' node2_adapters.json | sort > hashes2.txt
jq -r '.adapters[] | "\(.id): \(.hash)"' node3_adapters.json | sort > hashes3.txt

diff -u hashes1.txt hashes2.txt
```

#### 6.3 Run Determinism Test

```bash
# Execute same inference on all nodes
TEST_PROMPT="Explain Rust ownership rules"
TEST_SEED="determinism_test_seed_123"

for node in node1 node2 node3; do
  ssh $node "cd /opt/adapteros && \
    aosctl infer \
      --prompt '$TEST_PROMPT' \
      --adapter code_review \
      --seed $TEST_SEED \
      --temperature 0.0 \
      --json" > ${node}_output.json
done
```

#### 6.4 Compare Outputs

```bash
# Extract response text
jq -r '.response' node1_output.json > output1.txt
jq -r '.response' node2_output.json > output2.txt
jq -r '.response' node3_output.json > output3.txt

# Compare byte-for-byte
diff -u output1.txt output2.txt
diff -u output2.txt output3.txt

# Expected: No differences (deterministic inference)
```

#### 6.5 Verify Telemetry Hashes

```bash
# Check telemetry bundle hashes
for node in node1 node2 node3; do
  ssh $node "cd /opt/adapteros && \
    aosctl telemetry-verify \
      --bundle-dir ./var/telemetry \
      --json" > ${node}_telemetry.json
done

# Compare Merkle roots
jq -r '.merkle_root' node1_telemetry.json
jq -r '.merkle_root' node2_telemetry.json
jq -r '.merkle_root' node3_telemetry.json

# Note: Merkle roots will differ (different timestamps)
# But event hashes for same inputs should match
```

#### 6.6 Use Built-in Cluster Verification

```bash
# Run automated cluster determinism check
aosctl node-verify --all --verbose

# Expected output:
# Checking node1... ✓
# Checking node2... ✓
# Checking node3... ✓
#
# Determinism check: PASS
# - All nodes have identical adapter hashes
# - All nodes have identical environment fingerprints
# - Sample inference outputs match across cluster
```

#### 6.7 Document Verification

```bash
# Generate verification report
aosctl node-verify --all --json > verification_$(date +%Y%m%d).json

# Store in compliance directory
mv verification_$(date +%Y%m%d).json compliance/determinism-checks/
```

**Success criteria:**
- ✅ Environment fingerprints identical
- ✅ Adapter hashes match across nodes
- ✅ Inference outputs byte-identical
- ✅ No drift detected
- ✅ Verification report generated

---

## Playbook 7: Responding to Memory Pressure

**Objective:** Handle low VRAM conditions safely

**When to use:**
- VRAM headroom <15%
- Eviction events in telemetry
- Memory exhaustion alerts

**Prerequisites:**
- Monitoring system configured
- Alert thresholds set
- Access to worker API

### Steps

#### 7.1 Detect Memory Pressure

```bash
# Check current VRAM usage
curl http://localhost:8080/api/memory/usage | jq

# Expected output:
# {
#   "vram_total_mb": 16384,
#   "vram_used_mb": 14336,
#   "vram_free_mb": 2048,
#   "headroom_pct": 12.5,  # <15% threshold!
#   "adapters_loaded": 12,
#   "eviction_candidates": 4
# }
```

#### 7.2 Identify Eviction Candidates

```bash
# List adapters by activation percentage
curl http://localhost:8080/api/adapters | \
  jq '.adapters | sort_by(.activation_pct) | .[] | {id, state, activation_pct, vram_mb, pinned}'

# Example output:
# {
#   "id": "legacy_adapter",
#   "state": "Warm",
#   "activation_pct": 0.5,  # Low activation, good candidate
#   "vram_mb": 512,
#   "pinned": false
# }
```

#### 7.3 Check for Pinned Adapters

```bash
# List pinned adapters (cannot be evicted)
aosctl list-pinned --tenant tenant_acme

# If too many adapters pinned, need to unpin some
```

#### 7.4 Manual Eviction

```bash
# Evict low-activation adapters using CLI
aosctl adapter evict legacy_adapter --tenant tenant_acme --reason "Low activation + memory pressure"

# Evict another
aosctl adapter evict experimental_v1 --tenant tenant_acme --reason "Low activation + memory pressure"

# Alternative: Via API
curl -X POST http://localhost:8080/v1/adapters/legacy_adapter/evict \
  -H "Content-Type: application/json" \
  -d '{"reason": "Low activation + memory pressure"}'
```

#### 7.5 Verify Headroom Restored

```bash
# Check VRAM again
curl http://localhost:8080/api/memory/usage | jq '.headroom_pct'

# Expected: >15%
```

#### 7.6 Adjust Eviction Policy (if needed)

```bash
# Update config to be more aggressive
# Edit configs/cp.toml:
[memory]
min_headroom_pct = 20  # Increase from 15
evict_order = ["ephemeral_ttl", "cold_lru", "warm_lru"]
evict_threshold_activation = 5.0  # Evict adapters <5% activation

# Restart worker to apply
systemctl restart aos-worker
```

#### 7.7 Consider Scaling Out

```bash
# If eviction not sufficient, add capacity
# Option 1: Add node to cluster
# Option 2: Increase VRAM quota for tenant
# Option 3: Partition adapters across nodes

# Example: Split tenant adapters across 2 nodes
# Node 1: Production adapters
# Node 2: Experimental adapters
```

#### 7.8 Set Up Proactive Monitoring

```bash
# Create alert for memory pressure
cat > monitoring/alerts/vram_headroom.yaml <<EOF
alert: VRAMHeadroomLow
expr: vram_headroom_pct < 15
for: 5m
annotations:
  summary: "VRAM headroom below 15%"
  description: "Current: {{ \$value }}%"
  playbook: "docs/OPERATOR_PLAYBOOKS.md#playbook-7"
EOF
```

**Success criteria:**
- ✅ Headroom restored to >15%
- ✅ Low-activation adapters evicted
- ✅ Critical adapters still loaded
- ✅ Inference not interrupted
- ✅ Monitoring alerts configured

---

## Playbook 8: Drift Detection and Baseline Management

**Objective:** Monitor and manage environment drift

**When to use:**
- Before production deployments
- Regular compliance audits
- After OS/kernel updates
- Debugging non-determinism

**Prerequisites:**
- Baseline fingerprint exists
- Access to production environment
- Audit trail enabled

### Steps

#### 8.1 Create Initial Baseline (First Time)

```bash
# Capture clean environment fingerprint
aosctl drift-check --save-baseline

# Expected output:
# ✓ Environment fingerprint captured
# ✓ Baseline saved: var/baseline_fingerprint.json
#
# Fingerprint includes:
# - OS version: macOS 14.2.1
# - Hardware: Apple M3 Max
# - Rust version: 1.75.0
# - Kernel versions: Metal kernels b3:abc123...
# - Adapter hashes: 12 adapters
```

#### 8.2 Regular Drift Checks

```bash
# Check for drift (daily cron job)
aosctl drift-check --json > drift_$(date +%Y%m%d).json

# Example output (no drift):
# {
#   "drift_detected": false,
#   "baseline_hash": "b3:baseline_hash...",
#   "current_hash": "b3:baseline_hash...",
#   "checked_at": "2025-01-16T10:00:00Z"
# }
```

#### 8.3 Detect Drift

```bash
# After system update, drift detected
aosctl drift-check

# Example output:
# ⚠ Environment drift detected
#
# Differences:
# - OS version: macOS 14.2.1 → macOS 14.3.0 (changed)
# - Kernel hash: b3:abc123... → b3:def456... (changed)
# - Adapter hashes: 12 matching, 0 changed
#
# Drift hash: b3:drift_hash...
```

#### 8.4 Analyze Drift Impact

```bash
# Get detailed diff
aosctl drift-check --json | jq '.differences'

# Example:
# {
#   "os_version": {
#     "baseline": "macOS 14.2.1",
#     "current": "macOS 14.3.0",
#     "impact": "high"  # OS updates can affect determinism
#   },
#   "kernel_hash": {
#     "baseline": "b3:abc123...",
#     "current": "b3:def456...",
#     "impact": "critical"  # Kernel changes break determinism
#   }
# }
```

#### 8.5 Verify Determinism After Drift

```bash
# Run determinism test
TEST_PROMPT="Test prompt for drift analysis"
TEST_SEED="drift_test_123"

# Before drift (from telemetry)
OLD_OUTPUT=$(grep "drift_test_123" var/telemetry/events.jsonl | \
  jq -r '.response' | head -1)

# After drift
NEW_OUTPUT=$(aosctl infer \
  --prompt "$TEST_PROMPT" \
  --seed $TEST_SEED \
  --json | jq -r '.response')

# Compare
if [ "$OLD_OUTPUT" == "$NEW_OUTPUT" ]; then
  echo "✓ Determinism preserved despite drift"
else
  echo "✗ Determinism broken - output changed"
fi
```

#### 8.6 Update Baseline (if intended change)

```bash
# If drift is expected (e.g., planned kernel update)
# Save new baseline
aosctl drift-check --save-baseline

# Document the change
cat > compliance/drift-logs/$(date +%Y%m%d)_baseline_update.md <<EOF
# Baseline Update

**Date:** $(date)
**Reason:** Planned macOS update + kernel recompilation

## Changes
- OS: macOS 14.2.1 → 14.3.0
- Kernel: b3:abc123... → b3:def456...

## Verification
- Determinism test: PASS
- Cluster sync: PASS
- Signature verification: PASS

**Approved by:** operator
EOF
```

#### 8.7 Rollback if Unintended Drift

```bash
# If drift is unintended/breaks determinism
# Option 1: Rollback OS (if possible)
# Option 2: Recompile kernels from baseline
# Option 3: Restore from backup

# Example: Restore kernel from baseline
cp var/baseline_kernels.metallib target/kernels.metallib

# Verify restoration
aosctl drift-check
```

#### 8.8 Set Up Drift Monitoring

```bash
# Daily drift check cron job
cat > /etc/cron.daily/aos-drift-check <<'EOF'
#!/bin/bash
cd /opt/adapteros
aosctl drift-check --json > /var/log/aos/drift_$(date +%Y%m%d).json

if [ $? -ne 0 ]; then
  # Send alert
  echo "Drift detected on $(hostname)" | mail -s "AdapterOS Drift Alert" ops@example.com
fi
EOF

chmod +x /etc/cron.daily/aos-drift-check
```

**Success criteria:**
- ✅ Baseline fingerprint exists
- ✅ Drift detected and analyzed
- ✅ Determinism impact assessed
- ✅ Baseline updated (if intended)
- ✅ Monitoring configured

---

## Playbook 9: Incident Response - Adapter Failures

**Objective:** Diagnose and resolve adapter-related incidents

**When to use:**
- Adapter causing crashes
- Inference errors spiking
- Performance degradation
- Policy violations

**Prerequisites:**
- Telemetry logging enabled
- Access to worker logs
- Rollback capability

### Steps

#### 9.1 Detect the Incident

```bash
# Alert triggered: High error rate
# Check recent errors
aosctl telemetry-show \
  --bundle-dir ./var/telemetry \
  --filter "event_type=inference_error" \
  --since "15 minutes ago" \
  --json | jq

# Example output:
# [
#   {
#     "timestamp": "2025-01-16T14:32:15Z",
#     "event_type": "inference_error",
#     "adapter_id": "buggy_adapter",
#     "error": "Metal kernel assertion failed",
#     "request_id": "req-abc123"
#   }
# ]
```

#### 9.2 Identify Problematic Adapter

```bash
# Count errors by adapter
cat var/telemetry/events.jsonl | \
  jq -r 'select(.event_type == "inference_error") | .adapter_id' | \
  sort | uniq -c | sort -rn

# Example output:
# 47 buggy_adapter
#  3 other_adapter
#  1 null

# Clear culprit: buggy_adapter
```

#### 9.3 Immediate Mitigation - Remove from Production

```bash
# Hot-swap out the bad adapter
aosctl adapter-swap \
  --tenant tenant_acme \
  --remove buggy_adapter \
  --commit \
  --socket /var/run/aos/aos.sock

# Or: Unpin and let it evict
aosctl unpin-adapter \
  --tenant tenant_acme \
  --adapter buggy_adapter
```

#### 9.4 Collect Diagnostic Information

```bash
# Create incident directory
mkdir -p incidents/$(date +%Y%m%d)_buggy_adapter
cd incidents/$(date +%Y%m%d)_buggy_adapter

# Collect adapter info
aosctl adapter-info buggy_adapter --json > adapter_info.json

# Collect error telemetry
grep "buggy_adapter" ../../var/telemetry/events.jsonl > errors.jsonl

# Collect worker logs
journalctl -u aos-worker --since "1 hour ago" > worker.log

# Collect memory state
curl http://localhost:8080/api/memory/usage > memory_state.json

# Collect adapter provenance
aosctl verify --hash $(jq -r '.hash' adapter_info.json) --cas-root ../../var/cas > provenance.txt
```

#### 9.5 Analyze Root Cause

```bash
# Check adapter training lineage
jq -r '.training_job_id' adapter_info.json
# Follow to training job

curl http://localhost:8080/v1/training/jobs/$(jq -r '.training_job_id' adapter_info.json) | jq

# Check for common issues:
# - Rank too high (>32)?
# - Training loss too high (>0.5)?
# - Dataset quality issues?
# - Quantization errors?

# Extract error patterns
cat errors.jsonl | jq -r '.error' | sort | uniq -c
# Example:
# 45 Metal kernel assertion failed: buffer overflow
#  2 Invalid VRAM access
```

#### 9.6 Reproduce Locally

```bash
# Try to reproduce the error
aosctl infer \
  --prompt "$(cat test_prompts/failure_case.txt)" \
  --adapter buggy_adapter \
  --verbose

# Expected: Should reproduce the error
```

#### 9.7 Fix and Retrain (if dataset issue)

```bash
# If root cause is training data quality
# Option 1: Retrain with better dataset
# Option 2: Adjust training hyperparameters
# Option 3: Use different base model

# Example: Retrain with lower rank
curl -X POST http://localhost:8080/v1/training/jobs \
  -d '{
    "adapter_id": "fixed_adapter_v2",
    "dataset_id": "dataset-fixed",
    "config": {
      "rank": 8,  # Reduced from 16
      "alpha": 16,
      "epochs": 3
    }
  }'
```

#### 9.8 Document Incident

```bash
# Create incident report
cat > incident_report.md <<EOF
# Adapter Failure Incident Report

**Date:** $(date)
**Incident ID:** INC-$(date +%Y%m%d)-001
**Severity:** P1 (Production impact)

## Summary
Adapter 'buggy_adapter' caused 47 inference errors over 15 minutes due to Metal kernel buffer overflow.

## Timeline
- 14:30: First error detected
- 14:32: Alert triggered
- 14:35: Adapter removed from production
- 14:45: Root cause identified
- 15:00: Fix deployed

## Root Cause
- Adapter rank too high (32) for model size
- Training dataset had outlier sequence lengths
- Kernel buffer size insufficient

## Resolution
- Removed buggy_adapter from production
- Retrained with rank=8
- Added input length validation

## Prevention
- Add pre-deployment validation for adapter rank
- Limit max sequence length in training
- Add kernel buffer size checks

## Follow-up Actions
- [ ] Deploy fixed_adapter_v2
- [ ] Add integration test for this failure mode
- [ ] Update training templates with rank limits
EOF
```

**Success criteria:**
- ✅ Bad adapter removed from production
- ✅ Error rate back to normal
- ✅ Root cause identified
- ✅ Fix deployed
- ✅ Incident documented
- ✅ Prevention measures implemented

---

## Playbook 10: Telemetry Audit Trail Verification

**Objective:** Verify integrity of telemetry audit trail

**When to use:**
- Compliance audits
- Security investigations
- Determinism verification
- Before/after critical operations

**Prerequisites:**
- Telemetry bundles exist
- Signing keys available
- Merkle tree implementation enabled

### Steps

#### 10.1 Verify Telemetry Bundle Chain

```bash
# Verify all bundles in directory
aosctl telemetry-verify \
  --bundle-dir ./var/telemetry \
  --json | jq

# Expected output:
# {
#   "verified": true,
#   "bundles_checked": 145,
#   "merkle_root": "b3:merkle_root_hash...",
#   "earliest_timestamp": "2025-01-01T00:00:00Z",
#   "latest_timestamp": "2025-01-16T15:00:00Z",
#   "chain_integrity": "valid"
# }
```

#### 10.2 Verify Federation Signatures

```bash
# Verify cross-node signatures
aosctl federation-verify \
  --bundle-dir ./var/telemetry \
  --database ./var/cp.db \
  --json | jq

# Expected output:
# {
#   "verified": true,
#   "nodes_checked": 3,
#   "signatures_valid": 145,
#   "signatures_invalid": 0,
#   "cross_node_hash_matches": true
# }
```

#### 10.3 Audit Specific Event

```bash
# Find specific event
EVENT_ID="req-abc123"

cat var/telemetry/events.jsonl | \
  jq "select(.request_id == \"$EVENT_ID\")"

# Verify event hash
EVENT_JSON=$(cat var/telemetry/events.jsonl | jq -c "select(.request_id == \"$EVENT_ID\")")
echo -n "$EVENT_JSON" | b3sum

# Compare with stored hash in Merkle tree
```

#### 10.4 Generate Audit Report

```bash
# Create comprehensive audit report
aosctl audit \
  --start "2025-01-01" \
  --end "2025-01-16" \
  --output audit_report_$(date +%Y%m%d).json

# Report includes:
# - Total events
# - Events by type
# - Adapter activation history
# - Policy violations
# - Determinism checks
# - Merkle root
```

#### 10.5 Verify Specific Adapter Usage

```bash
# Audit adapter activation history
cat var/telemetry/events.jsonl | \
  jq 'select(.adapter_id == "code_review")' | \
  jq -r '[.timestamp, .event_type, .request_id] | @tsv' | \
  column -t

# Count activations by hour
cat var/telemetry/events.jsonl | \
  jq -r 'select(.adapter_id == "code_review" and .event_type == "inference_complete") | .timestamp[:13]' | \
  uniq -c
```

#### 10.6 Verify Determinism from Telemetry

```bash
# Find duplicate requests (same prompt + seed)
cat var/telemetry/events.jsonl | \
  jq -c 'select(.event_type == "inference_complete") | {prompt_hash: .prompt_hash, seed: .seed, response_hash: .response_hash}' | \
  sort | uniq -d

# If any duplicates with different response_hash → determinism violation
```

#### 10.7 Export Audit Trail for Compliance

```bash
# Export for external auditors
aosctl telemetry-show \
  --bundle-dir ./var/telemetry \
  --start "2025-01-01" \
  --end "2025-01-16" \
  --format csv \
  --output audit_trail_Q1_2025.csv

# Sign export
b3sum audit_trail_Q1_2025.csv > audit_trail_Q1_2025.csv.b3
gpg --sign audit_trail_Q1_2025.csv

# Package for delivery
tar -czf audit_trail_Q1_2025.tar.gz \
  audit_trail_Q1_2025.csv \
  audit_trail_Q1_2025.csv.b3 \
  audit_trail_Q1_2025.csv.gpg
```

#### 10.8 Set Up Continuous Verification

```bash
# Hourly verification cron job
cat > /etc/cron.hourly/aos-telemetry-verify <<'EOF'
#!/bin/bash
cd /opt/adapteros

# Verify telemetry integrity
aosctl telemetry-verify --bundle-dir ./var/telemetry --json > /tmp/verify_result.json

if [ $? -ne 0 ]; then
  # Alert on verification failure
  echo "Telemetry verification failed on $(hostname)" | \
    mail -s "CRITICAL: Telemetry Integrity Failure" security@example.com
fi

# Archive result
mv /tmp/verify_result.json var/audit/verify_$(date +%Y%m%d_%H%M).json
EOF

chmod +x /etc/cron.hourly/aos-telemetry-verify
```

**Success criteria:**
- ✅ All bundles verified
- ✅ Merkle root computed
- ✅ No chain integrity violations
- ✅ Federation signatures valid
- ✅ Audit report generated
- ✅ Continuous verification enabled

---

## Appendix: Quick Reference

### Common Commands

```bash
# Tenant management
aosctl init-tenant --id <tenant> --uid <uid> --gid <gid>

# Adapter management
aosctl register-adapter <id> <hash> --tier <tier> --rank <rank>
aosctl adapter-info <adapter_id>
aosctl list-adapters [--tier <tier>]

# Hot-swap
aosctl adapter-swap --tenant <tenant> --add <ids> --remove <ids> --commit

# Training
curl -X POST /api/training/datasets -d '{...}'
curl -X POST /api/training/jobs -d '{...}'

# Verification
aosctl drift-check [--save-baseline]
aosctl telemetry-verify --bundle-dir <dir>
aosctl node-verify --all

# Monitoring
curl http://localhost:8080/api/memory/usage
curl http://localhost:8080/api/adapters
```

### API Endpoints

```
# Adapters
GET    /api/adapters
GET    /api/adapters/:id
POST   /api/adapters/:id/evict

# Training
GET    /v1/training/templates
POST   /v1/training/datasets
GET    /v1/training/datasets/:id
POST   /v1/training/jobs
GET    /v1/training/jobs/:id

# Memory
GET    /api/memory/usage

# Stacks
GET    /v1/adapter-stacks
POST   /v1/adapter-stacks

# Inference
POST   /api/chat/completions
```

### Telemetry Event Types

```
- inference_complete
- inference_error
- adapter_loaded
- adapter_evicted
- adapter_swapped
- training_started
- training_complete
- policy_violation
- determinism_check
```

### Directory Structure

```
/opt/adapteros/
├── var/
│   ├── aos.db                  # Main database
│   ├── registry.db             # Adapter registry
│   ├── telemetry/              # Telemetry bundles
│   │   └── events.jsonl
│   ├── cas/                    # Content-addressed storage
│   │   └── b3/
│   └── baseline_fingerprint.json
├── configs/
│   └── cp.toml                 # Configuration
├── datasets/                   # Training datasets
├── incidents/                  # Incident reports
└── compliance/                 # Audit logs
    ├── drift-logs/
    └── determinism-checks/
```

---

## Next Steps

1. **Convert to Integration Tests**
   - Create test harness for each playbook
   - Automate verification steps
   - Add to CI/CD pipeline

2. **CLI Gaps to Fill**
   - ~~`aosctl adapter evict`~~ ✅ Implemented
   - `aosctl training status`
   - `aosctl audit generate`

3. **UI Flows**
   - Visual playbook execution
   - Guided wizards for common tasks
   - Real-time status dashboards

4. **Documentation**
   - Video walkthroughs
   - Interactive tutorials
   - Troubleshooting guide

---

**Version:** 1.0
**Last Updated:** 2025-01-16
**Maintained by:** AdapterOS Operations Team
