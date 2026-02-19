# AdapterOS UX & Architecture Execution Plan

> **Snapshot** — Epic status tracking. PRs may have merged; check repo for current state. See [DOCS_AUDIT_2026-02-18.md](../DOCS_AUDIT_2026-02-18.md).

Generated: 2026-02-06 | Updated: 2026-02-07 | Status: **In Progress — Phase B**

## North Star

**Primary goal: Time-to-first-chat < 10 minutes on a fresh install.**

Everything below serves this goal. Anything that does not is deferred.

## Epic Status

| Epic | Name | Status | PRs Merged | Target |
|------|------|--------|------------|--------|
| A | Stop Bleeding | **Complete** | 5/5 | Week 1 |
| B | First-Run Onboarding | Not Started | 0/4 | Weeks 2-3 |
| C | Quick Start Flow | Not Started | 0/4 | Weeks 3-6 |
| D | Architecture Velocity | Not Started | 0/4 | Weeks 6-10 |
| E | Ops Hardening | Not Started | 0/3 | Weeks 10-12 |

---

## Epic A: Stop Bleeding (Week 1)

**Objective:** Remove known footguns, fix obvious papercuts, make dev setup reliable.

**Scope:** Error handling consistency, chat UX fixes, build prereqs, test safety.
**Non-scope:** New features, navigation changes, architecture refactors.

### A1: Route raw `.to_string()` errors through unified UI handler

- **Problem:** 18 of 28 error sites bypass `report_error_with_toast()`, showing raw Rust error strings.
- **Scope:** All `error.set(Some(e.to_string()))` in `crates/adapteros-ui/src/pages/`.
- **Non-scope:** Error infrastructure changes; skeleton/loading state improvements.
- **Files:**
  - `pages/login.rs:130`
  - `pages/stacks/dialogs.rs:80,291`
  - `pages/training/dialogs.rs:315,513`
  - `pages/training/detail.rs:829,900`
  - `pages/training/dataset_wizard.rs:788`
  - `pages/training/data/mod.rs:223`
  - `pages/training/wizard.rs:426`
  - `pages/admin/api_keys.rs:65`
  - `pages/repositories/dialogs.rs:78`
  - `pages/collections.rs:93`
  - `pages/routing/rules.rs:196`
  - `pages/routing/decisions.rs:628`
  - `pages/diff.rs:93`
  - `pages/errors.rs:1214`
  - `pages/flight_recorder.rs:1913`
- **Also convert console-only errors:**
  - `pages/adapters.rs:402,425,548`
  - `pages/audit/mod.rs:141`
- **Acceptance:**
  - [x] `grep -rn 'error.set(Some(e.to_string' pages/` returns 0 results
  - [x] `grep -rn 'console::error_1' pages/` returns 0 results (all have user-visible feedback)
  - [x] Login page error is human-readable
  - [x] `cargo test -p adapteros-ui --lib` passes (175/175)
- **Follow-up debt:** 2 sites outside original scope: `documents.rs:772`, `upload_dialog.rs:263`
- **Branch:** `epic-A/A1-error-toast`

### A2: Make suggestion chips clickable in chat

- **Problem:** Chat suggestion chips (`chat.rs:1599-1608`) are `<span>` elements styled as buttons but do nothing.
- **Scope:** Make chips populate input and optionally send.
- **Non-scope:** Adding new suggestion types, changing suggestion text.
- **Files:** `pages/chat.rs:1599-1608`
- **Acceptance:**
  - [x] Clicking a chip populates the chat input
  - [x] Chip has hover/active states
  - [ ] Playwright test covers chip interaction (deferred — requires running browser)
- **Branch:** `epic-A/A2-clickable-chips`

### A3: Fix routing rule condition placeholder

- **Problem:** Placeholder `sentiment == 'negative'` is not valid JSON, but the field validates as JSON.
- **Scope:** Fix placeholder to valid JSON example.
- **Non-scope:** Changing validation logic or adding a DSL.
- **Files:** `pages/routing/rules.rs:210`
- **Acceptance:**
  - [x] Placeholder is valid JSON (`{"sentiment": "negative"}`)
  - [x] Existing tests pass
- **Branch:** `epic-A/A3-routing-placeholder`

### A4: Document sccache prerequisite or add safe fallback

- **Problem:** `.cargo/config.toml` sets `rustc-wrapper = "sccache"`. Build fails silently without it.
- **Scope:** Either add sccache to prereqs in CLAUDE.md, or make config resilient.
- **Non-scope:** Changing the actual build caching strategy.
- **Files:** `CLAUDE.md`, `.cargo/config.toml` (if adding fallback)
- **Acceptance:**
  - [x] A developer without sccache can build (via documented install)
  - [x] Prerequisites section is accurate
- **Branch:** `epic-A/A4-sccache-prereq`

### A5: Replace `env::set_var` in test infrastructure

- **Problem:** `std::env::set_var` is unsafe in multi-threaded contexts (Rust 1.74+). Used in 20+ test sites running under nextest parallelism.
- **Scope:** Replace with per-test config passing in `tests/common/test_harness.rs` and `tests/common/cleanup.rs`.
- **Non-scope:** Refactoring individual test files that set env vars for their own isolated use.
- **Files:**
  - `tests/common/test_harness.rs:36`
  - `tests/common/cleanup.rs:110,180,303,304`
- **Acceptance:**
  - [x] `test_harness.rs` no longer calls `env::set_var`
  - [x] `cleanup.rs` env manipulation uses `unsafe {}` blocks with SAFETY comments
  - [x] Workspace compilation passes (nextest parallel safety verified)
- **Branch:** `epic-A/A5-safe-test-env`

---

## Epic B: First-Run Onboarding (Weeks 2-3)

**Objective:** New users see a guided setup path, not an empty ops dashboard.

**Scope:** First-run detection, setup checklist, nav simplification, terminology.
**Non-scope:** Quick Start flow (Epic C), architecture changes, training flow redesign, core crate surgery.

### B1: First-run detection and /welcome route

- **Problem:** A fresh install lands on Dashboard showing "Not Ready" metrics, "No Workers Registered", and "No recent activity." No guidance on what to do first.
- **Scope:** Add `/welcome` route with first-run detection. Redirect from Dashboard when system has no models AND no adapters.
- **Non-scope:** Quick Start wizard (Epic C), changing auth flow.
- **Implementation:**
  - Create `pages/welcome.rs` with a `Welcome` component
  - Register `/welcome` route in `lib.rs` (inside `ProtectedRoute` + `Shell`)
  - Add `pub mod welcome;` to `pages/mod.rs`
  - In `Dashboard` component, check `system_status()` response: if `inference_blockers` contains `NoModelLoaded` AND `WorkerMissing`, redirect to `/welcome`
  - First-run detection: no models loaded AND no workers registered (both from existing `SystemStatusResponse`)
- **Files:**
  - NEW: `pages/welcome.rs`
  - EDIT: `pages/mod.rs` (add module)
  - EDIT: `lib.rs` (add route)
  - EDIT: `pages/dashboard.rs` (add redirect logic)
- **Acceptance:**
  - [ ] Fresh system (no models, no workers) redirects to `/welcome` after login
  - [ ] System with models+workers goes to Dashboard normally
  - [ ] `/welcome` is accessible directly via URL
  - [ ] WASM compilation passes
- **Branch:** `epic-B/B1-welcome-route`

### B2: Setup checklist with direct actions

- **Problem:** Even with a welcome page, users need to know the exact steps to get to first chat.
- **Scope:** Render a setup checklist on `/welcome` with status indicators and direct action links.
- **Non-scope:** Automating setup steps, backend changes.
- **Implementation:**
  - Checklist items (derived from `SystemStatusResponse` + API calls):
    1. "Models available" — check `kernel.models.total > 0` → action: "Seed Models" link to `/models`
    2. "Worker running" — check `readiness.checks.workers.status == Ready` → action: "Start Worker" link to `/workers`
    3. "First adapter" — check adapter count > 0 → action: "Train Adapter" link to `/training`
    4. "Ready to chat" — check `inference_ready == True` → action: "Open Chat" link to `/chat`
  - Use existing `EmptyState` patterns and inference guidance iconography
  - Each item shows: icon + label + status (pending/done) + action button
  - When all items are done, show "You're ready!" with prominent "Open Chat" CTA
- **Files:**
  - EDIT: `pages/welcome.rs` (add checklist rendering)
  - May need: `api/client.rs` (add `list_adapters_count()` or reuse existing)
- **Acceptance:**
  - [ ] Checklist shows correct status for each item
  - [ ] Action links navigate to correct pages
  - [ ] Completed items show checkmark
  - [ ] All-complete state shows "Open Chat" CTA
  - [ ] WASM compilation passes
- **Branch:** `epic-B/B2-setup-checklist`

### B3: Collapse nav into 5 groups

- **Problem:** 8 nav groups with 22 items overwhelm new users. Navigation surface area is too large.
- **Scope:** Collapse Full profile from 8 groups to 5 groups. Preserve all routes (no pages deleted).
- **Non-scope:** Deleting pages, changing Primary profile, adding new routes.
- **Implementation (nav_registry.rs):**
  - **Chat** (was "Infer"): Chat
  - **Data** (merge "Data" + "Train"): Documents, Collections, Datasets, Training Jobs, Repositories
  - **Adapters** (merge "Deploy" + "Route"): Adapters, Stacks, Models, Routing
  - **Observe** (merge "Observe" + "Govern"): Flight Recorder, Monitoring, Errors, Workers, Policies, Audit, Reviews, Diff
  - **Settings** (was "Org"): Agents, Admin, Settings, System
  - Update `Alt+` shortcuts: Alt+1 through Alt+5
  - Update `build_taskbar_modules()` to show 5 modules
- **Files:**
  - EDIT: `components/layout/nav_registry.rs` (restructure NAV_GROUPS_FULL)
- **Acceptance:**
  - [ ] Taskbar shows 5 module buttons (down from 8)
  - [ ] All 22+ routes still accessible via Start Menu
  - [ ] Alt+1 through Alt+5 work
  - [ ] No broken navigation links
  - [ ] WASM compilation passes
- **Branch:** `epic-B/B3-nav-collapse`

### B4: Terminology pass (labels only)

- **Problem:** Engineer jargon in nav labels: "Infer", "Deploy", "Govern", "Stacks", "Flight Recorder".
- **Scope:** Rename labels in nav_registry.rs only. No page content changes, no route changes.
- **Non-scope:** Page titles, component text, API field names.
- **Implementation:**
  - "Flight Recorder" → "Runs" (already the route)
  - "Stacks" → "Stacks" (keep — no better short term exists)
  - "Govern" → absorbed into "Observe" (B3)
  - "Infer" → "Chat" (B3)
  - "Deploy" → "Adapters" (B3, primary item is adapters)
  - "Org" → "Settings" (B3)
  - Verify all `keywords` arrays include old terms for command palette search
- **Files:**
  - EDIT: `components/layout/nav_registry.rs` (label strings only)
- **Acceptance:**
  - [ ] No jargon group labels in taskbar
  - [ ] Command palette still finds items by old names
  - [ ] WASM compilation passes
- **Branch:** `epic-B/B4-terminology`

**Definition of Done:**
- Empty system state triggers guided experience at `/welcome`
- User sees "what to do first" without reading docs
- Nav groups: Chat, Data, Adapters, Observe, Settings (5 groups)
- Engineer jargon replaced in nav labels

---

## Epic C: Quick Start Flow (Weeks 3-6)

**Objective:** Linear path from zero to chat in 4-6 steps.

**Scope:** New flow page, model picker, delete legacy dialog, progressive disclosure.
**Non-scope:** Backend API changes, new training capabilities.

### C1: One-page upload -> train/select -> chat flow
### C2: Model picker replaces base model free-text
### C3: Delete CreateJobDialog
### C4: Progressive disclosure for all config forms

**Definition of Done:**
- Steps to first chat: 4-6 (down from 12-13)
- Only one training creation path exists
- Base model selection is dropdown/picker

---

## Epic D: Architecture Velocity (Weeks 6-10)

**Objective:** Reduce build blast radius, eliminate code duplication.

**Scope:** Server type extraction, auth dedup, UI type migration, core dep hygiene.
**Non-scope:** New features, database schema changes.

### D1: Wire adapteros-server-api-types into spokes
### D2: Extract resolve_auth() from middleware
### D3: Migrate UI-local types to adapteros-api-types
### D4: Feature-gate heavy deps in adapteros-core

**Definition of Done:**
- AppState change does not recompile all spokes
- Auth middleware < 400 lines (down from ~850)
- UI types.rs reduced by >50%
- `adapteros-core` minimal feature builds without rusqlite/tokenizers

---

## Epic E: Ops Hardening (Weeks 10-12)

**Objective:** Remove security footguns, improve migration safety.

**Scope:** Shell script eval removal, migration rollbacks, dual-write decision.
**Non-scope:** New ops features.

### E1: Remove shell eval injection risks
### E2: Add rollback scripts for top-risk migrations
### E3: Dual-write: activate or delete

**Definition of Done:**
- No `eval` with user-controlled input in scripts
- Top 20 critical migrations have rollback scripts
- Dual-write has a decision doc (keep+activate or remove)
