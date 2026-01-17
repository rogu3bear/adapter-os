# adapterOS Minimum Viable Inference Loop (Working Plan)

## Objective
- Deliver one verified, boring, default path from dataset to MLX training to adapter registration to chat inference with receipts.
- Connect existing components only; no new architecture, no redesigns, no hidden failures.

## Current Evidence (Observed)
- Dataset ingest exists via control-plane API: `aosctl dataset ingest` hits `/v1/datasets/upload` and creates dataset/version records. Evidence: `crates/adapteros-cli/src/commands/datasets.rs`.
- Training pipeline consumes dataset versions or dataset_id via `TrainingDatasetManager`, which loads JSONL and tokenizes as needed. Evidence: `crates/adapteros-orchestrator/src/training_dataset_integration.rs`, `crates/adapteros-orchestrator/src/training/execution.rs`.
- Training uses `MicroLoRATrainer` with multi-backend GPU support (MLX behind feature flag) and requires `base_model_path`. Evidence: `crates/adapteros-lora-worker/src/training/trainer.rs`.
- Training job packages `.aos` and registers adapter with metadata. Evidence: `crates/adapteros-orchestrator/src/training/packaging.rs`, `crates/adapteros-cli/src/commands/register_adapter.rs`.
- Chat/infer endpoint returns receipts and deterministic metadata. Evidence: `crates/adapteros-api-types/src/inference.rs`, `crates/adapteros-cli/src/commands/chat.rs`.

## Known Gaps / Mismatches
- `train-base-adapter` default manifest path does not exist and dataset manifests under `training/datasets/*` do not match the loader schema. Evidence: `crates/adapteros-cli/src/commands/train_base_adapter.rs`, `training/datasets/*/manifest.json`, `crates/adapteros-lora-worker/src/training/loader.rs`.
- E2E tests are stubbed or reference missing paths (e.g., `training/datasets/base/...`). Evidence: `tests/e2e/dataset_to_inference.rs`, `tests/e2e/aos_workflow.rs`.
- CLI requires stored auth even in dev; `AOS_DEV_NO_AUTH=1` does not bypass CLI. Evidence: `crates/adapteros-cli/src/auth_store.rs`, `crates/adapteros-cli/src/commands/datasets.rs`.

## Golden Path (Target)
1. Start system: `AOS_DEV_NO_AUTH=1 ./start up` (or documented dev auth path).
2. Authenticate CLI (or documented dev bypass).
3. Dataset: ingest one minimal JSONL with prompt/response; validate it.
4. Training: start one MLX training job using that dataset version.
5. Adapter: packaged `.aos` is registered and discoverable.
6. Hydration: worker loads base model + adapter; status observable; inference blocks if not hydrated.
7. Chat: run inference with adapter (default or explicit) and return receipt with adapter id, determinism tier, routing info.

## Work Plan by Stage

### Dataset
- [ ] Pick canonical dataset format: JSONL with prompt/response (and optional metadata) compatible with `TrainingDatasetManager::load_examples_from_jsonl`.
- [ ] Add minimal dataset fixture for dev/test use and document single ingest command.
- [ ] Define one validation rule and surface explicit error messages on failure.
- [ ] Add dataset validation test that exercises real ingest + validate flow.

### Training (MLX)
- [ ] Confirm MLX selection path (feature flags, backend policy, base_model_path) and codify one command.
- [ ] Ensure training always records base model id, determinism tier, dataset hash in metadata.
- [ ] Verify training emits `.aos` artifact and registers adapter for the tenant.
- [ ] Add test that confirms training produces adapter artifact and registration entry.

### Adapter Registration / Discovery
- [ ] Ensure adapter is discoverable via `aosctl adapter list` and control-plane version listing.
- [ ] Verify adapter metadata includes base model, determinism tier compatibility, hash/identity.
- [ ] Add adapter registration test that validates metadata fields (not stubbed).

### Hydration
- [ ] Make hydration state observable (existing status endpoints or minimal extension).
- [ ] Block inference when model or adapter is not hydrated; return actionable error.
- [ ] Add a minimal hydration gate test.

### Chat / Inference / Receipt
- [ ] Ensure chat selects adapter default or explicit; fail clearly if none available.
- [ ] Verify receipt includes adapter identity, determinism tier, and routing info (deterministic_receipt/run_receipt/trace); add fields if missing.
- [ ] Add end-to-end test: dataset ingest -> training -> adapter -> chat -> receipt verification.

### Documentation
- [ ] Update README/QUICKSTART with "From zero to first chat response" (exact commands + expected outputs).
- [ ] Document the single golden path and remove/flag non-working alternatives.
- [ ] Coordinate with PLAN.md owner for AGENTS.md scope and PLAN.md Dataset/Training/Adapter/Chat sections.

## Evidence Pointers (Initial)
- Dataset ingest CLI: `crates/adapteros-cli/src/commands/datasets.rs`
- Dataset manager / tokenization: `crates/adapteros-orchestrator/src/training_dataset_integration.rs`
- Training execution: `crates/adapteros-orchestrator/src/training/execution.rs`
- Packaging + registration: `crates/adapteros-orchestrator/src/training/packaging.rs`, `crates/adapteros-cli/src/commands/register_adapter.rs`
- Receipts + inference response: `crates/adapteros-api-types/src/inference.rs`
- Chat CLI: `crates/adapteros-cli/src/commands/chat.rs`
