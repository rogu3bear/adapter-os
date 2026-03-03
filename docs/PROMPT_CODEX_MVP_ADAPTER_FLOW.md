# Codex Prompt: MVP Adapter Flow — User-Friendly Reframe

**Audience:** Codex-5.3-xh reviewing the codebase for MVP  
**Goal:** Reframe "Create adapter" and "Start training" so non-technical users can succeed  
**Constraint:** Datasets = whatever they upload (PDFs, docs, etc.). No data-science jargon.

---

## 1. User Persona (Non-Goals)

**Target user:** Someone who wants to use an operating system with little to no knowledge of how AI works.

**Not for:** Data scientists, ML engineers, people who know JSONL/CSV/prompt-response pairs.

**Mental model we want:**
- "I have files (PDFs, docs). I want an AI that knows them. I upload, train, then talk to it."
- No need to understand: dataset, manifest, JSONL, CSV column mapping, epochs, rank, alpha, LoRA.

---

## 2. Ideal MVP Flow (Target State)

```
Create Adapter
    │
    ├─► Step 1: "What should it know?"
    │       • Upload files (PDF, TXT, MD) — drag & drop or pick
    │       • OR pick from documents you already uploaded
    │       • One clear action: "Add your files"
    │
    ├─► Step 2: "Name it"
    │       • Short name (e.g. "support-docs", "my-handbook")
    │       • Optional: one-line purpose
    │
    ├─► Step 3: "Start training"
    │       • One button. Sensible defaults. No knobs.
    │       • Progress: "Training…" with simple status
    │
    └─► Done: "Talk to it"
            • CTA: "Open Chat" or "Try it"
            • Adapter appears in chat / adapters list
```

**Key principle:** The word "dataset" never appears. "Training" appears once, as a verb ("Start training"), not a noun. "Adapter" or "skill" is the artifact.

---

## 3. Current Pain Points (What to Fix)

### 3.1 Fragmented Upload Paths

**Current:**
- "Upload Dataset" → DatasetUploadWizard (JSONL, CSV, text + column mapping, TextStrategy, idempotency)
- "Generate from Document" → GenerateDatasetWizard (strategy, chunk_size, max_tokens, target_volume, seed_prompts)
- Documents tab → DocumentUploadDialog (PDF, TXT, MD) → then "Create dataset from document"
- Training Data tab → Documents vs Datasets vs Preprocessed (three sources)

**Problem:** User doesn't know which path to take. "Upload Dataset" sounds like the right thing, but it expects JSONL/CSV. "Generate from Document" sounds like a different feature. Documents live in a separate place.

**Fix direction:** One primary path: "Add your files" (PDF, TXT, MD, etc.). Backend already supports `upload_document` + `create_dataset_from_documents`. Use that. Hide JSONL/CSV upload behind "Advanced" or a separate power-user flow.

### 3.2 Wizard Steps Are Data-Scientist Oriented

**Current CreateJobWizard steps:**
1. Intro — Skill name, purpose ✓ (fine)
2. Dataset — "How should this skill learn?" → Upload Dataset (JSONL/CSV/text) | Generate from Document | Dataset ID
3. Model — Base model, category
4. Config — Epochs, learning rate, batch size, rank, alpha, presets
5. Review — Summary

**Problems:**
- "Dataset" step exposes format choices (JSONL, CSV, text) and "Generate from Document" as a sibling — confusing.
- Config step has many knobs; non-technical users will not tune them.
- "Dataset ID" as an option is power-user only.

**Fix direction:**
- Merge "Add files" into a single, format-agnostic upload. Detect PDF/TXT/MD → document path. Detect JSONL/CSV → advanced path (or auto-detect and handle).
- Config: One preset ("Balanced" or "Default"). Advanced users get "Show advanced" to reveal epochs, etc.
- Remove "Dataset ID" from primary flow; keep for power users in a collapsible section.

### 3.3 DatasetUploadWizard Complexity

**Current:** `dataset_wizard.rs` — UploadMode (ManifestJsonl, Csv, Text), TextStrategy (Echo, PairAdjacent), CSV column mapping (input_col, target_col, weight_col), idempotency key, preview rows, parse errors.

**For MVP:** This wizard is the wrong default. The primary path should be:
- Document upload (PDF, TXT, MD) → `create_dataset_from_documents`
- OR directory/folder upload (if backend supports) → same

**Recommendation:** 
- Introduce a **simple upload path** that accepts PDF/TXT/MD and calls `upload_document` + `create_dataset_from_documents`. Reuse `DocumentUploadDialog` patterns (drag-drop, progress).
- Keep DatasetUploadWizard for "I have JSONL/CSV" as a secondary, clearly labeled "Advanced" or "I have structured training data" option.

### 3.4 Terminology Overload

**Current terms:** Dataset, dataset version, manifest, JSONL, CSV, samples, prompt/response, target column, weight column, idempotency, preprocessed, CoreML, rank, alpha, epochs, validation split, early stopping.

**For MVP:** Use plain language:
- "Your files" or "Knowledge" instead of "dataset"
- "Examples" instead of "samples" when needed
- "Training" as a single action, not a configuration surface
- Hide: manifest, JSONL, CSV mapping, idempotency, preprocessed, CoreML (unless user explicitly needs it)

---

## 4. Reframe: "Create Adapter" as the Primary CTA

**Current:** Training page has "Create job" or similar. Chat empty state has "Teach New Skill". Multiple entry points.

**Target:** One clear CTA everywhere: **"Create Adapter"** (or "Create Skill" if that resonates better).

**Flow:**
1. User clicks "Create Adapter"
2. Modal or page: "What should it know?" → Upload files (PDF, TXT, MD) or pick existing documents
3. "Name it" → Adapter name + optional purpose
4. "Start training" → One button, progress indicator
5. Success: "Adapter ready. [Open Chat] [View Adapters]"

---

## 5. Upload Reframe: "Add Your Files"

**Current:** "Upload Dataset" implies a specific format. "Generate from Document" implies a different flow.

**Target:** "Add your files" — one drop zone. Accepted: PDF, TXT, MD, Markdown. Backend:
- `upload_document` for each file
- `create_dataset_from_documents` with the document IDs

**If user has JSONL/CSV:** Secondary option: "I have structured data (JSONL/CSV)" → opens current DatasetUploadWizard. Label it clearly as advanced.

**If user has a folder:** Future enhancement: "Upload folder" → batch upload documents, then create dataset. For MVP, multi-file select in the same dialog may suffice.

---

## 6. "Start Training" Reframe

**Current:** Config step with epochs, learning rate, batch size, rank, alpha, presets, backend, etc.

**Target:** 
- Primary: "Start training" — one button. Use a single preset (e.g. "Balanced" or "Default").
- Secondary: "Advanced options" expandable — reveal epochs, learning rate, etc. for power users.

**Progress:** Simple status: "Training… X% complete" or "Training… this may take a few minutes." No need to expose phases (preprocess, split, training_loop) unless user asks.

---

## 7. Implementation Anchors (Where to Change)

| Area | File(s) | Change |
|------|---------|--------|
| Create adapter flow | `wizard.rs` | Simplify steps; add "Add your files" as primary dataset path |
| Dataset step | `wizard.rs` (DatasetStepContent, DatasetChooseView) | Replace "Upload Dataset" / "Generate from Document" with "Add your files" + "Use existing document" |
| Upload UI | `dataset_wizard.rs`, `upload_dialog.rs`, `data/mod.rs` | Add simple document-based path; demote JSONL/CSV to advanced |
| Document upload | `upload_dialog.rs` | Reuse for "Add your files" in wizard; supports PDF, TXT, MD |
| API | `client.rs` | `upload_document`, `create_dataset_from_documents` — already exist |
| Config step | `wizard.rs` (ConfigStepContent) | Collapse to preset + "Advanced" |
| CTAs | `chat.rs`, `adapters.rs`, `training/mod.rs` | Standardize on "Create Adapter" |

---

## 8. Acceptance Criteria for MVP

1. **Non-technical user** can: upload PDFs → name adapter → click "Start training" → get adapter → talk to it in chat. No JSONL, CSV, or dataset ID.
2. **"Create Adapter"** is the primary CTA; flow has ≤4 steps.
3. **"Add your files"** accepts PDF, TXT, MD; no format selection in primary path.
4. **"Start training"** is one button with sensible defaults; advanced options hidden.
5. **Dataset** as a concept is hidden in the primary flow; "Your files" or "Knowledge" used instead.

---

## 9. Out of Scope for MVP

- Directory/folder upload (multi-file select is enough)
- JSONL/CSV in primary flow (keep as advanced)
- Config presets beyond one default
- Preprocessed cache visibility
- CoreML export controls

---

## 10. Prompt Summary for Codex

When reviewing or implementing:

1. **Prioritize the document path:** PDF/TXT/MD upload → `create_dataset_from_documents` → train. This is the happy path.
2. **Collapse the wizard:** Fewer steps, less jargon. "Add your files" → "Name it" → "Start training" → Done.
3. **Hide power-user options:** JSONL/CSV, dataset ID, config knobs — behind "Advanced" or a separate entry point.
4. **Unify terminology:** "Create Adapter", "Your files", "Start training". Avoid "dataset", "manifest", "samples" in primary UI.
5. **Reuse existing backend:** `upload_document`, `create_dataset_from_documents` already work. Don't add new endpoints for MVP.
6. **Mutate and consolidate, never recreate and delete:** Refactor in place. Merge duplicate components and flows into existing code. Do not create new files and delete old ones — edit, don't replace.

---

## 11. Backend Logic to Consider

The MVP flow touches several backend paths. These should be understood before UI changes:

| Area | Location | What matters for MVP |
|------|----------|----------------------|
| **Document → dataset timing** | `training_dataset.rs` `create_from_document_ids` | Documents must be **indexed** before `create_dataset_from_documents`. Chunking/indexing runs async after `upload_document`. UI must wait or poll. |
| **Single-call upload path** | `handlers/training_datasets.rs` `create_training_dataset_from_upload` | `POST /v1/training/datasets/from-upload` uploads a file and creates a dataset in one call (upload → process → dataset). Supports `training_strategy`: `text`, `qa`, `synthesis`. **No separate wait for indexing.** Gated by `#[cfg(feature = "embeddings")]`. |
| **Adapter-from-dataset shortcut** | `handlers/training_datasets.rs` `create_adapter_from_dataset` | `POST /v1/adapters/from-dataset/{dataset_id}` starts training directly from a dataset. Simpler than `create_training_job` — fewer params. |
| **Trust/validation gating** | `handlers/training.rs` `start_training_from_dataset` | `trust_state` (blocked, needs_approval) and `validation_status` can block training. Datasets from documents are typically `valid`; trust may need approval for first-time users. |
| **Capacity/memory gates** | `services/training_service.rs` `can_start_training` | Max concurrent jobs, memory pressure can block. Error messages surface to UI. |
| **Document upload** | `handlers/documents.rs` `upload_document` | PDF, TXT, MD. Returns `document_id`. Indexing is async — status moves `uploaded` → `processing` → `indexed`. |
| **Create dataset from docs** | `handlers/datasets/from_documents.rs` | `POST /v1/datasets/from-documents` with `document_ids` or `document_id`. Requires indexed docs. |
| **Create training job** | `handlers/training.rs` `create_training_job` | `POST /v1/training/jobs` — full params: dataset_id, base_model_id, adapter_name, config. |
| **Permissions** | `permissions.rs` | `DatasetUpload` (upload, create dataset), `TrainingStart` (start job). |

**Recommendation:** For MVP, prefer `create_training_dataset_from_upload` when the embeddings feature is enabled — it avoids the upload → wait-for-indexed → create_dataset sequence. If not, the UI must handle polling document status before calling `create_dataset_from_documents`. `create_adapter_from_dataset` is a good target for a "Start training" button when the user already has a dataset.

---

## Appendix: 10 Other Places to Consider

When implementing the MVP reframe, these areas should be reviewed or updated for consistency:

| # | Location | What to consider |
|---|----------|------------------|
| 1 | **`search/contextual.rs`** | Contextual actions: "Train adapter from this document", "Train new adapter", "Start new training job", "Upload dataset". Align with "Create Adapter" terminology. The `Execute("upload-document")` and `Execute("open-dataset-upload")` commands are **not implemented** in `command_palette.rs` — they fall through to "Unhandled command". Either implement them or remove from contextual actions. |
| 2 | **`components/command_palette.rs`** | `execute_command()` does not handle `upload-document` or `open-dataset-upload`. Add handlers that open the appropriate dialog/wizard, or remove these from contextual actions. |
| 3 | **`components/layout/nav_registry.rs`** | Nav labels: "Adapter Training", "Datasets" (keywords: "training, data, upload, versions, jsonl"). Consider renaming "Adapter Training" → "Create Adapter" or keeping "Train" but ensuring the primary CTA is clear. Datasets nav item exposes "jsonl" — consider softer keywords for non-technical users. |
| 4 | **`pages/dashboard.rs`** | Primary CTA: "Teach New Skill" → `/training?open_wizard=1`. Align with "Create Adapter" if that becomes the canonical label. |
| 5 | **`pages/chat.rs`** | Empty state: "Teach New Skill" CTA. Same alignment as dashboard. |
| 6 | **`pages/adapters.rs`** | `NEW_ADAPTER_PATH` = `/training?open_wizard=1`. Empty state and primary action likely say "Create" or "New adapter". Ensure consistency with "Create Adapter". |
| 7 | **`pages/training/data/source_nav.rs`** | Labels: "Upload Document" vs "Upload Dataset". For Datasets, clicking upload navigates to wizard. Consider: when user clicks "Upload" on Datasets, should it open document upload (primary path) or the JSONL/CSV wizard (advanced)? |
| 8 | **`pages/training/data/state.rs`** | `DataSource` labels: "Documents", "Datasets", "Preprocessed". For MVP, consider whether "Documents" and "Datasets" should be merged or relabeled (e.g. "Your files" vs "Training data") to reduce cognitive load. |
| 9 | **`pages/datasets.rs`** | Heavy dataset-centric flow: "Train Adapter", "Train an adapter from this dataset", trainability gating, training config panel. This page is power-user oriented. For MVP, ensure the primary path (Create Adapter wizard) doesn't require visiting /datasets. Datasets page can remain for advanced users. |
| 10 | **`pages/welcome.rs`** | First-run wizard (Database, Worker, Models, Ready). After "Start Using AdapterOS", consider adding a CTA: "Create your first adapter" that opens the simplified wizard. |
