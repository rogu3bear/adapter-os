# AdapterOS Self-Debugging Fortification Report

## Scope
This report is a read-only, evidence-backed scan of self-healing/self-debugging/rebuild behavior across CLI, runtime, control plane, storage, scripts, docs, and tests.

## Regrounded Anchor Set (Verified This Pass)
| Claim | Anchor | Grounding Note |
| --- | --- | --- |
| `aosctl` rebuild behavior is explicit-flag or missing-binary only. | `aosctl:11-18`, `aosctl:21` | Wrapper rebuilds on `--rebuild` or absent binary, then `exec`s directly. |
| Supervisor restart policy is modeled in config, with no direct runtime use found in current supervisor scan. | `crates/adapteros-service-supervisor/src/config.rs:60`, `crates/adapteros-service-supervisor/src/config.rs:94-102` | Search evidence: `rg -n "restart_policy" crates/adapteros-service-supervisor/src` returned config-only hits. |
| Supervisor HTTP health retry is fixed at 2 attempts with 100-500ms backoff. | `crates/adapteros-service-supervisor/src/service.rs:449-466` | Retry parameters are hardcoded, not sourced from `RestartPolicy`. |
| Health monitor loop records and logs transitions but does not perform remediation actions. | `crates/adapteros-service-supervisor/src/health.rs:69-107` | Loop updates statuses and emits logs on transition. |
| Deadlock recovery attempts supervisor restart and then unconditionally exits worker. | `crates/adapteros-lora-worker/src/deadlock.rs:153-162` | Exit occurs even after restart warning path. |
| SSE circuit breaker defaults are static (`threshold=5`, `recovery_timeout=30s`). | `crates/adapteros-server-api/src/handlers/streaming.rs:1118-1123`, `crates/adapteros-server-api/src/handlers/streaming.rs:1240` | No per-stream dynamic policy at these call sites. |
| Secure enclave attestation falls back to synthetic when real SEP attestation fails. | `crates/adapteros-secd/src/secure_enclave.rs:180-209` | Hardware-required mode fails closed via explicit error path. |
| Real SEP attestation path is stubbed and intentionally returns an error today. | `crates/adapteros-secd/src/secure_enclave.rs:217-219`, `crates/adapteros-secd/src/secure_enclave.rs:247-259` | Grounds synthetic fallback as current default in many environments. |
| Memory-headroom remediation uses a mock memory usage value in this module path. | `crates/adapteros-system-metrics/src/alerting.rs:1256-1259` | `get_current_memory_usage` returns `80.0` in the shown implementation. |
| Adapter eviction path currently logs eviction telemetry instead of performing SQL eviction update. | `crates/adapteros-system-metrics/src/alerting.rs:1434-1463` | SQL eviction body is commented and replaced by logging-only behavior. |
| SECD rotation daemon logs cycle failure and continues the loop. | `crates/adapteros-secd/src/rotation_daemon.rs:152-155` | Failure in cycle does not terminate daemon loop. |
| Crypto DEK re-encryption can partially fail; hard error is emitted when all retries fail. | `crates/adapteros-crypto/src/rotation_daemon.rs:500-520` | Partial failure warning and all-failed error are explicit. |
| Linux keychain backend retries Secret Service and falls back to kernel keyring. | `crates/adapteros-crypto/src/providers/keychain.rs:1625-1662`, `crates/adapteros-crypto/src/providers/keychain.rs:1676-1705` | Includes runtime health switch from Secret Service to kernel keyring. |
| Secd client uses 3-attempt exponential backoff for UDS RPC calls. | `crates/adapteros-artifacts/src/secd_client.rs:271-285` | Backoff steps are `100ms`, `200ms`, `400ms`. |
| Deterministic executor exposes `AuditOnly/Warn/Enforce` enforcement modes. | `crates/adapteros-deterministic-exec/src/lib.rs:285-295` | Enforcement posture is explicit in config model. |
| Ledger write failures are logged as divergence risk, not treated as immediate hard failure. | `crates/adapteros-deterministic-exec/src/lib.rs:736-742`, `crates/adapteros-deterministic-exec/src/lib.rs:775-780` | Runtime continues after logging error. |
| Strict determinism panics on wall-clock fallback; non-strict mode emits warning and continues. | `crates/adapteros-deterministic-exec/src/global_ledger.rs:263-284` | Behavior is feature-flag gated (`strict-determinism`). |
| Replay availability explicitly reports missing components before execution. | `crates/adapteros-replay/src/reproducible.rs:454-467`, `crates/adapteros-replay/src/reproducible.rs:481-499` | `AvailabilityCheckResult::unavailable` lists component deficits. |

## Rectified Core (Highest-Leverage Next Moves)
| Gap | Anchor | Smallest Rectification | Targeted Validation |
| --- | --- | --- | --- |
| `aosctl` may execute stale/corrupt binaries. | `aosctl:11-18`, `aosctl:21` | Add freshness/integrity check before `exec`, then one controlled rebuild retry. | Wrapper unit/smoke test for stale binary and rebuild-failure path. |
| Deadlock recovery exits immediately after restart attempt. | `crates/adapteros-lora-worker/src/deadlock.rs:153-162` | Require restart handshake success or emit durable crash artifact before process exit. | Deadlock integration test with supervisor-up and supervisor-down variants. |
| Restart policy config is not driving remediation flow. | `crates/adapteros-service-supervisor/src/config.rs:94-102`, `crates/adapteros-service-supervisor/src/service.rs:449-466`, `crates/adapteros-service-supervisor/src/health.rs:69-107` | Wire `RestartPolicy` into health-to-restart decision path with capped backoff and escalation. | Service-supervisor test proving policy-driven retries and threshold stop behavior. |
| Memory eviction path is telemetry-only in this code path. | `crates/adapteros-system-metrics/src/alerting.rs:1256-1259`, `crates/adapteros-system-metrics/src/alerting.rs:1434-1463` | Replace mock usage path and log-only eviction with concrete adapter state transition/update. | Alerting test that low headroom leads to measurable adapter state change. |
| Synthetic attestation fallback can hide trust downgrade if not surfaced. | `crates/adapteros-secd/src/secure_enclave.rs:180-209`, `crates/adapteros-secd/src/secure_enclave.rs:247-259` | Emit explicit attestation-mode telemetry/audit event and enforce policy gate for hardware-required paths. | Attestation test asserting `Hardware` vs `Synthetic` mode handling and policy rejection. |

## Existing Mechanisms (Evidence-Backed)
| Area | Mechanism | What It Does | Evidence |
| --- | --- | --- | --- |
| CLI bootstrap | `aosctl` rebuild fallback | Rebuilds `target/debug/aosctl` when missing or when `--rebuild` is passed. | `aosctl:11-21` |
| Error recovery | `ErrorRecoveryManager` strategy routing | Maps errors to recovery strategy (`Retry`, `RestoreFromBackup`, `Recreate`, `Manual`) and applies policy. | `crates/adapteros-error-recovery/src/lib.rs:145-305` |
| Error recovery | `RetryManager` exponential retry | Retries operations with progressive delay and stats tracking. | `crates/adapteros-error-recovery/src/retry.rs:13-200` |
| Telemetry durability | Diagnostics writer retry-on-failure | Keeps failed batch and retries next flush window. | `crates/adapteros-telemetry/src/diagnostics/writer.rs:210-246`, `crates/adapteros-telemetry/src/diagnostics/writer.rs:480-526` |
| Crash capture | Panic journal poison recovery | Uses poisoned-lock recovery to still persist crash context. | `crates/adapteros-telemetry/src/crash_journal.rs:126-177` |
| Server plugins | Health-check reload supervisor | Periodic plugin health checks trigger reload on failed status. | `crates/adapteros-server/src/plugin_registry.rs:43-80` |
| Worker backend resilience | GPU fallback coordinator | Detects GPU backend failure and switches to fallback path with state updates. | `crates/adapteros-lora-worker/src/backend_coordinator.rs:200-594` |
| Deadlock handling | Deadlock restart trigger | Requests supervisor restart and exits on detected deadlock. | `crates/adapteros-lora-worker/src/deadlock.rs:134-264` |
| Retry semantics | Retryable health-check error typing | Marks health-check failures as retryable. | `crates/adapteros-core/src/errors/network.rs:56-136` |
| Lifecycle safety | Worker status transition guard | Enforces legal worker status transitions with terminal states. | `crates/adapteros-core/src/worker_status.rs:5-152` |
| Ops scripts | Service manager restart + backoff loop | Restarts worker with backoff on transient failure patterns. | `scripts/service-manager.sh:1473-1552` |
| Ops runbooks | Worker crash recovery workflow | Documents detection and automated/service-managed restart flow. | `docs/runbooks/WORKER_CRASH.md:34-47` |
| Ops runbooks | Memory pressure mitigation | Documents eviction/restart actions under memory pressure. | `docs/runbooks/MEMORY_PRESSURE.md:31-41` |
| Deterministic safety tests | Crash recovery scenarios | Verifies rollback/recovery behavior after crash points. | `tests/executor_crash_recovery.rs:113-237`, `tests/executor_crash_recovery.rs:370-540` |
| DB self-healing | Classified DB retries with jitter and limits | Retries transient DB failures with class-aware backoff and max duration/attempt controls. | `crates/adapteros-db/src/retry.rs:231-448` |
| Stream resilience | SSE retry hint in events | Emits client reconnection delay (`retry`) for stream continuity. | `crates/adapteros-server-api/src/sse/event_manager.rs:18-135` |
| Stream resilience | SSE circuit breaker | Opens breaker after repeated errors and only allows timed probe recovery. | `crates/adapteros-server-api/src/handlers/streaming.rs:1100-1165` |
| Storage self-repair | Permission-fix + retry helpers | Attempts secure permission correction and retries open/create operations. | `crates/adapteros-storage/src/secure_fs/permissions.rs:210-358` |

## Deep Expedition Additions (Pass 2)
| Area | Mechanism | What It Does | Evidence |
| --- | --- | --- | --- |
| Service supervision | Managed service restart/log plumbing | Tracks restart counts, captures stdout/stderr logs, and exposes health-check helpers with retries. | `crates/adapteros-service-supervisor/src/service.rs:100-199`, `crates/adapteros-service-supervisor/src/service.rs:440-498` |
| Service supervision | Health monitor loop | Polls registered services and records health transitions on interval. | `crates/adapteros-service-supervisor/src/health.rs:36-140` |
| Infra supervisor | systemd always-restart unit | Keeps supervisor daemon alive with `Restart=always` and post-start active check. | `scripts/aos-supervisor.service:1-35` |
| Startup diagnostics | Watchdog readiness probe | Waits on backend/worker/UI readiness and dumps logs on timeout. | `scripts/watchdog.sh:176-269` |
| API resilience | Boot/ready/invariant health endpoints | Exposes boot trace IDs, failed phases, critical vs non-critical degraded status, and readiness probes. | `crates/adapteros-server-api-health/src/handlers.rs:1-220` |
| Admin recovery control | Maintenance + safe restart handlers | Supports maintenance signaling and guarded restart operations from admin API. | `crates/adapteros-server-api-admin/src/handlers/lifecycle.rs:1-260` |
| Training recovery API | Retry route | Exposes explicit training retry endpoint for failed jobs. | `crates/adapteros-server-api-training/src/routes.rs:1-120` |
| Security/key resilience | Rotation daemons (scheduled/manual/emergency) | Rotates keys with loop resilience, receipts, and re-encryption flow. | `crates/adapteros-secd/src/rotation_daemon.rs:108-220`, `crates/adapteros-crypto/src/rotation_daemon.rs:222-380` |
| Security/key resilience | Linux keyring backend fallback | Retries Secret Service and falls back to kernel keyring on health issues. | `crates/adapteros-crypto/src/providers/keychain.rs:1610-1722` |
| Security/key resilience | SECD RPC retry wrapper | Retries secd UDS requests with exponential backoff. | `crates/adapteros-artifacts/src/secd_client.rs:267-340` |
| Attestation continuity | Synthetic attestation fallback | Falls back to synthetic attestation when real SEP call fails/unavailable. | `crates/adapteros-secd/src/secure_enclave.rs:165-210` |
| Deterministic execution | Enforcement modes + deterministic tick guard | Uses enforcement modes and deterministic ledger/timestamp checks to fail-safe/warn on drift. | `crates/adapteros-deterministic-exec/src/lib.rs:285-344`, `crates/adapteros-deterministic-exec/src/global_ledger.rs:226-338` |
| Replay safety | Signature and component availability checks | Validates event hashes/signatures and detects missing prerequisites before replaying. | `crates/adapteros-replay/src/session.rs:225-347`, `crates/adapteros-replay/src/reproducible.rs:423-500` |
| Core protection | Generic circuit breaker + preflight rails | Applies open/half-open protections and centralized preflight checks with bypass auditing. | `crates/adapteros-core/src/circuit_breaker.rs:420-520`, `crates/adapteros-core/src/preflight/checks.rs:16-107` |
| Memory/model resilience | Pressure manager + model load fallback | Eviction/K-reduction under pressure and MLX-to-Rust loader fallback for model load failures. | `crates/adapteros-memory/src/pressure_manager.rs:103-220`, `crates/adapteros-lora-mlx-ffi/src/unified_loader.rs:53-140` |
| Backend crash recovery | Panic-safe kernel recovery wrapper | Catches panic, marks degraded, attempts command queue rebuild and recovery probes. | `crates/adapteros-lora-kernel-mtl/src/recovery.rs:55-190` |
| Policy-driven resilience | Circuit-breaker and incident policy packs | Defines service-specific breaker thresholds and incident escalation workflows. | `crates/adapteros-policy/src/packs/circuit_breaker.rs:24-158`, `crates/adapteros-policy/src/packs/incident.rs:240-489` |
| Metric-triggered remediation | Memory headroom alert + eviction ordering | Detects low headroom and runs policy-ordered eviction attempts with telemetry. | `crates/adapteros-system-metrics/src/alerting.rs:1216-1433` |
| Lifecycle reconciliation | K-reduction rollback + crash recovery + stale heartbeat recovery | Rolls back failed K-reduction, repairs stale lifecycle states after crash, and recovers stale heartbeat adapters. | `crates/adapteros-lora-lifecycle/src/lib.rs:499-844`, `crates/adapteros-lora-lifecycle/src/lib.rs:2479-2600` |
| Training recovery | Orphaned running-job recovery | Marks stale running jobs interrupted at startup to prevent permanent ghost jobs. | `crates/adapteros-orchestrator/src/training/service.rs:200-258` |
| DB resilience | Retry gating, stale-processing reset, conflict-safe updates, idempotent registration | Guards retries and conflicts while enabling stale state reset and safe duplicate suppression. | `crates/adapteros-db/src/documents.rs:947-1035`, `crates/adapteros-db/src/training_jobs.rs:1971-2055`, `crates/adapteros-db/src/adapter_repositories.rs:2140-2210`, `crates/adapteros-db/src/adapters/mod.rs:3021-3095` |

## Fortification Priorities
These priorities are anchored to the regrounded evidence above. The "Rectified Core" section is the recommended implementation order.

| Priority | Gap | Why It Matters | Smallest Strategic Hardening |
| --- | --- | --- | --- |
| P0 | `aosctl` rebuild path only checks missing binary or explicit flag. | Corrupt/stale binaries can fail late and confuse operators. | Add binary integrity/staleness check and one controlled rebuild retry with clear exit diagnostics. |
| P0 | Supervisor loops (plugins/service manager) can thrash on persistent failures. | Tight restart loops create noisy instability and hide root cause. | Add capped exponential backoff + failure counters + escalation signal after threshold. |
| P0 | Deadlock recovery exits even when supervisor restart path may be unavailable. | Can turn recoverable deadlock into hard downtime. | Verify restart handshake before exit; if unavailable, emit structured crash artifact and fallback recovery mode. |
| P1 | Retry behavior is scattered (error-recovery, DB, health checks, scripts). | Inconsistent retry policy can cause retry storms or silent stalls. | Introduce shared retry policy envelope (jitter, max elapsed time, classification hooks, telemetry tags). |
| P1 | Diagnostics retry persists failed batch but lacks explicit stale-batch escalation. | Silent diagnostic loss risk during prolonged persistence outage. | Add age/attempt thresholds and explicit alerting for undelivered telemetry batches. |
| P1 | SSE circuit breaker has hardcoded thresholds and limited observability. | Stream issues may degrade quietly or overreact in some workloads. | Make thresholds configurable and export breaker state metrics/events for operations. |
| P1 | Permission self-repair retries once with limited audit visibility. | Persistent permission drift remains opaque and manual-heavy. | Add audit trail + repeated-failure escalation path (operator message or remediation job hook). |
| P1 | Supervisor restart policy config is not fully wired into autonomous remediation loops. | Health checks may detect issues without taking corrective action, creating pseudo-self-healing gaps. | Bind `restart_policy` to automatic restart decisions and emit escalation after thresholded failures. |
| P1 | Attestation path can fall back to synthetic data without a hard operator signal. | Security posture may appear healthy while hardware-backed guarantees are absent. | Emit explicit attestation mode telemetry/audit event and gate high-trust workflows on hardware attestation availability. |
| P1 | SECD/key rotation flows can log and continue through repeated provider failures. | Silent long-tail key-rotation failure risk grows over time. | Add provider health counters, failure budgets, and hard alerting when consecutive failures exceed policy. |
| P1 | Memory-headroom remediation hooks stop short of guaranteed production eviction in some paths. | Alerting can fire repeatedly without deterministic remediation completion. | Wire alert evaluator output to concrete eviction executor and verify with end-to-end resilience tests. |
| P2 | Runbook behavior and runtime behavior are partially decoupled. | Drift between docs/scripts/runtime can slow incident response. | Add a periodic "recovery contract" check that validates runbook expectations against live flags/config. |
| P2 | Worker status invariants are local; cross-system reconciliation is limited. | DB/runtime divergence can accumulate unnoticed. | Add reconciliation watchdog for persisted worker rows vs actual runtime state. |
| P2 | Startup watchdog and graceful shutdown scripts are not tightly integrated with runtime self-heal loops. | Recovery posture depends on manual operations during extended incidents. | Integrate scripts with supervisor APIs/events or clearly enforce automation ownership in one control plane. |

## Focused Hardening TODO (Implementation-Ready)
- [ ] Build `aosctl doctor/rebuild-check` path that validates binary freshness/integrity before exec.
- [ ] Unify retry/backoff primitives for supervisor + runtime + DB with shared telemetry labels.
- [ ] Add restart-loop circuit breaker with escalation event payload (reason, retry_count, elapsed).
- [ ] Add deadlock recovery pre-exit verification and durable diagnostic dump.
- [ ] Add stale diagnostics batch alerts and retention safety guardrails.
- [ ] Expose SSE breaker metrics and configurable thresholds.
- [ ] Add periodic worker-state reconciliation job between DB and runtime.
- [ ] Wire `adapteros-service-supervisor` restart policies into automatic remediation and threshold escalation.
- [ ] Add explicit attestation-mode visibility and trust gating when synthetic fallback is active.
- [ ] Harden key-rotation with consecutive-failure budgets and provider health alarms.
- [ ] Validate metric-triggered eviction path end-to-end so headroom alerts produce deterministic remediation.
- [ ] Add replay/availability operator report path when required components are missing pre-replay.

## Residual Risk
- This pass was intentionally read-only and did not execute fault-injection tests.
- Some recovery paths are configuration-gated and were not runtime-validated here.
- The next safe step is implementing one P0 item at a time with targeted tests per subsystem.
