# Phase 5 Execution: UI Route/Action Source-of-Truth Alignment

## Objective
- Align router, nav registry, and contextual actions with explicit coverage and no duplicate route ownership.

## Deliverable A: Route/Action Coverage
- Contextual action coverage exists for:
`/documents`, `/adapters`, `/training`, `/chat`, `/collections`, `/models`, `/workers`, `/stacks`, `/repositories`, `/datasets`, `/runs`, `/reviews`, `/audit`, `/policies`, `/monitoring`, `/errors`, `/admin`, `/routing`, `/diff`.
- Router routes currently without contextual actions:
`/`, `/settings`, `/system`, `/user`, `/welcome`, `/agents`, `/files`, `/login`, `/safe`, `/style-audit`, `/dashboard`, `/flight-recorder`, `/flight-recorder/:id`.
- Contextual routes without router matches: none found.

## Deliverable B: Duplication + Risk Map
- Route literals are repeated between:
  - `crates/adapteros-ui/src/components/layout/nav_registry.rs` (`NavItem::new(..., "/route")`)
  - `crates/adapteros-ui/src/lib.rs` (`path!("/route")`)
  - `crates/adapteros-ui/src/search/contextual.rs` (`route.starts_with("/route")` and `SearchAction::Navigate("/route...")`)
- Highest duplication concern: `/chat`, `/training`, `/documents`, `/adapters`, `/runs` literals appear in multiple contextual navigation constructions.
- File-risk note: scoped UI files are currently under active modification in the working tree, so implementation must remain smallest-diff and ownership-safe.

## Deliverable C: Smallest-Diff Source-of-Truth Migration
1. Keep `nav_registry` as canonical route metadata source for nav/search surfaces.
2. Keep `search/index.rs` page derivation from `all_nav_items` (already present) and avoid parallel route registries.
3. Add contextual coverage for uncovered high-value routes (`/agents`, `/files`, `/settings`, `/system`) using shared route constants/helpers rather than new literals.
4. Normalize repeated query/path builders for `/chat` and `/training` actions into shared helpers to prevent drift.
5. Preserve router definitions in `lib.rs` as authoritative runtime map and validate parity through `rg` checks.

## Verification Run
- Ran route inventory:
`rg -o 'path!\("([^"]+)"\)' crates/adapteros-ui/src/lib.rs`
- Result: complete route list extracted.

- Ran contextual/nav registry scan:
`rg -n 'route\.starts_with|NavItem::new' crates/adapteros-ui/src/search/contextual.rs crates/adapteros-ui/src/components/layout/nav_registry.rs`
- Result: parity anchors and uncovered routes confirmed.

- Ran compilation gate:
`cargo check -p adapteros-ui`
- Result: passed.

## Phase 5 Completion
- [x] Coverage map delivered.
- [x] Duplication map delivered.
- [x] Source-of-truth migration sequence delivered.
- [x] Verification gate is green.
