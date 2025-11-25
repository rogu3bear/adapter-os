# Single-Node Golden Path Demo Script

**Purpose:** Step-by-step walkthrough of the complete AdapterOS workflow from dataset upload to chat inference
**Target Audience:** Product demo, customer onboarding, PRD-LAUNCH-01 validation
**Last Updated:** 2025-11-25
**Status:** Production Ready

---

## Prerequisites

1. **System Running:**
   ```bash
   # Terminal 1: Start backend server
   export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
   export DATABASE_URL=sqlite://var/aos-cp.sqlite3
   cargo run --release -p adapteros-server-api

   # Terminal 2: Start UI (optional)
   cd ui && pnpm dev
   ```

2. **Base Model Loaded:**
   - Qwen 2.5 7B MLX format downloaded
   - Server health check passes: `curl http://localhost:8080/healthz`

3. **Test Dataset Ready:**
   - Sample JSONL file with training examples
   - OR directory of code files for codebase-specific adapter

---

## Flow Summary

```
Directory Upload → Dataset Validation → Training Job → Adapter Registration
→ Stack Creation → Chat Integration → Audit Trail
```

**Timeline:** 10-15 minutes (depends on training duration)

---

## Step 1: Upload Dataset

### Option A: Single File Upload (Simple Path)

**UI Path:** `/training` → Datasets tab → Upload Dataset

**API:**
```bash
curl -X POST http://localhost:8080/v1/datasets/upload \
  -H "Authorization: Bearer $TOKEN" \
  -F "name=rust-code-examples" \
  -F "description=Rust code snippets for training" \
  -F "format=jsonl" \
  -F "file=@training-data.jsonl"
```

**Expected Response:**
```json
{
  "schema_version": "1.0",
  "dataset_id": "ds-abc123",
  "name": "rust-code-examples",
  "file_count": 1,
  "total_size_bytes": 50000,
  "format": "jsonl",
  "hash": "blake3-hash-here",
  "created_at": "2025-11-25T10:00:00Z"
}
```

**Verification:**
```bash
# Check dataset created
curl http://localhost:8080/v1/datasets/ds-abc123 \
  -H "Authorization: Bearer $TOKEN"

# Check files
curl http://localhost:8080/v1/datasets/ds-abc123/files \
  -H "Authorization: Bearer $TOKEN"
```

### Option B: Directory Upload (Advanced Path)

**UI Path:** `/training/datasets` → DatasetBuilder → Select directory → Upload

**Implementation Status:**
- ✅ Backend supports multiple file upload via multipart form
- ✅ UI has directory selection handler (`handleDirectorySelect`)
- ⚠️ UI simulation mode active - needs real API integration

**Real Implementation (CLI):**
```bash
# Create directory structure
mkdir -p datasets/codebase-example
cp -r /path/to/codebase/*.rs datasets/codebase-example/

# Upload via CLI helper
./scripts/upload_directory_dataset.sh datasets/codebase-example rust-codebase
```

**Storage Layout:**
```
var/datasets/
  ds-abc123/
    files/
      main.rs
      lib.rs
      utils.rs
    metadata.json
```

---

## Step 2: Validate Dataset

**UI Path:** `/training/datasets` → Select dataset → Validate button

**API:**
```bash
curl -X POST http://localhost:8080/v1/datasets/ds-abc123/validate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"check_format": true}'
```

**Expected Response:**
```json
{
  "schema_version": "1.0",
  "dataset_id": "ds-abc123",
  "is_valid": true,
  "validation_status": "valid",
  "errors": null,
  "validated_at": "2025-11-25T10:01:00Z"
}
```

**Validation Checks:**
- File existence and hash verification
- Format compliance (JSONL, JSON, CSV)
- Schema validation (input/target fields)

**Status Transition:** `pending` → `validating` → `valid`

---

## Step 3: Start Training Job

**UI Path:** `/training` → Jobs tab → New Training Job → TrainingWizard

**Training Wizard Steps:**
1. Select dataset: `ds-abc123`
2. Choose template: `general-code` (rank=16, alpha=32)
3. Configure hyperparameters:
   - Epochs: 3
   - Learning rate: 0.001
   - Batch size: 8
4. Name adapter: `rust-expert`

**API:**
```bash
curl -X POST http://localhost:8080/v1/training/start \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "adapter_name": "rust-expert",
    "config": {
      "rank": 16,
      "alpha": 32,
      "epochs": 3,
      "learning_rate": 0.001,
      "batch_size": 8,
      "targets": ["q_proj", "k_proj", "v_proj", "o_proj"]
    },
    "template_id": "general-code",
    "dataset_id": "ds-abc123"
  }'
```

**Expected Response:**
```json
{
  "schema_version": "1.0",
  "job_id": "train-xyz789",
  "adapter_name": "rust-expert",
  "status": "pending",
  "progress_pct": 0.0,
  "current_epoch": 0,
  "total_epochs": 3,
  "created_at": "2025-11-25T10:02:00Z"
}
```

**Training States:**
- `pending` → `running` → `completed` | `failed` | `cancelled`

**Monitor Progress:**
```bash
# Poll job status
curl http://localhost:8080/v1/training/jobs/train-xyz789 \
  -H "Authorization: Bearer $TOKEN"

# Get logs
curl http://localhost:8080/v1/training/jobs/train-xyz789/logs \
  -H "Authorization: Bearer $TOKEN"
```

**UI Updates:**
- Jobs tab shows real-time progress
- Progress bar updates every second
- Loss/tokens-per-second displayed

---

## Step 4: Automatic Adapter Registration

**On Training Completion (Automatic):**

1. **Quantization:** Weights quantized to Q15 format
2. **Packaging:** `.aos` archive created with 64-byte header
3. **Hash:** BLAKE3 hash computed for content addressing
4. **Registration:** Adapter registered in database

**Implementation Location:** `crates/adapteros-orchestrator/src/training.rs:549-591`

**Database Record:**
```sql
INSERT INTO adapters (
  adapter_id, name, hash_b3, rank, alpha, tier, category, scope, current_state
) VALUES (
  'adapter-xyz789', 'rust-expert', 'blake3-hash', 16, 32, 'warm', 'trained', 'global', 'cold'
);
```

**Verification:**
```bash
# Check adapter registered
curl http://localhost:8080/v1/adapters \
  -H "Authorization: Bearer $TOKEN" | jq '.adapters[] | select(.name=="rust-expert")'

# Get adapter details
curl http://localhost:8080/v1/adapters/adapter-xyz789 \
  -H "Authorization: Bearer $TOKEN"
```

**Audit Log Generated:**
```json
{
  "action": "training.start",
  "resource": "training_job",
  "resource_id": "train-xyz789",
  "status": "success",
  "user_id": "user@example.com",
  "timestamp": "2025-11-25T10:02:00Z"
}
```

---

## Step 5: Automatic Stack Creation

**On Adapter Registration (Automatic):**

The training pipeline automatically creates a stack with the new adapter:

**Implementation Location:** `crates/adapteros-orchestrator/src/training.rs:592-644`

**Stack Configuration:**
- Name: `stack.default.rust-expert`
- Description: `Auto-created stack for adapter rust-expert`
- Adapter IDs: `["adapter-xyz789"]`
- Workflow type: `sequential`

**Set as Default:**
```bash
# Automatic: Training pipeline calls database.set_default_stack()
# Manual verification:
curl http://localhost:8080/v1/tenants/default/default-stack \
  -H "Authorization: Bearer $TOKEN"
```

**Expected Response:**
```json
{
  "tenant_id": "default",
  "default_stack_id": "stack-abc456",
  "stack_name": "stack.default.rust-expert"
}
```

**Manual Stack Creation (Alternative):**
```bash
curl -X POST http://localhost:8080/v1/adapter-stacks \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tenant_id": "default",
    "name": "my-custom-stack",
    "description": "Custom stack for production",
    "adapter_ids": ["adapter-xyz789"],
    "workflow_type": "sequential"
  }'
```

---

## Step 6: Chat Inference with Stack

**UI Path:** `/inference` → ChatInterface

**Chat Interface Features:**
- ✅ Stack selection dropdown
- ✅ Session management (auto-save)
- ✅ Router activity sidebar (see adapter decisions)
- ✅ Context panel showing current stack

**Example Conversation:**

```
User: Explain Rust ownership
Assistant: [Response uses rust-expert adapter]

Router Decision:
{
  "request_id": "req-123",
  "session_id": "session-456",
  "selected_adapters": ["adapter-xyz789"],
  "gates": [0.95],
  "input_tokens": 5,
  "timestamp": "2025-11-25T10:15:00Z"
}
```

**API:**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain Rust ownership",
    "max_tokens": 200,
    "temperature": 0.7,
    "adapter_stack": {
      "stack_id": "stack-abc456",
      "tenant_id": "default"
    }
  }'
```

**Streaming Inference:**
```bash
curl -X POST http://localhost:8080/v1/infer/stream \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain Rust ownership",
    "max_tokens": 200,
    "stream": true,
    "adapter_stack": {
      "stack_id": "stack-abc456",
      "tenant_id": "default"
    }
  }'
```

**Response (SSE):**
```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Ownership"},...}]}
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":" is"},...}]}
...
data: [DONE]
```

---

## Step 7: Verify Audit Trail

**UI Path:** `/security` → Audit Logs tab

**Query Audit Logs:**
```bash
curl "http://localhost:8080/v1/audit/logs?action=training.start&limit=10" \
  -H "Authorization: Bearer $TOKEN"
```

**Expected Events:**
1. `dataset.upload` → Dataset created
2. `dataset.validate` → Dataset validated
3. `training.start` → Training job started
4. `adapter.register` → Adapter registered (implicit)
5. `stack.create` → Stack created (implicit)
6. `inference.execute` → Chat inference executed

**Full Trail:**
```json
{
  "logs": [
    {
      "id": "audit-001",
      "user_id": "user@example.com",
      "user_role": "admin",
      "tenant_id": "default",
      "action": "dataset.upload",
      "resource_type": "dataset",
      "resource_id": "ds-abc123",
      "status": "success",
      "timestamp": "2025-11-25T10:00:00Z"
    },
    {
      "id": "audit-002",
      "action": "training.start",
      "resource_type": "training_job",
      "resource_id": "train-xyz789",
      "status": "success",
      "timestamp": "2025-11-25T10:02:00Z"
    },
    {
      "id": "audit-003",
      "action": "inference.execute",
      "resource_type": "adapter_stack",
      "resource_id": "stack-abc456",
      "status": "success",
      "timestamp": "2025-11-25T10:15:00Z"
    }
  ],
  "total": 3
}
```

---

## Step 8: Generate Usage Report

**UI Path:** `/reports` → Generate Report

**Implementation Status:**
- ✅ Audit logs collected in database
- ✅ UI pages exist (`/reports`, `/security/compliance`)
- ⚠️ Report generation backend pending full implementation

**Manual Report (SQL):**
```sql
-- Usage summary by user
SELECT
  user_id,
  COUNT(*) as action_count,
  COUNT(DISTINCT resource_type) as resource_types,
  MAX(timestamp) as last_activity
FROM audit_logs
WHERE timestamp >= datetime('now', '-7 days')
GROUP BY user_id;

-- Training job success rate
SELECT
  status,
  COUNT(*) as job_count
FROM training_jobs
GROUP BY status;

-- Dataset upload statistics
SELECT
  DATE(created_at) as date,
  COUNT(*) as datasets_created,
  SUM(total_size_bytes) as total_bytes
FROM training_datasets
GROUP BY DATE(created_at);
```

**Expected Report Metrics:**
- Datasets uploaded: 1
- Training jobs started: 1
- Training job success rate: 100%
- Adapters registered: 1
- Stacks created: 1
- Inference requests: N
- Users active: 1

---

## Verification Checklist

- [ ] Dataset uploaded and visible in `/training/datasets`
- [ ] Dataset validation status = `valid`
- [ ] Training job completed with `status = completed`
- [ ] Adapter registered and visible in `/adapters`
- [ ] Stack auto-created and set as default
- [ ] Chat inference uses correct adapter (check router decisions)
- [ ] Audit logs show all actions
- [ ] No errors in server logs

---

## Troubleshooting

### Dataset Upload Fails

**Symptom:** 413 Payload Too Large
**Solution:** Check file size limits (100MB per file, 500MB total)

```bash
# Check dataset storage
du -sh var/datasets/
```

### Training Job Stuck in Pending

**Symptom:** Job stays in `pending` state
**Solution:** Check deterministic executor and memory pressure

```bash
# Check memory pressure
curl http://localhost:8080/v1/system/memory

# Check logs
tail -f logs/adapteros-server.log | grep "training-job"
```

### Adapter Not Showing in Stack

**Symptom:** Stack created but adapter not loaded
**Solution:** Check adapter registration and lifecycle state

```bash
# Verify adapter registration
curl http://localhost:8080/v1/adapters/adapter-xyz789

# Check lifecycle state
curl http://localhost:8080/v1/adapters/adapter-xyz789/detail
```

### Chat Doesn't Use Adapter

**Symptom:** Chat responses don't reflect training
**Solution:** Verify stack selection and router decisions

```bash
# Check default stack
curl http://localhost:8080/v1/tenants/default/default-stack

# Check router decisions
curl http://localhost:8080/v1/routing/decisions?session_id=session-456
```

---

## Key Implementation Patterns

### Dataset Upload
- **Handler:** `crates/adapteros-server-api/src/handlers/datasets.rs:202-509`
- **Storage:** `var/datasets/{dataset_id}/files/`
- **Audit:** `dataset.upload` action logged

### Training Pipeline
- **Orchestrator:** `crates/adapteros-orchestrator/src/training.rs:153-214`
- **Worker:** `crates/adapteros-lora-worker/src/training/trainer.rs`
- **Deterministic:** Training uses `spawn_deterministic` for reproducibility

### Adapter Registration
- **Automatic:** `crates/adapteros-orchestrator/src/training.rs:549-591`
- **On completion:** Quantize → Package → Register → Create Stack
- **Database:** `adapters` table via `adapteros_db::Db::register_adapter`

### Stack Management
- **Auto-creation:** `crates/adapteros-orchestrator/src/training.rs:592-644`
- **Default stack:** Automatically set for tenant
- **Handler:** `crates/adapteros-server-api/src/handlers/adapter_stacks.rs`

### Chat Integration
- **Frontend:** `ui/src/components/ChatInterface.tsx`
- **Backend:** `crates/adapteros-server-api/src/handlers/streaming_infer.rs`
- **Router:** K-sparse selection with Q15 quantized gates

### Audit Logging
- **Helper:** `crates/adapteros-server-api/src/audit_helper.rs`
- **Actions:** 40+ defined constants (see `audit_helper::actions`)
- **Storage:** `audit_logs` table with immutable records

---

## Next Steps

After completing the golden path:

1. **Multi-Adapter Stacks:** Add multiple adapters to a stack, test routing
2. **Hot-Swap:** Replace adapters in live stack without downtime
3. **Federation:** Sync adapters across multiple nodes
4. **Custom Policies:** Apply tenant-specific policies

---

## References

- [QUICKSTART.md](/Users/mln-dev/Dev/adapter-os/QUICKSTART.md) - System setup
- [docs/TRAINING_PIPELINE.md](/Users/mln-dev/Dev/adapter-os/docs/TRAINING_PIPELINE.md) - Training architecture
- [docs/ARCHITECTURE_PATTERNS.md](/Users/mln-dev/Dev/adapter-os/docs/ARCHITECTURE_PATTERNS.md) - K-sparse routing
- [CLAUDE.md](/Users/mln-dev/Dev/adapter-os/CLAUDE.md) - Developer guide
