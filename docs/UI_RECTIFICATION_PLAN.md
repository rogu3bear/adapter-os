# UI Rectification for Demo Readiness (Expanded Plan)

**Goal:** Make the UI demo-ready—less busy, clearer grouping, Guided Flow as the obvious spine. Tasteful arrangement; not a full rebuild.

---

## 1. CPO Considerations (Segmentation Rationale)

**Primary persona for demo:** Runtime operator or evaluator who needs to see the full value loop in under 5 minutes. Not an ML engineer or compliance auditor.

**What we're promising vs. delivering:**
- **Promise:** "Calm home," "create, chat, replay, verify, promote with confidence"
- **Gap:** Nav and pages expose 40+ routes and 120+ operations. The Guided Flow is buried.
- **CPO call:** Primary profile = demo persona. Full profile = power users. Segment by default; let users opt into complexity via Settings > UI Profile.

**Segmentation table:**

| Profile | Target | Nav groups | Demo-ready? |
|---------|--------|------------|-------------|
| Primary | Operators, evaluators, demos | 6–7 groups, Guided Flow complete | Yes |
| Full | ML engineers, admins | 8 groups + secondary collapsed | No—intentionally dense |

**CPO acceptance:** After rectification, a new user on Primary profile can complete the Guided Flow (Dashboard → Teach → Chat → Restore Points → Promote) without seeing Admin, Agents, Files, Audit, Diff, or Routing in the main nav.

---

## 2. Existing Patterns to Follow

Reference: [.claude/commands/production-ux-rectification.md](.claude/commands/production-ux-rectification.md)

- **SplitPanel:** `use_split_panel_selection_state` + `SplitPanel` (Workers, Models, Adapters)
- **Confirmation dialogs:** `ConfirmationDialog` with `Normal`, `Warning`, `Destructive`
- **Error reporting:** `report_error_with_toast()` for user-initiated failures
- **Success feedback:** `notifications.success()` or `notifications.success_with_action()`
- **Loading:** `SkeletonTable`, `SkeletonCard`—never bare `Spinner` for full page
- **Empty states:** `ListEmptyCard` with CTA
- **Navigation:** `use_navigate()`—never `window.location.set_href()`

---

## 3. Current State (Engineer Reference)

- **40+ routes**, 8 workflow groups (Full), 6 groups (Primary)
- **Primary** omits Runs and Update Center—Guided Flow incomplete
- **PageScaffold** ([page_scaffold.rs:79-98](crates/adapteros-ui/src/components/layout/page_scaffold.rs)): single `PageScaffoldActions` slot, no hierarchy
- **Sidebar** ([sidebar.rs:140](crates/adapteros-ui/src/components/layout/sidebar.rs)): `group_open` always starts `true`; does not use `NavGroup.collapsed_by_default`
- **StartMenu** ([start_menu.rs:40](crates/adapteros-ui/src/components/layout/start_menu.rs)): uses `collapsed_by_default` via `build_start_menu_modules`

---

## 4. Phase 1: Align Navigation with Guided Flow

### Problem

Primary profile lacks Runs and Update Center. Guided Flow is the demo story; nav does not surface it.

### Implementation

**File:** [nav_registry.rs](crates/adapteros-ui/src/components/layout/nav_registry.rs)

**1.1 Restructure Primary profile (lines 396–507)**

Current order: Infer → Data → Train → Deploy (Models) → Adapters → Observe (Workers, System).

New order to match Guided Flow:

1. **Train** (Alt+1) — Adapter Training
2. **Infer** (Alt+2) — Prompt Studio
3. **Verify** (Alt+3) — Restore Points (new group, route `/runs`)
4. **Promote** (Alt+4) — Update Center (new group, route `/update-center`)
5. **Deploy** (Alt+5) — Models + Adapters
6. **Observe** (Alt+6) — Workers + System

**Concrete changes:**
- Add `NavItem::new("runs", "Restore Points", "/runs").with_keywords(&["replay", "traces", "provenance", "receipts"])`
- Add `NavItem::new("update_center", "Update Center", "/update-center").with_keywords(&["promote", "production", "draft", "reviewed"])`
- Create new group "Verify" with Runs only; create "Promote" with Update Center only
- Reorder `NAV_GROUPS_PRIMARY` array; reassign `alt_shortcut` 1–6 to match flow

**1.2 Collapse secondary groups in Full profile**

In `NAV_GROUPS_FULL`:
- Govern (id `govern`): set `collapsed_by_default: true`
- Org (id `org`): set `collapsed_by_default: true`

**1.3 Wire Sidebar to collapsed_by_default**

**File:** [sidebar.rs](crates/adapteros-ui/src/components/layout/sidebar.rs)

In `SidebarGroup` (line 140), change:
```rust
let (group_open, set_group_open) = signal(true);
```
to:
```rust
let (group_open, set_group_open) = signal(!group.collapsed_by_default);
```

**1.4 Default profile**

**File:** [ui_profile.rs](crates/adapteros-ui/src/signals/ui_profile.rs)

Already defaults to `Primary` when runtime config absent (line 70). No change unless we add explicit "demo mode" in settings.

### Acceptance Criteria

- [ ] Primary profile shows Restore Points and Update Center in nav
- [ ] Alt+1..6 map to Train, Infer, Verify, Promote, Deploy, Observe
- [ ] Full profile: Govern and Org groups start collapsed in StartMenu
- [ ] Sidebar multi-item groups respect `collapsed_by_default` on first render

### Risk

- Alt shortcut reassignment may conflict with muscle memory. Document in release notes.

---

## 5. Phase 2: Action Hierarchy on Key Pages

### Problem

Pages show 4–6 actions with equal weight. No primary CTA.

### Implementation

**2.1 PageScaffold API change**

**File:** [page_scaffold.rs](crates/adapteros-ui/src/components/layout/page_scaffold.rs)

Add optional `PageScaffoldPrimaryAction` slot:
```rust
#[slot]
pub struct PageScaffoldPrimaryAction {
    children: Children,
}
```

In `PageScaffold` component (line 79), add prop:
```rust
#[prop(optional)]
page_scaffold_primary_action: Option<PageScaffoldPrimaryAction>,
```

Render order in header (line 166): primary action first (if present), then secondary actions. Primary gets `ButtonVariant::Primary`; caller supplies the button. Secondary actions render in existing `PageScaffoldActions` div.

**2.2 ActionsOverflow component**

**New file:** `crates/adapteros-ui/src/components/actions_overflow.rs`

```rust
#[component]
pub fn ActionsOverflow(
    label: &'static str,  // e.g. "More"
    items: Vec<(String, String)>,  // (label, href)
) -> impl IntoView
```

Pattern: Reuse dropdown from [topbar.rs](crates/adapteros-ui/src/components/layout/topbar.rs) user menu (lines 107–145): `(open, set_open)` signal, `NodeRef` for outside-click, `role="menu"`, `aria-expanded`. Items as `<a href>`. Use `Button` with `ButtonVariant::Ghost` and chevron icon.

**2.3 Apply to pages**

| Page | Primary | Secondary |
|------|---------|-----------|
| Dashboard | Teach New Skill (ButtonLink) | Refresh, View infrastructure |
| Adapters | Teach New Skill | Refresh |
| Training | Create Job (opens wizard) | Refresh |
| Runs | — | Refresh |
| Update Center | Teach New Skill | Refresh |

**Dashboard** ([dashboard.rs:50-73](crates/adapteros-ui/src/pages/dashboard.rs)): Wrap "Teach New Skill" in `PageScaffoldPrimaryAction`; keep Refresh and View infrastructure in `PageScaffoldActions`.

**Adapters** ([adapters.rs:136-155](crates/adapteros-ui/src/pages/adapters.rs)): Same pattern.

**Training** ([training/mod.rs](crates/adapteros-ui/src/pages/training/mod.rs)): Primary = Create Job; filters remain in `TrainingJobList` toolbar.

### Acceptance Criteria

- [ ] PageScaffold accepts optional primary action slot
- [ ] Dashboard, Adapters, Training, Runs, Update Center have exactly one primary CTA where applicable
- [ ] ActionsOverflow renders and closes on outside click / Escape

### Risk

- Slot API change may require updates to all PageScaffold usages. Audit with `rg PageScaffold` before editing.

---

## 6. Phase 3: Dashboard as Hero

### Problem

Guided Flow and status cards compete. Dashboard should lead with the flow.

### Implementation

**File:** [dashboard.rs](crates/adapteros-ui/src/pages/dashboard.rs)

**3.1 Reorder content in `DashboardContent` (lines 129–227)**

Current: Status cards (3-col grid) → `JourneyFlowSection`.

New: `JourneyFlowSection` first, then status cards.

**3.2 Simplify status cards**

Current: 3 cards (Kernel, Prompt Studio, System Services).

New: 2 cards above the fold—Kernel Status, Prompt Studio. System Services: replace with a compact row or inline link "View infrastructure →" that navigates to `/system`.

**3.3 Journey step affordance**

In `JourneyStep` (line 281), add optional prop `start_here: bool`. When true, render a small badge "Start here" on Step 1. Pass `start_here=true` only for the first step.

**3.4 Visual hierarchy**

Guided Flow card: ensure it uses `Card` with appropriate class. Status cards: add lighter border or opacity to make them visually secondary.

### Acceptance Criteria

- [ ] Guided Flow appears before status cards
- [ ] At most 2 status cards above the fold; System Services is compact or linked
- [ ] Step 1 has "Start here" or equivalent affordance

---

## 7. Phase 4: Group Secondary Surfaces

### Problem

Admin, Agents, Files, Audit, Diff, Routing—all visible in Full profile. Overwhelming.

### Implementation

**Recommended (simpler):** Set `collapsed_by_default: true` for Govern and Org in Full profile. No new "More" group. Achieves "collapsed by default" without restructuring.

**Alternative:** Create single "More" group containing Agents, Files, Admin, Audit, Diff, Routing, Policies, Reviews, Settings, System. Collapsed by default. More invasive; do only if CPO insists.

**Taskbar:** No change if we use the simpler approach. `build_taskbar_modules` already produces one module per group.

**Sidebar:** With `collapsed_by_default: true`, Govern and Org start collapsed. No structural change.

### Acceptance Criteria

- [ ] Full profile: Govern and Org groups start collapsed in sidebar
- [ ] All routes remain reachable via nav or Command Palette

---

## 8. Phase 5: Reduce Noise on Key Flows

### Implementation

**5.1 Training page** ([training/mod.rs](crates/adapteros-ui/src/pages/training/mod.rs))

- **Filters:** Wrap `TrainingStatusFilter` and `CoremlFilters` in a collapsible section. "Filters" button toggles visibility. Default: collapsed. Use `RwSignal<bool>` for filters visible state.
- **Backend Readiness Panel:** Wrap in collapsible; show as one-line "Backend status" with expand affordance. Default: collapsed.

**5.2 Chat page** ([chat.rs](crates/adapteros-ui/src/pages/chat.rs))

- **Adapter selection:** Default to showing suggested adapters only. "Choose adapter" or "All adapters" expands to full selection. Add `RwSignal<bool>` for "expanded adapter picker" with default false.
- **Scope:** Minimal change—add "Show all adapters" toggle rather than restructuring.

**5.3 Runs page** ([flight_recorder.rs](crates/adapteros-ui/src/pages/flight_recorder.rs))

- Keep list + detail. Ensure row click or "View" is the primary action.
- Filters: if any, move to compact toolbar.

### Acceptance Criteria

- [ ] Training: Filters collapsible, default collapsed
- [ ] Training: Backend Readiness collapsible or compact
- [ ] Chat: Adapter picker simplified by default; expand on demand
- [ ] Runs: No regression; primary action clear

---

## 9. Phase 6: Documentation and Rules

**9.1 Layout decision matrix** — New file `docs/UI_LAYOUT_MATRIX.md`

| Layout | When to use | Examples |
|--------|-------------|----------|
| SplitPanel | List + detail drill-down | Adapters, Training, Workers, Models |
| TabNav | Multiple views of same entity | Settings, Admin, Audit, Routing |
| DataTable only | List with row click → detail page | Documents, Datasets, Collections, Runs |

**9.2 Action hierarchy rule** — New file `docs/UI_ACTION_RULES.md`

- 1 primary CTA per page (or none for read-heavy pages)
- 2–3 secondary actions visible
- Rest in overflow or context menus

**9.3 Wizard vs dialog rule**

- Wizard: 3+ steps, branching, multi-source data (e.g. CreateJobWizard)
- Dialog: 1–2 steps, simple form (e.g. CreateStackDialog)

---

## 10. Execution Order

| Phase | Scope | Dependencies |
|-------|-------|--------------|
| 1 | nav_registry, Primary profile, collapse Govern/Org, sidebar collapsed_by_default | None |
| 2 | PageScaffold primary slot, ActionsOverflow, 5 pages | None |
| 3 | Dashboard layout | None |
| 4 | Full profile collapse (Phase 1 covers this) | Phase 1 |
| 5 | Training, Chat, Runs | Phase 2 |
| 6 | Docs | After 1–5 |

---

## 11. Out of Scope

- No full redesign
- No route removal
- No backend changes
- No new wizard/dialog patterns
- CSS fixes per CSS_STRATEGY.md only if blocking demo

---

## 12. Verification

- `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
- `cargo test -p adapteros-ui --lib`
- Manual: Dashboard → Teach New Skill → Chat → Restore Points → Update Center in < 2 min
- Manual: Primary profile shows full Guided Flow in nav; Full profile has Govern/Org collapsed
