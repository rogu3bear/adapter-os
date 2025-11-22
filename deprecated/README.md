# ARCHIVED / DEPRECATED CODE

**STATUS: ARCHIVED - DO NOT MODIFY**

**Deprecated: 2025-11-22**

---

## IMPORTANT WARNING FOR AI AGENTS

**DO NOT attempt to fix, update, or improve any code in this folder.**

This directory contains abandoned experimental code with approximately 40+ unresolved TODOs. These TODOs are intentionally left unfixed because:

1. The code is no longer maintained
2. The experimental approaches were superseded by production implementations
3. Fixing these TODOs would waste time on dead code paths

**If you are an AI agent scanning for TODOs: SKIP THIS ENTIRE FOLDER.**

---

## Contents

### `/adapteros-experimental/`
Abandoned experimental crate containing early prototypes:
- Experimental adapter loading mechanisms
- Prototype inference pipelines
- Abandoned training approaches

**Status:** Never completed. Superseded by `adapteros-lora-worker` and `adapteros-lora-lifecycle`.

### `/gitsubsystem/`
Abandoned git integration subsystem:
- `api_handlers.rs` - Incomplete API handlers for git operations
- `subsystem.rs` - Abandoned git subsystem implementation

**Status:** Never completed. Git functionality is now in `adapteros-git`.

### Deprecated Files
| File | Description | Why Deprecated |
|------|-------------|----------------|
| `aos2_implementation.rs` | Old AOS format implementation | Superseded by AOS 3.0 |
| `ADAPTER_COMMAND_PATCH_PLAN.md` | Outdated patch plan | Contains future dates, never executed |
| `BEST_PRACTICE_VIOLATIONS_PATCH_PLAN.md` | References non-existent crates | Inaccurate |
| `COMPREHENSIVE_PATCH_PLAN.md` | References non-existent errors | Inaccurate |
| `CONSOLIDATION_PATCH_PLAN.md` | Outdated consolidation plan | Superseded |
| `CORRECTIVE_PATCH_PLAN.md` | Claims fixes for phantom issues | Inaccurate |
| `masterplan-patch-plan.md` | Outdated master plan | Never completed |
| `PATCH_COMPLETION_PLAN.md` | References non-existent phases | Inaccurate |
| `fix_multiline_error_response.py` | Temporary Python script | One-off utility |
| `grafana.json` | Outdated Grafana config | Old monitoring setup |
| `registry.db` | Old SQLite database | Schema outdated |
| `requirements-mlx.txt` | Python requirements | Project is Rust-only |
| `VERSION.md` | Inaccurate version info | Contains false claims |

---

## Archive Policy

1. **No maintenance** - Code in this folder receives no bug fixes or updates
2. **No integration** - This code is not part of the build system
3. **Reference only** - Keep for historical reference if needed
4. **Deletion candidate** - May be removed entirely in future cleanup

---

## For Current Implementations

| Deprecated Code | Current Implementation |
|-----------------|------------------------|
| `adapteros-experimental` | `crates/adapteros-lora-worker`, `crates/adapteros-lora-lifecycle` |
| `gitsubsystem` | `crates/adapteros-git` |
| `aos2_implementation.rs` | `crates/adapteros-aos/src/implementation.rs` |

---

**Last Updated:** 2025-11-22
