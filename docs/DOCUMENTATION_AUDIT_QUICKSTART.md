# Documentation Audit Quick Start

**Purpose:** Quick reference for conducting the root-level documentation audit.

---

## Quick Start

### 1. List Files by Category
```bash
./scripts/list_root_docs.sh
```

### 2. Review Files in Batches
Use the prompt from [DOCUMENTATION_AUDIT_PROMPT.md](./DOCUMENTATION_AUDIT_PROMPT.md) with an AI assistant to evaluate files.

### 3. Track Results
Record findings in [DOCUMENTATION_AUDIT_RESULTS.md](./DOCUMENTATION_AUDIT_RESULTS.md).

---

## Batch Review Workflow

### Step 1: Get File List for a Pattern
```bash
# Example: Review all FIXES files
./scripts/list_root_docs.sh '*_FIXES_*.md'
```

### Step 2: Use the Audit Prompt
Copy the prompt from `docs/DOCUMENTATION_AUDIT_PROMPT.md` and provide the file list to your AI assistant.

### Step 3: Record Results
Update `docs/DOCUMENTATION_AUDIT_RESULTS.md` with the evaluation for each file.

### Step 4: Execute Actions (After Review)
```bash
# Move files (preserves git history)
git mv FILE.md docs/TARGET_DIR/

# Archive files
git mv FILE.md docs/archive/

# After 30-day review period, delete if truly ephemeral
git rm docs/archive/EPHEMERAL_FILE.md
```

---

## Common Patterns

### High Confidence Ephemeral (Archive/Delete)
- `*_FIXES_*.md` - Post-implementation summaries
- `*_SUMMARY.md` - One-time analysis outputs  
- `*_CHECKLIST.md` - Completed task tracking
- `*_ANALYSIS.md` - One-time analyses

### High Confidence Keep in Root
- `README.md`, `CONTRIBUTING.md`, `SECURITY.md`
- `CLAUDE.md`, `AGENTS.md`, `CITATIONS.md`
- `QUICKSTART*.md`, `PRD.md`

### High Confidence Move to docs/
- `BENCHMARK*.md` → `docs/`
- `MLX_*.md` → `docs/` (integration guides)
- `ERROR_*.md` → `docs/` (error handling patterns)

---

## Safety Checklist

Before moving or deleting any file:

- [ ] Check for code references: `grep -r "FILENAME" --include="*.md" --include="*.rs" --include="*.ts"`
- [ ] Verify no links in CLAUDE.md or AGENTS.md
- [ ] Use `git mv` to preserve history
- [ ] Archive before deleting (30-day review period)
- [ ] Update any cross-references in other docs

---

## Example Batch Review

```bash
# 1. Get list of FIXES files
./scripts/list_root_docs.sh '*_FIXES_*.md'

# Output:
# AUTH_FIXES_CHECKLIST.md
# AUTH_FIXES_SUMMARY.md
# ERROR_HANDLING_FIXES_SUMMARY.md
# ...

# 2. Use prompt with AI assistant
# [Copy prompt from DOCUMENTATION_AUDIT_PROMPT.md]
# [Provide file list]
# [Get evaluation for each file]

# 3. Record in DOCUMENTATION_AUDIT_RESULTS.md
# [Fill in table with results]

# 4. Execute HIGH confidence actions
git mv AUTH_FIXES_CHECKLIST.md docs/archive/
git mv ERROR_HANDLING_FIXES_SUMMARY.md docs/archive/
```

---

**See Also:**
- [DOCUMENTATION_AUDIT_PROMPT.md](./DOCUMENTATION_AUDIT_PROMPT.md) - Full audit prompt
- [DOCUMENTATION_AUDIT_RESULTS.md](./DOCUMENTATION_AUDIT_RESULTS.md) - Results tracking
- [DOCUMENTATION_MAINTENANCE.md](./DOCUMENTATION_MAINTENANCE.md) - Ongoing maintenance guide

