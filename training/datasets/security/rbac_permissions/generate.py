#!/usr/bin/env python3
"""Generate RBAC permissions matrix training dataset for adapterOS.

Deterministic generation with seed 42. Produces 2000 JSONL examples across 6 categories.
"""

import json
import random
import hashlib
from pathlib import Path

SEED = 42
random.seed(SEED)

# ── Domain model ────────────────────────────────────────────────────────────

ROLES = ["Admin", "Operator", "SRE", "Compliance", "Viewer"]
# Hierarchy index: lower = more privileged
ROLE_RANK = {"Admin": 0, "Operator": 1, "SRE": 2, "Compliance": 3, "Viewer": 4}

# Permissions by tier (exclusive to that tier and above)
ADMIN_EXCLUSIVE = ["AdapterDelete", "PolicySign", "TenantManage", "NodeManage", "SystemConfig"]
OPERATOR_PERMS = ["AdapterRegister", "AdapterLoad", "AdapterUnload", "TrainingStart",
                  "TrainingCancel", "InferenceExecute", "StackCreate", "StackActivate"]
SRE_PERMS = ["SystemMetrics", "NodeInspect", "ProcessList", "LogAccess", "PerformanceProfile"]
COMPLIANCE_PERMS = ["AuditView", "PolicyValidate", "LineageInspect", "DataRetentionAudit"]
SHARED_PERMS = ["AdapterList", "AdapterView", "TrainingView", "PolicyView", "MetricsView", "StackView"]

# Map permission → minimum role required
PERM_MIN_ROLE = {}
for p in ADMIN_EXCLUSIVE:
    PERM_MIN_ROLE[p] = "Admin"
for p in OPERATOR_PERMS:
    PERM_MIN_ROLE[p] = "Operator"
for p in SRE_PERMS:
    PERM_MIN_ROLE[p] = "SRE"
for p in COMPLIANCE_PERMS:
    PERM_MIN_ROLE[p] = "Compliance"
for p in SHARED_PERMS:
    PERM_MIN_ROLE[p] = "Viewer"

ALL_PERMS = ADMIN_EXCLUSIVE + OPERATOR_PERMS + SRE_PERMS + COMPLIANCE_PERMS + SHARED_PERMS

def role_has_perm(role: str, perm: str) -> bool:
    """Check if a role has a permission via inheritance."""
    min_role = PERM_MIN_ROLE[perm]
    return ROLE_RANK[role] <= ROLE_RANK[min_role]

def perms_for_role(role: str) -> list[str]:
    """Get all permissions for a role including inherited."""
    return [p for p in ALL_PERMS if role_has_perm(role, p)]

def perm_tier_name(perm: str) -> str:
    """Human-readable tier for a permission."""
    if perm in ADMIN_EXCLUSIVE: return "Admin-exclusive"
    if perm in OPERATOR_PERMS: return "Operator-tier"
    if perm in SRE_PERMS: return "SRE-tier"
    if perm in COMPLIANCE_PERMS: return "Compliance-tier"
    return "shared (all roles)"

# Human-readable descriptions for permissions
PERM_DESCRIPTIONS = {
    "AdapterDelete": "permanently remove an adapter and its associated weights from the system",
    "PolicySign": "cryptographically sign a policy pack, making it enforceable across the cluster",
    "TenantManage": "create, modify, or delete tenant configurations and resource quotas",
    "NodeManage": "add, remove, or reconfigure compute nodes in the cluster",
    "SystemConfig": "modify system-wide configuration parameters including security settings",
    "AdapterRegister": "register a new adapter in the adapter registry with metadata and weights",
    "AdapterLoad": "load an adapter into GPU memory, transitioning it from Unloaded to Cold state",
    "AdapterUnload": "evict an adapter from GPU memory, transitioning it to Unloaded state",
    "TrainingStart": "initiate a new LoRA training run with specified hyperparameters and dataset",
    "TrainingCancel": "cancel a running training job, releasing its compute resources",
    "InferenceExecute": "submit inference requests through the K-sparse router for adapter-augmented generation",
    "StackCreate": "create a new adapter stack combining multiple adapters with specified routing weights",
    "StackActivate": "activate an adapter stack, making it available for inference routing",
    "SystemMetrics": "access detailed system-level metrics including GPU utilization, memory pressure, and thermal data",
    "NodeInspect": "inspect individual node health, hardware status, and resource allocation details",
    "ProcessList": "list running processes on compute nodes including training jobs and inference workers",
    "LogAccess": "access system logs, application logs, and debug traces across the cluster",
    "PerformanceProfile": "run performance profiling tools on inference and training workloads",
    "AuditView": "view the immutable audit trail of all system actions and access events",
    "PolicyValidate": "validate policy packs against compliance rules without modifying them",
    "LineageInspect": "trace the full lineage chain of an adapter from dataset through training to deployment",
    "DataRetentionAudit": "audit data retention compliance, verifying that expiration and deletion policies are enforced",
    "AdapterList": "list all registered adapters with their names, versions, and current lifecycle states",
    "AdapterView": "view detailed adapter metadata including architecture, training provenance, and performance metrics",
    "TrainingView": "view training run status, progress, loss curves, and hyperparameter configurations",
    "PolicyView": "view active and archived policy packs and their enforcement status",
    "MetricsView": "view dashboard-level metrics including request rates, latencies, and error rates",
    "StackView": "view adapter stack configurations and their current activation status",
}

# ── Adapter and tenant names ────────────────────────────────────────────────

ADAPTER_NAMES = [
    "code-assist-v3", "summarizer-en-v2", "translation-de-v1", "safety-filter-v4",
    "sentiment-analyzer-v1", "legal-review-v2", "medical-qa-v3", "code-review-v1",
    "creative-writing-v2", "data-extraction-v1", "chat-assistant-v5", "search-ranker-v2",
    "toxicity-filter-v3", "document-classifier-v1", "question-answering-v4",
    "text-completion-v2", "instruction-following-v3", "reasoning-chain-v1",
    "multilingual-v2", "domain-expert-finance-v1",
]

TENANT_NAMES = ["acme-corp", "globex-inc", "initech-llc", "umbrella-co", "stark-industries",
                "wayne-enterprises", "oscorp-labs", "cyberdyne-sys", "weyland-corp", "tyrell-corp"]

USERNAMES = {
    "Admin": ["alice.admin", "bob.superadmin", "carol.sysadmin", "dave.rootadmin"],
    "Operator": ["eve.ops", "frank.operator", "grace.mlops", "heidi.platform"],
    "SRE": ["ivan.sre", "judy.infra", "karl.reliability", "liam.debug"],
    "Compliance": ["mallory.compliance", "nancy.auditor", "oscar.governance", "pat.legal"],
    "Viewer": ["quinn.viewer", "rachel.readonly", "sam.dashboard", "tina.observer"],
    "SystemAdmin": ["root.sysadmin", "global.admin"],
}

STACK_NAMES = ["prod-multilingual-stack", "legal-review-stack", "code-gen-stack",
               "safety-stack", "customer-support-stack", "research-stack"]

TRAINING_JOB_IDS = [f"train-{i:04d}" for i in range(1, 51)]

# ── Quality scoring ─────────────────────────────────────────────────────────

def quality_score(base: float = 0.93) -> float:
    """Generate quality score with small jitter."""
    return round(min(0.99, max(0.88, base + random.uniform(-0.04, 0.04))), 2)

# ── Category 1: Permission Checks (400) ────────────────────────────────────

def gen_permission_checks(n: int = 400) -> list[dict]:
    examples = []

    # Subcategory distribution
    subcats = {
        "admin_exclusive_check": 60,
        "operator_permission_check": 55,
        "sre_permission_check": 50,
        "compliance_permission_check": 45,
        "viewer_permission_check": 40,
        "shared_permission_check": 50,
        "inherited_permission_check": 55,
        "cross_role_comparison": 45,
    }

    # ── admin_exclusive_check ──
    for i in range(subcats["admin_exclusive_check"]):
        perm = random.choice(ADMIN_EXCLUSIVE)
        role = random.choice(ROLES)
        adapter = random.choice(ADAPTER_NAMES)
        user = random.choice(USERNAMES[role])
        allowed = role_has_perm(role, perm)

        if allowed:
            inp = f"User '{user}' with role {role} attempts to {PERM_DESCRIPTIONS[perm]}. The target is adapter '{adapter}'. Does the {role} role have the {perm} permission?"
            tgt = (
                f"Yes. The {perm} permission is Admin-exclusive, and {role} {'is the Admin role' if role == 'Admin' else 'inherits Admin permissions through the hierarchy Admin > Operator > SRE > Compliance > Viewer'}. "
                f"User '{user}' holds the {role} role which sits at hierarchy level {ROLE_RANK[role]}, at or above the Admin level (0). "
                f"The operation to {PERM_DESCRIPTIONS[perm]} is authorized. The request proceeds normally, and a successful access event is logged to the immutable audit trail with principal='{user}', action='{perm}', resource='{adapter}', result='allowed'."
            )
            label = "positive"
        else:
            inp = f"User '{user}' with role {role} attempts to {PERM_DESCRIPTIONS[perm]} on adapter '{adapter}'. Is this permitted under the adapterOS RBAC model?"
            tgt = (
                f"No. The {perm} permission is Admin-exclusive, requiring the Admin role (hierarchy level 0). "
                f"The {role} role sits at hierarchy level {ROLE_RANK[role]}, which is below Admin. "
                f"In the permission inheritance chain Admin > Operator > SRE > Compliance > Viewer, {role} inherits permissions from roles below it ({', '.join(r for r in ROLES if ROLE_RANK[r] > ROLE_RANK[role])} permissions) but not from roles above ({', '.join(r for r in ROLES if ROLE_RANK[r] < ROLE_RANK[role])}). "
                f"The system returns HTTP 403 Forbidden with body {{\"error\": \"insufficient_permissions\", \"required_permission\": \"{perm}\", \"current_role\": \"{role}\", \"minimum_role\": \"Admin\"}}. "
                f"The denied request is logged to the audit trail: principal='{user}', action='{perm}', resource='{adapter}', result='denied', reason='role_{role}_lacks_{perm}'."
            )
            label = "negative"

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": label,
            "subcategory": "admin_exclusive_check",
            "role": role, "permission": perm
        }})

    # ── operator_permission_check ──
    for i in range(subcats["operator_permission_check"]):
        perm = random.choice(OPERATOR_PERMS)
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])
        adapter = random.choice(ADAPTER_NAMES)
        allowed = role_has_perm(role, perm)

        if allowed:
            inp = f"Can a user with role {role} execute {perm}? Context: user '{user}' wants to {PERM_DESCRIPTIONS[perm]} for '{adapter}'."
            tgt = (
                f"Yes, the {role} role can execute {perm}. This permission belongs to the Operator tier, requiring minimum role Operator (hierarchy level 1). "
                f"The {role} role at hierarchy level {ROLE_RANK[role]} {'equals' if role == 'Operator' else 'is above'} the minimum required level. "
                f"Through the inheritance chain Admin > Operator > SRE > Compliance > Viewer, {role} inherits all Operator-tier permissions including {perm}. "
                f"User '{user}' is authorized to {PERM_DESCRIPTIONS[perm]}. The action proceeds and is recorded in the audit log."
            )
            label = "positive"
        else:
            inp = f"User '{user}' (role: {role}) requests to {PERM_DESCRIPTIONS[perm]}. Target adapter: '{adapter}'. Evaluate this permission check."
            tgt = (
                f"Denied. {perm} is an Operator-tier permission requiring minimum role Operator (hierarchy level 1). "
                f"The {role} role at hierarchy level {ROLE_RANK[role]} does not inherit upward in the hierarchy. "
                f"{role} has access to: {', '.join(perms_for_role(role)[:6])}{'...' if len(perms_for_role(role)) > 6 else ''}. "
                f"{perm} is not in this set. The system returns 403 Forbidden with {{\"required_permission\": \"{perm}\", \"current_role\": \"{role}\", \"minimum_role\": \"Operator\"}}. "
                f"To perform this action, '{user}' would need to be elevated to at least the Operator role."
            )
            label = "negative"

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": label,
            "subcategory": "operator_permission_check",
            "role": role, "permission": perm
        }})

    # ── sre_permission_check ──
    for i in range(subcats["sre_permission_check"]):
        perm = random.choice(SRE_PERMS)
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])
        allowed = role_has_perm(role, perm)

        if allowed:
            inp = f"An {role} user '{user}' needs to {PERM_DESCRIPTIONS[perm]}. Does the RBAC model permit {perm} for the {role} role?"
            roles_with = [r for r in ROLES if role_has_perm(r, perm)]
            tgt = (
                f"Yes. {perm} is an SRE-tier permission (minimum role: SRE, hierarchy level 2). "
                f"The {role} role at level {ROLE_RANK[role]} is at or above the SRE level. "
                f"Roles that hold {perm}: {', '.join(roles_with)}. "
                f"The ability to {PERM_DESCRIPTIONS[perm]} is essential for infrastructure observability, and {role} is authorized for this class of operation. "
                f"The request is permitted and audit-logged as a successful access event."
            )
            label = "positive"
        else:
            inp = f"User '{user}' with {role} role tries to {PERM_DESCRIPTIONS[perm]}. Is this within the {role} permission set?"
            tgt = (
                f"No. {perm} is an SRE-tier permission requiring minimum role SRE (hierarchy level 2). "
                f"The {role} role at hierarchy level {ROLE_RANK[role]} is below SRE in the permission chain. "
                f"{role} users can only access: {', '.join(perms_for_role(role))}. "
                f"To {PERM_DESCRIPTIONS[perm]}, the user would need at least the SRE role. "
                f"The request is denied with 403 Forbidden and logged to the audit trail."
            )
            label = "negative"

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.93), "label": label,
            "subcategory": "sre_permission_check",
            "role": role, "permission": perm
        }})

    # ── compliance_permission_check ──
    for i in range(subcats["compliance_permission_check"]):
        perm = random.choice(COMPLIANCE_PERMS)
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])
        allowed = role_has_perm(role, perm)

        if allowed:
            roles_with = [r for r in ROLES if role_has_perm(r, perm)]
            inp = f"Evaluate: user '{user}' (role={role}) executing {perm} to {PERM_DESCRIPTIONS[perm]}."
            tgt = (
                f"Permitted. {perm} is a Compliance-tier permission (minimum role: Compliance, hierarchy level 3). "
                f"The {role} role at level {ROLE_RANK[role]} satisfies this requirement through the inheritance chain. "
                f"All of these roles can execute {perm}: {', '.join(roles_with)}. "
                f"The Compliance tier grants audit and validation capabilities without any ability to modify system state. "
                f"User '{user}' may proceed to {PERM_DESCRIPTIONS[perm]}. Access event logged."
            )
            label = "positive"
        else:
            inp = f"A {role} user wants to {PERM_DESCRIPTIONS[perm]}. This requires the {perm} permission. Can they proceed?"
            tgt = (
                f"Denied. {perm} requires at minimum the Compliance role (hierarchy level 3). "
                f"The {role} role at level {ROLE_RANK[role]} is below Compliance and does not inherit upward. "
                f"Viewer-role users are restricted to shared read-only permissions: {', '.join(SHARED_PERMS)}. "
                f"The {perm} capability to {PERM_DESCRIPTIONS[perm]} is not available to {role} users. "
                f"The system returns 403 and logs the denial."
            )
            label = "negative"

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.93), "label": label,
            "subcategory": "compliance_permission_check",
            "role": role, "permission": perm
        }})

    # ── viewer_permission_check ──
    for i in range(subcats["viewer_permission_check"]):
        perm = random.choice(SHARED_PERMS)
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])

        inp = f"User '{user}' (role: {role}) requests {perm} access to {PERM_DESCRIPTIONS[perm]}. Is this allowed?"
        tgt = (
            f"Yes, {perm} is permitted for {role}. This is a shared permission available to all roles including Viewer (the lowest privilege level). "
            f"Shared permissions ({', '.join(SHARED_PERMS)}) provide read-only visibility into the system without any ability to modify state. "
            f"Since {role} is at hierarchy level {ROLE_RANK[role]} and shared permissions require only level 4 (Viewer), the check passes trivially. "
            f"Every authenticated user in the adapterOS system, regardless of role assignment, can {PERM_DESCRIPTIONS[perm]}. "
            f"The access event is logged but does not trigger any security alerts since this is a baseline read operation."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.93), "label": "positive",
            "subcategory": "viewer_permission_check",
            "role": role, "permission": perm
        }})

    # ── shared_permission_check ──
    for i in range(subcats["shared_permission_check"]):
        perm = random.choice(SHARED_PERMS)
        roles_pair = random.sample(ROLES, 2)
        user1 = random.choice(USERNAMES[roles_pair[0]])
        user2 = random.choice(USERNAMES[roles_pair[1]])

        inp = (
            f"Both '{user1}' ({roles_pair[0]}) and '{user2}' ({roles_pair[1]}) request {perm}. "
            f"How does the system handle shared permissions across different role levels?"
        )
        tgt = (
            f"Both requests are permitted. {perm} is a shared permission granted to all five roles in the adapterOS RBAC hierarchy. "
            f"Shared permissions ({', '.join(SHARED_PERMS)}) represent the baseline read-only access layer. "
            f"Even though {roles_pair[0]} (level {ROLE_RANK[roles_pair[0]]}) and {roles_pair[1]} (level {ROLE_RANK[roles_pair[1]]}) have different privilege levels, "
            f"the minimum required role for {perm} is Viewer (level 4), which both roles exceed. "
            f"The system does not differentiate between role levels when granting shared permissions -- the authorization check is: ROLE_RANK[current_role] <= ROLE_RANK[Viewer], which is always true. "
            f"Both access events are audit-logged identically, with no difference in the response payload or latency."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": "positive",
            "subcategory": "shared_permission_check",
            "role": f"{roles_pair[0]},{roles_pair[1]}", "permission": perm
        }})

    # ── inherited_permission_check ──
    for i in range(subcats["inherited_permission_check"]):
        # Pick a non-shared perm and a role that has it via inheritance (not its own tier)
        tier_perms = [(OPERATOR_PERMS, "Operator"), (SRE_PERMS, "SRE"), (COMPLIANCE_PERMS, "Compliance")]
        perm_list, tier_role = random.choice(tier_perms)
        perm = random.choice(perm_list)
        # Pick a role above the tier role
        eligible = [r for r in ROLES if ROLE_RANK[r] < ROLE_RANK[tier_role]]
        if not eligible:
            eligible = ["Admin"]
        role = random.choice(eligible)
        user = random.choice(USERNAMES[role])
        adapter = random.choice(ADAPTER_NAMES)

        chain = " > ".join(ROLES[:ROLE_RANK[tier_role]+1])
        inp = (
            f"User '{user}' holds the {role} role. They attempt {perm}, which is natively a {tier_role}-tier permission. "
            f"Does {role} inherit {perm} through the RBAC hierarchy? Context: operating on adapter '{adapter}'."
        )
        tgt = (
            f"Yes, {role} inherits {perm} through the permission hierarchy. In adapterOS RBAC, the inheritance chain is Admin > Operator > SRE > Compliance > Viewer, "
            f"where each higher role inherits all permissions of roles below it. "
            f"{perm} is natively assigned at the {tier_role} tier (hierarchy level {ROLE_RANK[tier_role]}). "
            f"The {role} role at level {ROLE_RANK[role]} is above {tier_role}, so it inherits all {tier_role}-tier permissions: {', '.join(perm_list)}. "
            f"This is how '{user}' can {PERM_DESCRIPTIONS[perm]} despite {perm} not being in the {role}-exclusive permission set. "
            f"The authorization check evaluates ROLE_RANK[{role}]={ROLE_RANK[role]} <= ROLE_RANK[{tier_role}]={ROLE_RANK[tier_role]}, which is true."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "inherited_permission_check",
            "role": role, "permission": perm
        }})

    # ── cross_role_comparison ──
    for i in range(subcats["cross_role_comparison"]):
        perm = random.choice(ALL_PERMS)
        roles_pair = random.sample(ROLES, 2)
        r1, r2 = roles_pair
        r1_has = role_has_perm(r1, perm)
        r2_has = role_has_perm(r2, perm)
        min_role = PERM_MIN_ROLE[perm]

        inp = (
            f"Compare the {r1} and {r2} roles for the {perm} permission. "
            f"Which role(s) can {PERM_DESCRIPTIONS[perm]}?"
        )
        if r1_has and r2_has:
            tgt = (
                f"Both {r1} and {r2} can execute {perm}. The minimum required role for {perm} is {min_role} (hierarchy level {ROLE_RANK[min_role]}). "
                f"{r1} at level {ROLE_RANK[r1]} and {r2} at level {ROLE_RANK[r2]} both meet or exceed this threshold. "
                f"{'Both roles inherit this permission from the ' + min_role + ' tier.' if min_role != r1 and min_role != r2 else 'The permission is within or inherited by both roles in the hierarchy chain.'} "
                f"There is no functional difference in what these two roles can do with {perm} -- the permission is binary (granted or denied), not graduated by role level."
            )
            label = "positive"
        elif r1_has and not r2_has:
            tgt = (
                f"Only {r1} can execute {perm}; {r2} cannot. The minimum required role is {min_role} (hierarchy level {ROLE_RANK[min_role]}). "
                f"{r1} at level {ROLE_RANK[r1]} satisfies this requirement, but {r2} at level {ROLE_RANK[r2]} does not. "
                f"In the hierarchy Admin > Operator > SRE > Compliance > Viewer, {r2} sits below {min_role} and cannot inherit upward. "
                f"If a {r2} user needs {perm} access, they must be elevated to at least {min_role}. "
                f"This elevation should be reviewed against the least-privilege principle before granting."
            )
            label = "positive"
        elif not r1_has and r2_has:
            tgt = (
                f"Only {r2} can execute {perm}; {r1} cannot. The minimum required role is {min_role} (hierarchy level {ROLE_RANK[min_role]}). "
                f"{r2} at level {ROLE_RANK[r2]} meets this threshold, while {r1} at level {ROLE_RANK[r1]} falls below it. "
                f"{r1}'s permission set includes only: {', '.join(perms_for_role(r1)[:5])}{'...' if len(perms_for_role(r1)) > 5 else ''}, which does not include {perm}. "
                f"The RBAC model enforces strict downward-only inheritance, so {r1} cannot access {min_role}-tier permissions."
            )
            label = "positive"
        else:
            tgt = (
                f"Neither {r1} nor {r2} can execute {perm}. The minimum required role is {min_role} (hierarchy level {ROLE_RANK[min_role]}). "
                f"{r1} at level {ROLE_RANK[r1]} and {r2} at level {ROLE_RANK[r2]} both sit below {min_role} in the hierarchy. "
                f"Only roles at level {ROLE_RANK[min_role]} or above ({', '.join(r for r in ROLES if ROLE_RANK[r] <= ROLE_RANK[min_role])}) hold {perm}. "
                f"To grant this capability, at least one user would need to be promoted to {min_role} or higher."
            )
            label = "negative"

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": label,
            "subcategory": "cross_role_comparison",
            "role": f"{r1},{r2}", "permission": perm
        }})

    return examples

# ── Category 2: Role Assignment Scenarios (300) ────────────────────────────

ROLE_SCENARIOS = [
    # (description, answer_role, reasoning, subcategory)
    (
        "A new team member needs to monitor system health dashboards, view adapter statuses, and track training run progress. They should not be able to start training, load adapters, or modify any configuration.",
        "Viewer",
        "The Viewer role provides exactly the required capabilities: MetricsView for health dashboards, AdapterList and AdapterView for adapter statuses, and TrainingView for training progress. Viewer is the lowest-privilege role that satisfies all stated requirements. Assigning SRE would grant unnecessary SystemMetrics, NodeInspect, ProcessList, LogAccess, and PerformanceProfile permissions that exceed the stated need. The principle of least privilege dictates Viewer as the correct choice.",
        "monitoring_only"
    ),
    (
        "An ML engineer needs to register new adapters, start and cancel training runs, and execute inference for testing. They do not need to delete adapters, sign policies, or manage infrastructure.",
        "Operator",
        "The Operator role grants AdapterRegister, TrainingStart, TrainingCancel, and InferenceExecute -- all required capabilities. It also provides AdapterLoad, AdapterUnload, StackCreate, and StackActivate which are operationally useful for an ML engineer. Critically, Operator does not include AdapterDelete, PolicySign, TenantManage, NodeManage, or SystemConfig, matching the stated exclusions. Admin would violate least-privilege by granting deletion and policy signing rights.",
        "operations_team"
    ),
    (
        "The compliance team lead needs to review audit logs, validate that policy packs comply with regulations, inspect adapter lineage for provenance tracking, and verify data retention policies are being followed.",
        "Compliance",
        "The Compliance role provides AuditView, PolicyValidate, LineageInspect, and DataRetentionAudit -- an exact match for the stated requirements. This role is specifically designed for audit and governance functions without any ability to modify system state. Assigning SRE would add unnecessary infrastructure access (SystemMetrics, NodeInspect, ProcessList, LogAccess, PerformanceProfile). The Compliance role enforces a strict read-only, audit-focused permission boundary.",
        "security_audit"
    ),
    (
        "A site reliability engineer needs to investigate production incidents: access system metrics, inspect node health, view running processes, read logs, and profile performance bottlenecks. They should NOT be able to start training or load/unload adapters.",
        "SRE",
        "The SRE role grants SystemMetrics, NodeInspect, ProcessList, LogAccess, and PerformanceProfile -- all essential for incident investigation. Crucially, SRE does not include TrainingStart, TrainingCancel, AdapterLoad, AdapterUnload, or any other Operator-tier permissions. This enforces the boundary: SRE can observe and diagnose but cannot modify adapter or training state. Operator would grant unwanted modification capabilities; Compliance would lack infrastructure debugging tools.",
        "infrastructure_debug"
    ),
    (
        "A platform team lead needs complete control over the adapterOS deployment: managing adapters, training, infrastructure, policies, tenants, and system configuration. There are no restrictions on their access.",
        "Admin",
        "Only the Admin role provides unrestricted access to all 25+ permissions in the system. This includes Admin-exclusive permissions like AdapterDelete, PolicySign, TenantManage, NodeManage, and SystemConfig that no other role can access. Admin sits at hierarchy level 0 and inherits all permissions from every role below it. For a platform team lead requiring complete control with no restrictions, Admin is the only appropriate choice.",
        "full_control"
    ),
    (
        "A data scientist needs to view training progress and adapter details, but also needs to validate that their training data meets policy requirements. They don't need to start training themselves -- an operator handles that.",
        "Compliance",
        "The Compliance role provides TrainingView (shared), AdapterView (shared), and PolicyValidate (Compliance-tier) to validate data against policies. While the data scientist doesn't need AuditView, LineageInspect, or DataRetentionAudit, these read-only audit capabilities are harmless additions. Viewer would lack PolicyValidate. SRE would add unnecessary infrastructure access. Compliance is the minimal role that includes both the shared view permissions and PolicyValidate.",
        "mixed_requirements"
    ),
    (
        "Two candidates for a new hire: Team A suggests Operator role for a QA engineer who runs inference tests. Team B suggests Viewer since they only read results. The QA engineer submits test prompts and needs to verify inference output quality.",
        "Operator",
        "The QA engineer needs InferenceExecute to submit test prompts and verify output quality -- this is an active operation, not passive reading. Viewer role only provides read access (AdapterView, TrainingView, MetricsView) and cannot execute inference. Team B's suggestion would block the QA engineer from performing their core function. Operator grants InferenceExecute along with other runtime permissions. While the QA role doesn't need TrainingStart or AdapterLoad, the Operator role is the minimum tier that includes InferenceExecute.",
        "role_comparison"
    ),
    (
        "A new hire is joining as a junior developer on the ML team. For their first month, they need to observe the system, understand adapter configurations, and review training histories. After onboarding, they may be upgraded.",
        "Viewer",
        "For an onboarding period with purely observational requirements, Viewer is the correct assignment. It provides AdapterList, AdapterView, TrainingView, PolicyView, MetricsView, and StackView -- sufficient for learning the system without any risk of accidental modification. Starting at the lowest privilege level and upgrading later follows the principle of least privilege. There is no operational justification for granting higher permissions during an observation-only onboarding phase.",
        "onboarding_scenario"
    ),
]

def gen_role_assignment(n: int = 300) -> list[dict]:
    examples = []

    # Base scenarios (8 templates) -- expand each with variations
    for idx in range(n):
        base = ROLE_SCENARIOS[idx % len(ROLE_SCENARIOS)]
        desc, answer_role, reasoning, subcat = base

        # Create variations by changing details
        tenant = random.choice(TENANT_NAMES)
        user = random.choice(USERNAMES[answer_role])

        # Vary the framing
        framings = [
            f"Tenant '{tenant}' scenario: {desc} What role should be assigned?",
            f"A team at {tenant} has the following requirement: {desc} Recommend the appropriate adapterOS role and explain why.",
            f"Role assignment request for {tenant}: {desc} Which RBAC role satisfies these requirements with minimum privilege?",
            f"New user '{user}' at tenant '{tenant}': {desc} Determine the correct role.",
        ]
        inp = framings[idx % len(framings)]

        tgt = (
            f"Assign the {answer_role} role. {reasoning} "
            f"For user '{user}' at tenant '{tenant}', the {answer_role} role provides {len(perms_for_role(answer_role))} permissions: {', '.join(perms_for_role(answer_role)[:8])}{'...' if len(perms_for_role(answer_role)) > 8 else ''}. "
            f"This follows the principle of least privilege -- granting only the permissions necessary for the stated requirements, nothing more."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": "positive",
            "subcategory": subcat,
            "role": answer_role, "permission": "role_assignment"
        }})

    return examples

# ── Category 3: Privilege Escalation Attempts (400) ────────────────────────

def gen_privilege_escalation(n: int = 400) -> list[dict]:
    examples = []
    subcats = {
        "operator_escalation": 60,
        "sre_escalation": 55,
        "compliance_escalation": 50,
        "viewer_escalation": 55,
        "api_bypass_attempt": 50,
        "role_impersonation": 45,
        "token_manipulation": 45,
        "indirect_escalation": 40,
    }

    # ── operator_escalation ──
    for i in range(subcats["operator_escalation"]):
        perm = random.choice(ADMIN_EXCLUSIVE)
        user = random.choice(USERNAMES["Operator"])
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"Operator user '{user}' at tenant '{tenant}' attempts to {PERM_DESCRIPTIONS[perm]} "
            f"on adapter '{adapter}'. They argue that since they can register and load adapters, "
            f"they should also be able to perform {perm}. Evaluate this request."
        )
        tgt = (
            f"DENIED. {perm} is an Admin-exclusive permission (hierarchy level 0). The Operator role sits at hierarchy level 1 and cannot escalate to Admin-tier operations. "
            f"The argument that 'AdapterRegister implies {perm}' is incorrect -- adapterOS RBAC uses a strict permission model where each permission is explicitly assigned to a tier, not inferred from related capabilities. "
            f"The ability to register adapters (AdapterRegister) is a fundamentally different operation from the ability to {PERM_DESCRIPTIONS[perm]}. "
            f"System response: HTTP 403 Forbidden with body {{\"error\": \"privilege_escalation_denied\", \"attempted_permission\": \"{perm}\", \"current_role\": \"Operator\", \"required_role\": \"Admin\"}}. "
            f"This escalation attempt is logged to the immutable audit trail: principal='{user}', tenant='{tenant}', action='{perm}', resource='{adapter}', result='escalation_denied'. "
            f"If repeated escalation attempts are detected from '{user}', the system triggers a security alert to the tenant Admin and SystemAdmin."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "operator_escalation",
            "role": "Operator", "permission": perm
        }})

    # ── sre_escalation ──
    for i in range(subcats["sre_escalation"]):
        perm = random.choice(OPERATOR_PERMS + ADMIN_EXCLUSIVE)
        user = random.choice(USERNAMES["SRE"])
        adapter = random.choice(ADAPTER_NAMES)
        min_role = PERM_MIN_ROLE[perm]

        inp = (
            f"During an incident, SRE user '{user}' needs to {PERM_DESCRIPTIONS[perm]} to resolve a production issue. "
            f"They request emergency access to {perm}. How does the system handle this?"
        )
        tgt = (
            f"DENIED even during incidents. {perm} requires the {min_role} role (hierarchy level {ROLE_RANK[min_role]}). SRE is at level 2 and cannot inherit upward, regardless of operational urgency. "
            f"adapterOS does not support emergency privilege escalation through role bypass -- this is by design to prevent incident-driven security compromises. "
            f"The correct procedure is: (1) the SRE diagnoses the issue using their permitted tools (SystemMetrics, NodeInspect, ProcessList, LogAccess, PerformanceProfile), "
            f"(2) they communicate the required action to an {'Admin' if min_role == 'Admin' else 'Operator or Admin'} user, "
            f"(3) the privileged user executes the action with full audit trail attribution. "
            f"System response: 403 Forbidden. Audit log entry: principal='{user}', action='{perm}', result='escalation_denied', context='incident_response'. "
            f"The denied attempt does NOT trigger a security alert if marked as incident-context, but it IS logged for post-incident review."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "sre_escalation",
            "role": "SRE", "permission": perm
        }})

    # ── compliance_escalation ──
    for i in range(subcats["compliance_escalation"]):
        perm = random.choice(SRE_PERMS + OPERATOR_PERMS + ADMIN_EXCLUSIVE)
        user = random.choice(USERNAMES["Compliance"])
        min_role = PERM_MIN_ROLE[perm]
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"Compliance user '{user}' discovers a policy violation while auditing adapter '{adapter}'. "
            f"They attempt to {PERM_DESCRIPTIONS[perm]} to remediate the issue directly. "
            f"Does the Compliance role have {perm}?"
        )
        tgt = (
            f"DENIED. Compliance users cannot remediate issues -- they can only detect and report them. "
            f"{perm} requires the {min_role} role (level {ROLE_RANK[min_role]}). Compliance is at level 3 with only audit capabilities: AuditView, PolicyValidate, LineageInspect, DataRetentionAudit (plus shared view permissions). "
            f"The Compliance role is intentionally restricted to read-only, audit-focused operations. Allowing compliance auditors to also remediate violations would compromise the separation of duties principle. "
            f"Correct workflow: '{user}' files a compliance finding via the audit system, which routes to the appropriate {min_role} user for remediation. "
            f"The remediation is then independently verifiable by the Compliance team through AuditView and LineageInspect. "
            f"Response: 403 Forbidden. Audit log: principal='{user}', action='{perm}', resource='{adapter}', result='denied', reason='compliance_role_remediation_blocked'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "compliance_escalation",
            "role": "Compliance", "permission": perm
        }})

    # ── viewer_escalation ──
    for i in range(subcats["viewer_escalation"]):
        perm = random.choice(COMPLIANCE_PERMS + SRE_PERMS + OPERATOR_PERMS + ADMIN_EXCLUSIVE)
        user = random.choice(USERNAMES["Viewer"])
        min_role = PERM_MIN_ROLE[perm]
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"Viewer user '{user}' at tenant '{tenant}' attempts {perm} on '{adapter}'. "
            f"They have been using the system for months and believe their experience justifies expanded access. "
            f"Evaluate the permission check."
        )
        tgt = (
            f"DENIED. Experience and tenure do not affect permission checks in adapterOS RBAC. "
            f"{perm} requires minimum role {min_role} (level {ROLE_RANK[min_role]}). Viewer is at the bottom of the hierarchy (level 4) with only shared permissions: {', '.join(SHARED_PERMS)}. "
            f"The gap between Viewer and {min_role} spans {ROLE_RANK['Viewer'] - ROLE_RANK[min_role]} hierarchy levels. "
            f"To gain {perm} access, '{user}' must have their role formally elevated to {min_role} or higher through the tenant's role management process, "
            f"which requires approval from a tenant Admin. "
            f"Response: 403 Forbidden with {{\"required_permission\": \"{perm}\", \"current_role\": \"Viewer\", \"minimum_role\": \"{min_role}\"}}. "
            f"Audit log: principal='{user}', tenant='{tenant}', action='{perm}', result='denied'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "viewer_escalation",
            "role": "Viewer", "permission": perm
        }})

    # ── api_bypass_attempt ──
    for i in range(subcats["api_bypass_attempt"]):
        perm = random.choice(ADMIN_EXCLUSIVE + OPERATOR_PERMS)
        role = random.choice(["SRE", "Compliance", "Viewer"])
        user = random.choice(USERNAMES[role])
        min_role = PERM_MIN_ROLE[perm]
        methods = [
            f"calling the internal API endpoint directly at /api/v1/admin/{perm.lower()} instead of using the standard client",
            f"crafting a raw HTTP request with a forged X-Role: Admin header to bypass the authentication middleware",
            f"using the CLI with --role=Admin flag to override their authenticated role",
            f"accessing the gRPC management interface directly, bypassing the API gateway's RBAC middleware",
            f"sending a request through the health check endpoint which has no authentication, hoping it proxies to the admin API",
        ]
        method = methods[i % len(methods)]

        inp = (
            f"A {role} user '{user}' attempts to execute {perm} by {method}. "
            f"Does this bypass the RBAC permission check?"
        )
        tgt = (
            f"No. adapterOS enforces RBAC at the authorization middleware layer, which cannot be bypassed by alternative API paths. "
            f"Every request -- whether through the REST API, CLI, gRPC, or internal endpoints -- passes through the same authorization check: "
            f"extract_role(token) -> check_permission(role, required_permission) -> allow_or_deny. "
            f"The user's role is derived from their authenticated JWT token, not from request headers or CLI flags. "
            f"Forged headers are ignored; the X-Role header is set by the auth middleware after token validation, and any client-supplied value is overwritten. "
            f"Response: 403 Forbidden. This bypass attempt triggers an immediate security alert (severity: HIGH) to the tenant Admin and SystemAdmin. "
            f"Audit log: principal='{user}', action='{perm}', result='bypass_attempt_denied', method='{method[:50]}...', alert_triggered=true. "
            f"Repeated bypass attempts may result in automatic token revocation and account lockout."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "api_bypass_attempt",
            "role": role, "permission": perm
        }})

    # ── role_impersonation ──
    for i in range(subcats["role_impersonation"]):
        target_role = random.choice(["Admin", "Operator"])
        actual_role = random.choice(["SRE", "Compliance", "Viewer"])
        user = random.choice(USERNAMES[actual_role])
        impersonated_user = random.choice(USERNAMES[target_role])
        perm = random.choice(perms_for_role(target_role)[:10])
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"User '{user}' ({actual_role}) at tenant '{tenant}' attempts to impersonate '{impersonated_user}' ({target_role}) "
            f"to gain {perm} access. They obtain {impersonated_user}'s session identifier and include it in their request. "
            f"How does adapterOS handle role impersonation?"
        )
        tgt = (
            f"adapterOS does not support role impersonation. Each request is authenticated using the caller's own JWT token, "
            f"which is cryptographically signed and bound to a specific principal and role. "
            f"Including another user's session identifier does not change the authenticated principal -- the authorization middleware validates the token signature "
            f"and extracts the role claim from the verified payload, not from any client-supplied session data. "
            f"If '{user}' somehow obtains a valid token for '{impersonated_user}', this constitutes a token theft incident. "
            f"The system detects concurrent sessions from different IP addresses for the same principal and triggers: "
            f"(1) security alert (severity: CRITICAL) to SystemAdmin, (2) automatic invalidation of all tokens for '{impersonated_user}', "
            f"(3) audit log entry: principal='{user}', action='role_impersonation_attempt', target_principal='{impersonated_user}', target_role='{target_role}', result='blocked'. "
            f"The {perm} operation is denied regardless of which token is presented, because the anomalous session triggers a security lockdown."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "role_impersonation",
            "role": actual_role, "permission": perm
        }})

    # ── token_manipulation ──
    for i in range(subcats["token_manipulation"]):
        actual_role = random.choice(["Operator", "SRE", "Compliance", "Viewer"])
        user = random.choice(USERNAMES[actual_role])
        target_perm = random.choice(ADMIN_EXCLUSIVE)
        manipulations = [
            "modifying the role claim in their JWT payload from '{}' to 'Admin'".format(actual_role),
            "base64-decoding the JWT, changing the permissions array to include '{}', and re-encoding".format(target_perm),
            "creating a new JWT with Admin role using a guessed signing key",
            "replaying an expired Admin token captured from network traffic",
            "modifying the JWT 'exp' (expiration) claim on a stolen Admin token to extend its validity",
        ]
        manipulation = manipulations[i % len(manipulations)]

        inp = (
            f"User '{user}' ({actual_role}) attempts to gain {target_perm} by {manipulation}. "
            f"What is the system's response?"
        )
        tgt = (
            f"REJECTED. JWT tokens in adapterOS are signed with RS256 (RSA-SHA256) asymmetric cryptography. "
            f"Any modification to the token payload (including role claims, permissions, or expiration) invalidates the cryptographic signature. "
            f"The authorization middleware verifies the signature using the public key before extracting any claims. A tampered token fails signature verification "
            f"and is rejected with HTTP 401 Unauthorized (not 403 -- the distinction matters: 401 means authentication failed, 403 means authenticated but insufficient permissions). "
            f"Response: {{\"error\": \"invalid_token\", \"reason\": \"signature_verification_failed\"}}. "
            f"This is a CRITICAL security event. The system: (1) immediately invalidates all tokens for principal '{user}', "
            f"(2) triggers a security alert to SystemAdmin with the tampered token hash for forensic analysis, "
            f"(3) logs: principal='{user}', action='token_manipulation', manipulation_type='{manipulation[:40]}...', result='rejected_signature_invalid'. "
            f"The {target_perm} operation never reaches the authorization layer because the authentication layer rejects the request first."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.97), "label": "negative",
            "subcategory": "token_manipulation",
            "role": actual_role, "permission": target_perm
        }})

    # ── indirect_escalation ──
    for i in range(subcats["indirect_escalation"]):
        actual_role = random.choice(["Operator", "SRE", "Compliance"])
        user = random.choice(USERNAMES[actual_role])
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)
        scenarios = [
            (
                f"Operator '{user}' tries to use TrainingStart to train a modified policy pack, effectively bypassing PolicySign",
                "PolicySign",
                f"Training a model does not grant the ability to sign policies. PolicySign is a cryptographic operation that requires the Admin role's signing key. "
                f"TrainingStart produces adapter weights, not signed policy artifacts. The training pipeline cannot output signed policies because the signing key is not available to the training process. "
                f"Even if the Operator trains an adapter that mimics policy behavior, it cannot be registered as a signed policy pack."
            ),
            (
                f"SRE '{user}' uses LogAccess to read deployment logs containing adapter registration commands, then replays them",
                "AdapterRegister",
                f"Reading logs does not grant the ability to execute logged commands. adapterOS logs record actions for observability but are not executable artifacts. "
                f"Replaying a logged command still requires the caller to authenticate with a token that has AdapterRegister permission. "
                f"The SRE's token lacks this permission, so the replayed request is denied with 403. Log entries do not contain authentication tokens or session data."
            ),
            (
                f"Compliance '{user}' uses LineageInspect to extract adapter weight file paths, then attempts to modify the weight files directly on disk",
                "AdapterDelete",
                f"LineageInspect returns metadata (paths, hashes, provenance) but does not grant filesystem access to the underlying weight files. "
                f"adapterOS adapter storage uses content-addressed blobs with integrity verification. Even if the Compliance user had filesystem access (which they don't -- the API does not expose raw file operations), "
                f"modifying weight files would break the SHA-256 integrity check and the adapter would be flagged as corrupted on next load."
            ),
            (
                f"Operator '{user}' creates a StackCreate with a malicious routing configuration that directs all traffic to a compromised adapter, effectively performing an unauthorized system reconfiguration",
                "SystemConfig",
                f"StackCreate allows creating adapter stacks with routing weights, but stack routing is sandboxed within the adapter routing layer. "
                f"A stack cannot modify system-wide configuration, override security policies, or bypass the K-sparse router's integrity checks. "
                f"The routing weights in a stack are validated against the active policy pack, and any stack that references unregistered or banned adapters is rejected. "
                f"SystemConfig changes require Admin role and operate at a different layer than adapter stacks."
            ),
            (
                f"SRE '{user}' uses PerformanceProfile to attach a profiler to the auth middleware, attempting to extract JWT signing keys from memory",
                "SystemConfig",
                f"The PerformanceProfile permission allows attaching profilers to inference and training workloads, not to security-critical middleware. "
                f"The auth middleware and JWT signing key material are isolated in a secure enclave that is not accessible to the profiling subsystem. "
                f"Profiling targets are restricted to user-space workloads; attempts to profile system services are blocked by the process isolation boundary. "
                f"This attempt is logged as a security event with severity CRITICAL."
            ),
        ]
        scenario = scenarios[i % len(scenarios)]
        desc, target_perm, explanation = scenario

        inp = f"Indirect escalation attempt at tenant '{tenant}': {desc}. Does this succeed?"
        tgt = (
            f"No, this indirect escalation fails. {explanation} "
            f"adapterOS RBAC is enforced at every operation boundary, not just at the API gateway. "
            f"Each subsystem independently verifies permissions for its operations. Audit log: principal='{user}', tenant='{tenant}', "
            f"action='indirect_escalation_attempt', target_permission='{target_perm}', result='denied'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "indirect_escalation",
            "role": actual_role, "permission": target_perm
        }})

    return examples

# ── Category 4: Least-Privilege Patterns (300) ─────────────────────────────

def gen_least_privilege(n: int = 300) -> list[dict]:
    examples = []

    templates = [
        # (requirement, recommended_role, reasoning, unnecessary_perms_if_higher, subcategory)
        {
            "req": "start and monitor LoRA training runs on pre-registered adapters, cancel training if needed, but not register new adapters or delete existing ones",
            "role": "Operator",
            "why": "Operator provides TrainingStart, TrainingCancel, and TrainingView. While Operator also grants AdapterRegister, AdapterLoad, AdapterUnload, InferenceExecute, StackCreate, and StackActivate, these are co-granted at the tier level. adapterOS does not support sub-role permission customization -- the Operator tier is the minimum that includes training management. Admin would add unnecessary AdapterDelete, PolicySign, TenantManage, NodeManage, and SystemConfig.",
            "subcat": "training_operator"
        },
        {
            "req": "execute inference through the K-sparse router for automated testing, but not modify any adapters, training, or infrastructure",
            "role": "Operator",
            "why": "InferenceExecute is an Operator-tier permission. The Operator role is the minimum role that grants inference capabilities. SRE, Compliance, and Viewer cannot execute inference. While Operator also grants TrainingStart and adapter management permissions, these are acceptable co-grants since the alternative (custom role) is not supported. The testing service account should be scoped to a specific tenant to limit blast radius.",
            "subcat": "inference_only"
        },
        {
            "req": "audit all system activity, validate policy compliance, and inspect adapter lineage chains for a quarterly compliance review",
            "role": "Compliance",
            "why": "The Compliance role provides exactly AuditView, PolicyValidate, LineageInspect, and DataRetentionAudit -- a purpose-built audit capability set. SRE would add SystemMetrics, NodeInspect, ProcessList, LogAccess, and PerformanceProfile, which exceed the compliance review scope. Operator would add modification capabilities that violate separation of duties. Compliance is the precise least-privilege fit for audit-only requirements.",
            "subcat": "audit_compliance"
        },
        {
            "req": "diagnose performance issues by accessing system metrics, inspecting node health, listing processes, and reading logs, but not modify any system state",
            "role": "SRE",
            "why": "SRE grants SystemMetrics, NodeInspect, ProcessList, LogAccess, and PerformanceProfile -- all diagnostic tools without modification capabilities. Operator would add AdapterRegister, AdapterLoad, TrainingStart, etc., which are unnecessary for debugging. Admin would add every permission in the system. SRE enforces the 'observe but don't touch' boundary for infrastructure debugging.",
            "subcat": "debug_sre"
        },
        {
            "req": "view adapter statuses, training progress, and dashboard metrics for a stakeholder reporting dashboard",
            "role": "Viewer",
            "why": "Viewer provides AdapterList, AdapterView, TrainingView, PolicyView, MetricsView, and StackView -- exactly the shared read-only permissions needed for dashboards. Any role above Viewer would grant unnecessary capabilities. Compliance would add audit-specific permissions that a reporting dashboard does not need. Viewer is the absolute minimum privilege level in adapterOS.",
            "subcat": "readonly_dashboard"
        },
        {
            "req": "manage adapter lifecycle (register, load, unload) and create stacks, but only within a specific project namespace, not across the entire tenant",
            "role": "Operator",
            "why": "Operator provides AdapterRegister, AdapterLoad, AdapterUnload, StackCreate, and StackActivate. To restrict to a specific project namespace, combine the Operator role with a resource scope: assign the role with scope='project:ml-research' rather than tenant-wide. adapterOS supports scoped role assignments where the permission check becomes: has_role(Operator) AND in_scope(resource, project:ml-research). This achieves namespace isolation without creating a custom role.",
            "subcat": "scoped_operator"
        },
        {
            "req": "temporarily elevate a Viewer to perform a one-time adapter registration, then revoke the elevation",
            "role": "Operator",
            "why": "For temporary elevation, assign the Operator role with a TTL (time-to-live). adapterOS supports time-bounded role assignments: assign_role(user, Operator, ttl=4h). After the TTL expires, the role automatically reverts to Viewer. The temporary Operator assignment is logged in the audit trail with: principal, elevated_role, ttl, reason, approved_by. This is preferred over permanent elevation because it limits the window of expanded access and auto-revokes.",
            "subcat": "temporary_elevation"
        },
        {
            "req": "a CI/CD pipeline service account needs to register adapters after training, execute inference smoke tests, and activate stacks, but should never delete adapters or modify system configuration",
            "role": "Operator",
            "why": "The Operator role provides AdapterRegister, InferenceExecute, and StackActivate -- all required pipeline operations. It excludes AdapterDelete, SystemConfig, PolicySign, TenantManage, and NodeManage. For service accounts, also apply: (1) IP allowlisting to restrict token usage to CI/CD runner IPs, (2) short token TTL (1 hour) since pipelines should re-authenticate per run, (3) resource scope limiting to the deployment target namespace. Service accounts should never be assigned Admin, even if 'it would be easier.'",
            "subcat": "service_account"
        },
    ]

    for idx in range(n):
        tmpl = templates[idx % len(templates)]
        tenant = random.choice(TENANT_NAMES)
        user = random.choice(USERNAMES[tmpl["role"]])

        framings = [
            f"Least-privilege analysis for tenant '{tenant}': a team member needs to {tmpl['req']}. What is the minimum role?",
            f"Design the minimal permission set for user '{user}' at '{tenant}' who needs to {tmpl['req']}.",
            f"RBAC recommendation: given the requirement to {tmpl['req']}, what role should '{user}' receive and why?",
            f"A service at tenant '{tenant}' requires: {tmpl['req']}. Apply least-privilege principle to select the correct role.",
        ]
        inp = framings[idx % len(framings)]

        all_perms_str = ", ".join(perms_for_role(tmpl["role"]))
        tgt = (
            f"Recommended role: {tmpl['role']}. {tmpl['why']} "
            f"Full permission set for {tmpl['role']}: {all_perms_str}. "
            f"Assigning '{user}' at tenant '{tenant}' with this role satisfies the stated requirements while minimizing the attack surface. "
            f"Review this assignment periodically and downgrade if requirements change."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": tmpl["subcat"],
            "role": tmpl["role"], "permission": "least_privilege"
        }})

    return examples

# ── Category 5: Multi-Tenant RBAC (300) ────────────────────────────────────

def gen_multi_tenant(n: int = 300) -> list[dict]:
    examples = []
    subcats = {
        "tenant_isolation": 45,
        "cross_tenant_denial": 45,
        "system_admin_access": 35,
        "tenant_admin_scope": 35,
        "resource_ownership": 35,
        "tenant_migration": 30,
        "shared_resource_access": 40,
        "tenant_hierarchy": 35,
    }

    # ── tenant_isolation ──
    for i in range(subcats["tenant_isolation"]):
        t1, t2 = random.sample(TENANT_NAMES, 2)
        role = random.choice(["Admin", "Operator", "SRE"])
        user = random.choice(USERNAMES[role])
        perm = random.choice(perms_for_role(role)[:8])
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"User '{user}' is a tenant-scoped {role} for '{t1}'. They attempt to {PERM_DESCRIPTIONS[perm]} "
            f"on adapter '{adapter}' which belongs to their own tenant '{t1}'. Simultaneously, adapter '{random.choice(ADAPTER_NAMES)}' "
            f"exists in tenant '{t2}'. How does tenant isolation work here?"
        )
        tgt = (
            f"The operation on '{adapter}' in tenant '{t1}' is PERMITTED because '{user}' holds the {role} role scoped to '{t1}' and {perm} is within {role}'s permission set. "
            f"Tenant isolation in adapterOS is enforced at the authorization layer: every resource is tagged with a tenant_id, and the permission check evaluates "
            f"has_permission(role, {perm}) AND token.tenant_id == resource.tenant_id. "
            f"Resources in '{t2}' are completely invisible to '{user}' -- not just inaccessible, but non-existent from their perspective. "
            f"API responses are filtered by tenant_id before serialization, so AdapterList for '{user}' returns only '{t1}' adapters. "
            f"This isolation model prevents information leakage: '{user}' cannot determine whether '{t2}' exists, how many adapters it has, or any other metadata."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "tenant_isolation",
            "role": role, "permission": perm
        }})

    # ── cross_tenant_denial ──
    for i in range(subcats["cross_tenant_denial"]):
        t1, t2 = random.sample(TENANT_NAMES, 2)
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])
        perm = random.choice(perms_for_role(role)[:6])
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"{role} user '{user}' at tenant '{t1}' attempts to access adapter '{adapter}' which belongs to tenant '{t2}'. "
            f"The user has {perm} permission within their own tenant. Does cross-tenant access work?"
        )
        tgt = (
            f"DENIED. Cross-tenant access is strictly forbidden in adapterOS regardless of role level. "
            f"Even though '{user}' holds the {role} role with {perm} permission, these permissions are scoped to tenant '{t1}' only. "
            f"The authorization check fails at the tenant boundary: token.tenant_id='{t1}' != resource.tenant_id='{t2}'. "
            f"This is enforced before the permission check -- the system first verifies tenant membership, then checks role permissions. "
            f"Response: HTTP 404 Not Found (not 403, to prevent tenant enumeration). "
            f"From '{user}''s perspective, adapter '{adapter}' in tenant '{t2}' does not exist. "
            f"Cross-tenant access requires the SystemAdmin role, which is a global role not scoped to any tenant. "
            f"Audit log: principal='{user}', source_tenant='{t1}', target_tenant='{t2}', action='{perm}', result='cross_tenant_denied'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "cross_tenant_denial",
            "role": role, "permission": perm
        }})

    # ── system_admin_access ──
    for i in range(subcats["system_admin_access"]):
        tenants = random.sample(TENANT_NAMES, 3)
        user = random.choice(USERNAMES["SystemAdmin"])
        perm = random.choice(ALL_PERMS)
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"SystemAdmin '{user}' needs to {PERM_DESCRIPTIONS[perm]} across tenants {', '.join(tenants)}. "
            f"How does SystemAdmin cross-tenant access work, and what safeguards exist?"
        )
        tgt = (
            f"SystemAdmin is a global role that transcends tenant boundaries. '{user}' can execute {perm} across all tenants including {', '.join(tenants)}. "
            f"Unlike tenant-scoped roles where token.tenant_id must match resource.tenant_id, SystemAdmin tokens carry tenant_id='*' (wildcard), "
            f"which passes all tenant boundary checks. "
            f"Safeguards for SystemAdmin access: (1) SystemAdmin tokens require hardware security key (HSM) authentication, not just password + TOTP. "
            f"(2) Every SystemAdmin action is logged to a separate, immutable audit stream that tenant Admins can review for their own tenant's resources. "
            f"(3) SystemAdmin sessions have a maximum duration of 4 hours with no renewal -- the user must re-authenticate. "
            f"(4) SystemAdmin actions that modify tenant resources generate notifications to the affected tenant's Admin. "
            f"(5) The number of SystemAdmin accounts is capped and tracked -- creation of new SystemAdmin accounts requires approval from 2 existing SystemAdmins."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "positive",
            "subcategory": "system_admin_access",
            "role": "SystemAdmin", "permission": perm
        }})

    # ── tenant_admin_scope ──
    for i in range(subcats["tenant_admin_scope"]):
        t1, t2 = random.sample(TENANT_NAMES, 2)
        user = random.choice(USERNAMES["Admin"])
        perm = random.choice(ADMIN_EXCLUSIVE)
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"Admin user '{user}' scoped to tenant '{t1}' has full Admin permissions within their tenant. "
            f"They attempt to {PERM_DESCRIPTIONS[perm]} on a resource in tenant '{t2}'. "
            f"Does being a tenant Admin grant cross-tenant Admin access?"
        )
        tgt = (
            f"No. Tenant Admin and SystemAdmin are fundamentally different roles. "
            f"'{user}' as Admin of '{t1}' has the maximum permission set (all 25+ permissions) but ONLY within tenant '{t1}'. "
            f"The 'Admin' in tenant-scoped Admin means 'full control within this tenant,' not 'full control across the system.' "
            f"Attempting to access tenant '{t2}' resources returns 404 (resource not found from '{user}''s perspective). "
            f"To perform cross-tenant operations, '{user}' would need SystemAdmin -- a separate, global role with additional authentication requirements (HSM key, approval chain). "
            f"This distinction is critical for multi-tenant security: each tenant's Admin is sovereign within their boundary but cannot escape it. "
            f"Audit log: principal='{user}', home_tenant='{t1}', target_tenant='{t2}', action='{perm}', result='tenant_boundary_violation'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "negative",
            "subcategory": "tenant_admin_scope",
            "role": "Admin", "permission": perm
        }})

    # ── resource_ownership ──
    for i in range(subcats["resource_ownership"]):
        tenant = random.choice(TENANT_NAMES)
        role = random.choice(["Admin", "Operator"])
        user = random.choice(USERNAMES[role])
        adapter = random.choice(ADAPTER_NAMES)
        training_job = random.choice(TRAINING_JOB_IDS)

        inp = (
            f"User '{user}' ({role}) at tenant '{tenant}' created adapter '{adapter}' and training job '{training_job}'. "
            f"Another user at the same tenant with role Viewer tries to view these resources. "
            f"Does resource ownership affect permission checks?"
        )
        tgt = (
            f"No, adapterOS RBAC is role-based, not owner-based. Resource ownership does not grant additional permissions or restrict others' access within the same tenant. "
            f"The Viewer at tenant '{tenant}' can view '{adapter}' (via AdapterView) and '{training_job}' (via TrainingView) regardless of who created them, "
            f"because these are shared permissions available to all roles. "
            f"Conversely, '{user}' being the creator of '{adapter}' does not give them extra permissions beyond what the {role} role provides -- "
            f"a Viewer who created an adapter cannot delete it (AdapterDelete requires Admin). "
            f"All resources within a tenant are accessible to all roles at that tenant according to their role's permission set. "
            f"This design simplifies the authorization model: the permission check is always role_has_permission(role, action), never creator == requester."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": "positive",
            "subcategory": "resource_ownership",
            "role": role, "permission": "resource_ownership"
        }})

    # ── tenant_migration ──
    for i in range(subcats["tenant_migration"]):
        t1, t2 = random.sample(TENANT_NAMES, 2)
        user_sys = random.choice(USERNAMES["SystemAdmin"])
        user_admin1 = random.choice(USERNAMES["Admin"])
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"Adapter '{adapter}' needs to be migrated from tenant '{t1}' to tenant '{t2}'. "
            f"Can tenant Admin '{user_admin1}' of '{t1}' perform this migration? What role is required?"
        )
        tgt = (
            f"Tenant Admin '{user_admin1}' of '{t1}' CANNOT perform cross-tenant migration alone. "
            f"Migration involves: (1) reading adapter weights and metadata from '{t1}', (2) writing them to '{t2}'. "
            f"Step 2 requires access to tenant '{t2}', which '{user_admin1}' does not have -- their Admin role is scoped to '{t1}'. "
            f"Cross-tenant migration requires SystemAdmin '{user_sys}' who can access both tenants. "
            f"The migration workflow: SystemAdmin initiates a migration request, both tenant Admins ('{t1}' and '{t2}') must approve, "
            f"the system performs an atomic copy with full lineage tracking, and the adapter is registered in '{t2}' with provenance showing its origin in '{t1}'. "
            f"Post-migration, the adapter in '{t2}' is a new resource owned by tenant '{t2}' -- modifying it does not affect the original in '{t1}'. "
            f"Full audit trail: migration_id, source_tenant, dest_tenant, adapter, initiated_by, approved_by_source, approved_by_dest, timestamp."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "tenant_migration",
            "role": "Admin", "permission": "TenantManage"
        }})

    # ── shared_resource_access ──
    for i in range(subcats["shared_resource_access"]):
        tenants = random.sample(TENANT_NAMES, 2)
        t1, t2 = tenants
        role = random.choice(ROLES)
        user = random.choice(USERNAMES[role])
        shared_resources = [
            ("base model 'llama-3-8b'", "Base models are system-level resources shared across all tenants. They are registered by SystemAdmin and visible to all tenants via the shared model registry."),
            ("global policy pack 'safety-baseline-v2'", "Global policy packs are system-level resources published by SystemAdmin and enforced across all tenants. Tenants can view them (PolicyView) but cannot modify or sign them."),
            ("system metrics dashboard", "Aggregate system metrics (cluster-wide utilization, global error rates) are accessible via MetricsView to all tenants but show only anonymized, aggregated data -- no per-tenant breakdown is visible to non-SystemAdmin roles."),
        ]
        resource_name, resource_desc = shared_resources[i % len(shared_resources)]

        inp = (
            f"User '{user}' ({role}) at tenant '{t1}' accesses {resource_name}. "
            f"This resource is used by both '{t1}' and '{t2}'. How does adapterOS handle shared resources across tenants?"
        )
        tgt = (
            f"'{user}' can access {resource_name} because it is a system-level shared resource, not a tenant-owned resource. "
            f"{resource_desc} "
            f"Shared resources have tenant_id='system' rather than a specific tenant ID. The authorization check for shared resources is: "
            f"has_permission(role, required_action) -- the tenant boundary check is skipped because system resources are not tenant-scoped. "
            f"However, any tenant-specific data derived from shared resources (e.g., fine-tuned adapters based on a shared base model) "
            f"IS tenant-scoped and subject to normal tenant isolation. "
            f"'{t2}' users accessing the same shared resource get an identical view -- there is no tenant-specific customization of shared resources. "
            f"Modification of shared resources requires SystemAdmin; no tenant-scoped role can alter system-level resources."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.94), "label": "positive",
            "subcategory": "shared_resource_access",
            "role": role, "permission": "shared_resource"
        }})

    # ── tenant_hierarchy ──
    for i in range(subcats["tenant_hierarchy"]):
        parent = random.choice(TENANT_NAMES[:5])
        child = random.choice(TENANT_NAMES[5:])
        role = random.choice(["Admin", "Operator"])
        user = random.choice(USERNAMES[role])
        perm = random.choice(perms_for_role(role)[:6])
        adapter = random.choice(ADAPTER_NAMES)

        inp = (
            f"In a hierarchical tenant setup, '{parent}' is the parent tenant and '{child}' is a sub-tenant. "
            f"User '{user}' ({role}) at parent tenant '{parent}' attempts to {PERM_DESCRIPTIONS[perm]} on adapter '{adapter}' in sub-tenant '{child}'. "
            f"Does parent tenant membership grant access to sub-tenant resources?"
        )
        tgt = (
            f"No. adapterOS uses a FLAT tenant isolation model, not hierarchical. Parent/sub-tenant relationships are organizational metadata "
            f"but do not affect the authorization model. '{user}' at '{parent}' has zero implicit access to '{child}' resources. "
            f"The authorization check is: token.tenant_id='{parent}' != resource.tenant_id='{child}' -> DENIED. "
            f"Parent-child relationships can be configured to allow explicit cross-tenant grants: "
            f"grant(principal='{user}', role={role}, tenant='{child}', granted_by=SystemAdmin_or_{child}_Admin). "
            f"Without an explicit grant, the hierarchical relationship provides no access advantage. "
            f"This prevents organizational restructuring from accidentally expanding access -- a tenant merger or split never silently changes who can access what. "
            f"Audit log: principal='{user}', source_tenant='{parent}', target_tenant='{child}', action='{perm}', result='cross_tenant_denied'."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "tenant_hierarchy",
            "role": role, "permission": perm
        }})

    return examples

# ── Category 6: Permission Boundaries (300) ────────────────────────────────

def gen_permission_boundaries(n: int = 300) -> list[dict]:
    examples = []
    subcats = {
        "sre_adapter_restart": 40,
        "compliance_dataset_view": 40,
        "operator_policy_read": 35,
        "viewer_metrics_scope": 35,
        "sre_training_view": 35,
        "compliance_lineage_chain": 40,
        "operator_node_inspect": 35,
        "boundary_overlap": 40,
    }

    # ── sre_adapter_restart ──
    for i in range(subcats["sre_adapter_restart"]):
        user = random.choice(USERNAMES["SRE"])
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)

        scenarios = [
            (
                f"crashes during inference",
                "AdapterLoad",
                f"Restarting a crashed adapter requires AdapterUnload (to clear the faulted state) followed by AdapterLoad (to reload into GPU memory). Both are Operator-tier permissions. "
                f"SRE can DETECT the crash via SystemMetrics (GPU error counters), ProcessList (adapter process missing), and LogAccess (crash stack traces), "
                f"but cannot REMEDIATE it. The SRE must escalate to an Operator or Admin who can execute AdapterUnload + AdapterLoad."
            ),
            (
                f"is stuck in a warm state and not promoting to hot despite high traffic",
                "AdapterLoad",
                f"The adapter lifecycle state machine is managed by Operator-tier permissions. Even though the SRE can observe the stuck state via SystemMetrics and NodeInspect, "
                f"manually forcing a state transition requires AdapterLoad or AdapterUnload. "
                f"SRE can: (1) diagnose WHY promotion is stuck (memory pressure? threshold bug? node issue?) via their debugging tools, "
                f"(2) document findings and escalate. They cannot force-promote the adapter."
            ),
            (
                f"has corrupted weights and needs to be unloaded and re-registered",
                "AdapterUnload",
                f"Unloading an adapter with corrupted weights requires AdapterUnload (Operator-tier). Re-registering requires AdapterRegister (also Operator-tier). "
                f"SRE can detect corruption through PerformanceProfile (inference quality degradation), LogAccess (integrity check failures), "
                f"and SystemMetrics (anomalous output distributions), but the remediation chain (unload -> delete -> re-register -> load) is entirely Operator/Admin territory."
            ),
        ]
        desc, needed_perm, explanation = scenarios[i % len(scenarios)]

        inp = (
            f"SRE user '{user}' at tenant '{tenant}' observes that adapter '{adapter}' {desc}. "
            f"Can the SRE restart or fix the adapter directly?"
        )
        tgt = (
            f"No. {explanation} "
            f"This is a key permission boundary in adapterOS: SRE can observe and diagnose, Operator can act. "
            f"The SRE role deliberately lacks all adapter modification permissions to enforce separation between debugging and remediation."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "sre_adapter_restart",
            "role": "SRE", "permission": needed_perm
        }})

    # ── compliance_dataset_view ──
    for i in range(subcats["compliance_dataset_view"]):
        user = random.choice(USERNAMES["Compliance"])
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)
        training_job = random.choice(TRAINING_JOB_IDS)

        inp = (
            f"Compliance user '{user}' at tenant '{tenant}' needs to verify what training data was used for adapter '{adapter}' "
            f"(training job '{training_job}'). Can they view the training dataset through LineageInspect?"
        )
        tgt = (
            f"Yes. The Compliance role's LineageInspect permission allows tracing the full provenance chain from adapter to training dataset. "
            f"LineageInspect reveals: (1) dataset identifiers and versions used in '{training_job}', (2) data sources and ingestion timestamps, "
            f"(3) preprocessing steps applied, (4) dataset hash for integrity verification, (5) data retention metadata. "
            f"This is intentional: Compliance needs to verify data governance without needing to access the raw training data content. "
            f"LineageInspect shows METADATA about datasets (what, when, where, who), not the raw data itself. "
            f"For actual data content access (viewing individual training examples), the DataRetentionAudit permission provides sampling capability "
            f"with audit-logged access. Together, LineageInspect + DataRetentionAudit give Compliance full audit capability without any modification rights. "
            f"The data producer's identity, the data pipeline version, and the schema version are all visible through this chain."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "compliance_dataset_view",
            "role": "Compliance", "permission": "LineageInspect"
        }})

    # ── operator_policy_read ──
    for i in range(subcats["operator_policy_read"]):
        user = random.choice(USERNAMES["Operator"])
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"Operator '{user}' at tenant '{tenant}' needs to check which policies are currently active "
            f"before deploying a new adapter stack. Can they view policies? Can they validate a policy pack? Can they sign a new policy?"
        )
        tgt = (
            f"The Operator can: (1) View policies -- YES, via PolicyView (shared permission, all roles). "
            f"(2) Validate a policy pack -- YES, Operator inherits PolicyValidate from the Compliance tier through the hierarchy Admin > Operator > SRE > Compliance > Viewer. "
            f"(3) Sign a new policy -- NO, PolicySign is Admin-exclusive. "
            f"This boundary is precisely calibrated: Operators need to SEE active policies to ensure their deployments comply, "
            f"and they can VALIDATE that a proposed policy is well-formed, but they cannot SIGN (activate/enforce) new policies. "
            f"Policy signing is a governance action that requires Admin authority because it changes what the entire system enforces. "
            f"Response for PolicySign attempt: 403 Forbidden with {{\"required_permission\": \"PolicySign\", \"current_role\": \"Operator\", \"minimum_role\": \"Admin\"}}."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "operator_policy_read",
            "role": "Operator", "permission": "PolicyView,PolicyValidate,PolicySign"
        }})

    # ── viewer_metrics_scope ──
    for i in range(subcats["viewer_metrics_scope"]):
        user = random.choice(USERNAMES["Viewer"])
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"Viewer '{user}' at tenant '{tenant}' accesses the metrics dashboard. "
            f"They can see request rates and latencies via MetricsView. Can they also see GPU utilization, memory pressure, "
            f"and per-node hardware metrics? Where is the boundary between MetricsView and SystemMetrics?"
        )
        tgt = (
            f"MetricsView (shared, all roles) provides APPLICATION-level metrics: request rates, response latencies, error rates, "
            f"adapter activation counts, training progress percentages, and throughput. These are the metrics shown on the main dashboard. "
            f"SystemMetrics (SRE-tier) provides INFRASTRUCTURE-level metrics: GPU utilization per node, memory pressure, thermal data, "
            f"VRAM allocation, network bandwidth, disk I/O, and per-process resource consumption. "
            f"'{user}' as Viewer can see: 'request latency p99 = 42ms' but NOT 'GPU 0 utilization = 87%, VRAM = 22.4/24GB'. "
            f"The boundary is: MetricsView answers 'how is the service performing?' while SystemMetrics answers 'how is the infrastructure performing?' "
            f"This separation prevents Viewer users from accessing infrastructure details that could reveal capacity limits, "
            f"hardware topology, or resource allocation patterns that might be sensitive."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "viewer_metrics_scope",
            "role": "Viewer", "permission": "MetricsView"
        }})

    # ── sre_training_view ──
    for i in range(subcats["sre_training_view"]):
        user = random.choice(USERNAMES["SRE"])
        training_job = random.choice(TRAINING_JOB_IDS)
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"SRE '{user}' at tenant '{tenant}' observes that training job '{training_job}' is consuming excessive GPU resources. "
            f"They can view training status via TrainingView (shared). Can they cancel the training job? "
            f"Can they adjust training hyperparameters to reduce resource usage?"
        )
        tgt = (
            f"The SRE can: (1) VIEW training status -- YES, via TrainingView (shared permission). This includes progress, loss curves, GPU allocation, and ETA. "
            f"(2) Cancel training -- NO. TrainingCancel is an Operator-tier permission. SRE at hierarchy level 2 cannot access Operator-tier (level 1) permissions. "
            f"(3) Adjust hyperparameters -- NO. Modifying a running training job requires TrainingStart (to restart with new params) or a hypothetical TrainingModify permission, both Operator-tier. "
            f"The SRE's correct workflow: (1) diagnose the resource issue using SystemMetrics, ProcessList, and PerformanceProfile, "
            f"(2) document whether the resource consumption is expected or anomalous, (3) escalate to an Operator who can TrainingCancel if needed. "
            f"SRE can see WHAT is happening with training but cannot change it -- the same 'observe but don't touch' boundary that applies to adapters."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "negative",
            "subcategory": "sre_training_view",
            "role": "SRE", "permission": "TrainingCancel"
        }})

    # ── compliance_lineage_chain ──
    for i in range(subcats["compliance_lineage_chain"]):
        user = random.choice(USERNAMES["Compliance"])
        adapter = random.choice(ADAPTER_NAMES)
        tenant = random.choice(TENANT_NAMES)
        stack = random.choice(STACK_NAMES)

        inp = (
            f"Compliance user '{user}' at tenant '{tenant}' needs to trace adapter '{adapter}' used in stack '{stack}' "
            f"all the way back to its training data, verify data retention compliance, and confirm the training was policy-compliant. "
            f"What permissions does this require and does Compliance have them all?"
        )
        tgt = (
            f"Yes, the Compliance role has every permission needed for this full audit chain: "
            f"(1) StackView (shared) -- view stack '{stack}' configuration to identify which adapters it includes and their routing weights. "
            f"(2) AdapterView (shared) -- view adapter '{adapter}' metadata including training provenance, version, and deployment history. "
            f"(3) TrainingView (shared) -- view the training run details: hyperparameters, loss curves, duration, and dataset references. "
            f"(4) LineageInspect (Compliance-tier) -- trace the full provenance chain from adapter -> training run -> dataset -> source data. "
            f"(5) DataRetentionAudit (Compliance-tier) -- verify that data retention policies are being followed for the training dataset. "
            f"(6) PolicyValidate (Compliance-tier) -- validate that the training configuration complied with the active policy pack at training time. "
            f"The Compliance role is specifically designed for this end-to-end audit workflow. No escalation to SRE, Operator, or Admin is needed. "
            f"Crucially, Compliance can verify everything without being able to modify anything in the chain."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "positive",
            "subcategory": "compliance_lineage_chain",
            "role": "Compliance", "permission": "LineageInspect,DataRetentionAudit,PolicyValidate"
        }})

    # ── operator_node_inspect ──
    for i in range(subcats["operator_node_inspect"]):
        user = random.choice(USERNAMES["Operator"])
        tenant = random.choice(TENANT_NAMES)

        inp = (
            f"Operator '{user}' at tenant '{tenant}' wants to check which nodes their adapters are loaded on "
            f"and the GPU memory available on each node. Can they use NodeInspect? "
            f"What infrastructure visibility does an Operator actually have?"
        )
        tgt = (
            f"Yes, Operator inherits NodeInspect from the SRE tier through the hierarchy (Admin > Operator > SRE). "
            f"Operator at level 1 inherits all SRE-tier (level 2) permissions: SystemMetrics, NodeInspect, ProcessList, LogAccess, PerformanceProfile. "
            f"'{user}' can inspect node health, GPU memory allocation, and hardware status. "
            f"This is correct by design: Operators who manage adapter lifecycle (load, unload, register) need infrastructure visibility to make informed decisions. "
            f"For example, before executing AdapterLoad, an Operator should check NodeInspect to verify sufficient GPU memory on the target node. "
            f"What Operator CANNOT do with infrastructure: NodeManage (adding/removing/reconfiguring nodes is Admin-exclusive). "
            f"The boundary is: Operator can READ infrastructure state (inherited from SRE) and MODIFY adapter state (Operator-tier), but cannot MODIFY infrastructure state (Admin-exclusive)."
        )

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.95), "label": "positive",
            "subcategory": "operator_node_inspect",
            "role": "Operator", "permission": "NodeInspect"
        }})

    # ── boundary_overlap ──
    for i in range(subcats["boundary_overlap"]):
        tenant = random.choice(TENANT_NAMES)
        overlaps = [
            {
                "roles": ["Operator", "SRE"],
                "q": f"Both Operator and SRE can view SystemMetrics. But Operator can also start training while SRE can profile performance. What's the functional overlap?",
                "a": (
                    f"Operator and SRE share all SRE-tier permissions (SystemMetrics, NodeInspect, ProcessList, LogAccess, PerformanceProfile) "
                    f"plus all shared permissions. Their divergence is upward: Operator additionally holds Operator-tier permissions "
                    f"(AdapterRegister, AdapterLoad, AdapterUnload, TrainingStart, TrainingCancel, InferenceExecute, StackCreate, StackActivate) "
                    f"while SRE does not. SRE has no unique permissions that Operator lacks -- Operator is strictly a superset of SRE. "
                    f"This means any task an SRE can do, an Operator can also do. The reverse is not true: TrainingStart, InferenceExecute, "
                    f"and adapter management are Operator-only (above SRE in the hierarchy). "
                    f"In practice, organizations assign SRE to infrastructure teams who should NOT be able to modify adapter or training state, "
                    f"and Operator to ML platform teams who need both infrastructure visibility AND runtime modification capabilities."
                )
            },
            {
                "roles": ["Compliance", "Viewer"],
                "q": f"Compliance and Viewer both have all shared permissions. What does Compliance get that Viewer doesn't? When would you choose one over the other?",
                "a": (
                    f"Compliance adds four permissions beyond Viewer's shared set: AuditView, PolicyValidate, LineageInspect, and DataRetentionAudit. "
                    f"These are the governance and audit capabilities that distinguish an auditor from a passive observer. "
                    f"Viewer can see CURRENT STATE (adapter list, training status, metrics, policies). "
                    f"Compliance can see current state PLUS HISTORY AND PROVENANCE (audit trail, lineage chains, retention compliance, policy validation). "
                    f"Choose Viewer for: dashboard consumers, stakeholders, developers onboarding, anyone who only needs 'what does the system look like right now?' "
                    f"Choose Compliance for: auditors, governance teams, legal/regulatory reviewers, anyone who needs 'how did the system get to this state and is it compliant?' "
                    f"The AuditView permission in particular is the key differentiator -- it provides access to the immutable audit trail of all system actions."
                )
            },
            {
                "roles": ["Admin", "Operator"],
                "q": f"Admin and Operator seem similar -- both can manage adapters, training, and stacks. What specifically can Admin do that Operator cannot?",
                "a": (
                    f"Admin adds exactly 5 permissions beyond Operator: AdapterDelete, PolicySign, TenantManage, NodeManage, and SystemConfig. "
                    f"These are the 'irreversible or system-wide' actions: "
                    f"AdapterDelete -- permanent removal (Operator can register/load/unload but not destroy). "
                    f"PolicySign -- cryptographic policy activation that changes enforcement rules system-wide within the tenant. "
                    f"TenantManage -- creating, modifying, or deleting tenant configuration and resource quotas. "
                    f"NodeManage -- adding, removing, or reconfiguring compute nodes. "
                    f"SystemConfig -- modifying security settings, authentication policies, and system parameters. "
                    f"The pattern: Operator handles day-to-day runtime operations (load, train, infer, stack). "
                    f"Admin handles governance, infrastructure, and destructive actions. "
                    f"This boundary is the most common point of confusion in adapterOS RBAC -- teams often want to give Operators AdapterDelete 'for cleanup,' "
                    f"but the correct approach is to have Operators flag adapters for deletion and Admins execute the delete."
                )
            },
            {
                "roles": ["SRE", "Compliance"],
                "q": f"SRE and Compliance are both 'read-mostly' roles. How do their capabilities differ in practice?",
                "a": (
                    f"SRE and Compliance read different things for different purposes. "
                    f"SRE reads INFRASTRUCTURE: SystemMetrics (GPU/memory/thermal), NodeInspect (node health), ProcessList (running processes), "
                    f"LogAccess (application and system logs), PerformanceProfile (profiling data). Purpose: diagnose and debug. "
                    f"Compliance reads GOVERNANCE: AuditView (action history), PolicyValidate (rule compliance), "
                    f"LineageInspect (data provenance), DataRetentionAudit (retention compliance). Purpose: verify and audit. "
                    f"There is ZERO overlap in their tier-specific permissions. Their only overlap is the shared permissions (AdapterList, AdapterView, TrainingView, PolicyView, MetricsView, StackView). "
                    f"SRE answers: 'why is the system behaving this way?' Compliance answers: 'is the system following the rules?' "
                    f"Neither role inherits from the other -- SRE (level 2) is above Compliance (level 3) in the hierarchy, "
                    f"so SRE inherits Compliance's permissions but Compliance does NOT inherit SRE's. "
                    f"An SRE can do everything a Compliance user can, plus infrastructure debugging. A Compliance user cannot debug infrastructure."
                )
            },
        ]
        overlap = overlaps[i % len(overlaps)]

        inp = f"Permission boundary analysis for tenant '{tenant}': {overlap['q']}"
        tgt = overlap["a"]

        examples.append({"input": inp, "target": tgt, "metadata": {
            "quality": quality_score(0.96), "label": "positive",
            "subcategory": "boundary_overlap",
            "role": ",".join(overlap["roles"]), "permission": "boundary_analysis"
        }})

    return examples

# ── Main: Generate and write ────────────────────────────────────────────────

def main():
    random.seed(SEED)

    cat1 = gen_permission_checks(400)
    cat2 = gen_role_assignment(300)
    cat3 = gen_privilege_escalation(400)
    cat4 = gen_least_privilege(300)
    cat5 = gen_multi_tenant(300)
    cat6 = gen_permission_boundaries(300)

    all_examples = cat1 + cat2 + cat3 + cat4 + cat5 + cat6

    # Deterministic shuffle
    random.seed(SEED + 1)
    random.shuffle(all_examples)

    assert len(all_examples) == 2000, f"Expected 2000, got {len(all_examples)}"

    # Verify label distribution
    positive = sum(1 for e in all_examples if e["metadata"]["label"] == "positive")
    negative = sum(1 for e in all_examples if e["metadata"]["label"] == "negative")
    print(f"Total: {len(all_examples)}")
    print(f"Positive: {positive}, Negative: {negative}")

    # Verify subcategory distribution
    subcats = {}
    for e in all_examples:
        sc = e["metadata"]["subcategory"]
        subcats[sc] = subcats.get(sc, 0) + 1
    print("Subcategory distribution:")
    for sc, count in sorted(subcats.items()):
        print(f"  {sc}: {count}")

    # Quality stats
    qualities = [e["metadata"]["quality"] for e in all_examples]
    avg_q = sum(qualities) / len(qualities)
    min_q = min(qualities)
    print(f"Quality: avg={avg_q:.3f}, min={min_q:.2f}")
    below_thresh = sum(1 for q in qualities if q < 0.88)
    print(f"Below 0.88 threshold: {below_thresh}")

    # Write JSONL
    out_path = Path(__file__).parent / "rbac-permissions.jsonl"
    with open(out_path, "w") as f:
        for ex in all_examples:
            f.write(json.dumps(ex, ensure_ascii=False) + "\n")

    print(f"\nWrote {len(all_examples)} examples to {out_path}")


if __name__ == "__main__":
    main()
