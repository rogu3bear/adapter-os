# Tool Selection Guide for AdapterOS

## Quick Decision Tree

| Task | Use This Tool |
|------|---------------|
| Find a Rust symbol by name | Serena `find_symbol` |
| See file structure/symbols | Serena `get_symbols_overview` |
| Find all references to a function | Serena `find_referencing_symbols` |
| Replace entire function/method body | Serena `replace_symbol_body` |
| Small inline edit (few lines) | Serena `replace_content` or Claude `Edit` |
| Read a file | Claude `Read` (simpler) |
| Search by filename pattern | Claude `Glob` |
| Search file contents (grep) | Claude `Grep` or Serena `search_for_pattern` |
| Run git/cargo commands | Claude `Bash` |
| Explore codebase patterns | Claude `Task` (Explore agent) |
| Persist project knowledge | Serena `write_memory` |

---

## Serena-First Patterns

### Editing Rust Code

```
1. Get overview: get_symbols_overview("crates/adapteros-lora-router/src/router.rs")
2. Find specific: find_symbol("route", include_body=true, depth=0)
3. Edit: replace_symbol_body() OR replace_content() for small changes
```

### Finding References

```
1. Find referencing: find_referencing_symbols("Router", "crates/adapteros-lora-router")
2. Review snippets in response
3. Update each callsite with replace_content()
```

### Exploring Unknown Code

```
1. Check memories: read_memory("lora_routing_patterns") 
2. If not covered: get_symbols_overview() on likely files
3. Drill down: find_symbol() with include_body=true
```

---

## Claude Code-First Patterns

### Running Commands

```bash
# Always Claude Bash for shell commands
cargo test -p adapteros-lora-router
cargo fmt --all
git status
```

### Broad Searches

```
# Use Claude Glob for filename patterns
Glob("**/router*.rs")

# Use Claude Grep for content patterns
Grep("Q15_GATE_DENOMINATOR")
```

### Complex Exploration

```
# Use Task tool with Explore agent
Task(subagent_type="Explore", prompt="How does K-sparse routing work?")
```

---

## Memory-First Workflow

Before exploring the codebase, check relevant memories:

| Memory | When to Read |
|--------|--------------|
| `suggested_commands` | Need to run build/test commands |
| `codebase_structure` | Need to find where code lives |
| `lora_routing_patterns` | Working on router/scoring/Q15 |
| `determinism_invariants` | Any seed/RNG/sorting changes |
| `api_middleware_stack` | Working on routes/handlers/auth |
| `leptos_ui_patterns` | Working on UI components/pages |
| `task_completion_checklist` | Before marking task complete |

---

## AdapterOS-Specific Rules

### Determinism-Critical Code
1. **Read `determinism_invariants` first**
2. Use `canonical_score_comparator()` for sorting
3. Use `derive_seed()` for all RNG
4. Test with `cargo test --test determinism_core_suite`

### UI Changes
1. Read `leptos_ui_patterns` for component patterns
2. Check WASM: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
3. Follow Liquid Glass design system

### API Changes
1. Read `api_middleware_stack` for route tiers
2. Types go in `adapteros-api-types` with `wasm` feature
3. Follow middleware ordering (auth → tenant → csrf → context → policy → audit)

---

## Working Directory

```
/Users/star/Dev/adapter-os
```

## Project

AdapterOS - Rust-based deterministic ML inference platform for Apple Silicon (~70 crates).
