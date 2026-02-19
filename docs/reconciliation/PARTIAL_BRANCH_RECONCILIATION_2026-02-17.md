# Partial Branch Reconciliation (2026-02-17)

> **Archived snapshot** — Branch reconciliation record. Current code is authoritative.

## Deterministic Scope
- Base branch: `main` @ `073d932b62f0d52c1122576ce0ecb84967e2f3b6`
- Stable integration branch: `c/stable-main-unify` @ `cc62ccbe7e544bfea3fc7ae7317516ee07fd467d`
- Partial branch set: `refs/heads/c/stage/*` + `refs/heads/c/ui-functional-dedup-sprint`
- Classification algorithm:
  1. `ancestor`: `git merge-base --is-ancestor <branch> c/stable-main-unify`
  2. `patch_equivalent`: not ancestor AND `git cherry c/stable-main-unify <branch>` has zero `+` commits
  3. `pending`: `git cherry ...` has one or more `+` commits

## Guideline Citations
- Existing code first: `AGENTS.md:6`, `AGENTS.md:8`
- Minimal diffs and avoid unnecessary refactor: `AGENTS.md:13`
- Smallest relevant verification commands: `AGENTS.md:12`, `AGENTS.md:37`, `AGENTS.md:48`

## Pre-Merge Classification
- Source artifact: `docs/reconciliation/partial_branch_reconciliation_2026-02-17.json`
- Total partial branches: `78`
- Ancestor: `77`
- Patch-equivalent: `1`
- Pending: `0`

### Patch-Equivalent Branch(es) Requiring Explicit Unification
- `c/ui-functional-dedup-sprint` @ `1e65b603154e143dfa8ee3eec8612054d1fc9d70` (ahead `4`, `git cherry +` count `0`)

## Explicit Merge Unification
- Executed merge commit: `cc62ccbe7e544bfea3fc7ae7317516ee07fd467d`
- Subject: `Merge branch 'c/ui-functional-dedup-sprint' into c/stable-main-unify`
- Parents: `2b023e1ab969c9d4f75420216fdb9b202530fcd8` + `1e65b603154e143dfa8ee3eec8612054d1fc9d70`
- Conflict resolution: no conflict markers; merge completed cleanly with `ort` strategy.
- Merge output note: auto-merged path `crates/adapteros-ui/src/components/mod.rs` with no resulting content delta in merge commit.

## Post-Merge Classification
- Source artifact: `docs/reconciliation/partial_branch_reconciliation_after_merge_2026-02-17.json`
- Ancestor: `78`
- Patch-equivalent: `0`
- Pending: `0`

## Obsolete Branch Removal
- Source artifact: `docs/reconciliation/partial_branch_deletions_2026-02-17.json`
- Deleted obsolete branches: `77`
- Retained checked-out branches: `1`

### Retained Branches (Safety)
- `c/ui-functional-dedup-sprint` @ `1e65b603154e143dfa8ee3eec8612054d1fc9d70` retained because it is checked out in another worktree.

### Branch Move Ledger (All Partial Branch Actions)
| Branch | Tip SHA | Action | Result |
|---|---|---|---|
| `c/stage/specdoc-01-adapteros-db-docs-readme` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-01-adapteros-db-docs-readme (was b15371f22).` |
| `c/stage/specdoc-02-adapteros-lora-kernel-coreml-docs-readme` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-02-adapteros-lora-kernel-coreml-docs-readme (was f65d1b43c).` |
| `c/stage/specdoc-03-adapteros-lora-kernel-mtl-docs-coreml-ffi` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-03-adapteros-lora-kernel-mtl-docs-coreml-ffi (was b15371f22).` |
| `c/stage/specdoc-04-adapteros-lora-mlx-ffi-docs-guide-develope` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-04-adapteros-lora-mlx-ffi-docs-guide-develope (was b15371f22).` |
| `c/stage/specdoc-05-adapteros-lora-mlx-ffi-docs-guide-memory-t` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-05-adapteros-lora-mlx-ffi-docs-guide-memory-t (was b15371f22).` |
| `c/stage/specdoc-06-adapteros-lora-mlx-ffi-docs-readme` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-06-adapteros-lora-mlx-ffi-docs-readme (was b15371f22).` |
| `c/stage/specdoc-07-adapteros-lora-mlx-ffi-docs-reference-api` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-07-adapteros-lora-mlx-ffi-docs-reference-api (was b15371f22).` |
| `c/stage/specdoc-08-adapteros-lora-mlx-ffi-docs-reference-memo` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-08-adapteros-lora-mlx-ffi-docs-reference-memo (was b15371f22).` |
| `c/stage/specdoc-09-adapteros-memory-docs-summary-build-system` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-09-adapteros-memory-docs-summary-build-system (was b15371f22).` |
| `c/stage/specdoc-10-adapteros-storage-docs-readme` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-10-adapteros-storage-docs-readme (was b15371f22).` |
| `c/stage/specdoc-11-adapteros-tui-docs-whats-working` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-11-adapteros-tui-docs-whats-working (was b15371f22).` |
| `c/stage/specdoc-12-docs-adapteros-deep-dive-ruthless` | `6afb1d33742fc0373205402b8fb607d70d3a3425` | `delete` | `Deleted branch c/stage/specdoc-12-docs-adapteros-deep-dive-ruthless (was 6afb1d337).` |
| `c/stage/specdoc-13-docs-api-guides` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-13-docs-api-guides (was b15371f22).` |
| `c/stage/specdoc-14-docs-api-reference` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-14-docs-api-reference (was b15371f22).` |
| `c/stage/specdoc-15-docs-backend-architecture` | `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `delete` | `Deleted branch c/stage/specdoc-15-docs-backend-architecture (was d22188ab1).` |
| `c/stage/specdoc-16-docs-backend-selection` | `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `delete` | `Deleted branch c/stage/specdoc-16-docs-backend-selection (was d22188ab1).` |
| `c/stage/specdoc-17-docs-coreml-backend` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-17-docs-coreml-backend (was f65d1b43c).` |
| `c/stage/specdoc-18-docs-coreml-lora-workflows` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-18-docs-coreml-lora-workflows (was f65d1b43c).` |
| `c/stage/specdoc-19-docs-deprecations` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-19-docs-deprecations (was b15371f22).` |
| `c/stage/specdoc-20-docs-documentation-prune-plan` | `cc23b785c7207c7b743f7c6439016a069de19650` | `delete` | `Deleted branch c/stage/specdoc-20-docs-documentation-prune-plan (was cc23b785c).` |
| `c/stage/specdoc-21-docs-lifecycle` | `27867dcea99c1e22be177500f2d4aedbe15b7b57` | `delete` | `Deleted branch c/stage/specdoc-21-docs-lifecycle (was 27867dcea).` |
| `c/stage/specdoc-22-docs-readme` | `cc23b785c7207c7b743f7c6439016a069de19650` | `delete` | `Deleted branch c/stage/specdoc-22-docs-readme (was cc23b785c).` |
| `c/stage/specdoc-23-docs-review-workflow` | `006ebbb9405bec302bda406310623d450037193c` | `delete` | `Deleted branch c/stage/specdoc-23-docs-review-workflow (was 006ebbb94).` |
| `c/stage/specdoc-24-docs-technical-specification` | `31306828db997ce3d7ae40e9de1f8ef4d466cd3f` | `delete` | `Deleted branch c/stage/specdoc-24-docs-technical-specification (was 31306828d).` |
| `c/stage/specdoc-25-docs-troubleshooting` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-25-docs-troubleshooting (was b15371f22).` |
| `c/stage/specdoc-26-docs-engineering-e2e-testing-strategy` | `e46ed5c19b07410006c4f1bba49eff65ed0931ce` | `delete` | `Deleted branch c/stage/specdoc-26-docs-engineering-e2e-testing-strategy (was e46ed5c19).` |
| `c/stage/specdoc-27-docs-engineering-handler-hygiene` | `e46ed5c19b07410006c4f1bba49eff65ed0931ce` | `delete` | `Deleted branch c/stage/specdoc-27-docs-engineering-handler-hygiene (was e46ed5c19).` |
| `c/stage/specdoc-28-docs-performance-k-sparse-router-baseline` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-28-docs-performance-k-sparse-router-baseline (was f65d1b43c).` |
| `c/stage/specdoc-29-docs-performance-readme` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-29-docs-performance-readme (was f65d1b43c).` |
| `c/stage/specdoc-30-docs-plans-2026-01-30-prd04-adapters-stack` | `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4` | `delete` | `Deleted branch c/stage/specdoc-30-docs-plans-2026-01-30-prd04-adapters-stack (was f6f3e1548).` |
| `c/stage/specdoc-31-docs-plans-2026-02-04-constellation-implem` | `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d` | `delete` | `Deleted branch c/stage/specdoc-31-docs-plans-2026-02-04-constellation-implem (was 0ea2ff3e0).` |
| `c/stage/specdoc-32-docs-plans-2026-02-04-constellation-landin` | `7deca1881edca1672a187af936f55f2b570c78ee` | `delete` | `Deleted branch c/stage/specdoc-32-docs-plans-2026-02-04-constellation-landin (was 7deca1881).` |
| `c/stage/specdoc-33-docs-plans-2026-02-05-audit-findings-track` | `27867dcea99c1e22be177500f2d4aedbe15b7b57` | `delete` | `Deleted branch c/stage/specdoc-33-docs-plans-2026-02-05-audit-findings-track (was 27867dcea).` |
| `c/stage/specdoc-34-docs-plans-2026-02-05-chat-queue-ux-design` | `8285e04144f5dab00f55aaaefaa5fa2759a77e22` | `delete` | `Deleted branch c/stage/specdoc-34-docs-plans-2026-02-05-chat-queue-ux-design (was 8285e0414).` |
| `c/stage/specdoc-35-docs-plans-cli-http-client` | `ad2f6b1844f157f2807bf4cd3885df34073d4db2` | `delete` | `Deleted branch c/stage/specdoc-35-docs-plans-cli-http-client (was ad2f6b184).` |
| `c/stage/specdoc-36-docs-program-execution-plan` | `4ba8d4771ecc2297a007a57534c188af86f94948` | `delete` | `Deleted branch c/stage/specdoc-36-docs-program-execution-plan (was 4ba8d4771).` |
| `c/stage/specdoc-37-docs-program-metrics` | `4ba8d4771ecc2297a007a57534c188af86f94948` | `delete` | `Deleted branch c/stage/specdoc-37-docs-program-metrics (was 4ba8d4771).` |
| `c/stage/specdoc-38-docs-program-release-notes-draft` | `4ba8d4771ecc2297a007a57534c188af86f94948` | `delete` | `Deleted branch c/stage/specdoc-38-docs-program-release-notes-draft (was 4ba8d4771).` |
| `c/stage/specdoc-39-docs-roadmap-database-performance-roadmap` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-39-docs-roadmap-database-performance-roadmap (was b15371f22).` |
| `c/stage/specdoc-40-docs-ui-migration` | `5d036d220c327ce1f41b3f632242909accceab68` | `delete` | `Deleted branch c/stage/specdoc-40-docs-ui-migration (was 5d036d220).` |
| `c/stage/specdoc-41-root-readme` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/specdoc-41-root-readme (was b15371f22).` |
| `c/stage/specdoc-42-scripts-docs-coreml-conversion` | `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `delete` | `Deleted branch c/stage/specdoc-42-scripts-docs-coreml-conversion (was f65d1b43c).` |
| `c/stage/unfinished-01-adapteros-api-types-lib` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-01-adapteros-api-types-lib (was b15371f22).` |
| `c/stage/unfinished-02-adapteros-cli-commands-aos` | `da6278a9195bd3f09a3f3c448219214dad0d1c22` | `delete` | `Deleted branch c/stage/unfinished-02-adapteros-cli-commands-aos (was da6278a91).` |
| `c/stage/unfinished-03-adapteros-cli-commands-dev` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-03-adapteros-cli-commands-dev (was b15371f22).` |
| `c/stage/unfinished-04-adapteros-cli-commands-migrate` | `7d4a5fe720588d1e16a0a65561fc793f2b7327cd` | `delete` | `Deleted branch c/stage/unfinished-04-adapteros-cli-commands-migrate (was 7d4a5fe72).` |
| `c/stage/unfinished-05-adapteros-cli-commands-worker-executor` | `c9a8eb0789afdb660fa245d9de5a7312340f3e8b` | `delete` | `Deleted branch c/stage/unfinished-05-adapteros-cli-commands-worker-executor (was c9a8eb078).` |
| `c/stage/unfinished-06-adapteros-client-native` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-06-adapteros-client-native (was b15371f22).` |
| `c/stage/unfinished-07-adapteros-codegraph-viewer-commands` | `fe8005e2b732f0a7f662826840fe804e5050434c` | `delete` | `Deleted branch c/stage/unfinished-07-adapteros-codegraph-viewer-commands (was fe8005e2b).` |
| `c/stage/unfinished-08-adapteros-core-backend` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-08-adapteros-core-backend (was b15371f22).` |
| `c/stage/unfinished-09-adapteros-core-circuit-breaker` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-09-adapteros-core-circuit-breaker (was b15371f22).` |
| `c/stage/unfinished-10-adapteros-crypto-providers-kms` | `aaa2f9398cd5d67335af6205d711aeff92610077` | `delete` | `Deleted branch c/stage/unfinished-10-adapteros-crypto-providers-kms (was aaa2f9398).` |
| `c/stage/unfinished-11-adapteros-db-index-hashes` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-11-adapteros-db-index-hashes (was b15371f22).` |
| `c/stage/unfinished-12-adapteros-lora-kernel-api-lib` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-12-adapteros-lora-kernel-api-lib (was b15371f22).` |
| `c/stage/unfinished-13-adapteros-lora-kernel-coreml-lib` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-13-adapteros-lora-kernel-coreml-lib (was b15371f22).` |
| `c/stage/unfinished-14-adapteros-lora-lifecycle-profiler` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-14-adapteros-lora-lifecycle-profiler (was b15371f22).` |
| `c/stage/unfinished-15-adapteros-lora-mlx-ffi-backend` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-15-adapteros-lora-mlx-ffi-backend (was b15371f22).` |
| `c/stage/unfinished-16-adapteros-lora-mlx-ffi-mlx-cpp-wrapper-real` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-16-adapteros-lora-mlx-ffi-mlx-cpp-wrapper-real (was b15371f22).` |
| `c/stage/unfinished-17-adapteros-lora-worker-training` | `b365537941754cfad6325ac771667616d8412f63` | `delete` | `Deleted branch c/stage/unfinished-17-adapteros-lora-worker-training (was b36553794).` |
| `c/stage/unfinished-18-adapteros-model-server-generated` | `f63a4adf9a4c279b2b28284da80df2c8687fd116` | `delete` | `Deleted branch c/stage/unfinished-18-adapteros-model-server-generated (was f63a4adf9).` |
| `c/stage/unfinished-19-adapteros-model-server-server` | `f63a4adf9a4c279b2b28284da80df2c8687fd116` | `delete` | `Deleted branch c/stage/unfinished-19-adapteros-model-server-server (was f63a4adf9).` |
| `c/stage/unfinished-20-adapteros-orchestrator-bootstrap` | `096045128d154a047e778816b223f97c9ca11257` | `delete` | `Deleted branch c/stage/unfinished-20-adapteros-orchestrator-bootstrap (was 096045128).` |
| `c/stage/unfinished-21-adapteros-orchestrator-code-training-gen` | `f8917f1aceda6f15fb10a6405d6f131971cf9950` | `delete` | `Deleted branch c/stage/unfinished-21-adapteros-orchestrator-code-training-gen (was f8917f1ac).` |
| `c/stage/unfinished-22-adapteros-orchestrator-federation-daemon` | `bc69787c77c39736f9ca1c8629bcfe0d5012388e` | `delete` | `Deleted branch c/stage/unfinished-22-adapteros-orchestrator-federation-daemon (was bc69787c7).` |
| `c/stage/unfinished-23-adapteros-policy-packs-production-readiness` | `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb` | `delete` | `Deleted branch c/stage/unfinished-23-adapteros-policy-packs-production-readiness (was c513d1120).` |
| `c/stage/unfinished-24-adapteros-policy-policy-packs` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-24-adapteros-policy-policy-packs (was b15371f22).` |
| `c/stage/unfinished-25-adapteros-secd-enclave` | `4c750efc4f46697aed61bf8b5bab7bcaacf82c9c` | `delete` | `Deleted branch c/stage/unfinished-25-adapteros-secd-enclave (was 4c750efc4).` |
| `c/stage/unfinished-26-adapteros-server-api-api-error` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-26-adapteros-server-api-api-error (was b15371f22).` |
| `c/stage/unfinished-27-adapteros-server-api-handlers-auth-enhanced` | `a89b884ed3bd138ee07736027f34c64d4ada1d03` | `delete` | `Deleted branch c/stage/unfinished-27-adapteros-server-api-handlers-auth-enhanced (was a89b884ed).` |
| `c/stage/unfinished-28-adapteros-server-api-handlers-routing-decisi` | `6de3657af2fd5131ed921977d781646328ef9c8c` | `delete` | `Deleted branch c/stage/unfinished-28-adapteros-server-api-handlers-routing-decisi (was 6de3657af).` |
| `c/stage/unfinished-29-adapteros-server-api-middleware-itar` | `84f3d7ee33f21a298057032e4f51f69b8cc536ec` | `delete` | `Deleted branch c/stage/unfinished-29-adapteros-server-api-middleware-itar (was 84f3d7ee3).` |
| `c/stage/unfinished-30-adapteros-server-security` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-30-adapteros-server-security (was b15371f22).` |
| `c/stage/unfinished-31-adapteros-storage-platform-windows` | `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `delete` | `Deleted branch c/stage/unfinished-31-adapteros-storage-platform-windows (was 5601bf7ef).` |
| `c/stage/unfinished-32-adapteros-storage-secure-fs` | `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `delete` | `Deleted branch c/stage/unfinished-32-adapteros-storage-secure-fs (was 5601bf7ef).` |
| `c/stage/unfinished-33-adapteros-telemetry-profiler` | `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `delete` | `Deleted branch c/stage/unfinished-33-adapteros-telemetry-profiler (was 5601bf7ef).` |
| `c/stage/unfinished-34-adapteros-ui-pages-settings` | `a89b884ed3bd138ee07736027f34c64d4ada1d03` | `delete` | `Deleted branch c/stage/unfinished-34-adapteros-ui-pages-settings (was a89b884ed).` |
| `c/stage/unfinished-35-adapteros-verify-keys` | `b15371f220d9d5f974d3c163b450424b3e1a7112` | `delete` | `Deleted branch c/stage/unfinished-35-adapteros-verify-keys (was b15371f22).` |
| `c/ui-functional-dedup-sprint` | `1e65b603154e143dfa8ee3eec8612054d1fc9d70` | `retained_checked_out` | `blocked` |

## Final Branch Inventory
- `c/stable-main-unify` @ `cc62ccbe7e544bfea3fc7ae7317516ee07fd467d`
- `c/ui-functional-dedup-sprint` @ `1e65b603154e143dfa8ee3eec8612054d1fc9d70`

## Verification
- `cargo check -p adapteros-ui --target wasm32-unknown-unknown` (pass)
- `python3 scripts/ui_component_similarity.py --threshold 0.80 --exclude-file-suffix components/icons.rs --max-qualifying 8` (pass, qualifying components `6`)
