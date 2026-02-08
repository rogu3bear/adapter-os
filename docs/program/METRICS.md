# AdapterOS Program Metrics

Generated: 2026-02-06
Updated: 2026-02-07

## Phase A Verification (2026-02-07)

All acceptance criteria verified:

| Check | Result |
|-------|--------|
| `grep 'error.set(Some(e.to_string' pages/` | **0 results** |
| `grep 'console::error_1' pages/` | **0 results** |
| Login page uses `report_error_with_toast` | **Yes** (login.rs:131) |
| Chat chips are `<button>` with `on:click` | **Yes** (chat.rs:1600-1620) |
| Routing placeholder is valid JSON | **Yes** (`{"sentiment": "negative"}`) |
| sccache in CLAUDE.md prerequisites | **Yes** (lines 12-14) |
| test_harness.rs has no `env::set_var` | **Yes** (only a comment at line 36) |
| cleanup.rs uses `unsafe {}` blocks | **Yes** (lines 119-120, 197-198) |
| WASM compilation | **Pass** |
| `cargo test -p adapteros-ui --lib` | **175 passed, 0 failed** |

**Follow-up debt:** 2 raw `.to_string()` sites remain outside original scope:
- `documents.rs:772`
- `training/data/upload_dialog.rs:263`

## Baseline (2026-02-06)

| Metric | Baseline | Target | Current |
|--------|----------|--------|---------|
| Steps to first chat | 12-13 | 4-6 | 12-13 |
| Error consistency (unified handler) | 21.4% (6/28) | >90% | **93.3% (28/30)** |
| Raw `.to_string()` error sites | 18 | 0 | **2** (follow-up) |
| Console-only (silent) error sites | 4 | 0 | **0** |
| Unified `report_error_with_toast` sites | 6 | 28+ | **28** |
| Nav groups in taskbar | 8 | 5 | 8 |
| Training job creation paths | 2 | 1 | 2 |
| Config fields (legacy dialog) | 22 | deleted | 22 |
| `env::set_var` in test infra | 20+ | 0 (in shared infra) | **0 in test_harness; cleanup.rs safety-wrapped** |
| Offline build works | No (tantivy git dep) | Yes | No |
| sccache documented as prereq | No | Yes | **Yes** |
| Migration rollback coverage | 1.6% (5/318) | Top 20 covered | 1.6% |

## How to Measure

### Steps to First Chat
Defined as: discrete user actions from login to receiving first inference response.
Count: clicks, page navigations, form submissions, waits.

Current path (12-13 steps):
1. Login
2. Navigate to Documents
3. Click Upload
4. Select file + wait
5. Navigate to document detail
6. Click "Train Adapter"
7. Wizard Step 1: choose data source
8. Wizard Step 1 sub: dataset sub-wizard
9. Wizard Step 2: name + model + category
10. Wizard Step 3: config (or accept preset)
11. Wizard Step 4: review + start
12. Wait for training
13. Navigate to adapter -> Open Chat

### Error Consistency
```bash
# Raw error sites
grep -rn 'error.set(Some(e.to_string' crates/adapteros-ui/src/pages/ | wc -l

# Console-only sites
grep -rn 'console::error_1' crates/adapteros-ui/src/pages/ | wc -l

# Unified handler sites
grep -rn 'report_error_with_toast' crates/adapteros-ui/src/pages/ | wc -l

# Consistency = unified / (raw + console + unified)
```

### Build Reproducibility
```bash
# Check for git-pinned deps
grep 'git = ' Cargo.toml | grep -v 'rev\|tag'

# Offline build test
cargo build --release -p adapteros-cli --features tui --offline
```
