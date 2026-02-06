# Audit Coordination Notes

**Date:** 2026-02-05
**Status:** Pre-audit assessment

## Summary

This document coordinates the multi-agent audit effort. The working tree has **77 modified files** and **2 untracked files** with substantial uncommitted changes. Several target files for the audit already contain relevant fixes.

## Files Being Modified by Audit Agents

| File | Agent | Conflict Status |
|------|-------|-----------------|
| `crates/adapteros-lora-router/src/types.rs` | Engineer 1 | **MODIFIED** - `partial_cmp` → `total_cmp` fix already applied |
| `crates/adapteros-lora-router/src/router.rs` | Engineer 1, 3 | **MODIFIED** - `total_cmp` fixes in 2 locations already applied |
| `crates/adapteros-ui/dist/components.css` | Engineer 2 | **MODIFIED** - 212 lines changed, glass blur variables standardized |
| `crates/adapteros-server-api/src/handlers/replay.rs` | Engineer 3 | **MODIFIED** - `MAX_TOKENS_LIMIT` validation already added |
| `crates/adapteros-ui/src/components/tabs.rs` | Designer 1 | **MODIFIED** - ARIA controls/labelledby added, badge aria-label |
| `crates/adapteros-ui/src/pages/adapters.rs` | Designer 1 | **MODIFIED** - Using new `PageScaffold`, show-more aria-label |
| `crates/adapteros-ui/src/components/toggle.rs` | Designer 3 | **MODIFIED** - `aria-label` prop added to `Select` component |

## Detected Conflicts and Risks

### High Risk: Existing Changes Cover Audit Targets

**Finding:** Most audit target files already have uncommitted changes that appear to address the same issues:

1. **Router determinism (Engineer 1):** `partial_cmp` → `total_cmp` migration complete in both `types.rs` and `router.rs`
2. **Replay validation (Engineer 3):** `MAX_TOKENS_LIMIT` check already added to `execute_replay_session`
3. **Tab accessibility (Designer 1):** ARIA `id`, `aria-controls`, `aria-labelledby` relationships implemented
4. **Select accessibility (Designer 3):** `aria_label` prop with smart application logic exists

### Medium Risk: Concurrent Work

**Finding:** The plan document `2026-02-05-chat-queue-ux-design.md` is untracked and describes Phase 1 implementation targeting:
- `chat_dock.rs` (modified)
- `signals.rs` / chat signals (modified)
- `components.css` (modified)

This is **active work-in-progress** on files that overlap with the CSS audit.

### Low Risk: New Component

**Finding:** Untracked file `page_scaffold.rs` introduces a new component already being used by `adapters.rs`. This should be staged together with `adapters.rs` changes.

## Recommendations

### 1. Verify Before Modifying

Before editing any target file, agents **must check the current diff** to see if the fix is already applied. Running:

```bash
git diff HEAD -- <file>
```

### 2. Do Not Duplicate Existing Work

The following fixes appear **already complete** in the working tree:

| Issue | Fix Location | Status |
|-------|--------------|--------|
| `partial_cmp` non-determinism | `types.rs:150`, `router.rs:950,2199` | Done |
| Missing `MAX_TOKENS_LIMIT` check | `replay.rs:555-563` | Done |
| Tab ARIA relationships | `tabs.rs` | Done |
| Select missing aria-label | `toggle.rs:90,108-112` | Done |
| Show-more button aria-label | `adapters.rs:287-295` | Done |

### 3. Integration Order

If committing the existing changes:

1. **First:** Core infrastructure
   - `adapteros-lora-router` (determinism fix)
   - `adapteros-server-api` (replay validation)
   - `adapteros-db`, `adapteros-config` changes

2. **Second:** UI accessibility
   - `adapteros-ui/src/components/tabs.rs`
   - `adapteros-ui/src/components/toggle.rs`
   - `adapteros-ui/src/components/layout/page_scaffold.rs` (new file)
   - `adapteros-ui/src/pages/adapters.rs`

3. **Third:** CSS cleanup
   - `adapteros-ui/dist/components.css`

4. **Fourth:** Remaining changes by logical group

### 4. Suggested Commit Groupings

```
# Commit 1: Router determinism
git add crates/adapteros-lora-router/src/types.rs \
        crates/adapteros-lora-router/src/router.rs
git commit -m "fix(router): use total_cmp for deterministic float ordering"

# Commit 2: API validation
git add crates/adapteros-server-api/src/handlers/replay.rs
git commit -m "fix(replay): add MAX_TOKENS_LIMIT validation to prevent resource exhaustion"

# Commit 3: UI accessibility
git add crates/adapteros-ui/src/components/tabs.rs \
        crates/adapteros-ui/src/components/toggle.rs \
        crates/adapteros-ui/src/components/layout/page_scaffold.rs \
        crates/adapteros-ui/src/pages/adapters.rs
git commit -m "fix(ui): improve ARIA accessibility for tabs, select, and adapters page"

# Commit 4: CSS glass variables
git add crates/adapteros-ui/dist/components.css
git commit -m "refactor(ui): standardize glass blur using CSS variables"
```

## Files Overview (77 Modified + 2 Untracked)

### Modified by Scope

**Core/Determinism:**
- `adapteros-lora-router/src/router.rs`
- `adapteros-lora-router/src/types.rs`
- `adapteros-lora-worker/src/training/*`
- `adapteros-lora-mlx-ffi/src/*`

**Database:**
- `adapteros-db/src/*` (5 files)

**Config/Crypto:**
- `adapteros-config/src/*` (3 files)
- `adapteros-crypto/src/*` (2 files)

**API:**
- `adapteros-server-api/src/handlers/*` (10 files)
- `adapteros-server-api/src/*` (5 other files)

**UI:**
- `adapteros-ui/src/components/*` (14 files)
- `adapteros-ui/src/pages/*` (3 files)
- `adapteros-ui/dist/components.css`

### Untracked (Need Addition)
- `crates/adapteros-ui/src/components/layout/page_scaffold.rs`
- `docs/plans/2026-02-05-chat-queue-ux-design.md`

## Conclusion

The working tree contains significant uncommitted work. **Most audit issues appear to already be addressed.** Agents should:

1. Verify fixes are present before making changes
2. Avoid duplicating or overwriting existing work
3. Coordinate any additional changes with the chat-queue-ux work in progress
4. Follow the suggested commit order to maintain logical groupings
