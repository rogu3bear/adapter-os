---
description: Launch parallel agents to investigate codebase health
argument-hint: [scope]
allowed-tools: Task, TodoWrite, Read
---

Launch a comprehensive codebase health investigation using parallel specialized agents.

## Scope

Target scope: $ARGUMENTS (defaults to entire codebase if not specified)

## Investigation Categories

Spawn these agents in parallel using Task tool with `run_in_background: true`:

1. **taste-auditor** - Find unprofessional content, inappropriate language
2. **performance-hunter** - Find hot path inefficiencies, O(n²) algorithms
3. **error-handling-auditor** - Find swallowed errors, missing context
4. **testing-gap-finder** - Find untested security code, missing coverage
5. **duplication-detector** - Find copy-paste patterns, consolidation opportunities
6. **api-ergonomics-reviewer** - Find naming inconsistencies, missing patterns

## Process

1. Create TodoWrite entries for tracking each agent
2. Partition the scope appropriately (by module, layer, or crate)
3. Launch all 6 agents in parallel with specific scopes
4. Track progress as agents complete
5. Compile consolidated report when all finish

## Report Format

When all agents complete, compile findings into:

```markdown
# Codebase Health Report

## The "Ew" Category (Taste)
[Summary of taste findings]

## Opportunities Found

### 🔴 CRITICAL
| Category | Issue | Location |
|----------|-------|----------|
| ... | ... | ... |

### 🟠 HIGH
...

### 🟡 MEDIUM
...

### 🟢 LOW
...

## Recommended Priority
1. Week 1: [Critical fixes]
2. Week 2: [High priority]
3. Week 3+: [Medium/Low]
```

Proceed to launch all agents now.
