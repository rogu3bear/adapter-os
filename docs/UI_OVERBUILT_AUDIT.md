# UI Overbuilt Areas — Audit & Anchors

**Purpose:** Verify and anchor analysis of overbuilt UI areas. Use this doc to prioritize simplification work.

**Scope:** `crates/adapteros-ui` (Leptos 0.7 CSR)

**Audit date:** 2026-02-18  
**Rectification date:** 2026-02-19

---

## 1. NavScope enum

**Claim:** Dead abstraction; `allows()` never called; `scope` field redundant.

**Verified:** **N/A** — `NavScope` and `scope` do not exist in current `nav_registry.rs`. `NavGroup` has: `id`, `label`, `icon`, `alt_shortcut`, `items`, `collapsed_by_default`, `show_in_taskbar`, `show_in_mobile`. No scope field. Analysis may be from an older version.

**Canonical:** `nav_registry.rs:14–32` (NavGroup struct)

---

## 2. Duplicate nav group definitions

**Claim:** Two full structures (NAV_GROUPS_FULL, NAV_GROUPS_PRIMARY) with overlapping layout/icons; changes require edits in both places.

**Verified:** **Yes.** `nav_registry.rs:123–356` (NAV_GROUPS_FULL, 8 groups), `359–479` (NAV_GROUPS_PRIMARY, 6 groups). `nav_groups(profile)` switches between them. Primary reorders workflow (Train first) and uses different labels (e.g. "Restore Points" vs "Flight Recorder").

**Recommendation:** Single array with `profiles: &[UiProfile]` per group, or accept duplication and document as dual source of truth.

---

## 3. LogicalControlRail (~600 lines)

**Claim:** Heavy abstraction for a status bar; most users care about "ready or not" and maybe one blocker.

**Verified:** **Yes.** `logical_rail.rs` ~594 lines. `LogicSnapshot`, `LogicTransition`, `ContractState`, fingerprint, base model, stack version, blockers. Rendered in `shell.rs:265` under topbar on every page.

**Canonical:** `components/layout/logical_rail.rs`, `shell.rs:265`

**Recommendation:** Slim status bar (inference ready + primary blocker). Move detailed logic to System page or Status Center.

---

## 4. Search infrastructure (7 modules)

**Claim:** 6 modules; contextual actions per route add a lot of code.

**Verified:** **Yes.** `search/`: `mod`, `contextual`, `fuzzy`, `index`, `providers`, `recent`, `types`. `generate_contextual_actions` in `contextual.rs` (~880 lines) — route-specific actions for documents, adapters, training, models, workers, etc. Used by `signals/search.rs`.

**Canonical:** `search/contextual.rs`, `search/providers.rs`, `signals/search.rs`

**Recommendation:** Keep fuzzy + command palette. Trim or defer route-specific contextual actions until usage justifies them.

---

## 5. Overlapping status surfaces

**Claim:** LogicalControlRail, Status Center, InferenceBanner, System page, SystemTray — same info, multiple UIs.

**Verified:** **Yes.** All five exist and show system/inference state:
- `LogicalControlRail` — shell (every page)
- `StatusCenterProvider` — Ctrl+Shift+S panel
- `InferenceBanner` — banner when inference not ready
- `pages/system/` — full diagnostics
- `system_tray.rs` — compact status

**Recommendation:** Pick 1–2 primary surfaces (e.g. InferenceBanner + System page). Fold LogicalRail into one or remove.

---

## 6. DetailPageShell (low usage)

**Claim:** Only used by Flight Recorder and Repositories; most detail pages use PageScaffold directly.

**Verified:** **Yes.** Used in `flight_recorder.rs:171`, `repositories/mod.rs:185`. Other detail pages (adapters/:id, models/:id, workers/:id, etc.) use PageScaffold directly.

**Canonical:** `components/layout/detail_page_shell.rs`

**Recommendation:** Migrate more detail pages to it (justify abstraction) or inline into those two and remove.

---

## 7. PageScaffold slots

**Claim:** Slots exist but aren't used consistently; no clear primary CTA.

**Verified:** **Partial.** `PageScaffoldPrimaryAction` **is used** on: adapters, training, dashboard, update_center (4 pages). `PageScaffoldActions` used on 20+ pages. `PageScaffoldInspector` — not found in grep (may not exist or named differently). The Material audit's "4–6 actions with equal weight" refers to visual hierarchy, not slot existence.

**Canonical:** `components/layout/page_scaffold.rs:68–98`

**Recommendation:** Use `PageScaffoldPrimaryAction` on more key pages (models, workers, documents) for clearer CTA hierarchy.

---

## 8. CSS duplication and unused tokens

**Claim:** ~9k lines; `--space-*` unused; two status systems; repeated `@supports`; responsive duplication.

**Verified:** **Yes.** See `docs/CSS_STRATEGY.md`. Contract violations (border-destructive/40, etc.) were rectified 2026-02-18; semantic tokens used. `--space-*` still unused in utilities. Two status systems (text-warning vs text-status-warning) coexist.

**Recommendation:** Wire `--space-*` to utilities; deprecate hardcoded status colors; centralize glass `@supports`.

---

## 9. Training module structure

**Claim:** Many entry points and state modules for a single workflow.

**Verified:** **Yes.** 17 files: `dataset_wizard`, `generate_wizard`, `wizard`, `readiness`, `config_presets`, `data/` (state, upload, source_nav, data_list, detail_panel), `detail/` (components, mod), `state`, `components`, `utils`.

**Canonical:** `pages/training/`

**Recommendation:** Consolidate wizards or share more state; reduce nesting.

---

## 10. UiProfile segmentation

**Claim:** Primary vs Full adds branching; value depends on usage.

**Verified:** **Yes.** `ui_profile.rs`, `nav_registry.rs` (nav_groups), `sidebar.rs`, `search/index.rs`, preferences. Toggle exists; both profiles used.

**Recommendation:** Validate usage (analytics, user feedback). If low, consider single nav with optional "compact" mode.

---

## 11. Shell chrome layers

**Claim:** Many persistent layers: TopBar, LogicalControlRail, Sidebar, Taskbar, StartMenu, SystemTray, ChatDock, InferenceBanner, OfflineBanner, TelemetryOverlay, Workspace.

**Verified:** **Yes.** All present in `shell.rs`. Each adds layout, keyboard, state.

**Recommendation:** Merge or remove (e.g. LogicalRail into TopBar or Status Center; TelemetryOverlay only when needed).

---

## Summary: Verified state

| Area | Claim accurate? | Recommendation |
|------|-----------------|----------------|
| NavScope | N/A (not present) | — |
| Duplicate nav | Yes | Single source or accept duplication |
| LogicalControlRail | Yes | Simplify or fold |
| Search contextual | Yes | Trim or defer |
| Status surfaces | Yes | Consolidate to 1–2 |
| DetailPageShell | Yes | Use more or remove |
| PageScaffold slots | Partial (PrimaryAction used on 4 pages) | Extend PrimaryAction usage |
| CSS | Yes | Per CSS_STRATEGY |
| Training module | Yes | Consolidate |
| UiProfile | Yes | Validate usage |
| Shell layers | Yes | Merge/remove |

---

## Prioritized simplification (if proceeding)

| P | Action | Effort | Risk | Status |
|---|--------|--------|------|--------|
| P0 | Slim LogicalControlRail to inference-ready + primary blocker | 2–4h | Medium | **Done** 2026-02-19 |
| P1 | Trim contextual actions (keep core routes only) | 2h | Low | **Done** 2026-02-19 |
| P2 | Fold LogicalRail into Status Center or TopBar | 4h | Medium | Deferred |
| P3 | Single nav array with profile filter | 3h | Low | Cancelled (different group structures) |
| P4 | Inline DetailPageShell into 2 pages and remove | 2h | Low | **Done** 2026-02-19 |
| P5 | Add PageScaffoldPrimaryAction to models, workers, documents | 1h | Low | **Done** 2026-02-19 |

---

## References

- `docs/CSS_STRATEGY.md` — CSS gaps and action plan
- `docs/UI_BEST_PRACTICES_CONTRAST.md` — Best-practice alignment
- `docs/audits/MATERIAL_UI_AUDIT.md` — Component audit
