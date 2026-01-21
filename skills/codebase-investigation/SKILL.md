---
name: codebase-investigation
description: This skill should be used when the user asks to "investigate the codebase", "audit code quality", "find technical debt", "spawn agents to analyze code", "run a codebase health check", or requests parallel agent investigations. Provides methodology for systematic codebase audits using specialized agents.
version: 0.1.0
---

# Codebase Investigation Methodology

Systematic approach to auditing codebases for quality issues and improvement opportunities using parallel specialized agents.

## Investigation Categories

Six investigation dimensions, each with a specialized agent:

| Category | Agent | Focus |
|----------|-------|-------|
| Taste | `taste-auditor` | Unprofessional content, inappropriate language |
| Performance | `performance-hunter` | Hot path inefficiencies, O(n²) algorithms |
| Error Handling | `error-handling-auditor` | Swallowed errors, missing context |
| Testing | `testing-gap-finder` | Untested security code, missing coverage |
| Duplication | `duplication-detector` | Copy-paste patterns, consolidation opportunities |
| API Ergonomics | `api-ergonomics-reviewer` | Naming inconsistencies, missing patterns |

## Running Investigations

### Parallel Agent Dispatch

Launch multiple agents simultaneously using the Task tool with `run_in_background: true`:

```
Task(subagent_type="taste-auditor", prompt="Audit crates/src/ for unprofessional content", run_in_background=true)
Task(subagent_type="performance-hunter", prompt="Find hot path issues in crates/core/", run_in_background=true)
Task(subagent_type="error-handling-auditor", prompt="Find swallowed errors in crates/server/", run_in_background=true)
```

### Scope Partitioning

For large codebases, partition by:
- **Crate/module**: Each agent investigates different modules
- **Layer**: Core, server, API, CLI separately
- **Concern**: Security-critical code vs general code

Example partitioning for ~70 crate workspace:
1. Core crates (router, worker, kernels)
2. Server/API layer
3. UI crate
4. Supporting crates (policy, telemetry, crypto)
5. CLI and tests

### Tracking Progress

Use TodoWrite to track agent completion:

```
todos = [
  {content: "Taste audit - core", status: "in_progress"},
  {content: "Performance - hot paths", status: "pending"},
  ...
]
```

Update status as agents complete. Compile findings when all finish.

## Report Format

### Per-Agent Findings

Each agent produces a prioritized table:

```markdown
| Severity | Issue | File:Line | Impact | Suggestion |
|----------|-------|-----------|--------|------------|
| 🔴 CRITICAL | [Description] | `file.rs:123` | [Why it matters] | [Fix] |
| 🟠 HIGH | ... | ... | ... | ... |
| 🟡 MEDIUM | ... | ... | ... | ... |
| 🟢 LOW | ... | ... | ... | ... |
```

### Consolidated Report

Combine findings into executive summary:

```markdown
# Codebase Health Report

## The "Ew" Category (Taste)
- X findings, Y low-severity terminology items

## Opportunities Found

### 🔴 CRITICAL
| Category | Issue | Location |
|----------|-------|----------|
| Testing | Zero tests for MFA | `mfa.rs` |
| Error Handling | Swallowed mkdir errors | `security.rs:71` |

### 🟠 HIGH
...

### 🟡 MEDIUM
...

## Recommended Priority
1. Week 1: [Critical fixes]
2. Week 2: [High priority]
3. Week 3+: [Medium/Low]
```

## Severity Classification

**CRITICAL**: Security vulnerabilities, data corruption risks, untested auth code
**HIGH**: Silent failures in production, performance hot paths, missing error context
**MEDIUM**: Code duplication, ergonomics issues, test coverage gaps
**LOW**: Style issues, terminology preferences, minor improvements

## Best Practices

### Agent Prompts

Be specific about scope and expectations:

✅ Good: "Investigate crates/adapteros-server/ and crates/adapteros-db/ for swallowed errors. Focus on production code paths."

❌ Bad: "Find error handling issues"

### Consolidation

When agents complete:
1. Collect all findings
2. Deduplicate overlapping issues
3. Prioritize by severity and effort
4. Group by actionability (quick wins vs architectural)
5. Present executive summary first, details on request

### Follow-up

After investigation, typical next steps:
- Create issues/tickets for critical findings
- Plan sprints around priority groups
- Run targeted investigations on specific areas
- Re-audit after fixes to verify improvement
