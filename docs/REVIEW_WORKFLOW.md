# Human-in-the-Loop Review Workflow

AdapterOS supports a human-in-the-loop review pattern where the system surfaces items needing review, and external reviewers (including AI assistants like Claude Code) can provide structured feedback.

## Architecture

```
AdapterOS (local)
      │
      ├── flags items "needs review"
      │   └── InferenceState::Paused(PauseReason::ReviewNeeded)
      │   └── Dataset trust_state = "needs_approval"
      │   └── Promotion gates failing
      │   └── Threat detection severity >= High
      │
      └── surfaces to User
              │
              └── User prompts Claude Code (or other reviewer)
                      │
                      └── Reviewer responds with structured feedback
                              │
                              └── User feeds back to AdapterOS
                                      │
                                      └── AdapterOS resumes/rejects operation
```

## Quick Start

```bash
# List pending reviews
aosctl review list

# Export context for Claude Code
aosctl review export <pause_id> -o context.json

# Share context.json with Claude Code, get response.json back

# Import the review response
aosctl review import <pause_id> -f response.json

# Or submit directly via CLI
aosctl review submit <pause_id> --approve --comment "LGTM"
```

## What Triggers "Needs Review"

| Trigger | Location | Description |
|---------|----------|-------------|
| Inference pause | `adapteros-api-types/src/review.rs` | `PauseKind::ReviewNeeded` halts inference |
| Dataset safety | `adapteros-server-api/src/handlers/datasets/safety.rs` | `trust_state = "needs_approval"` blocks training |
| Promotion gates | `adapteros-db/src/promotions.rs` | Failed gates require approval |
| Threat detection | `adapteros-policy/src/threat_detection.rs` | High/Critical severity triggers escalation |
| Quarantine | `adapteros-policy/src/quarantine.rs` | Only audit operations allowed |

## CLI Commands

### `aosctl review list`

List all items pending review.

```bash
aosctl review list                          # List all pending
aosctl review list --kind review-needed     # Filter by pause kind
aosctl review list --json                   # JSON output
```

### `aosctl review get <pause_id>`

Get details for a specific paused item.

```bash
aosctl review get pause-abc123
aosctl review get pause-abc123 --json
```

### `aosctl review submit <pause_id>`

Submit a review response directly.

```bash
# Approve
aosctl review submit pause-abc123 --approve

# Approve with suggestions
aosctl review submit pause-abc123 --approve -s "Consider adding tests"

# Request changes
aosctl review submit pause-abc123 --needs-changes \
  -i "Missing error handling" \
  -i "Security vulnerability in line 42"

# Reject
aosctl review submit pause-abc123 --reject -c "Fundamentally flawed approach"
```

### `aosctl review export <pause_id>`

Export review context for external reviewers (e.g., Claude Code).

```bash
aosctl review export pause-abc123              # Output to stdout
aosctl review export pause-abc123 -o ctx.json  # Output to file
```

Output format:
```json
{
  "pause_id": "pause-abc123",
  "inference_id": "inf-xyz789",
  "kind": "ReviewNeeded",
  "paused_at": "2024-01-15T10:30:00Z",
  "duration_secs": 120,
  "code": "fn process_data(input: &str) -> Result<Output> { ... }",
  "question": "Is this implementation correct?",
  "scope": ["Logic", "Security"],
  "instructions": "Review this item and respond with a JSON file..."
}
```

### `aosctl review import <pause_id>`

Import a review response from an external reviewer.

```bash
aosctl review import pause-abc123 -f response.json
aosctl review import pause-abc123 -f response.json --reviewer "claude-code"
```

Expected response format:
```json
{
  "assessment": "ApprovedWithSuggestions",
  "issues": [
    {
      "severity": "Medium",
      "category": "Security",
      "description": "Input not sanitized before use",
      "location": "line 15",
      "suggested_fix": "Add input validation"
    }
  ],
  "suggestions": [
    "Consider adding unit tests for edge cases"
  ],
  "comments": "Overall good implementation, minor issues noted.",
  "confidence": 0.85
}
```

## API Endpoints

### `GET /v1/reviews/paused`

List all paused inferences awaiting review.

**Response:**
```json
{
  "schema_version": "1.0.0",
  "paused": [
    {
      "inference_id": "inf-xyz789",
      "pause_id": "pause-abc123",
      "kind": "ReviewNeeded",
      "paused_at": "2024-01-15T10:30:00Z",
      "duration_secs": 120,
      "context_preview": "Is this implementation correct?"
    }
  ],
  "total": 1
}
```

### `GET /v1/reviews/{pause_id}`

Get details for a specific paused inference.

### `GET /v1/reviews/{pause_id}/context`

Export full review context for external consumption.

### `POST /v1/reviews/submit`

Submit a review response.

**Request:**
```json
{
  "pause_id": "pause-abc123",
  "review": {
    "assessment": "Approved",
    "issues": [],
    "suggestions": [],
    "comments": "Looks good!",
    "confidence": 0.95
  },
  "reviewer": "human"
}
```

**Response:**
```json
{
  "schema_version": "1.0.0",
  "accepted": true,
  "new_state": "Running",
  "message": "Review accepted, inference resumed"
}
```

## Review Protocol Types

Located in `crates/adapteros-api-types/src/review.rs`:

```rust
// Pause states
enum PauseKind {
    ReviewNeeded,      // Needs human review
    PolicyApproval,    // Policy gate requires sign-off
    ResourceWait,      // Waiting on resources
    UserRequested,     // Manual pause
}

// Review assessment outcomes
enum ReviewAssessment {
    Approved,
    ApprovedWithSuggestions,
    NeedsChanges,
    Rejected,
    Inconclusive,
}

// Issue severity levels
enum IssueSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

// Review scope categories
enum ReviewScope {
    Logic,
    EdgeCases,
    Security,
    Performance,
    Style,
    ApiDesign,
    Testing,
    Documentation,
}
```

## Integration with Claude Code

### Workflow

1. **AdapterOS pauses inference** when it detects something needing review
2. **User exports context**: `aosctl review export <pause_id> -o context.json`
3. **User shares with Claude Code**: "Please review this code and provide feedback"
4. **Claude Code analyzes** and generates structured response
5. **User imports response**: `aosctl review import <pause_id> -f response.json`
6. **AdapterOS resumes** or halts based on assessment

### Example Claude Code Prompt

```
I have a paused inference that needs review. Here's the context:

[paste context.json contents]

Please analyze this and provide a review response in JSON format with:
- assessment: Approved | ApprovedWithSuggestions | NeedsChanges | Rejected
- issues: list of {severity, category, description, location, suggested_fix}
- suggestions: list of improvement ideas
- comments: overall feedback
- confidence: 0.0-1.0
```

## Existing Infrastructure

### Database Layer
- **Promotion approvals**: Ed25519 signed approval records (`promotions.rs`)
- **Policy audit chain**: Merkle-chain with BLAKE3 hashing (`policy_audit.rs`)
- **Patch proposals**: Code patches with validation status (`patch_proposals.rs`)
- **Activity events**: Generic event tracking with metadata (`activity.rs`)

### Pause Registry
- **InferencePauseRegistry**: Tracks paused inferences in memory
- **PausedInferenceInfo**: Metadata about each pause
- **InferencePauseToken**: Allows inference to wait for review
- **InferencePauseHandle**: Given to inference task to await review

### UI Components
- `ConfirmationDialog` - Typed confirmation for destructive actions
- `Badge` variants - Status indicators (Success, Warning, Destructive)
- Two-column layouts - List + detail pattern
- Form validation - Field-level error handling

## Planned Extensions

### UI Pages (Not Yet Implemented)
- **Reviews Queue** - Pending items with filtering
- **Review Detail** - Context, history, submission form
- **Review History** - Timeline of assessments

### Webhook Integration (Planned)

```bash
# Configure webhook endpoint
aosctl config set review.webhook_url "https://example.com/review-callback"

# AdapterOS will POST to webhook when items need review
{
  "pause_id": "uuid",
  "kind": "ReviewNeeded",
  "context": {
    "code": "...",
    "question": "Is this logic correct?",
    "scope": ["Logic", "EdgeCases"]
  }
}
```

## Security Considerations

- All approval records are Ed25519 signed
- Audit chain uses BLAKE3 hashing for tamper detection
- Quarantine mode blocks all non-audit operations
- Review submissions logged to audit trail
- Tenant isolation enforced on all review operations

## Related Files

- `crates/adapteros-api-types/src/review.rs` - Review protocol types
- `crates/adapteros-cli/src/commands/review.rs` - CLI commands
- `crates/adapteros-server-api/src/handlers/review.rs` - API handlers
- `crates/adapteros-lora-worker/src/inference_pause.rs` - Pause registry
- `crates/adapteros-db/src/promotions.rs` - Promotion workflow
- `crates/adapteros-policy/src/quarantine.rs` - Quarantine system
- `crates/adapteros-policy/src/threat_detection.rs` - Threat detection
