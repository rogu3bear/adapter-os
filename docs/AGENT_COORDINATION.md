# Agent Coordination Directive

You are operating in a shared, multi-agent environment. To ensure your changes are isolated and do not conflict with other active agents, you **must** follow this coordination protocol.

## Protocol

### 1. Claim Your Scope
**Immediately** upon receiving a task, register your intent and the directories you expect to modify. This "locks" the scope logically and alerts other agents.

```bash
python3 scripts/dev_context.py claim --intent "Refactor Login" --paths "crates/auth" "ui/src/login"
```
*Result:* Returns a Context ID (e.g., `ctx-5921`). **Store this ID.**

### 2. Respect Boundaries
Before editing files, especially if your scope expands, check the global status. Do not edit files claimed by other agents without explicit coordination.

```bash
python3 scripts/dev_context.py status
```

### 3. Isolate Your Changes
The working directory may contain "noise" (changes from other agents or system processes). **Never** rely on a raw `git diff`. Always use your context ID to generate a clean diff of *only* your work.

```bash
python3 scripts/dev_context.py diff --id ctx-5921
```

### 4. Release on Completion
Once your code is submitted, committed, or the task is finished, release your lock to free the paths for others.

```bash
python3 scripts/dev_context.py release --id ctx-5921
```

## Error Handling

- **Conflict Error:** If `claim` fails due to a conflict, review `status` to see who owns the path. If you must proceed, use `--force` only if you are certain the conflict is resolved.
- **Context Not Found:** If your ID is lost or invalid, check `status` to find your session or create a new one.

