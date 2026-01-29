# Human-in-the-Loop Review Workflow

## Overview

Pause operations when external review is needed, resume when review is provided. Supports inference pauses, dataset safety gates, promotion approvals, threat escalation, and quarantine.

---

## Inference State Machine

```
Running → Paused(PauseReason) → Running → Complete/Failed/Cancelled
```

---

## Pause Kinds

| Kind | Description |
|------|-------------|
| `ReviewNeeded` | Needs human code review |
| `PolicyApproval` | Policy gate requires sign-off |
| `ResourceWait` | Waiting on resources |
| `UserRequested` | Manual pause |
| `ThreatEscalation` | High-severity threat detected |

---

## Review Assessment Outcomes

| Assessment | Meaning |
|------------|---------|
| `Approved` | No changes needed |
| `ApprovedWithSuggestions` | Minor suggestions |
| `NeedsChanges` | Changes required |
| `Rejected` | Should not proceed |
| `Inconclusive` | Need more info |

---

## API Endpoints (`handlers/review.rs`)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/infer/{id}/state` | GET | Check if paused |
| `/v1/infer/{id}/review` | POST | Submit review |
| `/v1/infer/paused` | GET | List all paused |
| `/v1/reviews/{pause_id}/context` | GET | Export for external reviewers |
| `/v1/reviews/submit` | POST | Submit review (CLI) |

---

## CLI Commands

```bash
# List pending reviews
aosctl review list [--kind <filter>] [--json]

# Get pause details
aosctl review get <pause_id>

# Submit review directly
aosctl review submit <pause_id> --approve|--reject|--needs-changes \
  [-c comment] [-i issue]...

# Export for external reviewer (e.g., Claude Code)
aosctl review export <pause_id> -o context.json

# Import review response
aosctl review import <pause_id> -f response.json
```

---

## External Reviewer Integration

**Workflow:**
1. AdapterOS pauses when review needed
2. Export: `aosctl review export <pause_id> -o context.json`
3. Share with external reviewer (Claude Code, human)
4. Reviewer generates structured response
5. Import: `aosctl review import <pause_id> -f response.json`
6. AdapterOS resumes/rejects based on assessment

**Response Format:**
```json
{
  "assessment": "Approved|NeedsChanges|Rejected",
  "issues": [{
    "severity": "Low|Medium|High|Critical",
    "category": "Logic|Security|...",
    "description": "...",
    "suggested_fix": "..."
  }],
  "suggestions": ["..."],
  "comments": "Overall feedback",
  "confidence": 0.85
}
```

---

## Key Types (`adapteros-api-types/src/review.rs`)

- `PauseReason` - pause_id, kind, context, created_at
- `ReviewContext` - code, question, scope[], metadata
- `Review` - assessment, issues[], suggestions[], confidence
- `SubmitReviewRequest` - pause_id, review, reviewer
- `ReviewContextExport` - Full context with instructions

---

## Quarantine System (`adapteros-policy/src/quarantine.rs`)

When policy hash violations detected:

**DENIED:** Inference, AdapterLoad, Training, PolicyUpdate
**ALLOWED:** Audit, Status, Metrics (read-only)

```rust
quarantine_manager.check_operation(QuarantineOperation::Inference)?;
```

---

## Promotion Workflow (`adapteros-db/src/promotions.rs`)

Golden run promotion with Ed25519 signed approvals:

- `PromotionRequest` - target_stage, status, notes
- `PromotionGate` - validation checks with pass/fail
- `PromotionApproval` - signed approval records

---

## Security

- All approvals Ed25519 signed
- Audit chain uses BLAKE3 hashing
- Fail-closed: paused inference stays paused until reviewed
- Tenant isolation enforced
