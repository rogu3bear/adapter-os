# AdapterOS Self-Debug/Self-Healing Scan Progress

## Objective
Document existing self-debugging, self-healing, retry, restart, and rebuild mechanisms across AdapterOS, then identify targeted fortification opportunities.

## Execution TODO
- [x] Search first across major ownership surfaces before writing anything.
- [x] Split scanning into non-overlapping agent-owned areas.
- [x] Gather evidence with file + line references.
- [x] Consolidate findings into `debug.md`.
- [x] Verify deliverables exist and include evidence references.
- [x] Reground highest-risk claims with exact source anchors and explicit inference labeling.
- [x] Rectify report focus with a high-leverage anchor-to-action core (`debug.md` "Rectified Core").
- [ ] Implement hardening changes (intentionally out of scope for this scan-only pass).

## Scan Coverage
| Area | Owned Paths | Status | Main Findings |
| --- | --- | --- | --- |
| CLI wrapper + command surface | `aosctl`, `commands` | Complete | `aosctl` rebuild fallback path present (`--rebuild` and missing-binary rebuild). |
| Error recovery + diagnostics core | `crates/adapteros-error-recovery`, `crates/adapteros-diagnostics`, `crates/adapteros-core/src/error_helpers`, `crates/adapteros-telemetry` | Complete | Strategy-based recovery, exponential retry manager, diagnostics persistence retry, panic-journal poison recovery. |
| Runtime supervision and worker resilience | `crates/adapteros-node`, `crates/adapteros-server`, `crates/adapteros-lora-worker`, `crates/adapteros-lora-router`, `crates/adapteros-core` | Complete | Plugin health/reload loop, GPU fallback coordinator, deadlock-triggered supervisor restart, retryable health check classification. |
| Docs/runbooks/scripts/tests resilience flows | `docs`, `scripts`, `tests`, `examples/diagnostics` | Complete | Service-manager restart/backoff loop, memory-pressure runbooks, deterministic crash-recovery tests. |
| Control plane/data plane extras | `crates/adapteros-aos`, `crates/adapteros-server-api`, `crates/adapteros-db`, `crates/adapteros-storage`, `monitoring` | Complete | DB retry framework with jitter/circuit-breaker limits, SSE retry/circuit breaker, secure-fs permission self-repair retry. |

## Deep Expedition Coverage (Pass 2)
| Area | Owned Paths | Status | Main Findings |
| --- | --- | --- | --- |
| Service supervision stack | `crates/adapteros-service-supervisor`, `scripts/watchdog.sh`, `scripts/aos-supervisor.service`, `scripts/graceful-shutdown.sh`, `scripts/worker-up.sh`, `scripts/supervisor.toml` | Complete | Service manager restart/health primitives exist, but restart policy wiring to automated remediation is partial. |
| API health/admin/training/inference surfaces | `crates/adapteros-api`, `crates/adapteros-server-api-admin`, `crates/adapteros-server-api-health`, `crates/adapteros-server-api-inference`, `crates/adapteros-server-api-models`, `crates/adapteros-server-api-training` | Complete | Rich healthz/readyz/invariant status APIs, admin maintenance/safe-restart controls, retryable streaming/training signals. |
| Security and key lifecycle | `crates/adapteros-secd`, `crates/adapteros-crypto`, `crates/adapteros-auth`, `crates/adapteros-artifacts` | Complete | Scheduled/manual/emergency key rotation, keyring fallback and RPC retries, attestation fallback path. |
| Determinism/replay guardrails | `crates/adapteros-deterministic-exec`, `crates/adapteros-replay`, `crates/adapteros-core/src/circuit_breaker.rs`, `crates/adapteros-core/src/preflight`, `crates/adapteros-core/src/recovery` | Complete | Enforcement modes, deterministic tick ledger checks, replay signature/availability checks, generic circuit-breaker protections, preflight audit/bypass controls. |
| Memory/model/backend recovery | `crates/adapteros-memory`, `crates/adapteros-model-hub`, `crates/adapteros-lora-kernel-mtl`, `crates/adapteros-lora-kernel-coreml`, `crates/adapteros-lora-mlx-ffi` | Complete | Memory pressure orchestration, model-load fallback to Rust loader, backend panic recovery wrapper, troubleshooting tooling. |
| Policy + metrics remediation hooks | `crates/adapteros-policy`, `crates/adapteros-system-metrics`, `crates/adapteros-metrics-exporter`, `crates/adapteros-telemetry-types` | Complete | Circuit-breaker and incident policy packs plus metric-triggered eviction orchestration signals. |
| Lifecycle/orchestrator/training recovery | `crates/adapteros-orchestrator`, `crates/adapteros-training-worker`, `crates/adapteros-lora-lifecycle`, `crates/adapteros-agent-spawn` | Complete | K-reduction rollback, crash/stale heartbeat recovery, orphaned training-job recovery on startup. |
| DB deep idempotency/retry layers | `crates/adapteros-db`, `crates/adapteros-registry`, `crates/adapteros-storage/src/repos` | Complete | Document retry gates, stale-processing reset, retry chain tracking, optimistic conflict-safe promotion updates, adapter registration idempotency. |

## Evidence Index (High-Value)
- `aosctl:11-21`
- `crates/adapteros-error-recovery/src/lib.rs:145-305`
- `crates/adapteros-error-recovery/src/retry.rs:13-200`
- `crates/adapteros-telemetry/src/diagnostics/writer.rs:210-246`
- `crates/adapteros-telemetry/src/diagnostics/writer.rs:480-526`
- `crates/adapteros-telemetry/src/crash_journal.rs:126-177`
- `crates/adapteros-server/src/plugin_registry.rs:43-80`
- `crates/adapteros-lora-worker/src/backend_coordinator.rs:200-594`
- `crates/adapteros-lora-worker/src/deadlock.rs:134-264`
- `crates/adapteros-core/src/errors/network.rs:56-136`
- `crates/adapteros-core/src/worker_status.rs:5-152`
- `scripts/service-manager.sh:1473-1552`
- `docs/runbooks/WORKER_CRASH.md:34-47`
- `docs/runbooks/MEMORY_PRESSURE.md:31-41`
- `tests/executor_crash_recovery.rs:113-237`
- `tests/executor_crash_recovery.rs:370-540`
- `crates/adapteros-db/src/retry.rs:231-448`
- `crates/adapteros-server-api/src/sse/event_manager.rs:18-135`
- `crates/adapteros-server-api/src/handlers/streaming.rs:1100-1165`
- `crates/adapteros-storage/src/secure_fs/permissions.rs:210-358`
- `crates/adapteros-service-supervisor/src/service.rs:100-199`
- `crates/adapteros-service-supervisor/src/service.rs:440-498`
- `crates/adapteros-service-supervisor/src/health.rs:36-140`
- `scripts/aos-supervisor.service:1-35`
- `scripts/watchdog.sh:176-269`
- `scripts/graceful-shutdown.sh:47-219`
- `crates/adapteros-server-api-health/src/handlers.rs:1-220`
- `crates/adapteros-server-api-admin/src/handlers/lifecycle.rs:1-260`
- `crates/adapteros-server-api-training/src/routes.rs:1-120`
- `crates/adapteros-server-api-inference/src/streaming.rs:1-220`
- `crates/adapteros-server-api-models/src/handlers.rs:1-220`
- `crates/adapteros-secd/src/rotation_daemon.rs:108-220`
- `crates/adapteros-secd/src/secure_enclave.rs:165-210`
- `crates/adapteros-crypto/src/rotation_daemon.rs:222-380`
- `crates/adapteros-crypto/src/providers/keychain.rs:1610-1722`
- `crates/adapteros-artifacts/src/secd_client.rs:267-340`
- `crates/adapteros-deterministic-exec/src/lib.rs:285-344`
- `crates/adapteros-deterministic-exec/src/global_ledger.rs:226-338`
- `crates/adapteros-replay/src/session.rs:225-347`
- `crates/adapteros-replay/src/reproducible.rs:423-500`
- `crates/adapteros-core/src/circuit_breaker.rs:420-520`
- `crates/adapteros-core/src/preflight/checks.rs:16-107`
- `crates/adapteros-memory/src/pressure_manager.rs:103-220`
- `crates/adapteros-memory/src/telemetry.rs:78-180`
- `crates/adapteros-lora-mlx-ffi/src/unified_loader.rs:53-140`
- `crates/adapteros-lora-kernel-mtl/src/recovery.rs:55-190`
- `crates/adapteros-policy/src/packs/circuit_breaker.rs:24-158`
- `crates/adapteros-policy/src/packs/incident.rs:240-489`
- `crates/adapteros-system-metrics/src/alerting.rs:1216-1433`
- `crates/adapteros-lora-lifecycle/src/lib.rs:499-844`
- `crates/adapteros-lora-lifecycle/src/lib.rs:2479-2600`
- `crates/adapteros-orchestrator/src/training/service.rs:200-258`
- `crates/adapteros-db/src/documents.rs:947-1035`
- `crates/adapteros-db/src/training_jobs.rs:1971-2055`
- `crates/adapteros-db/src/adapter_repositories.rs:2140-2210`
- `crates/adapteros-db/src/adapters/mod.rs:3021-3095`

## Search Log (Read-Only)
- `rg -n "rebuild" aosctl`
- `rg -n -e "rebuild" -e "reset" -e "repair" -e "self" commands`
- `rg --hidden -n "retry|recover|self[- ]heal" crates/adapteros-error-recovery crates/adapteros-diagnostics crates/adapteros-core/src/error_helpers crates/adapteros-telemetry`
- `rg -n "supervisor|watchdog|health" crates/adapteros-core`
- `rg -n "supervisor" crates/adapteros-core crates/adapteros-node crates/adapteros-server crates/adapteros-lora-worker crates/adapteros-lora-router`
- `rg -n "health_check" crates/adapteros-core crates/adapteros-node crates/adapteros-server crates/adapteros-lora-worker crates/adapteros-lora-router`
- `rg -n "self-heal|self healing|self-debug|self recovery|incident recovery" docs scripts tests`
- `rg -n "rebuild|restart|auto recover|self debug|incident" docs scripts tests`
- `rg -n -i "self-heal|self heal|self healing|selfheal|self debugging|self debug|self-recover|recovery|retry|rebuild" crates/adapteros-aos crates/adapteros-server-api crates/adapteros-db monitoring crates/adapteros-storage`
- `rg -n "supervis" crates/adapteros-service-supervisor scripts`
- `rg -n "restart_policy" crates/adapteros-service-supervisor/src`
- `rg -n "health" crates/adapteros-api crates/adapteros-server-api-admin crates/adapteros-server-api-health crates/adapteros-server-api-inference crates/adapteros-server-api-models crates/adapteros-server-api-training`
- `rg -n "key rotation|attestation|fallback|retry|repair" crates/adapteros-secd crates/adapteros-crypto crates/adapteros-auth crates/adapteros-artifacts`
- `rg -n "deterministic|fail-safe|EnforcementMode|replay" crates/adapteros-deterministic-exec crates/adapteros-replay`
- `rg -n "memory (pressure|abort|watch)|(load|fallback|retry)|recovery|diagnostic" crates/adapteros-memory crates/adapteros-model-hub crates/adapteros-lora-kernel-mtl crates/adapteros-lora-kernel-coreml crates/adapteros-lora-mlx-ffi`
- `rg -n "circuit breaker|incident|heal" crates/adapteros-policy crates/adapteros-system-metrics crates/adapteros-metrics-exporter crates/adapteros-telemetry-types`
- `rg -n "heartbeat|reconcile|recovery|rollback" crates/adapteros-orchestrator crates/adapteros-training-worker crates/adapteros-lora-lifecycle crates/adapteros-agent-spawn`
- `rg -n "retry|idempot" crates/adapteros-db crates/adapteros-registry crates/adapteros-storage/src/repos`

## Residual Scan Risk
- Some resilience behavior may be gated by runtime flags/env settings not validated in this read-only pass.
- Operational behavior from runbooks/scripts is documented and test-backed in places, but not executed here.
- Several self-heal hooks are policy-defined but still require runtime wiring (for example supervisor restart-policy enforcement and certain eviction actions).
- One control claim is inference-backed from repository search coverage (explicitly labeled in `debug.md`) and should be re-checked after supervisor refactors.
- The rectified core narrows execution risk to five anchor-backed fixes with targeted validation paths.
- `debug.md` includes prioritized fortification actions to close these risks.
