# Unfinished Feature Isolation Audit (2026-02-17)

## Deterministic Scope
- Repo: `/Users/star/Dev/adapter-os`
- Scan source: `var/reports/unfinished-audit/scan.json`
- Execution branch: `c/ui-functional-dedup-sprint`
- Execution HEAD: `1e65b603154e143dfa8ee3eec8612054d1fc9d70`
- Source strict hits: `93`
- Source units: `35`
- Documentation/speculative hits: `98`
- Documentation/speculative units: `42`
- Total isolated units: `77`
- Total staging branches created/updated: `77`

## Track A Filter (Executable Source)
- Included paths: `crates/**`, `scripts/**`, `.github/**`
- Excluded paths: `**/*.md`, `tests/**`, `**/tests/**`, `**/test/**`, `**/*.snap`
- Markers: `unimplemented|not implemented|todo!|unimplemented!` OR `TODO|FIXME|TBD|WIP|stub|placeholder` paired with `implement|wire|hook|pending|later|future|phase|complete|support`

## Track B Filter (Documentation/Speculative)
- Included paths: `docs/**/*.md`, `README.md`, `crates/**/*.md`, `scripts/**/*.md`
- Excluded paths: `tests/**`, `**/tests/**`, `training/datasets/**`
- Markers: `not implemented|TBD|WIP|placeholder|future work|coming soon|planned|roadmap`

## Branch Move Ledger - Track A (Executable Source)
| # | Unit | Branch | Branch SHA | Prior SHA | Anchor Commit | Latest Touch | Evidence |
|---|---|---|---|---|---|---|---|
| 01 | `adapteros-api-types/lib` | `c/stage/unfinished-01-adapteros-api-types-lib` | `b15371f220d9` | `NEW` | `b15371f220d9` | `5e020a86ae13` | `crates/adapteros-api-types/src/lib.rs:284` |
| 02 | `adapteros-cli/commands/aos` | `c/stage/unfinished-02-adapteros-cli-commands-aos` | `da6278a9195b` | `NEW` | `da6278a9195b` | `3f0caf1b195d` | `crates/adapteros-cli/src/commands/aos.rs:105` |
| 03 | `adapteros-cli/commands/dev` | `c/stage/unfinished-03-adapteros-cli-commands-dev` | `b15371f220d9` | `NEW` | `b15371f220d9` | `169cda211835` | `crates/adapteros-cli/src/commands/dev.rs:613` |
| 04 | `adapteros-cli/commands/migrate` | `c/stage/unfinished-04-adapteros-cli-commands-migrate` | `7d4a5fe72058` | `NEW` | `7d4a5fe72058` | `2ad5a7586d9e` | `crates/adapteros-cli/src/commands/migrate.rs:25` |
| 05 | `adapteros-cli/commands/worker_executor` | `c/stage/unfinished-05-adapteros-cli-commands-worker-executor` | `c9a8eb0789af` | `NEW` | `c9a8eb0789af` | `b20fbaa2f916` | `crates/adapteros-cli/src/commands/worker_executor.rs:204` |
| 06 | `adapteros-client/native` | `c/stage/unfinished-06-adapteros-client-native` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b04d15eb3860` | `crates/adapteros-client/src/native.rs:354` |
| 07 | `adapteros-codegraph-viewer/commands` | `c/stage/unfinished-07-adapteros-codegraph-viewer-commands` | `fe8005e2b732` | `NEW` | `fe8005e2b732` | `2ad5a7586d9e` | `crates/adapteros-codegraph-viewer/src-tauri/src/commands.rs:321` |
| 08 | `adapteros-core/backend` | `c/stage/unfinished-08-adapteros-core-backend` | `b15371f220d9` | `NEW` | `b15371f220d9` | `3f0caf1b195d` | `crates/adapteros-core/src/backend.rs:118` |
| 09 | `adapteros-core/circuit_breaker` | `c/stage/unfinished-09-adapteros-core-circuit-breaker` | `b15371f220d9` | `NEW` | `b15371f220d9` | `f6594d2d1c0a` | `crates/adapteros-core/src/circuit_breaker.rs:161` |
| 10 | `adapteros-crypto/providers/kms` | `c/stage/unfinished-10-adapteros-crypto-providers-kms` | `aaa2f9398cd5` | `NEW` | `aaa2f9398cd5` | `d6d2eaddc901` | `crates/adapteros-crypto/src/providers/kms.rs:1719` |
| 11 | `adapteros-db/index_hashes` | `c/stage/unfinished-11-adapteros-db-index-hashes` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b15371f220d9` | `crates/adapteros-db/src/index_hashes.rs:118` |
| 12 | `adapteros-lora-kernel-api/lib` | `c/stage/unfinished-12-adapteros-lora-kernel-api-lib` | `b15371f220d9` | `NEW` | `b15371f220d9` | `9b704da6e337` | `crates/adapteros-lora-kernel-api/src/lib.rs:450` |
| 13 | `adapteros-lora-kernel-coreml/lib` | `c/stage/unfinished-13-adapteros-lora-kernel-coreml-lib` | `b15371f220d9` | `NEW` | `b15371f220d9` | `4c750efc4f46` | `crates/adapteros-lora-kernel-coreml/src/lib.rs:2106` |
| 14 | `adapteros-lora-lifecycle/profiler` | `c/stage/unfinished-14-adapteros-lora-lifecycle-profiler` | `b15371f220d9` | `NEW` | `b15371f220d9` | `700c85c745cb` | `crates/adapteros-lora-lifecycle/src/profiler/metrics.rs:336` |
| 15 | `adapteros-lora-mlx-ffi/backend` | `c/stage/unfinished-15-adapteros-lora-mlx-ffi-backend` | `b15371f220d9` | `NEW` | `b15371f220d9` | `d728ef39abf8` | `crates/adapteros-lora-mlx-ffi/src/backend.rs:228` |
| 16 | `adapteros-lora-mlx-ffi/mlx_cpp_wrapper_real.cpp` | `c/stage/unfinished-16-adapteros-lora-mlx-ffi-mlx-cpp-wrapper-real` | `b15371f220d9` | `NEW` | `b15371f220d9` | `45db48650c32` | `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp:2649` |
| 17 | `adapteros-lora-worker/training` | `c/stage/unfinished-17-adapteros-lora-worker-training` | `b36553794175` | `NEW` | `b36553794175` | `4e4881992b21` | `crates/adapteros-lora-worker/src/training/embedding_trainer.rs:624` |
| 18 | `adapteros-model-server/generated` | `c/stage/unfinished-18-adapteros-model-server-generated` | `f63a4adf9a4c` | `NEW` | `f63a4adf9a4c` | `f63a4adf9a4c` | `crates/adapteros-model-server/src/generated/adapteros.model_server.rs:1151` |
| 19 | `adapteros-model-server/server` | `c/stage/unfinished-19-adapteros-model-server-server` | `f63a4adf9a4c` | `NEW` | `f63a4adf9a4c` | `393dafa9209d` | `crates/adapteros-model-server/src/server.rs:116` |
| 20 | `adapteros-orchestrator/bootstrap` | `c/stage/unfinished-20-adapteros-orchestrator-bootstrap` | `096045128d15` | `NEW` | `096045128d15` | `096045128d15` | `crates/adapteros-orchestrator/src/bootstrap.rs:80` |
| 21 | `adapteros-orchestrator/code_training_gen` | `c/stage/unfinished-21-adapteros-orchestrator-code-training-gen` | `f8917f1aceda` | `NEW` | `f8917f1aceda` | `f8917f1aceda` | `crates/adapteros-orchestrator/src/code_training_gen.rs:35` |
| 22 | `adapteros-orchestrator/federation_daemon` | `c/stage/unfinished-22-adapteros-orchestrator-federation-daemon` | `bc69787c77c3` | `NEW` | `bc69787c77c3` | `8596934d7188` | `crates/adapteros-orchestrator/src/federation_daemon.rs:91` |
| 23 | `adapteros-policy/packs/production_readiness` | `c/stage/unfinished-23-adapteros-policy-packs-production-readiness` | `c513d1120cbd` | `NEW` | `c513d1120cbd` | `4e4881992b21` | `crates/adapteros-policy/src/packs/production_readiness.rs:69` |
| 24 | `adapteros-policy/policy_packs` | `c/stage/unfinished-24-adapteros-policy-policy-packs` | `b15371f220d9` | `NEW` | `b15371f220d9` | `2048b34cf83b` | `crates/adapteros-policy/src/policy_packs.rs:842` |
| 25 | `adapteros-secd/enclave` | `c/stage/unfinished-25-adapteros-secd-enclave` | `4c750efc4f46` | `NEW` | `4c750efc4f46` | `07e2f3977907` | `crates/adapteros-secd/src/enclave/mod.rs:39` |
| 26 | `adapteros-server-api/api_error` | `c/stage/unfinished-26-adapteros-server-api-api-error` | `b15371f220d9` | `NEW` | `b15371f220d9` | `8c756e2b6d90` | `crates/adapteros-server-api/src/api_error.rs:220` |
| 27 | `adapteros-server-api/handlers/auth_enhanced` | `c/stage/unfinished-27-adapteros-server-api-handlers-auth-enhanced` | `a89b884ed3bd` | `NEW` | `a89b884ed3bd` | `a89b884ed3bd` | `crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:415` |
| 28 | `adapteros-server-api/handlers/routing_decisions` | `c/stage/unfinished-28-adapteros-server-api-handlers-routing-decisi` | `6de3657af2fd` | `NEW` | `6de3657af2fd` | `7c90e7d50aee` | `crates/adapteros-server-api/src/handlers/routing_decisions.rs:73` |
| 29 | `adapteros-server-api/middleware/itar` | `c/stage/unfinished-29-adapteros-server-api-middleware-itar` | `84f3d7ee33f2` | `NEW` | `84f3d7ee33f2` | `f6594d2d1c0a` | `crates/adapteros-server-api/src/middleware/itar.rs:141` |
| 30 | `adapteros-server/security` | `c/stage/unfinished-30-adapteros-server-security` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b15371f220d9` | `crates/adapteros-server/src/security.rs:28` |
| 31 | `adapteros-storage/platform/windows` | `c/stage/unfinished-31-adapteros-storage-platform-windows` | `5601bf7ef2fd` | `NEW` | `5601bf7ef2fd` | `82f0205ba618` | `crates/adapteros-storage/src/platform/windows.rs:231` |
| 32 | `adapteros-storage/secure_fs` | `c/stage/unfinished-32-adapteros-storage-secure-fs` | `5601bf7ef2fd` | `NEW` | `5601bf7ef2fd` | `82f0205ba618` | `crates/adapteros-storage/src/secure_fs/mod.rs:423` |
| 33 | `adapteros-telemetry/profiler` | `c/stage/unfinished-33-adapteros-telemetry-profiler` | `5601bf7ef2fd` | `NEW` | `5601bf7ef2fd` | `5601bf7ef2fd` | `crates/adapteros-telemetry/src/profiler/metrics.rs:336` |
| 34 | `adapteros-ui/pages/settings` | `c/stage/unfinished-34-adapteros-ui-pages-settings` | `a89b884ed3bd` | `NEW` | `a89b884ed3bd` | `a89b884ed3bd` | `crates/adapteros-ui/src/pages/settings/security.rs:320` |
| 35 | `adapteros-verify/keys` | `c/stage/unfinished-35-adapteros-verify-keys` | `b15371f220d9` | `NEW` | `b15371f220d9` | `8dcae031da13` | `crates/adapteros-verify/src/keys.rs:284` |

## Branch Move Ledger - Track B (Documentation/Speculative)
| # | Unit | Branch | Branch SHA | Prior SHA | Anchor Commit | Latest Touch | Evidence |
|---|---|---|---|---|---|---|---|
| 01 | `adapteros-db/docs/README` | `c/stage/specdoc-01-adapteros-db-docs-readme` | `b15371f220d9` | `NEW` | `b15371f220d9` | `f6594d2d1c0a` | `crates/adapteros-db/benches/README.md:209` |
| 02 | `adapteros-lora-kernel-coreml/docs/README` | `c/stage/specdoc-02-adapteros-lora-kernel-coreml-docs-readme` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `46b681ded8e0` | `crates/adapteros-lora-kernel-coreml/README.md:174` |
| 03 | `adapteros-lora-kernel-mtl/docs/COREML_FFI_INTEGRATION` | `c/stage/specdoc-03-adapteros-lora-kernel-mtl-docs-coreml-ffi` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b15371f220d9` | `crates/adapteros-lora-kernel-mtl/COREML_FFI_INTEGRATION.md:493` |
| 04 | `adapteros-lora-mlx-ffi/docs/GUIDE_DEVELOPER_MEMORY` | `c/stage/specdoc-04-adapteros-lora-mlx-ffi-docs-guide-develope` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b15371f220d9` | `crates/adapteros-lora-mlx-ffi/GUIDE_DEVELOPER_MEMORY.md:355` |
| 05 | `adapteros-lora-mlx-ffi/docs/GUIDE_MEMORY_TRACKING` | `c/stage/specdoc-05-adapteros-lora-mlx-ffi-docs-guide-memory-t` | `b15371f220d9` | `NEW` | `b15371f220d9` | `f6594d2d1c0a` | `crates/adapteros-lora-mlx-ffi/GUIDE_MEMORY_TRACKING.md:278` |
| 06 | `adapteros-lora-mlx-ffi/docs/README` | `c/stage/specdoc-06-adapteros-lora-mlx-ffi-docs-readme` | `b15371f220d9` | `NEW` | `b15371f220d9` | `da6278a9195b` | `crates/adapteros-lora-mlx-ffi/README.md:75` |
| 07 | `adapteros-lora-mlx-ffi/docs/REFERENCE_API` | `c/stage/specdoc-07-adapteros-lora-mlx-ffi-docs-reference-api` | `b15371f220d9` | `NEW` | `b15371f220d9` | `b15371f220d9` | `crates/adapteros-lora-mlx-ffi/docs/REFERENCE_API.md:250` |
| 08 | `adapteros-lora-mlx-ffi/docs/REFERENCE_MEMORY_MANAGEMENT` | `c/stage/specdoc-08-adapteros-lora-mlx-ffi-docs-reference-memo` | `b15371f220d9` | `NEW` | `b15371f220d9` | `f6594d2d1c0a` | `crates/adapteros-lora-mlx-ffi/REFERENCE_MEMORY_MANAGEMENT.md:360` |
| 09 | `adapteros-memory/docs/SUMMARY_BUILD_SYSTEM` | `c/stage/specdoc-09-adapteros-memory-docs-summary-build-system` | `b15371f220d9` | `NEW` | `b15371f220d9` | `31306828db99` | `crates/adapteros-memory/SUMMARY_BUILD_SYSTEM.md:164` |
| 10 | `adapteros-storage/docs/README` | `c/stage/specdoc-10-adapteros-storage-docs-readme` | `b15371f220d9` | `NEW` | `b15371f220d9` | `d22188ab120a` | `crates/adapteros-storage/src/repos/README.md:273` |
| 11 | `adapteros-tui/docs/WHATS_WORKING` | `c/stage/specdoc-11-adapteros-tui-docs-whats-working` | `b15371f220d9` | `NEW` | `b15371f220d9` | `eb770562ce45` | `crates/adapteros-tui/docs/WHATS_WORKING.md:302` |
| 12 | `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS` | `c/stage/specdoc-12-docs-adapteros-deep-dive-ruthless` | `6afb1d33742f` | `NEW` | `6afb1d33742f` | `6afb1d33742f` | `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS.md:5` |
| 13 | `docs/API_GUIDES` | `c/stage/specdoc-13-docs-api-guides` | `b15371f220d9` | `NEW` | `b15371f220d9` | `31306828db99` | `docs/API_GUIDES.md:207` |
| 14 | `docs/API_REFERENCE` | `c/stage/specdoc-14-docs-api-reference` | `b15371f220d9` | `NEW` | `b15371f220d9` | `a89b884ed3bd` | `docs/API_REFERENCE.md:2943` |
| 15 | `docs/BACKEND_ARCHITECTURE` | `c/stage/specdoc-15-docs-backend-architecture` | `d22188ab120a` | `NEW` | `d22188ab120a` | `f6594d2d1c0a` | `docs/BACKEND_ARCHITECTURE.md:204` |
| 16 | `docs/BACKEND_SELECTION` | `c/stage/specdoc-16-docs-backend-selection` | `d22188ab120a` | `NEW` | `d22188ab120a` | `275adc571e09` | `docs/BACKEND_SELECTION.md:85` |
| 17 | `docs/COREML_BACKEND` | `c/stage/specdoc-17-docs-coreml-backend` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `cc23b785c720` | `docs/COREML_BACKEND.md:823` |
| 18 | `docs/COREML_LORA_WORKFLOWS` | `c/stage/specdoc-18-docs-coreml-lora-workflows` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `4d742d5a2fcb` | `docs/COREML_LORA_WORKFLOWS.md:16` |
| 19 | `docs/DEPRECATIONS` | `c/stage/specdoc-19-docs-deprecations` | `b15371f220d9` | `NEW` | `b15371f220d9` | `d51d865bc140` | `docs/DEPRECATIONS.md:119` |
| 20 | `docs/DOCUMENTATION_PRUNE_PLAN` | `c/stage/specdoc-20-docs-documentation-prune-plan` | `cc23b785c720` | `NEW` | `cc23b785c720` | `cc23b785c720` | `docs/DOCUMENTATION_PRUNE_PLAN.md:146` |
| 21 | `docs/LIFECYCLE` | `c/stage/specdoc-21-docs-lifecycle` | `27867dcea99c` | `NEW` | `27867dcea99c` | `27867dcea99c` | `docs/LIFECYCLE.md:1029` |
| 22 | `docs/README` | `c/stage/specdoc-22-docs-readme` | `cc23b785c720` | `NEW` | `cc23b785c720` | `40f65ca934a2` | `docs/README.md:525` |
| 23 | `docs/REVIEW_WORKFLOW` | `c/stage/specdoc-23-docs-review-workflow` | `006ebbb9405b` | `NEW` | `006ebbb9405b` | `f6594d2d1c0a` | `docs/REVIEW_WORKFLOW.md:301` |
| 24 | `docs/TECHNICAL_SPECIFICATION` | `c/stage/specdoc-24-docs-technical-specification` | `31306828db99` | `NEW` | `31306828db99` | `8f4b4fcc1b8e` | `docs/TECHNICAL_SPECIFICATION.md:5` |
| 25 | `docs/TROUBLESHOOTING` | `c/stage/specdoc-25-docs-troubleshooting` | `b15371f220d9` | `NEW` | `b15371f220d9` | `31306828db99` | `docs/TROUBLESHOOTING.md:1188` |
| 26 | `docs/engineering/E2E_TESTING_STRATEGY` | `c/stage/specdoc-26-docs-engineering-e2e-testing-strategy` | `e46ed5c19b07` | `NEW` | `e46ed5c19b07` | `f6594d2d1c0a` | `docs/engineering/E2E_TESTING_STRATEGY.md:21` |
| 27 | `docs/engineering/HANDLER_HYGIENE` | `c/stage/specdoc-27-docs-engineering-handler-hygiene` | `e46ed5c19b07` | `NEW` | `e46ed5c19b07` | `e46ed5c19b07` | `docs/engineering/HANDLER_HYGIENE.md:99` |
| 28 | `docs/performance/K_SPARSE_ROUTER_BASELINE` | `c/stage/specdoc-28-docs-performance-k-sparse-router-baseline` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `f6594d2d1c0a` | `docs/performance/K_SPARSE_ROUTER_BASELINE.md:175` |
| 29 | `docs/performance/README` | `c/stage/specdoc-29-docs-performance-readme` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `cc23b785c720` | `docs/performance/README.md:93` |
| 30 | `docs/plans/2026-01-30-prd04-adapters-stacks-lifecycle` | `c/stage/specdoc-30-docs-plans-2026-01-30-prd04-adapters-stack` | `f6f3e1548d1e` | `NEW` | `f6f3e1548d1e` | `f6f3e1548d1e` | `docs/plans/2026-01-30-prd04-adapters-stacks-lifecycle.md:543` |
| 31 | `docs/plans/2026-02-04-constellation-implementation` | `c/stage/specdoc-31-docs-plans-2026-02-04-constellation-implem` | `0ea2ff3e0eec` | `NEW` | `0ea2ff3e0eec` | `0ea2ff3e0eec` | `docs/plans/2026-02-04-constellation-implementation.md:514` |
| 32 | `docs/plans/2026-02-04-constellation-landing-design` | `c/stage/specdoc-32-docs-plans-2026-02-04-constellation-landin` | `7deca1881edc` | `NEW` | `7deca1881edc` | `7deca1881edc` | `docs/plans/2026-02-04-constellation-landing-design.md:104` |
| 33 | `docs/plans/2026-02-05-audit-findings-tracker` | `c/stage/specdoc-33-docs-plans-2026-02-05-audit-findings-track` | `27867dcea99c` | `NEW` | `27867dcea99c` | `27867dcea99c` | `docs/plans/2026-02-05-audit-findings-tracker.md:27` |
| 34 | `docs/plans/2026-02-05-chat-queue-ux-design` | `c/stage/specdoc-34-docs-plans-2026-02-05-chat-queue-ux-design` | `8285e04144f5` | `NEW` | `8285e04144f5` | `8285e04144f5` | `docs/plans/2026-02-05-chat-queue-ux-design.md:64` |
| 35 | `docs/plans/cli-http-client` | `c/stage/specdoc-35-docs-plans-cli-http-client` | `ad2f6b1844f1` | `NEW` | `ad2f6b1844f1` | `ad2f6b1844f1` | `docs/plans/cli-http-client.md:305` |
| 36 | `docs/program/EXECUTION_PLAN` | `c/stage/specdoc-36-docs-program-execution-plan` | `4ba8d4771ecc` | `NEW` | `4ba8d4771ecc` | `4ba8d4771ecc` | `docs/program/EXECUTION_PLAN.md:76` |
| 37 | `docs/program/METRICS` | `c/stage/specdoc-37-docs-program-metrics` | `4ba8d4771ecc` | `NEW` | `4ba8d4771ecc` | `4ba8d4771ecc` | `docs/program/METRICS.md:16` |
| 38 | `docs/program/RELEASE_NOTES_DRAFT` | `c/stage/specdoc-38-docs-program-release-notes-draft` | `4ba8d4771ecc` | `NEW` | `4ba8d4771ecc` | `4ba8d4771ecc` | `docs/program/RELEASE_NOTES_DRAFT.md:8` |
| 39 | `docs/roadmap/DATABASE_PERFORMANCE_ROADMAP` | `c/stage/specdoc-39-docs-roadmap-database-performance-roadmap` | `b15371f220d9` | `NEW` | `b15371f220d9` | `f6594d2d1c0a` | `docs/roadmap/DATABASE_PERFORMANCE_ROADMAP.md:1` |
| 40 | `docs/ui/MIGRATION` | `c/stage/specdoc-40-docs-ui-migration` | `5d036d220c32` | `NEW` | `5d036d220c32` | `5d036d220c32` | `docs/ui/MIGRATION.md:109` |
| 41 | `root/readme` | `c/stage/specdoc-41-root-readme` | `b15371f220d9` | `NEW` | `b15371f220d9` | `275adc571e09` | `README.md:393` |
| 42 | `scripts/docs/COREML_CONVERSION` | `c/stage/specdoc-42-scripts-docs-coreml-conversion` | `f65d1b43c5a9` | `NEW` | `f65d1b43c5a9` | `31306828db99` | `scripts/COREML_CONVERSION.md:59` |

## Representative Evidence (Track A)
### A01. adapteros-api-types/lib
- Branch: `c/stage/unfinished-01-adapteros-api-types-lib` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-api-types/src/lib.rs:284` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `5e020a86ae13c11b3b3fb803172897f2b88e8182`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-api-types/src/lib.rs:284` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `// 501 Not Implemented`

### A02. adapteros-cli/commands/aos
- Branch: `c/stage/unfinished-02-adapteros-cli-commands-aos` -> `da6278a9195bd3f09a3f3c448219214dad0d1c22`
- Anchor: `crates/adapteros-cli/src/commands/aos.rs:105` @ `da6278a9195bd3f09a3f3c448219214dad0d1c22`
- Latest touch: `3f0caf1b195d4f0886e96aa0307e5632ef452fc6`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-cli/src/commands/aos.rs:105` @ `da6278a9195bd3f09a3f3c448219214dad0d1c22` | `/// Migrate .aos file to current format version [NOT IMPLEMENTED]`

### A03. adapteros-cli/commands/dev
- Branch: `c/stage/unfinished-03-adapteros-cli-commands-dev` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-cli/src/commands/dev.rs:613` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `169cda211835be05537ba539fd1cc9cf6e7bb286`
- Hits/files: `2` hits across `1` files
- Evidence: `crates/adapteros-cli/src/commands/dev.rs:613` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `warn!("Process check not implemented for this platform");`
- Evidence: `crates/adapteros-cli/src/commands/dev.rs:652` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `warn!("Process termination not implemented for this platform");`

### A04. adapteros-cli/commands/migrate
- Branch: `c/stage/unfinished-04-adapteros-cli-commands-migrate` -> `7d4a5fe720588d1e16a0a65561fc793f2b7327cd`
- Anchor: `crates/adapteros-cli/src/commands/migrate.rs:25` @ `7d4a5fe720588d1e16a0a65561fc793f2b7327cd`
- Latest touch: `2ad5a7586d9ec1e6c6ab8055f973ea0716df095c`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-cli/src/commands/migrate.rs:25` @ `7d4a5fe720588d1e16a0a65561fc793f2b7327cd` | `/// Migrate adapter directory to .aos file [NOT IMPLEMENTED]`

### A05. adapteros-cli/commands/worker_executor
- Branch: `c/stage/unfinished-05-adapteros-cli-commands-worker-executor` -> `c9a8eb0789afdb660fa245d9de5a7312340f3e8b`
- Anchor: `crates/adapteros-cli/src/commands/worker_executor.rs:204` @ `c9a8eb0789afdb660fa245d9de5a7312340f3e8b`
- Latest touch: `b20fbaa2f916e889dbef9f7672031b411a9f4d56`
- Hits/files: `10` hits across `1` files
- Evidence: `crates/adapteros-cli/src/commands/worker_executor.rs:204` @ `c9a8eb0789afdb660fa245d9de5a7312340f3e8b` | `Unimplemented,`
- Evidence: `crates/adapteros-cli/src/commands/worker_executor.rs:248` @ `c9a8eb0789afdb660fa245d9de5a7312340f3e8b` | `// Check for unimplemented!() macro`
- Evidence: `crates/adapteros-cli/src/commands/worker_executor.rs:249` @ `c9a8eb0789afdb660fa245d9de5a7312340f3e8b` | `if trimmed.contains("unimplemented!()") {`

### A06. adapteros-client/native
- Branch: `c/stage/unfinished-06-adapteros-client-native` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-client/src/native.rs:354` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b04d15eb3860abd3a72d932034f008c36fb9411e`
- Hits/files: `2` hits across `1` files
- Evidence: `crates/adapteros-client/src/native.rs:354` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `// Code Policy - STUB (endpoint not implemented in API)`
- Evidence: `crates/adapteros-client/src/native.rs:368` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `// Metrics Dashboard - STUB (endpoints not implemented in API)`

### A07. adapteros-codegraph-viewer/commands
- Branch: `c/stage/unfinished-07-adapteros-codegraph-viewer-commands` -> `fe8005e2b732f0a7f662826840fe804e5050434c`
- Anchor: `crates/adapteros-codegraph-viewer/src-tauri/src/commands.rs:321` @ `fe8005e2b732f0a7f662826840fe804e5050434c`
- Latest touch: `2ad5a7586d9ec1e6c6ab8055f973ea0716df095c`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-codegraph-viewer/src-tauri/src/commands.rs:321` @ `fe8005e2b732f0a7f662826840fe804e5050434c` | `return Err("File opening not implemented for this platform".to_string());`

### A08. adapteros-core/backend
- Branch: `c/stage/unfinished-08-adapteros-core-backend` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-core/src/backend.rs:118` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `3f0caf1b195d4f0886e96aa0307e5632ef452fc6`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-core/src/backend.rs:118` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `// are not implemented for CPU today.`

### A09. adapteros-core/circuit_breaker
- Branch: `c/stage/unfinished-09-adapteros-core-circuit-breaker` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-core/src/circuit_breaker.rs:161` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-core/src/circuit_breaker.rs:161` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `Err(AosError::Internal("call_boxed not implemented".to_string()))`

### A10. adapteros-crypto/providers/kms
- Branch: `c/stage/unfinished-10-adapteros-crypto-providers-kms` -> `aaa2f9398cd5d67335af6205d711aeff92610077`
- Anchor: `crates/adapteros-crypto/src/providers/kms.rs:1719` @ `aaa2f9398cd5d67335af6205d711aeff92610077`
- Latest touch: `d6d2eaddc901a9a369c394cc020be5df011297df`
- Hits/files: `3` hits across `1` files
- Evidence: `crates/adapteros-crypto/src/providers/kms.rs:1719` @ `aaa2f9398cd5d67335af6205d711aeff92610077` | `// CRYPTO-GAP-001: PKCS#11 HSM provider not implemented`
- Evidence: `crates/adapteros-crypto/src/providers/kms.rs:1724` @ `aaa2f9398cd5d67335af6205d711aeff92610077` | `"PKCS#11 HSM not implemented - using mock provider \`
- Evidence: `crates/adapteros-crypto/src/providers/kms.rs:1730` @ `aaa2f9398cd5d67335af6205d711aeff92610077` | `"PKCS#11 HSM provider not implemented (CRYPTO-GAP-001). \`

### A11. adapteros-db/index_hashes
- Branch: `c/stage/unfinished-11-adapteros-db-index-hashes` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-db/src/index_hashes.rs:118` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-db/src/index_hashes.rs:118` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `let stacks_db = db.list_adapter_stacks(tenant_id).await?; // Placeholder: implement if needed`

### A12. adapteros-lora-kernel-api/lib
- Branch: `c/stage/unfinished-12-adapteros-lora-kernel-api-lib` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-kernel-api/src/lib.rs:450` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `9b704da6e3376b6581979fdc60473399a8d0f11f`
- Hits/files: `8` hits across `1` files
- Evidence: `crates/adapteros-lora-kernel-api/src/lib.rs:450` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"GPU fingerprint storage not implemented for this backend".to_string(),`
- Evidence: `crates/adapteros-lora-kernel-api/src/lib.rs:476` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"GPU fingerprint verification not implemented for this backend".to_string(),`
- Evidence: `crates/adapteros-lora-kernel-api/src/lib.rs:489` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `/// * within_tolerance: bool - false if not implemented (no baseline available)`

### A13. adapteros-lora-kernel-coreml/lib
- Branch: `c/stage/unfinished-13-adapteros-lora-kernel-coreml-lib` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-kernel-coreml/src/lib.rs:2106` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `4c750efc4f46697aed61bf8b5bab7bcaacf82c9c`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-kernel-coreml/src/lib.rs:2106` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"CoreML stub mode: inference step complete"`

### A14. adapteros-lora-lifecycle/profiler
- Branch: `c/stage/unfinished-14-adapteros-lora-lifecycle-profiler` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-lifecycle/src/profiler/metrics.rs:336` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `700c85c745cb2cc26303693f20dec9e72dca23b4`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-lifecycle/src/profiler/metrics.rs:336` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `/// Quality delta tracker (placeholder for future quality measurement)`

### A15. adapteros-lora-mlx-ffi/backend
- Branch: `c/stage/unfinished-15-adapteros-lora-mlx-ffi-backend` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/src/backend.rs:228` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `d728ef39abf846776c99306b08f051fccc8e8e72`
- Hits/files: `3` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/src/backend.rs:228` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"MLX backend built without real MLX support (stub FFI); determinism attestation disabled"`
- Evidence: `crates/adapteros-lora-mlx-ffi/src/backend.rs:734` @ `eb770562ce453e09559e87219856bdc6b8039fea` | `"Failover command execution not implemented in reference mode: {}",`
- Evidence: `crates/adapteros-lora-mlx-ffi/src/backend.rs:1224` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"MLX stub inference complete"`

### A16. adapteros-lora-mlx-ffi/mlx_cpp_wrapper_real.cpp
- Branch: `c/stage/unfinished-16-adapteros-lora-mlx-ffi-mlx-cpp-wrapper-real` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp:2649` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `45db48650c32471ca7c59f63b6193d01dda8af21`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp:2649` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `#warning "Compiling without real MLX support - using stub implementation"`

### A17. adapteros-lora-worker/training
- Branch: `c/stage/unfinished-17-adapteros-lora-worker-training` -> `b365537941754cfad6325ac771667616d8412f63`
- Anchor: `crates/adapteros-lora-worker/src/training/embedding_trainer.rs:624` @ `b365537941754cfad6325ac771667616d8412f63`
- Latest touch: `4e4881992b21a11f344c982d605e6b44e5da2dc2`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-worker/src/training/embedding_trainer.rs:624` @ `b365537941754cfad6325ac771667616d8412f63` | `unimplemented!("Test requires mock tokenizer setup")`

### A18. adapteros-model-server/generated
- Branch: `c/stage/unfinished-18-adapteros-model-server-generated` -> `f63a4adf9a4c279b2b28284da80df2c8687fd116`
- Anchor: `crates/adapteros-model-server/src/generated/adapteros.model_server.rs:1151` @ `f63a4adf9a4c279b2b28284da80df2c8687fd116`
- Latest touch: `f63a4adf9a4c279b2b28284da80df2c8687fd116`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-model-server/src/generated/adapteros.model_server.rs:1151` @ `f63a4adf9a4c279b2b28284da80df2c8687fd116` | `(tonic::Code::Unimplemented as i32).into(),`

### A19. adapteros-model-server/server
- Branch: `c/stage/unfinished-19-adapteros-model-server-server` -> `f63a4adf9a4c279b2b28284da80df2c8687fd116`
- Anchor: `crates/adapteros-model-server/src/server.rs:116` @ `f63a4adf9a4c279b2b28284da80df2c8687fd116`
- Latest touch: `393dafa9209d1a3ebca85c5c0f112f78ca848937`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-model-server/src/server.rs:116` @ `f63a4adf9a4c279b2b28284da80df2c8687fd116` | `// TODO: Add UDS support via tonic-uds or similar`

### A20. adapteros-orchestrator/bootstrap
- Branch: `c/stage/unfinished-20-adapteros-orchestrator-bootstrap` -> `096045128d154a047e778816b223f97c9ca11257`
- Anchor: `crates/adapteros-orchestrator/src/bootstrap.rs:80` @ `096045128d154a047e778816b223f97c9ca11257`
- Latest touch: `096045128d154a047e778816b223f97c9ca11257`
- Hits/files: `11` hits across `1` files
- Evidence: `crates/adapteros-orchestrator/src/bootstrap.rs:80` @ `096045128d154a047e778816b223f97c9ca11257` | `/// Fill in `todo!()` / `unimplemented!()` function bodies.`
- Evidence: `crates/adapteros-orchestrator/src/bootstrap.rs:694` @ `096045128d154a047e778816b223f97c9ca11257` | `if trimmed.contains("todo!()")`
- Evidence: `crates/adapteros-orchestrator/src/bootstrap.rs:695` @ `096045128d154a047e778816b223f97c9ca11257` | `|| trimmed.contains("unimplemented!()")`

### A21. adapteros-orchestrator/code_training_gen
- Branch: `c/stage/unfinished-21-adapteros-orchestrator-code-training-gen` -> `f8917f1aceda6f15fb10a6405d6f131971cf9950`
- Anchor: `crates/adapteros-orchestrator/src/code_training_gen.rs:35` @ `f8917f1aceda6f15fb10a6405d6f131971cf9950`
- Latest touch: `f8917f1aceda6f15fb10a6405d6f131971cf9950`
- Hits/files: `9` hits across `1` files
- Evidence: `crates/adapteros-orchestrator/src/code_training_gen.rs:35` @ `f8917f1aceda6f15fb10a6405d6f131971cf9950` | `/// `surrounding N lines + placeholder → complete function``
- Evidence: `crates/adapteros-orchestrator/src/code_training_gen.rs:415` @ `f8917f1aceda6f15fb10a6405d6f131971cf9950` | `prompt.push_str(&format!("// TODO: implement {}\n", symbol.name));`
- Evidence: `crates/adapteros-orchestrator/src/code_training_gen.rs:702` @ `f8917f1aceda6f15fb10a6405d6f131971cf9950` | `/// Skip functions whose body is just `todo!()`, `unimplemented!()`, or `panic!()`.`

### A22. adapteros-orchestrator/federation_daemon
- Branch: `c/stage/unfinished-22-adapteros-orchestrator-federation-daemon` -> `bc69787c77c39736f9ca1c8629bcfe0d5012388e`
- Anchor: `crates/adapteros-orchestrator/src/federation_daemon.rs:91` @ `bc69787c77c39736f9ca1c8629bcfe0d5012388e`
- Latest touch: `8596934d71885ef1f59d0a1896cad6992b7de92e`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-orchestrator/src/federation_daemon.rs:91` @ `bc69787c77c39736f9ca1c8629bcfe0d5012388e` | `/// # let daemon: Arc<FederationDaemon> = todo!();`

### A23. adapteros-policy/packs/production_readiness
- Branch: `c/stage/unfinished-23-adapteros-policy-packs-production-readiness` -> `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb`
- Anchor: `crates/adapteros-policy/src/packs/production_readiness.rs:69` @ `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb`
- Latest touch: `4e4881992b21a11f344c982d605e6b44e5da2dc2`
- Hits/files: `11` hits across `1` files
- Evidence: `crates/adapteros-policy/src/packs/production_readiness.rs:69` @ `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb` | `/// Require all handlers to be committed (no TODO/unimplemented! handlers)`
- Evidence: `crates/adapteros-policy/src/packs/production_readiness.rs:215` @ `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb` | `/// Handlers that are not committed (contain unimplemented!, todo!, etc.)`
- Evidence: `crates/adapteros-policy/src/packs/production_readiness.rs:310` @ `c513d1120cbdf6b3c19dc2769db5175a8a9eeaeb` | `/// Type of uncommitted code (unimplemented!, todo!, panic!)`

### A24. adapteros-policy/policy_packs
- Branch: `c/stage/unfinished-24-adapteros-policy-policy-packs` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-policy/src/policy_packs.rs:842` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `2048b34cf83bba082b72b6757ff02436686a8c2c`
- Hits/files: `4` hits across `1` files
- Evidence: `crates/adapteros-policy/src/policy_packs.rs:842` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `// Check if this is an unimplemented-but-known policy`
- Evidence: `crates/adapteros-policy/src/policy_packs.rs:895` @ `b5004f35fb19935c0a1598e9d125dbab4c25672d` | `// Security principle: unimplemented policies must fail-closed, not fail-open.`
- Evidence: `crates/adapteros-policy/src/policy_packs.rs:900` @ `b5004f35fb19935c0a1598e9d125dbab4c25672d` | `"Unimplemented policy pack requested - failing closed for security"`

### A25. adapteros-secd/enclave
- Branch: `c/stage/unfinished-25-adapteros-secd-enclave` -> `4c750efc4f46697aed61bf8b5bab7bcaacf82c9c`
- Anchor: `crates/adapteros-secd/src/enclave/mod.rs:39` @ `4c750efc4f46697aed61bf8b5bab7bcaacf82c9c`
- Latest touch: `07e2f39779077b34b3e7a9c410665dbb7ac145b3`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-secd/src/enclave/mod.rs:39` @ `4c750efc4f46697aed61bf8b5bab7bcaacf82c9c` | `//! | Attestation | Synthetic (stub - real SEP not implemented) | Not available |`

### A26. adapteros-server-api/api_error
- Branch: `c/stage/unfinished-26-adapteros-server-api-api-error` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-server-api/src/api_error.rs:220` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `8c756e2b6d90b5c6a4ea8071ac54c14083c1c416`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-server-api/src/api_error.rs:220` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `/// Not implemented - 501 Not Implemented`

### A27. adapteros-server-api/handlers/auth_enhanced
- Branch: `c/stage/unfinished-27-adapteros-server-api-handlers-auth-enhanced` -> `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Anchor: `crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:415` @ `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Latest touch: `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Hits/files: `2` hits across `1` files
- Evidence: `crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:415` @ `a89b884ed3bd138ee07736027f34c64d4ada1d03` | `ErrorResponse::new("Endpoint not implemented in this build.")`
- Evidence: `crates/adapteros-server-api/src/handlers/auth_enhanced/stubs.rs:436` @ `a89b884ed3bd138ee07736027f34c64d4ada1d03` | `ErrorResponse::new("Endpoint not implemented in this build.")`

### A28. adapteros-server-api/handlers/routing_decisions
- Branch: `c/stage/unfinished-28-adapteros-server-api-handlers-routing-decisi` -> `6de3657af2fd5131ed921977d781646328ef9c8c`
- Anchor: `crates/adapteros-server-api/src/handlers/routing_decisions.rs:73` @ `6de3657af2fd5131ed921977d781646328ef9c8c`
- Latest touch: `7c90e7d50aee8c90a22dbddac986a0454d110013`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-server-api/src/handlers/routing_decisions.rs:73` @ `6de3657af2fd5131ed921977d781646328ef9c8c` | `/// Status indicating not implemented`

### A29. adapteros-server-api/middleware/itar
- Branch: `c/stage/unfinished-29-adapteros-server-api-middleware-itar` -> `84f3d7ee33f21a298057032e4f51f69b8cc536ec`
- Anchor: `crates/adapteros-server-api/src/middleware/itar.rs:141` @ `84f3d7ee33f21a298057032e4f51f69b8cc536ec`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-server-api/src/middleware/itar.rs:141` @ `84f3d7ee33f21a298057032e4f51f69b8cc536ec` | `// Note: ITAR geo-blocking is not implemented`

### A30. adapteros-server/security
- Branch: `c/stage/unfinished-30-adapteros-server-security` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-server/src/security.rs:28` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-server/src/security.rs:28` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `bail!("PF egress check not implemented for this platform");`

### A31. adapteros-storage/platform/windows
- Branch: `c/stage/unfinished-31-adapteros-storage-platform-windows` -> `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Anchor: `crates/adapteros-storage/src/platform/windows.rs:231` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Latest touch: `82f0205ba6187644b2947510b2e862054bfdea46`
- Hits/files: `2` hits across `1` files
- Evidence: `crates/adapteros-storage/src/platform/windows.rs:231` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `// Fail fast - full metadata update not implemented`
- Evidence: `crates/adapteros-storage/src/platform/windows.rs:235` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `"Windows file metadata update not implemented (file times require Windows API bindings)".to_string(),`

### A32. adapteros-storage/secure_fs
- Branch: `c/stage/unfinished-32-adapteros-storage-secure-fs` -> `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Anchor: `crates/adapteros-storage/src/secure_fs/mod.rs:423` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Latest touch: `82f0205ba6187644b2947510b2e862054bfdea46`
- Hits/files: `4` hits across `2` files
- Evidence: `crates/adapteros-storage/src/secure_fs/mod.rs:423` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `debug!("Windows permissions not implemented yet");`
- Evidence: `crates/adapteros-storage/src/secure_fs/permissions.rs:69` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `debug!("Windows file permissions not implemented yet");`
- Evidence: `crates/adapteros-storage/src/secure_fs/permissions.rs:93` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `debug!("Windows directory permissions not implemented yet");`

### A33. adapteros-telemetry/profiler
- Branch: `c/stage/unfinished-33-adapteros-telemetry-profiler` -> `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Anchor: `crates/adapteros-telemetry/src/profiler/metrics.rs:336` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Latest touch: `5601bf7ef2fda5315730ad31517af32bdb27f50e`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-telemetry/src/profiler/metrics.rs:336` @ `5601bf7ef2fda5315730ad31517af32bdb27f50e` | `/// Quality delta tracker (placeholder for future quality measurement)`

### A34. adapteros-ui/pages/settings
- Branch: `c/stage/unfinished-34-adapteros-ui-pages-settings` -> `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Anchor: `crates/adapteros-ui/src/pages/settings/security.rs:320` @ `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Latest touch: `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-ui/src/pages/settings/security.rs:320` @ `a89b884ed3bd138ee07736027f34c64d4ada1d03` | `// Server errors (5xx) likely mean MFA is not implemented`

### A35. adapteros-verify/keys
- Branch: `c/stage/unfinished-35-adapteros-verify-keys` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-verify/src/keys.rs:284` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `8dcae031da13bbf53d2bfe3692f49d00de78a6a2`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-verify/src/keys.rs:284` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"Secure Enclave key management not implemented".to_string(),`

## Representative Evidence (Track B)
### B01. adapteros-db/docs/README
- Branch: `c/stage/specdoc-01-adapteros-db-docs-readme` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-db/benches/README.md:209` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-db/benches/README.md:209` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `## Future Work`

### B02. adapteros-lora-kernel-coreml/docs/README
- Branch: `c/stage/specdoc-02-adapteros-lora-kernel-coreml-docs-readme` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `crates/adapteros-lora-kernel-coreml/README.md:174` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `46b681ded8e0378377b7d599cd22ff8f15c27200`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-kernel-coreml/README.md:174` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `1. **Metal/MLX Sidecar Pipeline** (future work):`

### B03. adapteros-lora-kernel-mtl/docs/COREML_FFI_INTEGRATION
- Branch: `c/stage/specdoc-03-adapteros-lora-kernel-mtl-docs-coreml-ffi` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-kernel-mtl/COREML_FFI_INTEGRATION.md:493` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-kernel-mtl/COREML_FFI_INTEGRATION.md:493` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `### Planned Features (Agent 2 Responsibility)`

### B04. adapteros-lora-mlx-ffi/docs/GUIDE_DEVELOPER_MEMORY
- Branch: `c/stage/specdoc-04-adapteros-lora-mlx-ffi-docs-guide-develope` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/GUIDE_DEVELOPER_MEMORY.md:355` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/GUIDE_DEVELOPER_MEMORY.md:355` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `A: Currently no, but this is planned for future versions. Workaround: reset, load adapter, query memory.`

### B05. adapteros-lora-mlx-ffi/docs/GUIDE_MEMORY_TRACKING
- Branch: `c/stage/specdoc-05-adapteros-lora-mlx-ffi-docs-guide-memory-t` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/GUIDE_MEMORY_TRACKING.md:278` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/GUIDE_MEMORY_TRACKING.md:278` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `## Limitations & Future Work`

### B06. adapteros-lora-mlx-ffi/docs/README
- Branch: `c/stage/specdoc-06-adapteros-lora-mlx-ffi-docs-readme` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/README.md:75` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `da6278a9195bd3f09a3f3c448219214dad0d1c22`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/README.md:75` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `## Future Work`

### B07. adapteros-lora-mlx-ffi/docs/REFERENCE_API
- Branch: `c/stage/specdoc-07-adapteros-lora-mlx-ffi-docs-reference-api` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/docs/REFERENCE_API.md:250` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/docs/REFERENCE_API.md:250` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `Planned optimization for concurrent requests:`

### B08. adapteros-lora-mlx-ffi/docs/REFERENCE_MEMORY_MANAGEMENT
- Branch: `c/stage/specdoc-08-adapteros-lora-mlx-ffi-docs-reference-memo` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-lora-mlx-ffi/REFERENCE_MEMORY_MANAGEMENT.md:360` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-lora-mlx-ffi/REFERENCE_MEMORY_MANAGEMENT.md:360` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `## Limitations and Future Work`

### B09. adapteros-memory/docs/SUMMARY_BUILD_SYSTEM
- Branch: `c/stage/specdoc-09-adapteros-memory-docs-summary-build-system` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-memory/SUMMARY_BUILD_SYSTEM.md:164` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Hits/files: `1` hits across `1` files
- Evidence: `crates/adapteros-memory/SUMMARY_BUILD_SYSTEM.md:164` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `- Placeholder in `metal_heap_get_migration_events()``

### B10. adapteros-storage/docs/README
- Branch: `c/stage/specdoc-10-adapteros-storage-docs-readme` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-storage/src/repos/README.md:273` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `d22188ab120a9a7e02f1b977d3e87e299b77696d`
- Hits/files: `3` hits across `1` files
- Evidence: `crates/adapteros-storage/src/repos/README.md:273` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `| 6. Migration Tool | 📅 Planned | Batch data migration |`
- Evidence: `crates/adapteros-storage/src/repos/README.md:274` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `| 7. Read Cutover | 📅 Planned | Switch reads to KV |`
- Evidence: `crates/adapteros-storage/src/repos/README.md:275` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `| 8. SQL Deprecation | 📅 Planned | Remove SQL code |`

### B11. adapteros-tui/docs/WHATS_WORKING
- Branch: `c/stage/specdoc-11-adapteros-tui-docs-whats-working` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `crates/adapteros-tui/docs/WHATS_WORKING.md:302` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `eb770562ce453e09559e87219856bdc6b8039fea`
- Hits/files: `2` hits across `1` files
- Evidence: `crates/adapteros-tui/docs/WHATS_WORKING.md:302` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `- ⚠️ Telemetry integration not implemented`
- Evidence: `crates/adapteros-tui/docs/WHATS_WORKING.md:304` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `- Search not implemented`

### B12. docs/ADAPTEROS_DEEP_DIVE_RUTHLESS
- Branch: `c/stage/specdoc-12-docs-adapteros-deep-dive-ruthless` -> `6afb1d33742fc0373205402b8fb607d70d3a3425`
- Anchor: `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS.md:5` @ `6afb1d33742fc0373205402b8fb607d70d3a3425`
- Latest touch: `6afb1d33742fc0373205402b8fb607d70d3a3425`
- Hits/files: `5` hits across `1` files
- Evidence: `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS.md:5` @ `6afb1d33742fc0373205402b8fb607d70d3a3425` | `Audit rules: status set is strictly {Verified, Documented, Not implemented, Not found}; every O/R row has one paragraph answer (3-7 sentences), strict evidence references, and caveats.`
- Evidence: `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS.md:98` @ `6afb1d33742fc0373205402b8fb607d70d3a3425` | `| R39 | Not implemented | CPU affinity handling is explicitly marked best-effort and the current pinning function logs assignment without enforcing OS-level affinity. The strict initialization path st`
- Evidence: `docs/ADAPTEROS_DEEP_DIVE_RUTHLESS.md:111` @ `6afb1d33742fc0373205402b8fb607d70d3a3425` | `## Evidence-of-absence appendix (for Not found / Not implemented)`

### B13. docs/API_GUIDES
- Branch: `c/stage/specdoc-13-docs-api-guides` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `docs/API_GUIDES.md:207` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Hits/files: `4` hits across `1` files
- Evidence: `docs/API_GUIDES.md:207` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `**Status:** Planned for future release`
- Evidence: `docs/API_GUIDES.md:215` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `**Note:** v2 is not yet available. This is a placeholder for future planning.`
- Evidence: `docs/API_GUIDES.md:274` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `- No sunset date planned`

### B14. docs/API_REFERENCE
- Branch: `c/stage/specdoc-14-docs-api-reference` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `docs/API_REFERENCE.md:2943` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `a89b884ed3bd138ee07736027f34c64d4ada1d03`
- Hits/files: `2` hits across `1` files
- Evidence: `docs/API_REFERENCE.md:2943` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `"reason": "Planned maintenance",`
- Evidence: `docs/API_REFERENCE.md:2955` @ `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4` | `"reason": "Planned maintenance",`

### B15. docs/BACKEND_ARCHITECTURE
- Branch: `c/stage/specdoc-15-docs-backend-architecture` -> `d22188ab120a9a7e02f1b977d3e87e299b77696d`
- Anchor: `docs/BACKEND_ARCHITECTURE.md:204` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/BACKEND_ARCHITECTURE.md:204` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `| 5 | **CPU** | Observability only (not implemented for inference) |`

### B16. docs/BACKEND_SELECTION
- Branch: `c/stage/specdoc-16-docs-backend-selection` -> `d22188ab120a9a7e02f1b977d3e87e299b77696d`
- Anchor: `docs/BACKEND_SELECTION.md:85` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d`
- Latest touch: `275adc571e091d87f8137940378c240ff0f43e07`
- Hits/files: `9` hits across `1` files
- Evidence: `docs/BACKEND_SELECTION.md:85` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `| **CPU**       | CPU-only execution                                          | All                         | N/A              | Not implemented for inference (observability only)       |`
- Evidence: `docs/BACKEND_SELECTION.md:127` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `| 5        | **CPU**       | Terminal entry (observability only - not implemented)       |`
- Evidence: `docs/BACKEND_SELECTION.md:175` @ `d22188ab120a9a7e02f1b977d3e87e299b77696d` | `| `cpu`       | `cpu_only`, `cpu-only`                   | Not implemented for inference |`

### B17. docs/COREML_BACKEND
- Branch: `c/stage/specdoc-17-docs-coreml-backend` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `docs/COREML_BACKEND.md:823` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `cc23b785c7207c7b743f7c6439016a069de19650`
- Hits/files: `4` hits across `1` files
- Evidence: `docs/COREML_BACKEND.md:823` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `**Status:** ⚠️ **STUB MODE** - Infrastructure exists but LoRA computation is not implemented`
- Evidence: `docs/COREML_BACKEND.md:825` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `**Why stubbed?** CoreML models are compiled and opaque - we cannot access intermediate layer activations. True runtime fusion requires a Metal/MLX sidecar pipeline, which adds ~20-30% overhead and is `
- Evidence: `docs/COREML_BACKEND.md:842` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `| **Hot-Swap** | ❌ No | ⚠️ Planned |`

### B18. docs/COREML_LORA_WORKFLOWS
- Branch: `c/stage/specdoc-18-docs-coreml-lora-workflows` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `docs/COREML_LORA_WORKFLOWS.md:16` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `4d742d5a2fcb88248b5c539878292748e3a6ef49`
- Hits/files: `4` hits across `1` files
- Evidence: `docs/COREML_LORA_WORKFLOWS.md:16` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `| **Runtime Sidecar** | ⚠️ Stub/Planned | Dynamic hot-swapping | ~20-30% overhead |`
- Evidence: `docs/COREML_LORA_WORKFLOWS.md:238` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `│              Runtime Sidecar Pipeline (PLANNED)               │`
- Evidence: `docs/COREML_LORA_WORKFLOWS.md:516` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `**A:** Not yet. LoRA fusion for MoE models is planned but not implemented. MoE models currently support base inference only.`

### B19. docs/DEPRECATIONS
- Branch: `c/stage/specdoc-19-docs-deprecations` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `docs/DEPRECATIONS.md:119` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `d51d865bc140673c6dbc5cb94fcbafc08424cc0a`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/DEPRECATIONS.md:119` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `- Notes: Planned but never executed. Would have created compatibility shim crates in `crates/compat/`. No compat/ directory exists.`

### B20. docs/DOCUMENTATION_PRUNE_PLAN
- Branch: `c/stage/specdoc-20-docs-documentation-prune-plan` -> `cc23b785c7207c7b743f7c6439016a069de19650`
- Anchor: `docs/DOCUMENTATION_PRUNE_PLAN.md:146` @ `cc23b785c7207c7b743f7c6439016a069de19650`
- Latest touch: `cc23b785c7207c7b743f7c6439016a069de19650`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/DOCUMENTATION_PRUNE_PLAN.md:146` @ `cc23b785c7207c7b743f7c6439016a069de19650` | `2. Create placeholder files with "TODO: Document this", OR`

### B21. docs/LIFECYCLE
- Branch: `c/stage/specdoc-21-docs-lifecycle` -> `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Anchor: `docs/LIFECYCLE.md:1029` @ `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Latest touch: `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/LIFECYCLE.md:1029` @ `27867dcea99c1e22be177500f2d4aedbe15b7b57` | `**API (roadmap):**`

### B22. docs/README
- Branch: `c/stage/specdoc-22-docs-readme` -> `cc23b785c7207c7b743f7c6439016a069de19650`
- Anchor: `docs/README.md:525` @ `cc23b785c7207c7b743f7c6439016a069de19650`
- Latest touch: `40f65ca934a271330ae4b7030d4c5ab1c18782ab`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/README.md:525` @ `cc23b785c7207c7b743f7c6439016a069de19650` | `| [**roadmap/DATABASE_PERFORMANCE_ROADMAP.md**](roadmap/DATABASE_PERFORMANCE_ROADMAP.md) | Database performance roadmap |`

### B23. docs/REVIEW_WORKFLOW
- Branch: `c/stage/specdoc-23-docs-review-workflow` -> `006ebbb9405bec302bda406310623d450037193c`
- Anchor: `docs/REVIEW_WORKFLOW.md:301` @ `006ebbb9405bec302bda406310623d450037193c`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `2` hits across `1` files
- Evidence: `docs/REVIEW_WORKFLOW.md:301` @ `006ebbb9405bec302bda406310623d450037193c` | `## Planned Extensions`
- Evidence: `docs/REVIEW_WORKFLOW.md:308` @ `006ebbb9405bec302bda406310623d450037193c` | `### Webhook Integration (Planned)`

### B24. docs/TECHNICAL_SPECIFICATION
- Branch: `c/stage/specdoc-24-docs-technical-specification` -> `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Anchor: `docs/TECHNICAL_SPECIFICATION.md:5` @ `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Latest touch: `8f4b4fcc1b8e5ff47465e7faefc6213f7dfce8d2`
- Hits/files: `10` hits across `1` files
- Evidence: `docs/TECHNICAL_SPECIFICATION.md:5` @ `31306828db997ce3d7ae40e9de1f8ef4d466cd3f` | `> **Status**: Living document. All claims verified against source code unless marked *Planned*.`
- Evidence: `docs/TECHNICAL_SPECIFICATION.md:45` @ `31306828db997ce3d7ae40e9de1f8ef4d466cd3f` | `- [8. Roadmap & Open Items](#8-roadmap--open-items)`
- Evidence: `docs/TECHNICAL_SPECIFICATION.md:47` @ `31306828db997ce3d7ae40e9de1f8ef4d466cd3f` | `- [8.2 Planned for v0.15](#82-planned-for-v015)`

### B25. docs/TROUBLESHOOTING
- Branch: `c/stage/specdoc-25-docs-troubleshooting` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `docs/TROUBLESHOOTING.md:1188` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/TROUBLESHOOTING.md:1188` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `| 501 | Not Implemented | Stub endpoint or disabled feature |`

### B26. docs/engineering/E2E_TESTING_STRATEGY
- Branch: `c/stage/specdoc-26-docs-engineering-e2e-testing-strategy` -> `e46ed5c19b07410006c4f1bba49eff65ed0931ce`
- Anchor: `docs/engineering/E2E_TESTING_STRATEGY.md:21` @ `e46ed5c19b07410006c4f1bba49eff65ed0931ce`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/engineering/E2E_TESTING_STRATEGY.md:21` @ `e46ed5c19b07410006c4f1bba49eff65ed0931ce` | `| **Golden Fixtures** | `tests/fixtures/golden/replay_*.json` | Placeholder golden vectors for replay |`

### B27. docs/engineering/HANDLER_HYGIENE
- Branch: `c/stage/specdoc-27-docs-engineering-handler-hygiene` -> `e46ed5c19b07410006c4f1bba49eff65ed0931ce`
- Anchor: `docs/engineering/HANDLER_HYGIENE.md:99` @ `e46ed5c19b07410006c4f1bba49eff65ed0931ce`
- Latest touch: `e46ed5c19b07410006c4f1bba49eff65ed0931ce`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/engineering/HANDLER_HYGIENE.md:99` @ `e46ed5c19b07410006c4f1bba49eff65ed0931ce` | `## Prioritized Split Roadmap`

### B28. docs/performance/K_SPARSE_ROUTER_BASELINE
- Branch: `c/stage/specdoc-28-docs-performance-k-sparse-router-baseline` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `docs/performance/K_SPARSE_ROUTER_BASELINE.md:175` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/performance/K_SPARSE_ROUTER_BASELINE.md:175` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `## Future Work`

### B29. docs/performance/README
- Branch: `c/stage/specdoc-29-docs-performance-readme` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `docs/performance/README.md:93` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `cc23b785c7207c7b743f7c6439016a069de19650`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/performance/README.md:93` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `| Throughput | TBD | - |`

### B30. docs/plans/2026-01-30-prd04-adapters-stacks-lifecycle
- Branch: `c/stage/specdoc-30-docs-plans-2026-01-30-prd04-adapters-stack` -> `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4`
- Anchor: `docs/plans/2026-01-30-prd04-adapters-stacks-lifecycle.md:543` @ `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4`
- Latest touch: `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/plans/2026-01-30-prd04-adapters-stacks-lifecycle.md:543` @ `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4` | `placeholder="Enter reason for this transition..."`

### B31. docs/plans/2026-02-04-constellation-implementation
- Branch: `c/stage/specdoc-31-docs-plans-2026-02-04-constellation-implem` -> `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d`
- Anchor: `docs/plans/2026-02-04-constellation-implementation.md:514` @ `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d`
- Latest touch: `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d`
- Hits/files: `5` hits across `1` files
- Evidence: `docs/plans/2026-02-04-constellation-implementation.md:514` @ `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d` | `.constellation-input::placeholder {`
- Evidence: `docs/plans/2026-02-04-constellation-implementation.md:1098` @ `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d` | `// Adaptive placeholder based on state`
- Evidence: `docs/plans/2026-02-04-constellation-implementation.md:1099` @ `0ea2ff3e0eecf8b600ca2f902614e05f52752e2d` | `let placeholder = move || {`

### B32. docs/plans/2026-02-04-constellation-landing-design
- Branch: `c/stage/specdoc-32-docs-plans-2026-02-04-constellation-landin` -> `7deca1881edca1672a187af936f55f2b570c78ee`
- Anchor: `docs/plans/2026-02-04-constellation-landing-design.md:104` @ `7deca1881edca1672a187af936f55f2b570c78ee`
- Latest touch: `7deca1881edca1672a187af936f55f2b570c78ee`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/plans/2026-02-04-constellation-landing-design.md:104` @ `7deca1881edca1672a187af936f55f2b570c78ee` | `- The input might have context: "Continue with Medical QA?" as placeholder text`

### B33. docs/plans/2026-02-05-audit-findings-tracker
- Branch: `c/stage/specdoc-33-docs-plans-2026-02-05-audit-findings-track` -> `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Anchor: `docs/plans/2026-02-05-audit-findings-tracker.md:27` @ `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Latest touch: `27867dcea99c1e22be177500f2d4aedbe15b7b57`
- Hits/files: `2` hits across `1` files
- Evidence: `docs/plans/2026-02-05-audit-findings-tracker.md:27` @ `27867dcea99c1e22be177500f2d4aedbe15b7b57` | `| 2 | Resolved | Default backend profile is embeddings-capable; default builds no longer emit 501 "not implemented" for document/dataset embedding paths. |`
- Evidence: `docs/plans/2026-02-05-audit-findings-tracker.md:33` @ `27867dcea99c1e22be177500f2d4aedbe15b7b57` | `| 8 | Resolved | Inference spoke handlers/routes delegate to real production inference handlers (no placeholder payloads). |`

### B34. docs/plans/2026-02-05-chat-queue-ux-design
- Branch: `c/stage/specdoc-34-docs-plans-2026-02-05-chat-queue-ux-design` -> `8285e04144f5dab00f55aaaefaa5fa2759a77e22`
- Anchor: `docs/plans/2026-02-05-chat-queue-ux-design.md:64` @ `8285e04144f5dab00f55aaaefaa5fa2759a77e22`
- Latest touch: `8285e04144f5dab00f55aaaefaa5fa2759a77e22`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/plans/2026-02-05-chat-queue-ux-design.md:64` @ `8285e04144f5dab00f55aaaefaa5fa2759a77e22` | `| "Start a conversation" placeholder | Dead text, input has its own placeholder |`

### B35. docs/plans/cli-http-client
- Branch: `c/stage/specdoc-35-docs-plans-cli-http-client` -> `ad2f6b1844f157f2807bf4cd3885df34073d4db2`
- Anchor: `docs/plans/cli-http-client.md:305` @ `ad2f6b1844f157f2807bf4cd3885df34073d4db2`
- Latest touch: `ad2f6b1844f157f2807bf4cd3885df34073d4db2`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/plans/cli-http-client.md:305` @ `ad2f6b1844f157f2807bf4cd3885df34073d4db2` | `- Connection pooling optimization (defer to future work)`

### B36. docs/program/EXECUTION_PLAN
- Branch: `c/stage/specdoc-36-docs-program-execution-plan` -> `4ba8d4771ecc2297a007a57534c188af86f94948`
- Anchor: `docs/program/EXECUTION_PLAN.md:76` @ `4ba8d4771ecc2297a007a57534c188af86f94948`
- Latest touch: `4ba8d4771ecc2297a007a57534c188af86f94948`
- Hits/files: `5` hits across `1` files
- Evidence: `docs/program/EXECUTION_PLAN.md:76` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `### A3: Fix routing rule condition placeholder`
- Evidence: `docs/program/EXECUTION_PLAN.md:78` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `- **Problem:** Placeholder `sentiment == 'negative'` is not valid JSON, but the field validates as JSON.`
- Evidence: `docs/program/EXECUTION_PLAN.md:79` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `- **Scope:** Fix placeholder to valid JSON example.`

### B37. docs/program/METRICS
- Branch: `c/stage/specdoc-37-docs-program-metrics` -> `4ba8d4771ecc2297a007a57534c188af86f94948`
- Anchor: `docs/program/METRICS.md:16` @ `4ba8d4771ecc2297a007a57534c188af86f94948`
- Latest touch: `4ba8d4771ecc2297a007a57534c188af86f94948`
- Hits/files: `1` hits across `1` files
- Evidence: `docs/program/METRICS.md:16` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `| Routing placeholder is valid JSON | **Yes** (`{"sentiment": "negative"}`) |`

### B38. docs/program/RELEASE_NOTES_DRAFT
- Branch: `c/stage/specdoc-38-docs-program-release-notes-draft` -> `4ba8d4771ecc2297a007a57534c188af86f94948`
- Anchor: `docs/program/RELEASE_NOTES_DRAFT.md:8` @ `4ba8d4771ecc2297a007a57534c188af86f94948`
- Latest touch: `4ba8d4771ecc2297a007a57534c188af86f94948`
- Hits/files: `7` hits across `1` files
- Evidence: `docs/program/RELEASE_NOTES_DRAFT.md:8` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `- TBD`
- Evidence: `docs/program/RELEASE_NOTES_DRAFT.md:11` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `- TBD`
- Evidence: `docs/program/RELEASE_NOTES_DRAFT.md:14` @ `4ba8d4771ecc2297a007a57534c188af86f94948` | `- TBD: All error displays now show plain-English messages with recovery guidance instead of raw technical strings.`

### B39. docs/roadmap/DATABASE_PERFORMANCE_ROADMAP
- Branch: `c/stage/specdoc-39-docs-roadmap-database-performance-roadmap` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `docs/roadmap/DATABASE_PERFORMANCE_ROADMAP.md:1` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `f6594d2d1c0ab8a733d9a498263bd4d85db94a40`
- Hits/files: `2` hits across `1` files
- Evidence: `docs/roadmap/DATABASE_PERFORMANCE_ROADMAP.md:1` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `# Database Performance Roadmap`
- Evidence: `docs/roadmap/DATABASE_PERFORMANCE_ROADMAP.md:5` @ `f6594d2d1c0ab8a733d9a498263bd4d85db94a40` | `This roadmap outlines the evolution of adapterOS database architecture to support 10,000+ tenants with sub-millisecond query latency.`

### B40. docs/ui/MIGRATION
- Branch: `c/stage/specdoc-40-docs-ui-migration` -> `5d036d220c327ce1f41b3f632242909accceab68`
- Anchor: `docs/ui/MIGRATION.md:109` @ `5d036d220c327ce1f41b3f632242909accceab68`
- Latest touch: `5d036d220c327ce1f41b3f632242909accceab68`
- Hits/files: `2` hits across `1` files
- Evidence: `docs/ui/MIGRATION.md:109` @ `5d036d220c327ce1f41b3f632242909accceab68` | `1. **Tokens tab**: Shows placeholder values; needs backend integration for actual token counts`
- Evidence: `docs/ui/MIGRATION.md:111` @ `5d036d220c327ce1f41b3f632242909accceab68` | `3. **Diff tab**: Not implemented in this iteration (would compare two runs)`

### B41. root/readme
- Branch: `c/stage/specdoc-41-root-readme` -> `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Anchor: `README.md:393` @ `b15371f220d9d5f974d3c163b450424b3e1a7112`
- Latest touch: `275adc571e091d87f8137940378c240ff0f43e07`
- Hits/files: `4` hits across `1` files
- Evidence: `README.md:393` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `**📋 Completion Roadmap**: See project board for the comprehensive plan to complete all features.`
- Evidence: `README.md:538` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `### Planned`
- Evidence: `README.md:614` @ `b15371f220d9d5f974d3c163b450424b3e1a7112` | `## 🗺️ Roadmap & Vision`

### B42. scripts/docs/COREML_CONVERSION
- Branch: `c/stage/specdoc-42-scripts-docs-coreml-conversion` -> `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Anchor: `scripts/COREML_CONVERSION.md:59` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4`
- Latest touch: `31306828db997ce3d7ae40e9de1f8ef4d466cd3f`
- Hits/files: `2` hits across `1` files
- Evidence: `scripts/COREML_CONVERSION.md:59` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `### Option B: Runtime Sidecar (Future Work)`
- Evidence: `scripts/COREML_CONVERSION.md:61` @ `f65d1b43c5a92a340ee9616a27efa68b5d8d9ae4` | `**Status:** ⚠️ **STUBBED** - Infrastructure exists but LoRA computation not implemented`
