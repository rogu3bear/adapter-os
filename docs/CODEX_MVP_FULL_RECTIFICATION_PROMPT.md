# Codex Full Rectification Prompt — Production UI for Non-Technical Users

**Model:** Codex-5.3-xh  
**Mode:** Autonomous, long-running (hours)  
**Scope:** `crates/adapteros-ui` + minimal adjacent backend touchpoints  
**Outcome:** Production-ready UI that a non-technical user can use to create adapters and chat with them. No jargon. Consolidated. Friendly. Advanced features deferred.

---

## Launch Prompt (Paste This to Start)

```
Execute the full rectification described in docs/CODEX_MVP_FULL_RECTIFICATION_PROMPT.md.

Work through phases 1–7 sequentially. After each phase, run the verification commands. Fix any failures before proceeding. Do not skip phases. Do not use the word "demo" anywhere. The outcome is a production-ready UI.

Mutate and consolidate, never recreate and delete: refactor in place, merge duplicates into existing code. Do not create new files and delete old ones.

You have full autonomy to edit crates/adapteros-ui and make the changes described. When in doubt, prefer consolidation over addition, plain language over jargon, and the primary path over advanced options.
```

---

## Prohibited Language

**Never use these words in UI copy, comments, or docs you add:**

- "demo" — use "production", "release", or "ready" instead
- "MVP" in user-facing strings — use "Create Adapter", "Your files", etc.
- "dataset" in primary flow — use "your files" or "knowledge"
- "manifest", "JSONL", "CSV" in primary flow — only in Advanced sections
- "samples" — use "examples" if needed
- "training job" — use "training" or "adapter"

---

## Mission

Refactor the AdapterOS Leptos UI so that someone with **no AI or ML knowledge** can:

1. Upload their files (PDFs, docs)
2. Create an adapter from those files
3. Talk to it in chat

The result must feel like a **production application** — polished, consistent, easy to understand. Do not use the word "demo" anywhere; this is the production experience for non-technical users.

---

## Autonomy Grant

You are authorized to:

- Edit any file in `crates/adapteros-ui` and related UI touchpoints
- Add new components, hooks, or pages when they reduce complexity
- Remove or consolidate duplicate UI surfaces
- Change copy, labels, and terminology across the UI
- Refactor wizard flows, navigation, and CTAs
- Add cached hooks to deduplicate API calls
- Touch `crates/adapteros-server-api` only when required for a UI contract (e.g. request/response shape); prefer UI-side adaptation

You must NOT:

- Change RBAC, auth, or permission logic
- Add new backend endpoints unless the UI contract cannot be satisfied otherwise
- Modify determinism, seeding, or core inference logic
- Run `cargo clean` or full-workspace tests without explicit checkpoint approval
- Edit files outside the scope of a phase without documenting why

---

## Operating Principles

1. **Existing code first** — Reuse `DocumentUploadDialog`, `use_system_status`, `create_dataset_from_documents`, etc. Do not reinvent.
2. **Mutate and consolidate, never recreate and delete** — Refactor in place. Merge duplicate components, hooks, and flows into existing files. Do not create new files/components and then delete the old ones. Edit, don't replace. This avoids duplication and preserves git history.
3. **Minimal diffs** — Prefer small, incremental changes. One logical change per commit when possible.
4. **Friendly over clever** — Plain language. "Your files" not "dataset". "Create Adapter" not "Create job". "Start training" not "Launch training pipeline".
5. **Consolidate, don't multiply** — Merge duplicate surfaces. One status bar, one health source, one primary flow.
6. **Advanced later** — Power-user options (JSONL, CSV, epochs, rank, dataset ID) go behind "Advanced" or a collapsible section. Primary path has zero knobs.
7. **Verify incrementally** — After each phase, run `cargo check -p adapteros-ui --target wasm32-unknown-unknown` and `./scripts/build-ui.sh`. Fix before proceeding.

---

## Phase 0: Pre-Flight

Before starting Phase 1:

1. Run `cargo check -p adapteros-ui --target wasm32-unknown-unknown` — must pass
2. Run `./scripts/build-ui.sh` — must succeed
3. Confirm `docs/PROMPT_CODEX_MVP_ADAPTER_FLOW.md` exists for reference
4. Confirm `AGENTS.md` and `CLAUDE.md` are available for repo invariants

If any fail: fix the environment first. Do not begin Phase 1 with a broken build.

---

## Phase 1: Adapter Creation Flow (Primary Path)

**Goal:** Non-technical user can upload PDFs → name adapter → start training → get adapter → talk to it.

### 1.1 Target Flow

```
Create Adapter (CTA everywhere)
    │
    ├─► Step 1: "What should it know?"
    │       • "Add your files" — drag & drop PDF, TXT, MD
    │       • OR "Pick from documents you've already uploaded"
    │       • No format selection. No JSONL/CSV in primary path.
    │
    ├─► Step 2: "Name it"
    │       • Adapter name (required)
    │       • Purpose (optional, one line)
    │
    ├─► Step 3: "Start training"
    │       • One button. One preset (Balanced/Default).
    │       • Progress: "Training…" with simple status.
    │       • No epochs, learning rate, batch size, rank, alpha in primary view.
    │
    └─► Done
            • "Adapter ready. [Open Chat] [View Adapters]"
```

### 1.2 Implementation Tasks

| Task | File(s) | Action |
|------|---------|--------|
| 1.2.1 | `wizard.rs` | Collapse to ≤4 steps. Rename steps: "Knowledge" (not "Dataset"), "Name", "Train", "Confirm". |
| 1.2.2 | `wizard.rs` DatasetStepContent | Replace "Upload Dataset" / "Generate from Document" with "Add your files" (document upload) as primary. Add "Use existing document" as secondary. |
| 1.2.3 | `wizard.rs` | Add document-based path: when user adds files, call `upload_document` per file, then `create_dataset_from_documents` when indexed. If `create_training_dataset_from_upload` is available (embeddings feature), prefer it — single call, no wait. |
| 1.2.4 | `wizard.rs` ConfigStepContent | Collapse to one preset. Add "Advanced" expandable for epochs, learning rate, etc. |
| 1.2.5 | `dataset_wizard.rs` | Keep for power users. Add entry point: "I have structured data (JSONL/CSV)" — opens current wizard. Do not show in primary flow. |
| 1.2.6 | `upload_dialog.rs`, `data/mod.rs` | Reuse DocumentUploadDialog for "Add your files". Supports PDF, TXT, MD. Multi-file select if not already present. |
| 1.2.7 | Document indexing wait | If using `upload_document` + `create_dataset_from_documents`: poll document status until "indexed", then call create. Show "Preparing your files…" during wait. |

### 1.3 Terminology Changes

| Old | New |
|-----|-----|
| Dataset | Your files / Knowledge |
| Upload Dataset | Add your files |
| Generate from Document | (merge into "Add your files" or "Use existing document") |
| Create job | Create Adapter |
| Teach New Skill | Create Adapter |
| Training Examples | Your files |
| Dataset ID | (hide in Advanced) |
| Samples | Examples (if needed at all) |

### 1.4 Verification

- [ ] `cargo check -p adapteros-ui --target wasm32-unknown-unknown` passes
- [ ] `./scripts/build-ui.sh` succeeds
- [ ] Manual: Open wizard, add PDF, name adapter, start training — no JSONL/CSV/format selection in path
- [ ] Manual: "Create Adapter" appears as CTA on dashboard, chat empty state, adapters page

---

## Phase 2: CTA and Entry Point Unification

**Goal:** One canonical CTA ("Create Adapter") and one canonical path (`/training?open_wizard=1`).

### 2.1 Tasks

| Task | File(s) | Action |
|------|---------|--------|
| 2.1.1 | `dashboard.rs` | Change "Teach New Skill" → "Create Adapter". Link: `/training?open_wizard=1`. |
| 2.1.2 | `chat.rs` | Empty state: "Teach New Skill" → "Create Adapter". Same link. |
| 2.1.3 | `adapters.rs` | Ensure primary CTA is "Create Adapter". `NEW_ADAPTER_PATH` = `/training?open_wizard=1`. |
| 2.1.4 | `training/mod.rs` | Page title/primary action: "Create Adapter" not "Create job" or "New job". |
| 2.1.5 | `training/components.rs` | TrainingJobList empty state: "Create Adapter" not "New Job". |
| 2.1.6 | `nav_registry.rs` | Consider: "Adapter Training" → "Create Adapter" or "Train" with clear primary CTA. Datasets nav: remove "jsonl" from keywords; use "files", "knowledge". |

### 2.2 Verification

- [ ] All entry points show "Create Adapter"
- [ ] All link to same wizard flow

---

## Phase 3: Consolidate System/Status Surfaces

**Goal:** One source of truth for system status. No duplicate health fetches. Fewer overlapping UIs.

### 3.1 Current Duplication (from UI_OVERBUILT_AUDIT)

- **LogicalControlRail** — shell, every page
- **StatusCenterProvider** — Ctrl+Shift+S panel
- **InferenceBanner** — banner when inference not ready
- **System page** — full diagnostics
- **SystemTray** — compact status

Plus: System page and Monitoring page both fetch `/healthz`, `/readyz`, `/healthz/all`, `/system/ready` with separate uncached `use_api_resource` calls.

### 3.2 Tasks

| Task | File(s) | Action |
|------|---------|--------|
| 3.2.1 | `hooks/mod.rs` | Add `use_health_endpoints()` — cached hook returning healthz, readyz, healthz/all, system/ready. Cache key: `health_endpoints`. TTL: STATUS. |
| 3.2.2 | `pages/system/mod.rs` | Replace 5 separate `use_api_resource` calls with `use_health_endpoints()`. |
| 3.2.3 | `pages/monitoring.rs` | Replace 5 separate `use_api_resource` calls with `use_health_endpoints()`. |
| 3.2.4 | `hooks/mod.rs` | Add `use_health()` — cached hook for `client.health()` (`/healthz`). Used for connectivity checks. |
| 3.2.5 | `system_tray.rs`, `offline_banner.rs`, `settings/system_info.rs`, `api_config.rs` | Replace uncached `client.health()` / `use_api_resource(health)` with `use_health()`. |
| 3.2.6 | Status surfaces | Per UI_OVERBUILT_AUDIT: Pick 1–2 primary surfaces. Recommendation: **InferenceBanner** + **System page**. Fold LogicalControlRail into InferenceBanner or a slim status bar. StatusCenter can remain for power users. SystemTray stays for compact always-visible status. Document the choice; do not remove all five without a clear replacement. |

### 3.3 Verification

- [ ] System page and Monitoring page share cached health data (no duplicate fetches)
- [ ] `use_health()` used for connectivity checks
- [ ] Status surfaces reduced or clearly documented

---

## Phase 4: Command Palette and Contextual Actions

**Goal:** All contextual actions work. No dead commands.

### 4.1 Current Gaps

- `Execute("upload-document")` — not implemented in `command_palette.rs`
- `Execute("open-dataset-upload")` — not implemented
- Contextual actions use "Train adapter", "Upload dataset" — align with "Create Adapter", "Add your files"

### 4.2 Tasks

| Task | File(s) | Action |
|------|---------|--------|
| 4.2.1 | `command_palette.rs` | Implement `upload-document`: open DocumentUploadDialog or navigate to training with upload. Implement `open-dataset-upload`: open Create Adapter wizard. Or remove these from contextual actions if not feasible. |
| 4.2.2 | `search/contextual.rs` | Align actions: "Create Adapter" (not "Train new adapter"), "Add your files" (not "Upload dataset"). Ensure actions point to correct flows. |

### 4.3 Verification

- [ ] No "Unhandled command" for upload-document or open-dataset-upload (either implemented or removed)
- [ ] Contextual actions use consistent terminology

---

## Phase 5: Training Data Tab and Datasets Page

**Goal:** Primary path does not require visiting /datasets or Training Data tab. Those remain for power users.

### 5.1 Tasks

| Task | File(s) | Action |
|------|---------|--------|
| 5.1.1 | `training/data/source_nav.rs` | When user clicks "Upload" on Datasets: open document upload (primary) or wizard. Label: "Add files" not "Upload Dataset" for Documents. |
| 5.1.2 | `training/data/state.rs` | Consider relabeling: "Documents" → "Your files", "Datasets" → "Training data". Optional; only if it reduces confusion. |
| 5.1.3 | `pages/datasets.rs` | Keep as power-user page. Ensure "Create Adapter" wizard is the primary path; /datasets is not required for happy path. |
| 5.1.4 | `pages/welcome.rs` | After "Start Using AdapterOS", add CTA: "Create your first adapter" → opens wizard. |

### 5.2 Verification

- [ ] Happy path does not require /datasets
- [ ] Welcome flow guides to first adapter

---

## Phase 6: Polish and Consistency

**Goal:** UI feels cohesive. Copy is friendly. No leftover jargon.

### 6.1 Tasks

| Task | Scope | Action |
|------|-------|--------|
| 6.1.1 | Global | Audit all user-facing strings for: dataset, manifest, JSONL, CSV, samples, epochs, rank, alpha, idempotency. Replace or hide. |
| 6.1.2 | Error messages | Ensure training/upload errors use plain language. "Your files aren't ready yet" not "Document must be indexed". |
| 6.1.3 | Empty states | Consistent tone. "Add your files to get started" not "No dataset selected". |
| 6.1.4 | Loading states | "Preparing…", "Training…", "Almost ready…" — friendly, not technical. |

### 6.2 Verification

- [ ] Grep for "dataset", "JSONL", "CSV" in user-facing strings — only in Advanced sections or power-user pages
- [ ] Error messages are human-readable

---

## Phase 7: Backend Integration Check

**Goal:** UI uses the right backend paths. No unnecessary waits.

### 7.1 Decision Tree

```
Is embeddings feature enabled?
├─ YES → Prefer create_training_dataset_from_upload (POST /v1/training/datasets/from-upload)
│        Single call: upload + process + dataset. No indexing wait.
│
└─ NO  → Use upload_document + create_dataset_from_documents
         Must poll document status until "indexed" before create_dataset_from_documents.
         Show "Preparing your files…" during wait.
```

### 7.2 Tasks

| Task | Action |
|------|--------|
| 7.2.1 | Check if `create_training_dataset_from_upload` is available. If yes, wire "Add your files" to it. If no, use document path with polling. |
| 7.2.2 | "Start training" button: use `create_adapter_from_dataset` (POST /v1/adapters/from-dataset/{id}) when user has dataset. Simpler than create_training_job. |
| 7.2.3 | Handle trust/validation blocks: show clear message. "Your files need a quick review before training." Do not expose trust_state, validation_status. |
| 7.2.4 | Handle capacity/memory blocks: show "Training is busy. Try again in a few minutes." |

### 7.3 Verification

- [ ] Happy path works with embeddings and without
- [ ] Blocking conditions have friendly messages

---

## Checkpoint Protocol

After each phase:

1. Run `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
2. Run `./scripts/build-ui.sh`
3. If both pass: proceed to next phase
4. If either fails: fix before proceeding. Do not accumulate debt.
5. Optionally: `AOS_DEV_NO_AUTH=1 ./start` and manually verify the flow.

---

## Completion Criteria

The rectification is complete when:

1. **Non-technical user** can: upload PDFs → name adapter → start training → get adapter → talk to it. No JSONL, CSV, dataset ID, or config knobs in primary path.
2. **"Create Adapter"** is the sole CTA for this flow. All entry points converge.
3. **"Add your files"** accepts PDF, TXT, MD. No format selection.
4. **"Start training"** is one button. Advanced options hidden.
5. **Health/status** is consolidated. No duplicate uncached fetches for healthz, readyz, etc.
6. **Command palette** has no dead commands for upload/dataset actions.
7. **Copy** is friendly. No jargon in primary UI.
8. **Build** passes. UI loads. Manual smoke test succeeds.

---

## Out of Scope (Defer to Advanced Mode)

- JSONL/CSV upload in primary flow
- Dataset ID manual entry in primary flow
- Config presets beyond one default
- Preprocessed cache visibility
- CoreML export controls
- Directory/folder batch upload
- Multiple training presets in primary view
- Exposing epochs, learning rate, batch size, rank, alpha in primary view

---

## File Inventory (Quick Reference)

| Area | Key Files |
|------|-----------|
| Wizard | `pages/training/wizard.rs` |
| Dataset wizard | `pages/training/dataset_wizard.rs` |
| Generate wizard | `pages/training/generate_wizard.rs` |
| Document upload | `pages/training/data/upload_dialog.rs`, `pages/documents.rs` |
| Training data | `pages/training/data/mod.rs`, `source_nav.rs`, `state.rs` |
| CTAs | `dashboard.rs`, `chat.rs`, `adapters.rs`, `training/mod.rs` |
| Hooks | `hooks/mod.rs` |
| System/health | `pages/system/mod.rs`, `pages/monitoring.rs`, `system_tray.rs`, `logical_rail.rs` |
| Command palette | `components/command_palette.rs` |
| Contextual actions | `search/contextual.rs` |
| Nav | `components/layout/nav_registry.rs` |
| API client | `api/client.rs` |

---

## Recovery (If Build Breaks Mid-Phase)

If `cargo check` or `./scripts/build-ui.sh` fails during a phase:

1. Read the error output. Identify the file and line.
2. If the error is in a file you just edited: revert the problematic change or fix the syntax/type error.
3. If the error is in a file you did not edit: you may have introduced an incompatible change. Check imports, prop types, and call sites.
4. Run `cargo fmt -p adapteros-ui` and `cargo clippy -p adapteros-ui -- -D warnings` to catch style/quality issues.
5. Do not proceed to the next phase until the current phase's verification passes.

---

## When to Stop

**Stop and report** (do not proceed) if:

- Backend endpoint required for the flow does not exist and cannot be added within scope
- RBAC or auth change is required to unblock the UI (per AGENTS.md: do not change auth to unblock UI)
- A phase verification fails repeatedly after multiple fix attempts and the root cause is outside `crates/adapteros-ui`
- You discover a blocking dependency (e.g. embeddings feature always disabled and no fallback path)

**Continue** (do not stop) if:

- A verification fails — fix it and re-run
- You find an adjacent issue — document it, then continue with the phase
- A file has more complexity than expected — refactor incrementally

---

## When to Escalate

Create a brief report (in a comment or separate note) when:

- You defer a task to "Advanced" but the current UI still exposes it somewhere
- Two valid backend paths exist and the choice is ambiguous (e.g. embeddings on/off)
- A consolidation would remove functionality that another page depends on

Do not block the phase on escalation. Document and proceed with the recommended choice.

---

## Final Note

Work through phases sequentially. Do not skip verification. If a phase reveals a dependency on an earlier phase, backtrack and fix. The goal is a production-ready UI that feels friendly and consolidated — not a quick patch. Take the time to do it right.
