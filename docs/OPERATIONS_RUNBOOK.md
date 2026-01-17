## Operations Runbook: Repo → Dataset → Validate → Train → Publish

Use backend on 8080 and UI on 3200. All routes are tenant-scoped; use your tenant token.

1) Create adapter repository
- API: `POST /v1/repos` with `name`, `base_model`, `default_branch`.
- UI: Repositories → “Create repo”. Repo list is tenant-filtered.

2) Create dataset version
- API: `POST /v1/datasets/{dataset_id}/versions` with manifest reference.
- UI: Dataset detail → “Create version” (if available).

3) Validate dataset
- API: `POST /v1/datasets/{dataset_id}/validate`.
- UI: Dataset detail → “Validate” (only when draft/invalid). Training button stays disabled if trust is blocked/unknown.

4) Start training from dataset version
- API: `POST /v1/training/jobs` (or repo version training route) using dataset version IDs. Training fails closed when trust is not allowed.
- UI: Dataset detail → “Start Training Job” (enabled only when trust is allowed).

5) Promote adapter version (publish)
- Preconditions: version `release_state=ready` AND serveable (trust not blocked/unknown).
- API: `POST /v1/adapter-versions/{version_id}/promote` with `repo_id`. Non-serveable returns `NOT_SERVEABLE`.
- UI: Repo detail → Promote button enabled only for ready + serveable versions; tooltip shows reason when disabled.

6) Verify serveability
- API: `GET /v1/repos/{repo_id}/versions` shows `serveable` and `serveable_reason`.
- Router/publish paths select only serveable versions.

7) Known limits / cautions
- CoreML determinism parity still under validation.
- Reconciliation load can surface as temporary non-serveable status; re-run reconcile before promoting.
- Multi-tenant: cross-tenant repo/dataset/health/promotion operations are forbidden.

MLNavigator Inc Thursday Dec 11, 2025.

---

## Operationalization: Zero to First Chat

### Quick Start (One Command)

```bash
./scripts/golden_path_adapter_chat.sh
```

This script runs the complete flow: boot system, seed model, create dataset, train adapter, run inference with receipts. Expected runtime: 5-30 minutes depending on hardware.

### Common Blockers and Fixes

| Symptom | Cause | Fix |
|---------|-------|-----|
| `/readyz` returns 503 | Server still booting | Wait 10-30s, check `var/logs/server.log` |
| "No models seeded" | Model DB empty | Run `aosctl models seed --model-path var/model-cache/models/<model>` |
| "Worker not registered" | Worker not started | Run `./scripts/worker-up.sh` or check worker logs |
| "Model not hydrated" | Worker hasn't loaded model | Wait for hydration or check worker memory |
| "Dataset trust blocked" | Dataset not validated | `POST /v1/datasets/{id}/validate` or mark trusted |
| "Training failed" | MLX/backend issue | Check `var/logs/worker.log`, ensure MLX installed |
| Config path errors | Absolute paths in config | Use `configs/dev.toml` with relative `var/` paths |

### CI Gate vs Full Golden Path

- **CI gate** (`scripts/ci/golden_path_smoke.sh`): Boot + health checks, <60s, no training. Run on every commit.
- **Full golden path** (`scripts/golden_path_adapter_chat.sh`): Complete training flow. Run manually or nightly.
