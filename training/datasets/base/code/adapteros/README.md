# AdapterOS Code Adapter Dataset

This directory houses the locally curated examples that specialize the base **code** LoRA for AdapterOS itself. It is safe to check in metadata, but keep any proprietary snippets encrypted or scrubbed before committing.

## Layout

- `*.jsonl` & `*.yaml` — structured prompt/response pairs used for supervised fine-tuning.
- `*.diff` — before/after patches aligned with compiler or linter output.
- `*.positive.*` — positive-weight samples that reinforce desired AdapterOS behaviour. Use `.positive.` in the filename so the trainer gives them full weight.
- `*.negative.*` — negative-weight samples that should push the adapter away from undesired behaviour (for example, hallucinated API calls). The `.negative.` stem in the filename is required so the trainer can down-weight these items automatically.
- `adapteros-positive-examples.positive.jsonl` — canonical success stories aligned with the MasterPlan hierarchy (deterministic routing, resident base adapter, policy compliance).
- `adapteros-negative-examples.negative.jsonl` — curated guardrail prompts covering git hallucinations, policy bypass attempts, and other AdapterOS-specific antipatterns. Extend this file (or add siblings) as you discover new failure modes.
- `manifest.json` — dataset manifest consumed by the MPLoRA packager. Lists all positive/negative inputs with sampling weights.

Current manifest version: `0.2.0` (adds loader regression coverage and packaging provenance metadata).

Feel free to add subfolders for specific tracks (e.g. `refactors/`, `security/`, `docs/`) as the dataset grows.

For the end-to-end training workflow, see `docs/training/base_adapter.md`.

## Contribution Checklist

1. Ensure the example compiles or lint-checks after the “positive” patch is applied (and explicitly mark reinforcing samples with `.positive.`).
2. Include a short provenance note (repository, commit, synthetic recipe) in the sample metadata.
3. Use `.negative.` filenames for any counter-examples so the training pipeline assigns negative weights correctly.
4. Run the dataset lints (`cargo xtask dataset:check`) before committing new data.
