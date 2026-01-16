# PLAN_STATUS

## Decisions
- Bootstrap: wrapper script scripts/dev-up.sh (locked)
- Model default: smallest available (0.5B) for first success (locked)
- Dataset contract: PLAN_4.md supervised + raw_continuation_v1 (locked)

## Coordination Index
Cycle: 3
Snapshot: 2026-01-15 22:43:16 CST
Plans last updated: PLAN.md 2026-01-15 22:34:43; PLAN_2.md 2026-01-15 22:16:53; PLAN_3.md 2026-01-15 22:23:07; PLAN_4.md 2026-01-15 22:42:52
Changed files since last cycle: 157 tracked diffs across core/lora/worker, server-api, ui, tests/benchmarks, scripts, migrations, docs; untracked additions include PLAN.md, PLAN_2.md, PLAN_3.md, PLAN_4.md, scripts/dev-up.sh, scripts/golden_path_adapter_chat.sh, scripts/make_minimal_dataset.py, scripts/start_minimal_training.sh, scripts/upload_minimal_dataset.sh, docs/EXECUTION_CONTRACT.md, tests/hydration_gating_test.rs, and crates/adapteros-ui/src/pages/training/data/*.

## Work Map
| Area | Owner Agent | Plan reference | Current status | Evidence link |
| --- | --- | --- | --- | --- |
| Boot/UI | Agent 3 | PLAN.md Checklist items 5-8; PLAN_3.md "Golden Path Command" + "Truth Surface UI" | Done-Needs-Proof (reassigned from Agent 1; proof pending) | scripts/dev-up.sh; QUICKSTART.md |
| Dataset | Agent 4 | PLAN_2.md "Dataset" | Not started (reassigned from Agent 2; prior owner unresponsive) | N/A |
| Training | Agent 4 | PLAN_2.md "Training (MLX)" | Not started (reassigned from Agent 2; prior owner unresponsive) | N/A |
| Adapter | Agent 1 | PLAN_2.md "Adapter Registration / Discovery" | Not started (reassigned from Agent 3; prior owner unresponsive) | N/A |
| Hydration | Agent 1 | PLAN_2.md "Hydration" | Not started (reassigned from Agent 3; prior owner unresponsive) | N/A |
| Chat | Agent 1 | PLAN_2.md "Chat / Inference / Receipt" | Not started (reassigned from Agent 3; prior owner unresponsive) | N/A |
| Tests | Agent 2 | PLAN_2.md test bullets; PLAN_3.md "CI Regression Gate" | In progress (reassigned from Agent 4; test/bench changes unmapped) | tests/benchmark/*; crates/adapteros-server-api/tests/* |
| Docs | Agent 2 | PLAN.md Checklist item 7; PLAN_2.md "Documentation"; PLAN_3.md "Short Runbook" | In progress (reassigned from Agent 4; Quickstart claim needs proof, runbook missing) | QUICKSTART.md |

## Unresponsive Agents
- Agent 1: no acknowledgement this cycle; previous owner Boot/UI; reassigned Boot/UI to Agent 3.
- Agent 2: no acknowledgement this cycle; previous owner Dataset/Training; reassigned Dataset/Training to Agent 4.
- Agent 3: no acknowledgement this cycle; previous owner Adapter/Hydration/Chat; reassigned Adapter/Hydration/Chat to Agent 1.
- Agent 4: no acknowledgement this cycle; previous owner Tests/Docs; reassigned Tests/Docs to Agent 2.

## Done Claims (Proof Required)
- PLAN.md Checklist item 1: Baseline repository + guidance review. Status: Done-Needs-Proof.
  Evidence Required: `ls` -> output includes `AGENTS.md`, `start`, `scripts/`, `crates/`, `configs/`; `cat AGENTS.md` -> output includes `## Project Structure & Module Organization`.
- PLAN.md Checklist item 2: UI asset gap confirmed. Status: Done-Needs-Proof.
  Evidence Required: `ls static` -> `No such file or directory`; `sed -n '1,200p' crates/adapteros-server/src/assets.rs` -> includes `UI not built. Run: cd crates/adapteros-ui && trunk build --release`; `sed -n '700,820p' scripts/service-manager.sh` -> includes `UI is served by the backend from static/`.
- PLAN.md Checklist item 3: UI chat blank states confirmed. Status: Done-Needs-Proof.
  Evidence Required: `sed -n '1,160p' crates/adapteros-ui/src/components/layout.rs` -> includes `ChatDockPanel`; `sed -n '240,520p' crates/adapteros-ui/src/components/chat_dock.rs` -> includes `Start a conversation`; `sed -n '220,520p' crates/adapteros-ui/src/pages/chat.rs` -> includes `No messages yet. Start the conversation!`.
- PLAN.md Checklist item 4: Agent scope captured. Status: Done-Needs-Proof.
  Evidence Required: `sed -n '45,80p' AGENTS.md` -> includes `## Agent Role & Scope (Alignment)`.
- PLAN.md Checklist item 5: Choose bootstrap strategy for the single-command dev boot (Option 2: scripts/dev-up.sh). Status: Done-Needs-Proof.
  Evidence Required: `ls -l scripts/dev-up.sh` -> output shows file present; `sed -n '1,120p' scripts/dev-up.sh` -> includes bootstrap entrypoint.
- PLAN.md Checklist item 6: Implement bootstrap flow for server binary, migrations, and UI assets. Status: Done-Needs-Proof.
  Evidence Required: `sed -n '1,200p' scripts/dev-up.sh` -> includes build steps (cargo build) and UI build/asset checks.
- PLAN.md Checklist item 7: Update docs to name the single command and describe readiness. Status: Done-Needs-Proof.
  Evidence Required: `sed -n '70,130p' QUICKSTART.md` -> includes `./scripts/dev-up.sh`.
- PLAN.md Checklist item 8: Run the documented command and capture evidence. Status: Done-Needs-Proof.
  Evidence Required: `./scripts/dev-up.sh 2>&1 | tee /tmp/dev-up-run.log` -> includes `/healthz` 200 and `/readyz` 200; `curl -sS http://127.0.0.1:8080/ | sed -n '1,30p'` -> includes `<!DOCTYPE html>`.

## Conflict Notices
1) File: PLAN_4.md (contaminated untracked). SHA256: ee0e15a54be689824d89aabe60a786669b0d1506c2d10011417023191191e789. First 20 lines:
   1: # PLAN_4.md
   2:
   3: Purpose
   4: This file defines the only supported dataset → training path. Its goal is to eliminate ambiguity and enable execution: a dataset enters in a locked schema, is framed deterministically, and exits as an adapter with recorded provenance.
   5:
   6: Accepted Dataset Schemas (Locked)
   7: A) Supervised schema (JSONL)
   8: Each line:
   9: { "prompt": "string", "completion": "string" }
   10: Rules:
   11: - Both fields required.
   12: - UTF-8.
   13: - Empty strings are invalid.
   14:
   15: B) Raw text schema (JSONL)
   16: Each line:
   17: { "text": "string" }
   18: Rules:
   19: - Field required.
   20: - UTF-8.
   Action taken: contaminated file deleted; canonical PLAN_4.md recreated. Required action: commit.
2) Files: crates/adapteros-core/*, crates/adapteros-lora-*/*, crates/adapteros-lora-kernel-mtl/*, crates/adapteros-lora-worker/*, crates/adapteros-core/src/seed.rs, migrations/signatures.json. Nature: determinism-critical surface changed without mapped plan item. Recommended owner: Agent 1. Required action: claim ownership, map each change to PLAN_2 objective, or revert.
3) Files: crates/adapteros-ui/src/pages/training/*, crates/adapteros-ui/src/pages/training/data/*, crates/adapteros-api-types/src/system_status.rs, crates/adapteros-server-api/src/handlers/system_status.rs, crates/adapteros-server-api/src/state.rs. Nature: UI status/training pages changed alongside API types and handlers; risk of schema/UI drift. Recommended owner: Agent 3. Required action: confirm shared types + compile path, map to PLAN_3 truth surface.
4) Files: tests/benchmark/*, crates/adapteros-server-api/tests/*, scripts/test/all.sh. Nature: test/bench changes overlap with CI gate plan; ownership unclear. Recommended owner: Agent 2. Required action: define which changes belong to CI gate and which are unrelated; provide evidence or quarantine.
5) Files: scripts/make_minimal_dataset.py, scripts/start_minimal_training.sh, scripts/upload_minimal_dataset.sh, test_data/*. Nature: dataset/training scripts exist without plan mapping; contract locked to PLAN_4.md. Recommended owner: Agent 4. Required action: map to PLAN_2 dataset/training or remove.

## Hole Alerts
- PLAN_3 still specifies scripts/golden_path.sh; decision is locked to scripts/dev-up.sh.
- Model default locked to 0.5B but no script/config evidence mapping this default.
- No verified dataset fixture under test_data/ that matches the locked dataset contract.
- Untracked tests/hydration_gating_test.rs exists without plan mapping or evidence.
- No scripts/ci/golden_path_smoke.sh or equivalent CI gate evidence.
- PLAN_4.md committed after recreation (closed).

## Critical Paths
- Dashboard + blank chat: 1) scripts/dev-up.sh exists and is canonical; 2) run ./scripts/dev-up.sh and verify /healthz + /readyz 200 and / serves UI HTML; 3) load / and confirm dashboard renders with explicit blank chat state.
- One adapter-trained chat response: 1) complete dashboard + blank chat; 2) create/ingest dataset fixture per PLAN_4 contract; 3) train with 0.5B default and capture adapter id + .aos; 4) register + hydrate adapter and verify inference_ready; 5) run chat/infer and capture receipt fields.

## Directives
Agent 1: Own Adapter/Hydration/Chat (PLAN_2 sections). Next actions: map adapter discovery + hydration gating to plan; verify receipt fields via one inference; provide adapter list/load + /v1/system/status outputs. Evidence required: command outputs + file paths. Avoid touching: scripts/*, docs/*, crates/adapteros-ui/*, tests/benchmark/*.
Agent 2: Own Tests + Docs (PLAN_2 tests + documentation; PLAN_3 CI gate + runbook). Next actions: classify test/bench changes; define CI gate script or harness; update runbook. Evidence required: script paths + test output snippets + doc paths. Avoid touching: crates/adapteros-core/*, crates/adapteros-lora-*/*, crates/adapteros-ui/*.
Agent 3: Own Boot/UI (PLAN.md items 5-8; PLAN_3 truth surface). Next actions: verify scripts/dev-up.sh evidence; confirm QUICKSTART.md mention; verify UI blank state in / output. Evidence required: dev-up log + curl output + file paths. Avoid touching: crates/adapteros-core/*, crates/adapteros-lora-*/*, tests/*.
Agent 4: Own Dataset + Training (PLAN_2 sections; dataset contract locked). Next actions: define dataset fixture per PLAN_4; map untracked dataset/training scripts; document ingest + training commands. Evidence required: file paths + command outputs. Avoid touching: crates/adapteros-ui/*, tests/benchmark/*, scripts/test/*.
