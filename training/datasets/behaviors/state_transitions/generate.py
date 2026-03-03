#!/usr/bin/env python3
"""
Generator for state_transitions training dataset.

Adapter lifecycle states: Unloaded → Cold → Warm → Hot → Resident
850 examples across 4 categories.
"""

import json
import random
from typing import Any

random.seed(42)

# ── Domain constants ──────────────────────────────────────────────────────────

STATES = ["Unloaded", "Cold", "Warm", "Hot", "Resident"]

STATE_MEMORY = {
    "Unloaded": 0,
    "Cold": 100,
    "Warm": 150,
    "Hot": 200,
    "Resident": 200,
}

STATE_ACTIVATION_MS = {
    "Unloaded": 500,
    "Cold": 50,
    "Warm": 5,
    "Hot": 1,
    "Resident": 0,  # pinned, always ready
}

ADAPTER_PREFIXES = [
    "sentiment-v2", "code-assist", "legal-summarizer", "med-qa", "finance-extractor",
    "doc-classifier", "chat-persona", "math-solver", "translation-fr", "translation-de",
    "translation-es", "translation-ja", "translation-zh", "safety-filter", "tone-adjuster",
    "sql-gen", "test-writer", "refactor-aide", "commit-msg", "pr-reviewer",
    "customer-svc", "ticket-triage", "summarizer-short", "summarizer-long", "rag-fusion",
    "entity-extractor", "slot-filler", "dialogue-state", "reranker-v3", "embedder-v1",
    "bias-checker", "toxicity-filter", "paraphrase-v2", "headline-gen", "abstract-writer",
    "clinical-notes", "icd-coder", "drug-interaction", "audit-log-analyzer", "compliance-scan",
]

def adapter_id(n: int) -> str:
    prefix = ADAPTER_PREFIXES[n % len(ADAPTER_PREFIXES)]
    return f"{prefix}-{n:04d}"

def ts(minutes_ago: int = 0, base: str = "2026-02-26T10:00:00Z") -> str:
    """Return a rough ISO timestamp offset from base."""
    from datetime import datetime, timedelta, timezone
    dt = datetime(2026, 2, 26, 10, 0, 0, tzinfo=timezone.utc) - timedelta(minutes=minutes_ago)
    return dt.strftime("%Y-%m-%dT%H:%M:%SZ")

# ── Category 1: Valid Promotions (200 examples) ───────────────────────────────

def gen_valid_promotions() -> list[dict[str, Any]]:
    examples: list[dict[str, Any]] = []

    # Cold → Warm: activation_pct > 30%
    cold_to_warm_inputs = [
        # Varied activation_pct, request counts, adapter names, observation windows
        (32, 85, "30s window, steady traffic spike"),
        (35, 120, "burst of 120 inference requests in 30s"),
        (38, 200, "sustained read-heavy workload from RAG pipeline"),
        (42, 310, "batch job driving repeated activations"),
        (45, 400, "live API traffic from mobile clients"),
        (50, 500, "high-frequency slot-filling loop"),
        (55, 640, "pipeline stage processing document queue"),
        (60, 710, "concurrency of 8 parallel inference workers"),
        (62, 800, "real-time scoring from event stream"),
        (65, 900, "peak-hour usage pattern detected"),
        (68, 950, "A/B experiment with this adapter selected"),
        (70, 1000, "shadow mode: matched primary adapter requests"),
        (72, 1050, "evaluation harness running repeated passes"),
        (75, 1100, "customer facing feature rollout at 10% traffic"),
        (78, 1150, "monitoring detected steady-state above threshold"),
        (80, 1200, "canary deployment receiving increasing traffic"),
        (85, 1300, "load test with ramp-up profile"),
        (88, 1400, "analytics workload processing hourly batch"),
        (90, 1500, "full traffic shift after blue-green promotion"),
        (95, 1600, "new feature GA: all traffic routing to adapter"),
        (31, 90, "just above 30% threshold, marginal promotion"),
        (33, 100, "incremental traffic growth over 5 minutes"),
        (36, 130, "second request wave hitting cold adapter"),
        (39, 160, "pattern: 3 activations per second sustained"),
        (41, 190, "warm-up requests from prefetch signal"),
        (46, 220, "multi-tenant traffic sharing this adapter"),
        (48, 250, "downstream service polling at 2 rps"),
        (51, 280, "orchestrator pre-warming based on schedule"),
        (54, 320, "high-priority queue draining through adapter"),
        (57, 360, "activation_pct crossed threshold at T+45s"),
    ]

    for i, (pct, reqs, context) in enumerate(cold_to_warm_inputs):
        aid = adapter_id(i)
        inp = (
            f"Adapter '{aid}' is in state Cold (memory: {STATE_MEMORY['Cold']} MB, "
            f"activation_pct: {pct}%). Observation: {context}. "
            f"Total inference requests in window: {reqs}. "
            f"Promotion threshold Cold→Warm is 30%. Evaluate state transition."
        )
        target = (
            f"Promote '{aid}' from Cold to Warm. "
            f"activation_pct={pct}% exceeds the Cold→Warm threshold of 30%. "
            f"Memory will increase from {STATE_MEMORY['Cold']} MB to {STATE_MEMORY['Warm']} MB (+50 MB). "
            f"Activation latency improves from ~50ms to ~5ms. "
            f"No state steps are skipped; Cold→Warm is a valid sequential promotion."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.88 + random.uniform(0, 0.10), 2),
                "label": "positive",
                "subcategory": "cold_to_warm",
            },
        })

    # Warm → Hot: activation_pct > 60%
    warm_to_hot_inputs = [
        (62, 1100, "sustained inference load from production pipeline"),
        (65, 1250, "high-priority queue draining continuously"),
        (68, 1400, "real-time fraud detection, latency SLA < 2ms"),
        (70, 1500, "live chat: adapter handles all session state"),
        (72, 1600, "recommendation engine at peak evening load"),
        (75, 1700, "automated testing harness hitting adapter"),
        (78, 1800, "migration: old model retired, traffic shifted here"),
        (80, 1900, "GPU-backed pipeline: adapter is hot path"),
        (82, 2000, "concurrency=16, adapter utilization at 80%"),
        (85, 2100, "after canary: full rollout drives high activation"),
        (87, 2200, "event-driven processing, 500 events/minute"),
        (90, 2400, "peak Black Friday traffic on e-commerce adapter"),
        (92, 2600, "CI/CD evaluation loop: 90 inferences per minute"),
        (94, 2800, "streaming inference: 100 tokens/s per user × 30 users"),
        (95, 3000, "production traffic: this adapter is primary path"),
        (61, 1050, "just above 60% threshold, borderline promotion"),
        (63, 1150, "gradual traffic increase tipping threshold"),
        (66, 1300, "second wave of daily traffic peak"),
        (69, 1420, "multi-region traffic aggregated above threshold"),
        (73, 1580, "warm state for 45 minutes, consistently above 60%"),
        (76, 1680, "orchestrator detects prolonged high activation"),
        (79, 1780, "predictive scaling: pre-promote before peak"),
        (81, 1880, "SLA enforcement: Hot state required for < 2ms"),
        (83, 1990, "analytics: adapter in critical path of dashboard"),
        (86, 2150, "feature flag fully enabled, all users hitting adapter"),
        (88, 2250, "load spike from scheduled report generation"),
        (91, 2450, "peak usage pattern matches historical model"),
        (93, 2650, "activation_pct stable at 93% for 10 minutes"),
        (96, 2900, "burst from external event: adapter essential"),
        (98, 3100, "maximum observed activation in production"),
    ]

    for i, (pct, reqs, context) in enumerate(warm_to_hot_inputs):
        aid = adapter_id(100 + i)
        inp = (
            f"Adapter '{aid}' is in state Warm (memory: {STATE_MEMORY['Warm']} MB, "
            f"activation_pct: {pct}%). Context: {context}. "
            f"Requests in observation window: {reqs}. "
            f"Promotion threshold Warm→Hot is 60%. Evaluate state transition."
        )
        target = (
            f"Promote '{aid}' from Warm to Hot. "
            f"activation_pct={pct}% exceeds the Warm→Hot threshold of 60%. "
            f"Memory increases from {STATE_MEMORY['Warm']} MB to {STATE_MEMORY['Hot']} MB (+50 MB). "
            f"Activation latency improves from ~5ms to ~1ms. "
            f"Warm→Hot is a valid sequential promotion with no skipped states."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.88 + random.uniform(0, 0.10), 2),
                "label": "positive",
                "subcategory": "warm_to_hot",
            },
        })

    # Hot → Resident: explicit pin request
    hot_to_resident_inputs = [
        ("ops team", "critical inference path, 99.99% SLA required"),
        ("deployment automation", "blue-green cutover: adapter must not be evicted"),
        ("admin API call", "customer contract requires sub-1ms guaranteed latency"),
        ("scheduler", "nightly batch job requires adapter pinned during 2-hour window"),
        ("orchestrator", "model ensemble: this adapter is fixed anchor"),
        ("SRE runbook", "incident recovery: pin adapter to prevent flap"),
        ("policy engine", "compliance workload: eviction would violate audit trail"),
        ("warmup cron", "pre-pinning before predicted traffic surge at 09:00"),
        ("traffic manager", "A/B test: this adapter must remain stable for 24h"),
        ("feature flag system", "experiment requires consistent latency across all users"),
        ("load balancer", "sticky sessions: users pinned to this adapter variant"),
        ("capacity planner", "reserved capacity for tier-1 customer"),
        ("chaos controller", "fault injection disabled for this adapter during test"),
        ("deployment gate", "canary analysis: adapter must not be displaced"),
        ("platform team", "infrastructure constraint: only resident adapters allowed in region"),
        ("monitoring system", "adapter is golden signal baseline, must not drift"),
        ("benchmark harness", "performance test requires deterministic cold-start elimination"),
        ("security scanner", "adapter under review: must remain pinned for forensics"),
        ("rate limiter", "quota-protected adapter: resident to prevent cold starts on burst"),
        ("cost optimizer", "resident state chosen over repeated load cycle cost"),
        ("workflow engine", "long-running job requires adapter stable for 6 hours"),
        ("MLOps pipeline", "model comparison: both adapters pinned for fair evaluation"),
        ("inference gateway", "hot path protection for payment processing adapter"),
        ("data pipeline", "streaming ETL: adapter must not evict mid-stream"),
        ("alert rule", "SLO breach prevention: pin adapter when p99 > 5ms"),
        ("rollout controller", "staged rollout: adapter pinned at 50% traffic for 1h"),
        ("partner integration", "external SLA: partner traffic requires < 1ms guaranteed"),
        ("CI runner", "integration test suite: pin adapter for test isolation"),
        ("quota manager", "high-value tenant: always-resident policy applied"),
        ("manual operator", "explicit pin: operator knows this adapter will spike"),
        ("failover controller", "failover scenario: pin secondary adapter as standby"),
        ("compliance module", "GDPR workload: adapter pinned for audit window"),
        ("resource allocator", "memory headroom allows pinning without pressure"),
        ("service mesh", "sidecar detected sub-1ms requirement: escalated to pin"),
        ("test fixture", "determinism test: Resident state ensures no eviction variance"),
        ("traffic spike predictor", "ML model predicts 10x spike in 5 minutes: pin now"),
        ("admin CLI", "operator ran: aos adapter pin --id {aid}"),
        ("quota enforcer", "platinum tier user: resident adapter is baseline entitlement"),
        ("event processor", "real-time event stream: latency spikes from eviction unacceptable"),
        ("config reload", "adapter config update requires resident state before hot-reload"),
    ]

    for i, (requester, reason) in enumerate(hot_to_resident_inputs):
        aid = adapter_id(200 + i)
        inp = (
            f"Adapter '{aid}' is in state Hot (memory: {STATE_MEMORY['Hot']} MB, "
            f"activation_pct: {random.randint(70, 99)}%). "
            f"Pin request from: {requester}. Reason: {reason}. "
            f"Evaluate transition to Resident state."
        )
        target = (
            f"Pin '{aid}' from Hot to Resident. "
            f"Hot→Resident is a valid explicit-pin transition. "
            f"Memory remains at {STATE_MEMORY['Resident']} MB (no change). "
            f"Adapter becomes eviction-proof; it will not be displaced by memory pressure. "
            f"Activation latency remains ~1ms but is now guaranteed (no eviction window). "
            f"Requester '{requester}' must explicitly call unpin to allow demotion."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.90 + random.uniform(0, 0.08), 2),
                "label": "positive",
                "subcategory": "hot_to_resident",
            },
        })

    assert len(examples) == 100, f"Expected 100 promotion examples, got {len(examples)}"

    # Generate remaining 100 by varying the above patterns with different adapter IDs and params
    extra: list[dict[str, Any]] = []

    # Extra Cold→Warm with varied memory pressure context
    for i in range(35):
        pct = random.randint(31, 95)
        reqs = random.randint(80, 1600)
        memory_pressure = random.randint(40, 84)
        aid = adapter_id(300 + i)
        inp = (
            f"Adapter '{aid}' in Cold state. activation_pct={pct}%, requests={reqs}, "
            f"system memory pressure={memory_pressure}% (below eviction threshold of 85%). "
            f"Cold→Warm threshold is 30%. Should this adapter be promoted?"
        )
        target = (
            f"Yes, promote '{aid}' from Cold to Warm. "
            f"activation_pct={pct}% clears the 30% threshold. "
            f"Memory pressure at {memory_pressure}% is below the 85% eviction threshold, "
            f"so promotion is safe. Memory: {STATE_MEMORY['Cold']} MB → {STATE_MEMORY['Warm']} MB. "
            f"Latency: ~50ms → ~5ms."
        )
        extra.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.87 + random.uniform(0, 0.11), 2),
                "label": "positive",
                "subcategory": "cold_to_warm",
            },
        })

    # Extra Warm→Hot with SLA framing
    for i in range(35):
        pct = random.randint(61, 99)
        sla_ms = random.choice([1, 2, 3])
        aid = adapter_id(350 + i)
        inp = (
            f"Adapter '{aid}' in Warm state. activation_pct={pct}%. "
            f"SLA requirement: p99 inference latency ≤ {sla_ms}ms. "
            f"Current Warm latency ~5ms exceeds SLA. Warm→Hot threshold is 60%. "
            f"Evaluate transition."
        )
        target = (
            f"Promote '{aid}' from Warm to Hot. "
            f"activation_pct={pct}% satisfies the 60% threshold. "
            f"Hot state provides ~1ms activation latency, meeting the {sla_ms}ms SLA. "
            f"Memory: {STATE_MEMORY['Warm']} MB → {STATE_MEMORY['Hot']} MB (+50 MB). "
            f"Sequential promotion; no steps skipped."
        )
        extra.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.89 + random.uniform(0, 0.09), 2),
                "label": "positive",
                "subcategory": "warm_to_hot",
            },
        })

    # Extra Hot→Resident with time-bound pin context
    for i in range(30):
        duration_h = random.randint(1, 24)
        aid = adapter_id(400 + i)
        pct = random.randint(70, 99)
        inp = (
            f"Adapter '{aid}' in Hot state (activation_pct={pct}%). "
            f"Time-bound pin requested for {duration_h} hour(s). "
            f"After pin window expires, adapter should be evaluated for demotion. "
            f"Is Hot→Resident transition valid?"
        )
        target = (
            f"Yes, Hot→Resident is a valid explicit-pin transition for '{aid}'. "
            f"Memory stays at {STATE_MEMORY['Resident']} MB. "
            f"Adapter is eviction-proof for the {duration_h}-hour window. "
            f"At pin expiry, the system re-evaluates: if activation_pct < 60%, demote to Warm; "
            f"if activation_pct < 30%, demote to Cold. "
            f"Resident→Hot demotion requires explicit unpin call."
        )
        extra.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.90 + random.uniform(0, 0.08), 2),
                "label": "positive",
                "subcategory": "hot_to_resident",
            },
        })

    examples.extend(extra)
    assert len(examples) == 200, f"Expected 200 promotion examples, got {len(examples)}"
    return examples


# ── Category 2: Valid Demotions (200 examples) ────────────────────────────────

def gen_valid_demotions() -> list[dict[str, Any]]:
    examples: list[dict[str, Any]] = []

    # Hot → Warm: >15 min inactive
    hot_to_warm_scenarios = [
        (16, 0, "traffic dropped after batch job completed"),
        (18, 2, "adapter idle since last inference request"),
        (20, 0, "off-peak hours, no scheduled jobs"),
        (22, 5, "feature flag disabled, routing removed"),
        (25, 0, "A/B test concluded, traffic shifted away"),
        (30, 8, "upstream service deployed and stopped calling adapter"),
        (35, 0, "scheduled maintenance window: zero traffic"),
        (40, 3, "canary retired, traffic returned to primary model"),
        (45, 0, "nightly lull: no user activity"),
        (50, 10, "service deprecation: downstream calls dropped to zero"),
        (60, 0, "experiment ended, adapter no longer selected"),
        (90, 0, "idle since business day ended"),
        (120, 5, "weekend: no weekday traffic pattern"),
        (180, 0, "adapter provisioned speculatively, never heavily used"),
        (240, 3, "auto-scaling: load balancer stopped routing to this replica"),
        (16, 1, "single stale request counted; effectively idle"),
        (17, 0, "burst traffic passed, adapter cooling"),
        (19, 4, "rate limiter triggered, adapter paused"),
        (21, 0, "grace period elapsed with no new activations"),
        (23, 6, "traffic manager rerouted to lower-latency replica"),
        (26, 0, "downstream queue drained, no more work"),
        (28, 2, "shadow mode ended, no primary traffic"),
        (32, 0, "SLA tier downgraded: Hot no longer required"),
        (36, 7, "deployment rollback: traffic reverted to old model"),
        (38, 0, "user cohort left A/B test, no activations"),
        (42, 1, "integration test completed, adapter released"),
        (46, 0, "CI pipeline finished, no more test traffic"),
        (55, 9, "partner API rate-limited, traffic stopped"),
        (70, 0, "quarterly report generated, batch complete"),
        (100, 4, "adapter kept warm during incident, now resolved"),
        (15 + 1, 0, "exactly 1 minute past 15-minute threshold"),
        (16, 0, "minimum qualifying inactivity for Hot→Warm"),
        (18, 0, "rapid cooldown after spike"),
        (19, 0, "predictive: scheduler signals no work for 20+ min"),
        (21, 0, "manual trigger: operator initiated cooldown"),
        (24, 0, "orchestrator rebalancing: demote to recover memory"),
        (27, 0, "traffic shaping: adapter no longer in SLA window"),
        (33, 0, "time-of-day policy: Hot reserved for peak hours only"),
        (37, 0, "quota enforcer: Hot tier reserved, tenant downgraded"),
        (41, 0, "health check: adapter healthy but idle, safe to demote"),
    ]

    for i, (idle_min, stale_reqs, context) in enumerate(hot_to_warm_scenarios):
        aid = adapter_id(500 + i)
        activation_pct = max(0, random.randint(0, 20))
        inp = (
            f"Adapter '{aid}' is in state Hot (memory: {STATE_MEMORY['Hot']} MB). "
            f"Last inference request was {idle_min} minutes ago. "
            f"Stale pending requests: {stale_reqs}. "
            f"Current activation_pct: {activation_pct}%. Context: {context}. "
            f"Hot→Warm demotion threshold: >15 minutes inactive. Evaluate."
        )
        target = (
            f"Demote '{aid}' from Hot to Warm. "
            f"Inactivity duration of {idle_min} minutes exceeds the 15-minute Hot→Warm threshold. "
            f"Memory: {STATE_MEMORY['Hot']} MB → {STATE_MEMORY['Warm']} MB (-50 MB recovered). "
            f"Activation latency increases from ~1ms to ~5ms. "
            f"Hot→Warm is a valid sequential demotion; adapter remains in memory. "
            f"If inactivity continues past 30 minutes total, evaluate Warm→Cold demotion."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.88 + random.uniform(0, 0.10), 2),
                "label": "positive",
                "subcategory": "hot_to_warm",
            },
        })

    # Warm → Cold: >30 min inactive
    warm_to_cold_scenarios = [
        (31, "traffic never recovered after Hot→Warm demotion"),
        (35, "off-peak: no activations since 2am"),
        (40, "feature turned off: no calls since deployment"),
        (45, "migration complete: old adapter no longer needed"),
        (50, "experiment finished with no traffic scheduled"),
        (60, "service in maintenance: zero inbound requests"),
        (75, "nightly lull: adapter demoted from Hot 60 minutes ago"),
        (90, "weekend: activity dropped 98% from weekday baseline"),
        (120, "adapter loaded speculatively, never promoted past Cold"),
        (150, "kept Warm for quick recall, now too long idle"),
        (180, "daily batch complete: 3-hour post-job idle"),
        (240, "standby adapter: not called in 4 hours"),
        (360, "DR standby: idle 6 hours, safe to release memory"),
        (31, "exactly 1 minute past threshold: minimum qualifying"),
        (32, "second check confirms inactivity, demote now"),
        (36, "inactivity growing: no signal of incoming traffic"),
        (38, "quota enforcer: Warm tier reserved for active tenants"),
        (42, "memory reclaim priority: this adapter has lowest score"),
        (48, "time-of-day policy: Warm reserved for business hours"),
        (55, "health check: adapter healthy, idle, demote to Cold"),
        (65, "auto-scale down: reduce memory footprint"),
        (80, "SLO met at Cold for this traffic tier"),
        (100, "long-tail adapter: infrequent use, Cold is appropriate"),
        (130, "adapter transitioned to Cold for cost efficiency"),
        (160, "resource rebalancing: Cold frees 50 MB for hot adapters"),
        (31, "canary test adapter: test window closed"),
        (33, "shadow mode: metrics collected, adapter released"),
        (37, "partner integration idle: partner not calling"),
        (44, "CI runner released adapter after test suite"),
        (53, "manual demotion: operator request to reclaim memory"),
        (31, "inactivity policy triggered automatically"),
        (34, "orchestrator scheduled demotion during low-traffic window"),
        (39, "adaptive threshold: 30-min rule applied"),
        (47, "memory headroom request: this adapter demoted first"),
        (58, "adapter ranked lowest by access frequency score"),
        (70, "rolling demotion: system cycling idle adapters to Cold"),
        (85, "predictive: no traffic expected for 90 minutes"),
        (110, "adapter lifecycle: standard Cold archival after Warm idle"),
        (140, "memory manager: Cold allows 50 MB reclamation"),
        (200, "extended idle: Warm→Cold is overdue"),
    ]

    for i, (idle_min, context) in enumerate(warm_to_cold_scenarios):
        aid = adapter_id(600 + i)
        activation_pct = max(0, random.randint(0, 15))
        inp = (
            f"Adapter '{aid}' is in state Warm (memory: {STATE_MEMORY['Warm']} MB, "
            f"activation_pct: {activation_pct}%). "
            f"Last inference request: {idle_min} minutes ago. "
            f"Context: {context}. "
            f"Warm→Cold demotion threshold: >30 minutes inactive. Evaluate."
        )
        target = (
            f"Demote '{aid}' from Warm to Cold. "
            f"Inactivity duration of {idle_min} minutes exceeds the 30-minute Warm→Cold threshold. "
            f"Memory: {STATE_MEMORY['Warm']} MB → {STATE_MEMORY['Cold']} MB (-50 MB recovered). "
            f"Activation latency increases from ~5ms to ~50ms. "
            f"Warm→Cold is a valid sequential demotion; adapter remains in memory. "
            f"If inactivity continues or memory pressure rises, Cold→Unloaded eviction may follow."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.88 + random.uniform(0, 0.10), 2),
                "label": "positive",
                "subcategory": "warm_to_cold",
            },
        })

    # Resident → Hot: explicit unpin (20 examples)
    for i in range(20):
        aid = adapter_id(700 + i)
        unpin_reasons = [
            "pin TTL expired after scheduled window",
            "operator explicitly unpinned via CLI: aos adapter unpin",
            "batch job completed, pin no longer needed",
            "A/B test window closed, traffic policy removed",
            "incident resolved, emergency pin lifted",
            "capacity rebalancing: unpin to allow eviction candidacy",
            "compliance audit completed, pin released",
            "deployment complete, adapter re-enters normal lifecycle",
            "cost optimization: Resident tier too expensive for current usage",
            "auto-unpin triggered by inactivity monitor after grace period",
            "SLA tier downgraded: Hot sufficient, Resident not required",
            "partner SLA expired, pin entitlement revoked",
            "manual unpin: operator reclaiming memory for new adapter",
            "rolling restart: adapter unpinned for safe reload",
            "policy engine revoked pin due to activation drop",
            "pin transferred to upgraded adapter version",
            "memory pressure critical: admin forcibly unpinned",
            "test isolation ended: pin released by test framework",
            "quota enforcer: pinned slot returned to pool",
            "scheduler: time-bound pin expired at scheduled time",
        ]
        reason = unpin_reasons[i % len(unpin_reasons)]
        pct = random.randint(40, 95)
        inp = (
            f"Adapter '{aid}' is in state Resident (memory: {STATE_MEMORY['Resident']} MB, "
            f"activation_pct: {pct}%). "
            f"Unpin event: {reason}. "
            f"Evaluate transition after unpin."
        )
        target = (
            f"Transition '{aid}' from Resident to Hot after unpin. "
            f"Resident→Hot is the valid post-unpin demotion. "
            f"Memory remains at {STATE_MEMORY['Hot']} MB (no immediate change). "
            f"Adapter re-enters normal lifecycle: it is now an eviction candidate. "
            f"If activation_pct ({pct}%) stays above 60%, adapter remains in Hot. "
            f"If inactivity > 15 minutes, evaluate Hot→Warm. "
            f"Reason for unpin: {reason}."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.90 + random.uniform(0, 0.08), 2),
                "label": "positive",
                "subcategory": "resident_to_hot",
            },
        })

    # Pad to 200: generate extra Hot→Warm and Warm→Cold variants
    extra: list[dict[str, Any]] = []

    # 50 more Hot→Warm with varied idle durations and contexts
    extra_hot_warm_contexts = [
        (16, "workload shifted to new adapter variant"),
        (17, "scheduled downtime in downstream service"),
        (18, "traffic shaping policy reduced this adapter's share"),
        (19, "API version deprecated: client traffic migrated"),
        (20, "experiment phase ended, adapter released"),
        (22, "rate limiter engaged, no new requests allowed"),
        (24, "circuit breaker opened: downstream error rate high"),
        (26, "health check revealed upstream issue, traffic cut"),
        (28, "CRON job finished: batch workload complete"),
        (30, "partner webhook delivery stopped"),
        (32, "SLA window closed: adapter freed until next window"),
        (34, "adaptive routing moved traffic to lower-cost replica"),
        (36, "blue adapter took over from green: this adapter idle"),
        (38, "multi-region failover: traffic moved to other region"),
        (40, "config reload: adapter suspended during validation"),
        (42, "model comparison test concluded"),
        (44, "nightly retraining window: inference paused"),
        (46, "user session ended: adapter no longer in request path"),
        (48, "streaming pipeline paused: no events flowing"),
        (50, "quota exhausted: tenant requests rejected, adapter idle"),
        (52, "feature rollback: traffic returned to stable model"),
        (54, "latency spike triggered adaptive routing away"),
        (56, "monitoring alert: traffic paused pending investigation"),
        (58, "weekend traffic pattern: 95% reduction"),
        (60, "batch window not yet started: adapter waiting"),
        (62, "API gateway rate limit engaged"),
        (64, "circuit breaker reset: traffic will return shortly"),
        (66, "orchestrator pre-emptive demotion for memory headroom"),
        (68, "deployment lock: adapter frozen pre-release"),
        (70, "inactive since scheduled maintenance window opened"),
        (72, "traffic zero for 72 minutes: demote triggered"),
        (74, "shadow mode disabled: no mirrored traffic"),
        (76, "fallback path not activated: adapter unused"),
        (78, "A/B test paused: no new assignments"),
        (80, "load test completed: synthetic traffic removed"),
        (82, "ETL pipeline step skipped due to upstream failure"),
        (84, "SLO breach: traffic diverted to backup"),
        (86, "platform event: adapter idle during region migration"),
        (88, "manual demote: operator reclaiming Hot slots"),
        (90, "predictive engine: no traffic expected next 30 min"),
        (92, "partner API quota reset: no calls until midnight"),
        (94, "service discovery removed adapter from rotation"),
        (96, "model warming: old adapter demoted, new one warming"),
        (98, "audit log closed: processing adapter idle"),
        (100, "idle since last inference, inactivity confirmed"),
        (110, "multi-phase batch: phase 1 done, phase 2 delayed"),
        (120, "long lull: sustained inactivity over 2 hours"),
        (140, "adapter in standby mode, not selected by scheduler"),
        (160, "no scheduled work until next business day"),
        (180, "extended maintenance: adapter demoted proactively"),
    ]
    for i, (idle_min, context) in enumerate(extra_hot_warm_contexts):
        aid = adapter_id(2000 + i)
        pct = max(0, random.randint(0, 18))
        inp = (
            f"Adapter '{aid}' is in state Hot (memory: {STATE_MEMORY['Hot']} MB, "
            f"activation_pct: {pct}%). Last request: {idle_min} min ago. "
            f"Context: {context}. Hot→Warm threshold: >15 min inactive. Evaluate."
        )
        target = (
            f"Demote '{aid}' from Hot to Warm. "
            f"Inactivity of {idle_min} minutes exceeds the 15-minute Hot→Warm threshold. "
            f"Memory: {STATE_MEMORY['Hot']} MB → {STATE_MEMORY['Warm']} MB (-50 MB). "
            f"Activation latency: ~1ms → ~5ms. Valid sequential demotion."
        )
        extra.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.87 + random.uniform(0, 0.11), 2),
                "label": "positive",
                "subcategory": "hot_to_warm",
            },
        })

    # 50 more Warm→Cold with varied idle durations
    extra_warm_cold_contexts = [
        (31, "traffic failed to recover after Hot→Warm demotion"),
        (32, "adapter unused since end of shift"),
        (33, "off-hours: zero scheduled or ad-hoc requests"),
        (34, "upstream service in maintenance, no calls"),
        (35, "experiment concluded with negative result"),
        (36, "A/B variant retired, users assigned to winner"),
        (37, "migration completed: adapter no longer routed"),
        (38, "batch window not yet started: adapter waiting 35 min"),
        (39, "partner rate limit in effect: no traffic"),
        (40, "ETL pipeline paused: no data to process"),
        (41, "latency budget exceeded: traffic diverted"),
        (42, "circuit breaker open: no requests passing"),
        (43, "deployment locked: adapter suspended"),
        (44, "config validation: adapter paused pending review"),
        (45, "user cohort migrated to new adapter"),
        (46, "cost optimization: Warm no longer cost-justified"),
        (47, "quota enforcer: tenant downgraded, traffic removed"),
        (48, "nightly ETL complete: 48 min idle since finish"),
        (49, "health check: adapter healthy but not needed"),
        (50, "pre-maintenance demotion: releasing memory"),
        (55, "weekend: business-day traffic pattern not present"),
        (60, "multi-hour idle: standard Warm→Cold trigger"),
        (65, "shadow traffic removed: adapter no longer mirroring"),
        (70, "model warming: this adapter replaced in routing"),
        (75, "synthetic test traffic ended, adapter idle"),
        (80, "long dwell: 80 minutes idle post-demotion"),
        (85, "standby adapter: not called, demote to Cold"),
        (90, "extended maintenance window"),
        (95, "DR test ended: adapter no longer active"),
        (100, "adapter provisioned for capacity planning, unused"),
        (31, "inactivity confirmed by second health probe"),
        (33, "scheduler has no jobs for this adapter today"),
        (35, "adaptive routing found lower-cost path"),
        (37, "orchestrator scheduled rolling demotion"),
        (39, "resource rebalancer targeting this adapter"),
        (41, "SLA tier downgraded: Cold acceptable for this tenant"),
        (43, "memory headroom request: demote lowest priority"),
        (45, "workload finished earlier than expected"),
        (47, "no events in event-driven pipeline"),
        (50, "batch job cancelled: adapter not used"),
        (53, "traffic spike did not materialize"),
        (56, "partner SLA expired: Warm entitlement revoked"),
        (58, "predictive demotion: no traffic projected 60 min"),
        (61, "idle since midnight: standard overnight demotion"),
        (64, "adapter rotated out of load balancer pool"),
        (67, "feature disabled via runtime config"),
        (70, "cache warmed elsewhere: this adapter no longer needed"),
        (73, "data pipeline source went offline"),
        (76, "integration partner paused calls for maintenance"),
        (79, "rolling restart: adapter released for upgrade"),
    ]
    for i, (idle_min, context) in enumerate(extra_warm_cold_contexts):
        aid = adapter_id(2100 + i)
        pct = max(0, random.randint(0, 12))
        inp = (
            f"Adapter '{aid}' in Warm state (memory: {STATE_MEMORY['Warm']} MB, "
            f"activation_pct: {pct}%). Last request: {idle_min} min ago. "
            f"Context: {context}. Warm→Cold threshold: >30 min inactive. Evaluate."
        )
        target = (
            f"Demote '{aid}' from Warm to Cold. "
            f"Inactivity of {idle_min} minutes exceeds the 30-minute Warm→Cold threshold. "
            f"Memory: {STATE_MEMORY['Warm']} MB → {STATE_MEMORY['Cold']} MB (-50 MB). "
            f"Activation latency: ~5ms → ~50ms. Valid sequential demotion."
        )
        extra.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.87 + random.uniform(0, 0.11), 2),
                "label": "positive",
                "subcategory": "warm_to_cold",
            },
        })

    examples.extend(extra)
    assert len(examples) == 200, f"Expected 200 demotion examples, got {len(examples)}"
    return examples


# ── Category 3: Invalid Transitions (200 examples) ───────────────────────────

def gen_invalid_transitions() -> list[dict[str, Any]]:
    examples: list[dict[str, Any]] = []

    # Pattern A: Skipping states upward (e.g., Cold→Hot, Unloaded→Hot, Cold→Resident, etc.)
    skip_forward = [
        ("Cold", "Hot", 75, "high activation detected",
         "Cold→Hot skips Warm. Must go Cold→Warm first (threshold 30%), then Warm→Hot (threshold 60%)."),
        ("Cold", "Hot", 80, "SLA < 1ms required immediately",
         "State skipping is not permitted. Cold→Hot is illegal. Promote Cold→Warm first, then Warm→Hot."),
        ("Cold", "Hot", 65, "burst traffic justifies jump",
         "No burst exception exists. Transitions must be sequential: Cold→Warm, then Warm→Hot."),
        ("Cold", "Hot", 90, "operator forced skip",
         "Forced state skips violate the lifecycle contract. Cold→Hot is invalid. Sequential promotion required."),
        ("Cold", "Resident", 85, "direct pin from cold",
         "Cold→Resident skips two states. Must traverse Cold→Warm→Hot before explicit pin to Resident."),
        ("Cold", "Resident", 95, "adapter needed immediately at resident tier",
         "Cannot jump to Resident from Cold. Required path: Cold→Warm→Hot→Resident."),
        ("Unloaded", "Warm", 0, "preloading to Warm directly",
         "Unloaded→Warm skips Cold. Must load to Cold first, then promote to Warm if activation_pct > 30%."),
        ("Unloaded", "Hot", 0, "operator requested Hot state immediately",
         "Unloaded→Hot skips Cold and Warm. Required path: Unloaded→Cold→Warm→Hot."),
        ("Unloaded", "Resident", 0, "critical adapter needs immediate pin",
         "Unloaded→Resident skips three states. Required path: Unloaded→Cold→Warm→Hot→Resident."),
        ("Warm", "Resident", 88, "operator wants to skip Hot and pin",
         "Warm→Resident is invalid; Hot must be reached before pinning. Path: Warm→Hot→Resident."),
        ("Cold", "Hot", 70, "latency SLA demands immediate Hot state",
         "SLA requirements do not override lifecycle state machine. Promote sequentially: Cold→Warm, then Warm→Hot."),
        ("Cold", "Hot", 82, "activation jumped from 10% to 82% suddenly",
         "Sudden activation changes still require sequential promotion. Cold→Warm first (now valid), then Warm→Hot."),
        ("Unloaded", "Warm", 0, "batch pre-warm requested to Warm directly",
         "Unloaded adapters must first be loaded to Cold. Then Cold→Warm if activation_pct > 30%."),
        ("Cold", "Resident", 88, "time-sensitive workload requires pinning",
         "Pinning requires Hot state as prerequisite. Cold→Warm→Hot→Resident is the only valid path."),
        ("Warm", "Resident", 91, "traffic spike: pin without waiting for Hot",
         "Cannot pin from Warm. Must reach Hot first via Warm→Hot (threshold 60%), then Hot→Resident."),
        ("Unloaded", "Cold", 0, "standard load, this is actually valid — but testing recognition",
         "Actually valid: Unloaded→Cold is the initial load transition. This is not an invalid transition."),
        ("Cold", "Hot", 78, "inference engine assumes all in-memory adapters can be Hot",
         "Wrong assumption. Cold and Warm are distinct in-memory states. Sequential promotion is required."),
        ("Cold", "Hot", 85, "deployment system skipping state to save time",
         "Deployment systems must respect the lifecycle state machine. No shortcuts exist. Cold→Warm→Hot."),
        ("Unloaded", "Hot", 0, "manual override: mark as Hot without loading",
         "Manual overrides cannot violate the state machine. Unloaded→Hot is not a valid transition."),
        ("Cold", "Resident", 76, "customer contract requires pinned adapter",
         "Customer requirements do not change state machine rules. Path to Resident from Cold: Cold→Warm→Hot→Resident."),
    ]

    for i, (from_state, to_state, pct, context, explanation) in enumerate(skip_forward):
        aid = adapter_id(800 + i)
        inp = (
            f"Request to transition adapter '{aid}' directly from {from_state} to {to_state}. "
            f"Current state: {from_state} (memory: {STATE_MEMORY[from_state]} MB, activation_pct: {pct}%). "
            f"Justification: {context}. Is this transition valid?"
        )
        target = (
            f"Invalid transition: {from_state}→{to_state} is not permitted. "
            f"{explanation} "
            f"Adapter '{aid}' must remain in {from_state} and follow sequential promotion rules."
        )
        examples.append({
            "input": inp,
            "target": target,
            "metadata": {
                "quality": round(0.90 + random.uniform(0, 0.08), 2),
                "label": "negative",
                "subcategory": "illegal_skip_forward",
            },
        })

    # Pattern B: Demoting Resident without unpin
    resident_no_unpin = [
        ("Warm", "memory pressure at 87%", "eviction policy cannot demote Resident adapters"),
        ("Cold", "system low on memory", "Resident adapters are eviction-proof; unpin required first"),
        ("Unloaded", "critical memory shortage", "even critical pressure cannot evict a Resident adapter"),
        ("Hot", "inactivity timeout triggered", "Resident adapters are exempt from inactivity demotion"),
        ("Cold", "automatic eviction scan selected this adapter", "Resident adapters are excluded from eviction scans"),
        ("Warm", "rolling demotion cycle included this adapter", "Rolling demotion must skip Resident adapters"),
        ("Hot", "30-minute inactivity threshold exceeded", "Inactivity rules do not apply to Resident state; unpin first"),
        ("Unloaded", "operator command: evict all idle adapters", "Resident adapters are not idle-eligible; unpin required"),
        ("Warm", "cost optimizer flagged adapter for demotion", "Cost optimizer must respect Resident protection"),
        ("Cold", "quota enforcer demoting low-priority adapters", "Resident protection overrides quota enforcement"),
        ("Hot", "load balancer no longer routing to this adapter", "Routing decisions do not trigger Resident demotion"),
        ("Warm", "adapter not used in 2 hours", "Inactivity does not demote Resident. Explicit unpin required."),
        ("Unloaded", "emergency memory reclaim", "Even emergency reclaim cannot evict Resident without unpin"),
        ("Cold", "adaptive lifecycle policy demoting adapter", "Lifecycle policies must be Resident-aware; skip this adapter"),
        ("Hot", "admin demoted adapter via API without unpin", "API must reject demotion requests for Resident adapters without prior unpin"),
        ("Warm", "memory pressure 90%", "At 90% pressure, Cold/Warm are evicted first. Resident is last resort and requires unpin."),
        ("Cold", "auto-scale down: all adapters demoted", "Auto-scale must exclude Resident adapters from bulk demotion"),
        ("Hot", "TTL-based expiry applied", "TTL rules cannot forcibly transition Resident adapters without unpin"),
        ("Unloaded", "migration: old adapters cleaned up", "Migration scripts must call unpin before evicting Resident adapters"),
        ("Warm", "resource allocation rebalance", "Rebalancing must preserve Resident adapters; target non-resident adapters first"),
    ]

    for i, (target_state, trigger, reason) in enumerate(resident_no_unpin):
        aid = adapter_id(900 + i)
        inp = (
            f"Adapter '{aid}' is in state Resident (memory: {STATE_MEMORY['Resident']} MB). "
            f"Trigger: {trigger}. "
            f"Attempted transition: Resident→{target_state}. "
            f"No unpin call was made. Is this transition valid?"
        )
        target_text = (
            f"Invalid transition: Resident→{target_state} without prior unpin. "
            f"{reason}. "
            f"To demote '{aid}', first call unpin (e.g., `aos adapter unpin --id {aid}`). "
            f"After unpin, adapter returns to Hot and follows normal demotion rules."
        )
        examples.append({
            "input": inp,
            "target": target_text,
            "metadata": {
                "quality": round(0.90 + random.uniform(0, 0.08), 2),
                "label": "negative",
                "subcategory": "resident_demotion_without_unpin",
            },
        })

    # Pattern C: Backward skips and other illegal moves
    illegal_misc = [
        ("Hot", "Cold", 5, "double demotion requested",
         "Hot→Cold skips Warm. Must demote sequentially: Hot→Warm (after 15min inactive), then Warm→Cold (after 30min inactive)."),
        ("Hot", "Unloaded", 2, "evict directly from Hot",
         "Hot→Unloaded is not a valid transition. Must demote: Hot→Warm→Cold→Unloaded, or unpin if Resident."),
        ("Warm", "Unloaded", 0, "fast evict from Warm",
         "Warm→Unloaded is only valid under memory pressure eviction. Without pressure trigger, follow Warm→Cold→Unloaded."),
        ("Resident", "Unloaded", 60, "permanent removal skipping unpin",
         "Cannot jump from Resident to Unloaded. Required: unpin (Resident→Hot), then demote Hot→Warm→Cold→Unloaded."),
        ("Hot", "Cold", 8, "cost optimization: skip Warm to save time",
         "No cost exception permits skipping states. Demotion must be sequential: Hot→Warm, then Warm→Cold."),
        ("Cold", "Warm", 25, "activation_pct 25% is just below threshold but operator wants promotion",
         "Cannot promote Cold→Warm at 25% activation. Threshold is 30%. Wait for activation_pct to exceed 30%."),
        ("Warm", "Hot", 58, "activation 58% is close enough to 60%",
         "Cannot promote Warm→Hot at 58% activation. Threshold is exactly 60%. 58% does not qualify."),
        ("Cold", "Warm", 29, "one percent below threshold",
         "Cannot promote at 29%. Cold→Warm requires activation_pct > 30%. This does not qualify."),
        ("Warm", "Hot", 59, "59% activation: just below threshold",
         "Cannot promote at 59%. Warm→Hot requires activation_pct > 60%. 59% does not meet the threshold."),
        ("Hot", "Warm", 70, "activation 70% but operator wants demotion",
         "Cannot demote Hot→Warm when activation_pct=70% (above Hot threshold of 60%). Demotion only valid when inactive >15 minutes or activation drops below threshold."),
        ("Warm", "Cold", 45, "demotion requested but adapter is still active",
         "Cannot demote Warm→Cold when activation_pct=45% (above Cold threshold of 30%). Inactivity >30 minutes required."),
        ("Unloaded", "Resident", 0, "pre-pin an unloaded adapter",
         "Cannot pin an Unloaded adapter. Must first load (Unloaded→Cold), promote (Cold→Warm→Hot), then pin (Hot→Resident)."),
        ("Hot", "Unloaded", 0, "memory pressure eviction of Hot adapter",
         "Memory pressure eviction targets Cold first, then Warm. Hot adapters are not eviction candidates under normal pressure."),
        ("Resident", "Warm", 40, "skip Hot when unpinning",
         "Cannot skip Hot when demoting from Resident. Unpin transitions Resident→Hot. From Hot, normal demotion rules apply."),
        ("Cold", "Cold", 50, "re-entering same state",
         "No self-transitions exist in the lifecycle. Cold→Cold is not a valid operation."),
        ("Warm", "Warm", 70, "refresh Warm state",
         "No self-transitions defined. Warm→Warm is not valid. If activation_pct > 60%, promote to Hot instead."),
        ("Hot", "Hot", 80, "refresh Hot state",
         "No self-transitions defined. Hot→Hot is not valid. If pinning is needed, use explicit Hot→Resident."),
        ("Warm", "Cold", 5, "only 5 minutes inactive but force-demoting",
         "Cannot demote Warm→Cold after only 5 minutes. Threshold is >30 minutes inactive. Wait longer."),
        ("Hot", "Warm", 3, "3 minutes inactive but forcing demotion",
         "Cannot demote Hot→Warm after only 3 minutes. Threshold is >15 minutes inactive. Wait longer."),
        ("Cold", "Warm", 0, "no activation data available, assume promotion",
         "Cannot promote Cold→Warm without activation_pct exceeding 30%. Missing or zero activation_pct does not qualify."),
        ("Warm", "Hot", 0, "no activation data, force Hot",
         "Cannot promote Warm→Hot without activation_pct exceeding 60%. Missing data is treated as 0%, which does not qualify."),
        ("Hot", "Cold", 0, "double demotion: skip Warm directly",
         "Hot→Cold is not a valid single-step transition. Must go Hot→Warm first (if inactive >15min), then Warm→Cold (if inactive >30min)."),
        ("Resident", "Cold", 20, "bulk demotion included Resident",
         "Bulk demotion operations must exclude Resident adapters. Resident→Cold skips Hot and Warm; also requires unpin. Invalid."),
        ("Unloaded", "Hot", 0, "inference engine tried to activate to Hot directly",
         "Inference activation must follow the state machine. Unloaded→Hot skips two states. Load to Cold first."),
        ("Cold", "Resident", 50, "half-way there, just pin it",
         "Cold→Resident skips Warm and Hot. Pinning is only available from Hot state. Follow Cold→Warm→Hot→Resident."),
        ("Warm", "Resident", 75, "activation high: skip to Resident",
         "High activation does not permit skipping Hot. Warm→Resident is invalid. Promote Warm→Hot first, then pin."),
        ("Hot", "Unloaded", 85, "high activation but eviction requested",
         "Hot adapters with high activation should not be evicted. Additionally Hot→Unloaded is not a valid direct transition."),
        ("Resident", "Hot", 80, "implicit unpin from high activation",
         "High activation alone does not trigger Resident→Hot. Only an explicit unpin call causes Resident→Hot transition."),
        ("Cold", "Hot", 62, "activation is Hot-tier level but adapter is still Cold",
         "Activation level does not permit skipping states. Cold must be promoted to Warm first (since 62%>30%), then Warm→Hot (since 62%>60%)."),
        ("Warm", "Unloaded", 18, "manual eviction from Warm",
         "Manual eviction of a Warm adapter is only valid under memory pressure (>85%). At current pressure, follow normal demotion: Warm→Cold, then Cold→Unloaded if needed."),
    ]

    for i, (from_state, to_state, pct, context, explanation) in enumerate(illegal_misc):
        aid = adapter_id(1000 + i)
        idle_min = random.randint(0, 10)
        inp = (
            f"Adapter '{aid}' is in state {from_state} "
            f"(memory: {STATE_MEMORY[from_state]} MB, activation_pct: {pct}%, "
            f"last active: {idle_min} min ago). "
            f"Requested transition: {from_state}→{to_state}. Context: {context}. "
            f"Is this transition valid?"
        )
        target_text = (
            f"Invalid transition: {from_state}→{to_state}. "
            f"{explanation}"
        )
        examples.append({
            "input": inp,
            "target": target_text,
            "metadata": {
                "quality": round(0.89 + random.uniform(0, 0.09), 2),
                "label": "negative",
                "subcategory": "illegal_misc",
            },
        })

    # Pad to 200
    while len(examples) < 200:
        i = len(examples)
        aid = adapter_id(1100 + i)
        from_state = random.choice(["Cold", "Warm", "Hot"])
        skip_map = {
            "Cold": ["Hot", "Resident"],
            "Warm": ["Resident"],
            "Hot": ["Cold", "Unloaded"],
        }
        to_state = random.choice(skip_map[from_state])
        pct = random.randint(20, 95)
        inp = (
            f"Adapter '{aid}' in {from_state} (activation_pct={pct}%). "
            f"Direct transition {from_state}→{to_state} requested. Valid?"
        )
        target_text = (
            f"Invalid: {from_state}→{to_state} is not a recognized transition. "
            f"The adapter lifecycle state machine only permits sequential steps. "
            f"Review the valid transition graph: Unloaded→Cold→Warm→Hot→Resident (promotions), "
            f"and Hot→Warm→Cold→Unloaded (demotions)."
        )
        examples.append({
            "input": inp,
            "target": target_text,
            "metadata": {
                "quality": round(0.87 + random.uniform(0, 0.10), 2),
                "label": "negative",
                "subcategory": "illegal_skip",
            },
        })

    return examples[:200]


# ── Category 4: Full Lifecycle Traces (250 examples) ─────────────────────────

def gen_lifecycle_traces() -> list[dict[str, Any]]:
    examples: list[dict[str, Any]] = []

    def fmt_trace(events: list[tuple[str, str, str, int, float]]) -> str:
        """Format: (time_label, state, event, memory_mb, activation_pct)"""
        lines = []
        for t, state, event, mem, pct in events:
            lines.append(f"  [{t}] state={state}, event={event}, memory={mem}MB, activation_pct={pct:.0%}")
        return "\n".join(lines)

    # Template traces — each varied with different adapter IDs, times, details
    trace_templates = [
        # 1. Full happy path: Unloaded → Cold → Warm → Hot → Resident → Hot → Warm → Cold → Unloaded
        {
            "name": "full_lifecycle_A",
            "description": "Complete lifecycle with peak traffic, pin, unpin, and graceful eviction",
            "template": lambda aid, i: {
                "input": (
                    f"Trace the full lifecycle of adapter '{aid}' over a {6 + i % 12}-hour window:\n"
                    f"  [T+0m] Loaded from disk. State: Unloaded. Memory: 0MB.\n"
                    f"  [T+1m] Load triggered by inference request. State transitions to Cold.\n"
                    f"  [T+5m] activation_pct={31 + i % 40}%. Promotion threshold (30%) crossed.\n"
                    f"  [T+10m] activation_pct={62 + i % 30}%. Promotion threshold (60%) crossed.\n"
                    f"  [T+15m] Operator pins adapter. Explicit Hot→Resident.\n"
                    f"  [T+{120 + i * 5}m] Pin TTL expired. Unpin called.\n"
                    f"  [T+{140 + i * 5}m] No traffic. Inactivity timer starts.\n"
                    f"  [T+{155 + i * 5}m] 15min inactivity exceeded. Hot→Warm.\n"
                    f"  [T+{190 + i * 5}m] 30min inactivity exceeded. Warm→Cold.\n"
                    f"  [T+{220 + i * 5}m] Memory pressure 87%. Eviction: Cold→Unloaded.\n"
                    f"Verify each transition and identify any violations."
                ),
                "target": (
                    f"All transitions for '{aid}' are valid:\n"
                    f"  Unloaded→Cold: initial load. Valid.\n"
                    f"  Cold→Warm: activation_pct={31 + i % 40}% > 30%. Valid promotion.\n"
                    f"  Warm→Hot: activation_pct={62 + i % 30}% > 60%. Valid promotion.\n"
                    f"  Hot→Resident: explicit operator pin. Valid.\n"
                    f"  Resident→Hot: TTL expiry triggered unpin. Valid.\n"
                    f"  Hot→Warm: 15min inactivity exceeded. Valid demotion.\n"
                    f"  Warm→Cold: 30min inactivity exceeded. Valid demotion.\n"
                    f"  Cold→Unloaded: memory pressure 87% > 85% threshold. Valid eviction.\n"
                    f"No violations. Lifecycle completed cleanly."
                ),
            },
        },
        # 2. Short lifecycle: load, quick promotion, pressure eviction
        {
            "name": "quick_eviction_B",
            "description": "Fast promotion followed by memory pressure eviction before full warmup",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' lifecycle:\n"
                    f"  [T+0m] Loaded. Unloaded→Cold. Memory: 100MB.\n"
                    f"  [T+3m] activation_pct={32 + i % 30}%. Cold→Warm (>30%). Memory: 150MB.\n"
                    f"  [T+8m] Memory pressure spikes to {86 + i % 10}%. Eviction scan runs.\n"
                    f"  [T+8m] Warm adapter selected for eviction. Warm→Unloaded.\n"
                    f"  [T+10m] Memory pressure drops to {60 + i % 20}%.\n"
                    f"What is the lifecycle verdict?"
                ),
                "target": (
                    f"Valid lifecycle for '{aid}':\n"
                    f"  Unloaded→Cold: initial load. Valid.\n"
                    f"  Cold→Warm: activation_pct={32 + i % 30}% > 30%. Valid promotion.\n"
                    f"  Warm→Unloaded: memory pressure {86 + i % 10}% > 85%. Valid eviction. "
                    f"Warm adapters are valid eviction candidates under pressure.\n"
                    f"Total in-memory duration: ~8 minutes. Full lifecycle completed correctly."
                ),
            },
        },
        # 3. Demotion cascade: Hot → Warm → Cold → Unloaded from inactivity
        {
            "name": "inactivity_cascade_C",
            "description": "Adapter promoted to Hot, then fully demoted by inactivity cascade",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' lifecycle trace:\n"
                    f"  [T+0m] Cold state. Memory: 100MB. activation_pct: {61 + i % 30}%.\n"
                    f"  [T+1m] Cold→Warm (pct={31 + i % 20}%). Memory: 150MB.\n"
                    f"  [T+2m] Warm→Hot (pct={62 + i % 30}%). Memory: 200MB.\n"
                    f"  [T+{17 + i % 10}m] Traffic stops. Inactivity starts.\n"
                    f"  [T+{17 + i % 10 + 15}m] Hot→Warm (15min inactive). Memory: 150MB.\n"
                    f"  [T+{17 + i % 10 + 45}m] Warm→Cold (30min inactive). Memory: 100MB.\n"
                    f"  [T+{17 + i % 10 + 90}m] No traffic. No memory pressure. Cold remains.\n"
                    f"Validate all transitions."
                ),
                "target": (
                    f"All transitions valid for '{aid}':\n"
                    f"  Cold→Warm: activation exceeded 30%. Valid promotion.\n"
                    f"  Warm→Hot: activation exceeded 60%. Valid promotion.\n"
                    f"  Hot→Warm: 15min inactivity threshold crossed. Valid demotion. Memory: -50MB.\n"
                    f"  Warm→Cold: 30min inactivity threshold crossed. Valid demotion. Memory: -50MB.\n"
                    f"  Cold remains: no memory pressure, no eviction trigger. Correct.\n"
                    f"Inactivity cascade completed correctly. Adapter in Cold, available for recall."
                ),
            },
        },
        # 4. Resident protection under pressure
        {
            "name": "resident_protection_D",
            "description": "Resident adapter survives memory pressure eviction that targets Cold/Warm peers",
            "template": lambda aid, i: {
                "input": (
                    f"System state at T+{30 + i * 2}m:\n"
                    f"  Adapter '{aid}': Resident (pinned). Memory: 200MB.\n"
                    f"  Adapter 'peer-{i:03d}a': Cold. Memory: 100MB.\n"
                    f"  Adapter 'peer-{i:03d}b': Warm. Memory: 150MB.\n"
                    f"  System memory pressure: {86 + i % 10}%. Eviction threshold: 85%.\n"
                    f"  Eviction scan selects lowest-priority adapters.\n"
                    f"Which adapters are evicted? Is '{aid}' affected?"
                ),
                "target": (
                    f"Eviction outcome:\n"
                    f"  'peer-{i:03d}a' (Cold): evicted. Cold→Unloaded. -100MB.\n"
                    f"  'peer-{i:03d}b' (Warm): evicted if pressure remains > 85%. Warm→Unloaded. -150MB.\n"
                    f"  '{aid}' (Resident): NOT evicted. Resident adapters are eviction-proof.\n"
                    f"Resident pin protects '{aid}' from all eviction events regardless of pressure. "
                    f"To evict '{aid}', an explicit unpin must be called first."
                ),
            },
        },
        # 5. Multiple promotions and demotion in one day
        {
            "name": "daily_traffic_pattern_E",
            "description": "Adapter follows daily traffic pattern: morning ramp, peak, afternoon lull, evening spike",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' over a 24-hour workday:\n"
                    f"  [06:00] Loaded: Unloaded→Cold. Memory: 100MB.\n"
                    f"  [07:30] Morning ramp. activation_pct={33 + i % 20}%. Cold→Warm.\n"
                    f"  [09:00] Peak load. activation_pct={65 + i % 25}%. Warm→Hot.\n"
                    f"  [12:00] Lunch lull. activation_pct drops to {10 + i % 15}%.\n"
                    f"  [12:15] No traffic for 15+ min. Hot→Warm.\n"
                    f"  [13:30] Afternoon traffic. activation_pct={62 + i % 20}%. Warm→Hot.\n"
                    f"  [17:00] End of day. Traffic drops sharply.\n"
                    f"  [17:15] Hot→Warm (15min inactive).\n"
                    f"  [17:45] Warm→Cold (30min inactive).\n"
                    f"  [20:00] Memory pressure 88%. Cold→Unloaded.\n"
                    f"Validate the complete trace."
                ),
                "target": (
                    f"All transitions valid for '{aid}':\n"
                    f"  Unloaded→Cold: initial load at 06:00. Valid.\n"
                    f"  Cold→Warm: morning ramp, pct>{30}%. Valid.\n"
                    f"  Warm→Hot: peak load, pct>{60}%. Valid.\n"
                    f"  Hot→Warm: 15min lunch inactivity. Valid demotion.\n"
                    f"  Warm→Hot: afternoon traffic, pct>{60}%. Valid re-promotion.\n"
                    f"  Hot→Warm: 15min EOD inactivity. Valid demotion.\n"
                    f"  Warm→Cold: 30min EOD inactivity. Valid demotion.\n"
                    f"  Cold→Unloaded: memory pressure 88%>85%. Valid eviction.\n"
                    f"No violations. Two promotion/demotion cycles in one day. Clean lifecycle."
                ),
            },
        },
        # 6. Resident pin with TTL and re-use
        {
            "name": "pin_ttl_reuse_F",
            "description": "Adapter pinned, TTL expires, re-activated, re-promoted",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' trace:\n"
                    f"  [T+0m] Hot state. activation_pct={72 + i % 20}%.\n"
                    f"  [T+5m] Operator pins: Hot→Resident.\n"
                    f"  [T+{60 + i * 3}m] Pin TTL expires. Unpin: Resident→Hot.\n"
                    f"  [T+{65 + i * 3}m] Traffic drops. activation_pct={8 + i % 10}%.\n"
                    f"  [T+{65 + i * 3 + 15}m] 15min inactive. Hot→Warm.\n"
                    f"  [T+{65 + i * 3 + 20}m] New traffic burst. activation_pct={68 + i % 20}%.\n"
                    f"  [T+{65 + i * 3 + 21}m] Warm→Hot (pct>60%).\n"
                    f"  [T+{65 + i * 3 + 25}m] Second pin requested: Hot→Resident.\n"
                    f"Validate trace."
                ),
                "target": (
                    f"All transitions valid for '{aid}':\n"
                    f"  Hot→Resident: explicit pin. Valid.\n"
                    f"  Resident→Hot: TTL expiry + unpin. Valid.\n"
                    f"  Hot→Warm: 15min inactivity. Valid demotion.\n"
                    f"  Warm→Hot: traffic burst, pct>60%. Valid re-promotion.\n"
                    f"  Hot→Resident: second pin. Valid.\n"
                    f"Adapters can be pinned, unpinned, and re-pinned multiple times. All valid."
                ),
            },
        },
        # 7. Cold adapter evicted before promotion
        {
            "name": "cold_eviction_before_promotion_G",
            "description": "Adapter stays in Cold, never promoted, evicted under memory pressure",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' trace:\n"
                    f"  [T+0m] Loaded: Unloaded→Cold. Memory: 100MB.\n"
                    f"  [T+1m] activation_pct={5 + i % 20}%. Below 30% threshold. No promotion.\n"
                    f"  [T+{10 + i}m] System memory at {86 + i % 10}%. Eviction scan triggered.\n"
                    f"  [T+{10 + i}m] '{aid}' is Cold with low activation. Selected for eviction.\n"
                    f"  [T+{10 + i}m] Cold→Unloaded. Memory: 100MB freed.\n"
                    f"Validate."
                ),
                "target": (
                    f"Valid lifecycle for '{aid}':\n"
                    f"  Unloaded→Cold: initial load. Valid.\n"
                    f"  Cold (held): activation_pct below 30% threshold. No promotion. Correct.\n"
                    f"  Cold→Unloaded: memory pressure {86 + i % 10}% > 85% eviction threshold. Valid.\n"
                    f"Cold adapters are first eviction priority under pressure. Lifecycle correct."
                ),
            },
        },
        # 8. Adapter with multiple demotions then stabilizes
        {
            "name": "traffic_oscillation_H",
            "description": "Traffic oscillates, adapter repeatedly crosses thresholds",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' traffic oscillation pattern:\n"
                    f"  [T+0m] Cold. pct={31 + i % 10}%. Cold→Warm.\n"
                    f"  [T+5m] pct={62 + i % 10}%. Warm→Hot.\n"
                    f"  [T+20m] pct drops to {8 + i % 10}%. 15min idle: Hot→Warm.\n"
                    f"  [T+52m] pct still {8 + i % 10}%. 30min idle: Warm→Cold.\n"
                    f"  [T+55m] New request. pct={35 + i % 15}%. Cold→Warm.\n"
                    f"  [T+58m] pct={65 + i % 15}%. Warm→Hot.\n"
                    f"  [T+80m] Stable pct={70 + i % 20}%. Hot maintained.\n"
                    f"Validate all transitions."
                ),
                "target": (
                    f"All transitions valid for '{aid}':\n"
                    f"  Cold→Warm: pct>30%. Valid.\n"
                    f"  Warm→Hot: pct>60%. Valid.\n"
                    f"  Hot→Warm: 15min inactivity. Valid demotion.\n"
                    f"  Warm→Cold: 30min inactivity. Valid demotion.\n"
                    f"  Cold→Warm: traffic returned, pct>30%. Valid re-promotion.\n"
                    f"  Warm→Hot: pct>60%. Valid re-promotion.\n"
                    f"  Hot maintained: pct>60% and active. Correct steady state.\n"
                    f"Oscillating traffic handled correctly. Two full promotion cycles."
                ),
            },
        },
        # 9. Shared trace with memory snapshots
        {
            "name": "memory_snapshot_trace_I",
            "description": "Lifecycle trace with precise memory accounting at each step",
            "template": lambda aid, i: {
                "input": (
                    f"Memory-tracked lifecycle for adapter '{aid}':\n"
                    f"  [T+0m] Unloaded. System memory used: {40 + i}MB / 1024MB.\n"
                    f"  [T+1m] Load: Unloaded→Cold. Memory: +100MB. Total: {140 + i}MB.\n"
                    f"  [T+4m] Promote Cold→Warm. Memory: +50MB. Total: {190 + i}MB.\n"
                    f"  [T+7m] Promote Warm→Hot. Memory: +50MB. Total: {240 + i}MB.\n"
                    f"  [T+30m] Demote Hot→Warm. Memory: -50MB. Total: {190 + i}MB.\n"
                    f"  [T+62m] Demote Warm→Cold. Memory: -50MB. Total: {140 + i}MB.\n"
                    f"  [T+90m] Evict Cold→Unloaded. Memory: -100MB. Total: {40 + i}MB.\n"
                    f"Validate memory accounting and transitions."
                ),
                "target": (
                    f"Memory accounting valid for '{aid}':\n"
                    f"  Unloaded→Cold: +100MB (Cold baseline). Valid.\n"
                    f"  Cold→Warm: +50MB (Warm overhead). Valid. Total: {190 + i}MB.\n"
                    f"  Warm→Hot: +50MB (Hot optimization cache). Valid. Total: {240 + i}MB.\n"
                    f"  Hot→Warm: -50MB (optimization cache released). Valid demotion.\n"
                    f"  Warm→Cold: -50MB (Warm overhead released). Valid demotion.\n"
                    f"  Cold→Unloaded: -100MB (fully deallocated). Valid eviction.\n"
                    f"Final system memory matches initial: {40 + i}MB. Accounting correct."
                ),
            },
        },
        # 10. Load → immediate heavy use → pin → maintain
        {
            "name": "immediate_pin_J",
            "description": "Adapter loaded, rapidly promoted to Hot, pinned and kept Resident",
            "template": lambda aid, i: {
                "input": (
                    f"Adapter '{aid}' rapid deployment trace:\n"
                    f"  [T+0m] Deployment triggered: Unloaded→Cold.\n"
                    f"  [T+0.5m] Pre-warm signals: activation_pct={35 + i % 20}%. Cold→Warm.\n"
                    f"  [T+1m] Traffic arrives: activation_pct={70 + i % 20}%. Warm→Hot.\n"
                    f"  [T+2m] Operator pins: Hot→Resident.\n"
                    f"  [T+{100 + i * 10}m] Adapter still Resident. Memory: 200MB. pct={75 + i % 20}%.\n"
                    f"  [T+{100 + i * 10}m] No eviction events. No demotion events.\n"
                    f"Validate and explain Resident behavior."
                ),
                "target": (
                    f"All transitions valid for '{aid}':\n"
                    f"  Unloaded→Cold: deployment load. Valid.\n"
                    f"  Cold→Warm: pre-warm activation, pct>30%. Valid. Rapid but sequential.\n"
                    f"  Warm→Hot: live traffic, pct>60%. Valid. Rapid but sequential.\n"
                    f"  Hot→Resident: explicit pin. Valid.\n"
                    f"  Resident maintained: pinned adapters are eviction-proof and inactivity-immune.\n"
                    f"  No demotion or eviction events: correct for Resident state.\n"
                    f"Rapid but fully compliant lifecycle. Resident state correctly prevents interference."
                ),
            },
        },
    ]

    # Generate traces using each template with varied i values
    traces_per_template = 25
    for t_idx, template_spec in enumerate(trace_templates):
        for i in range(traces_per_template):
            global_i = t_idx * traces_per_template + i
            aid = adapter_id(1200 + global_i)
            rendered = template_spec["template"](aid, i)
            examples.append({
                "input": rendered["input"],
                "target": rendered["target"],
                "metadata": {
                    "quality": round(0.91 + random.uniform(0, 0.07), 2),
                    "label": "positive",
                    "subcategory": f"trace_{template_spec['name']}",
                },
            })

    assert len(examples) == 250, f"Expected 250 trace examples, got {len(examples)}"
    return examples


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    import os

    out_dir = os.path.dirname(os.path.abspath(__file__))
    out_path = os.path.join(out_dir, "state-transitions.jsonl")

    promotions = gen_valid_promotions()
    demotions = gen_valid_demotions()
    invalids = gen_invalid_transitions()
    traces = gen_lifecycle_traces()

    all_examples = promotions + demotions + invalids + traces
    random.shuffle(all_examples)

    assert len(all_examples) == 850, f"Expected 850 examples, got {len(all_examples)}"

    with open(out_path, "w", encoding="utf-8") as f:
        for ex in all_examples:
            f.write(json.dumps(ex, ensure_ascii=False) + "\n")

    print(f"Wrote {len(all_examples)} examples to {out_path}")

    # Category breakdown
    cats: dict[str, int] = {}
    for ex in all_examples:
        label = ex["metadata"]["label"]
        cats[label] = cats.get(label, 0) + 1
    print("Label breakdown:", cats)

    subcats: dict[str, int] = {}
    for ex in all_examples:
        sc = ex["metadata"]["subcategory"]
        subcats[sc] = subcats.get(sc, 0) + 1
    print("Subcategory breakdown:", {k: v for k, v in sorted(subcats.items())})


if __name__ == "__main__":
    main()
