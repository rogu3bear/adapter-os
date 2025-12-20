# Multi-Agent Coordination

This directory contains coordination files for multi-agent development on AdapterOS.

## Active Work Tracking

The `active-work.json` file tracks which agents are working on which PRDs/files.
Before starting work, check this file and update it.

## Rules

1. **Before starting work**: Read `active-work.json` to check for conflicts
2. **Claim your work**: Add your agent ID and files to `active-work.json`
3. **Release on completion**: Remove your entry when done
4. **Respect boundaries**: Don't modify files claimed by another agent

## File Format

```json
{
  "work_items": [
    {
      "prd": "PRD-RECT-001",
      "agent_id": "agent-abc123",
      "started_at": "2024-12-16T10:00:00Z",
      "files": ["crates/adapteros-db/src/adapters.rs"],
      "status": "in_progress"
    }
  ]
}
```

## Conflict Resolution

If two agents claim the same file:
1. Check timestamps - earlier claim has priority
2. If timestamps are close, coordinate via the PRD README
3. Prefer additive changes (new functions) over modifications
