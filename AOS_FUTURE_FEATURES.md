# AOS Future Features

> **NOT PART OF MVP**
>
> These features are exploratory ideas for future development. They require **explicit approval TWICE** before any implementation work begins. Do not work on these without:
> 1. First approval: Concept review and prioritization
> 2. Second approval: Technical design sign-off

---

## Feature 1: Self-Improving Adapters

**Concept:** The model generates training data for new LoRA adapters based on its own mistakes or gaps in knowledge.

**Flow:**
```
User asks question → Model responds → User gives negative feedback →
Model generates (prompt, better_response) pairs →
Pairs queued for fine-tuning → New adapter created
```

**Prerequisites:**
- Feedback capture mechanism (thumbs up/down in UI)
- Training data queue table
- Automated training job triggers

**Complexity:** Medium-Hard

**Open Questions:**
- How do we validate generated training pairs before fine-tuning?
- What's the minimum feedback volume before triggering a training job?
- How do we prevent drift or reinforcement of bad patterns?

---

## Feature 2: Multi-Adapter Collaboration

**Concept:** Multiple specialized adapters work together on a task, reviewing and improving each other's output.

**Example Flow:**
```
User request →
  "code-writer" adapter generates initial code →
  "code-reviewer" adapter critiques the output →
  "code-writer" revises based on feedback →
  "code-reviewer" approves or requests further changes →
Final output returned to user
```

**Prerequisites:**
- Orchestration layer for multi-adapter sequencing
- Role-based prompt templates
- Configurable collaboration patterns (debate, review, ensemble)

**Complexity:** Medium

**Open Questions:**
- How many rounds of revision before giving up?
- How do we handle disagreement between adapters?
- Does this integrate with existing chat sessions or need a new abstraction?

---

## Feature 3: Adversarial Adapter Testing

**Concept:** One adapter attempts to break another through edge cases, weird inputs, and prompt injections. The defender adapter tries to handle them gracefully.

**Flow:**
```
"red-team" adapter generates adversarial input →
"defender" adapter processes input →
Evaluate: did defender handle it correctly? →
Log results for analysis and potential training data
```

**Prerequisites:**
- Adversarial prompt generation adapter
- Evaluation criteria for "correct handling"
- Results logging and analysis tooling

**Complexity:** Medium

**Open Questions:**
- What constitutes a "successful" attack vs defense?
- How do we ensure red-team doesn't generate actually harmful content?
- How do we convert adversarial findings into defensive training data?

---

## Feature 4: Live Code Companion

**Concept:** Model watches a git repository and provides real-time feedback as you code.

**Flow:**
```
Developer saves file →
File watcher detects change →
Git diff extracted →
Model analyzes diff →
Suggestions printed to terminal (or shown in UI)
```

**CLI Interface:**
```bash
aosctl watch ./src --adapter code-companion
```

**Prerequisites:**
- File watcher daemon (`notify` crate)
- Git diff parsing and formatting
- Optional: test runner integration for automated test feedback

**Complexity:** Low-Medium

**Open Questions:**
- How chatty should it be? Every save? Only significant changes?
- Should it run tests automatically?
- How do we handle large diffs or refactors?

---

## Implementation Priority (When Approved)

| Feature | Difficulty | Immediate Value | Suggested Order |
|---------|------------|-----------------|-----------------|
| Live Code Companion | Low-Medium | High | 1st |
| Multi-Adapter Collaboration | Medium | Medium | 2nd |
| Adversarial Testing | Medium | Medium | 3rd |
| Self-Improving Adapters | Medium-Hard | High (long-term) | 4th |

**Rationale:** Start with Live Code Companion as it's the quickest win and provides immediate developer utility. Use learnings to inform the more complex features.

---

## Approval Tracking

| Feature | First Approval | Second Approval | Status |
|---------|----------------|-----------------|--------|
| Self-Improving Adapters | - | - | Not Started |
| Multi-Adapter Collaboration | - | - | Not Started |
| Adversarial Testing | - | - | Not Started |
| Live Code Companion | - | - | Not Started |

---

## Notes

- These features are additive to the core MVP functionality
- None of these should block or interfere with current development priorities
- Technical designs should be reviewed before implementation begins
- Consider determinism implications for each feature (especially Features 1-3)
