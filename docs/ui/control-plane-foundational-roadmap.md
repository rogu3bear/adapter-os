## Control Plane Foundational Roadmap

### 1. Provider & Context Inventory

- **CoreProviders** wraps `ThemeProvider`, `AuthProvider`, and `ResizeProvider`, giving us theme persistence, cookie-backed session verification, and resizable split storage in a single shell.[source: ui/src/providers/CoreProviders.tsx L1-L207][source: ui/src/providers/AppProviders.tsx L14-L27]
- **FeatureProviders** layers in tenant selection on top of auth, persisting the chosen tenant and issuing toast feedback for context switches.[source: ui/src/providers/FeatureProviders.tsx L1-L73]
- **BookmarkProvider** adds client-side persistence with cross-tab sync, while **UndoRedoProvider** centralizes reversible operations and keyboard shortcuts.[source: ui/src/contexts/BookmarkContext.tsx L1-L136][source: ui/src/contexts/UndoRedoContext.tsx L1-L90]
- **CommandPaletteProvider** is mounted inside `RootLayout`, ingesting route metadata plus live entity fetches to power global search and navigation.[source: ui/src/layout/RootLayout.tsx L118-L305][source: ui/src/contexts/CommandPaletteContext.tsx L55-L315]
- **Legacy LayoutProvider** re-exports the new providers for backward compatibility, highlighting remaining technical debt.[source: ui/src/layout/LayoutProvider.tsx L1-L41]

> **Opportunity:** Shift from a nested-provider tree to an explicit module map (e.g., `@/state/core`, `@/state/feature`, `@/state/ux`) so each concern exposes a deterministic store plus hooks, while `AppProviders` only wires them together.

### 2. Deterministic State Management Plan

1. **Create Store Modules:** For each concern (theme, auth, tenant, bookmarks, undo/redo, navigation) expose a composable store with a pure-action reducer or signal-based state. Leverage React’s `useSyncExternalStore` to keep subscriptions deterministic and traceable.[source: ui/src/providers/CoreProviders.tsx L1-L207][source: ui/src/contexts/UndoRedoContext.tsx L1-L90]
2. **Canonical Data Flow:** Gate all server interactions behind `apiClient`, and route mutations through domain stores so undo/redo can snapshot deltas rather than component state.[source: ui/src/api/client.ts L16-L399][source: ui/src/components/Adapters.tsx L174-L397]
3. **Snapshot Logging:** Extend `UndoRedoProvider` to emit structured telemetry (action id, domain, payload hash) before and after execution, satisfying deterministic audit trails.[source: ui/src/contexts/UndoRedoContext.tsx L1-L90]
4. **Tenant-Aware Hydration:** Move tenant selection plus entity-prefetch into a `tenantStore` that reacts to auth changes, guaranteeing consistent scoping for palette/entity queries.[source: ui/src/providers/FeatureProviders.tsx L1-L73][source: ui/src/contexts/CommandPaletteContext.tsx L97-L315]
5. **Parallel Provider Cutover:** Introduce stores alongside existing contexts, then migrate providers to become thin adapters until all consumers use the new hooks. This allows incremental adoption without breaking compatibility.

### 3. Progressive Disclosure Guidelines

- **Default Collapsed Groups:** Persist nav group collapse state per role, defaulting to collapsed for rarely used clusters (e.g., Administration) while keeping Home visible. Filter groups via `shouldShowNavGroup` so capability gating stays centralized.[source: ui/src/layout/RootLayout.tsx L82-L228][source: ui/src/utils/navigation.ts L21-L90]
- **Contextual Surfacing:** Couple Feature Layout panels with role-aware affordances (e.g., show bulk adapters panel after the user requests it) by storing disclosure state in the new `uxStore` keyed by route.[source: ui/src/layout/FeatureLayout.tsx L18-L131]
- **Help & Search Triggers:** Keep `/` and `?` shortcuts universal, but gate advanced overlays (command palette power tools, help center) until prerequisite telemetry confirms onboarding completion.[source: ui/src/utils/accessibility.ts L81-L109][source: ui/src/layout/RootLayout.tsx L60-L144]
- **Audit by Capability:** Log every disclosure change with role, route, and component id to align with policy packs that mandate deterministic evidence trails.[source: ui/src/components/CommandPalette.tsx L95-L312]

### 4. Navigation Redesign Proposal

**Desktop Shell**
- Convert the sidebar into a two-tier structure: primary groups (Home, ML Pipeline, Monitoring, Operations, Compliance, Administration, Communication) with collapsible submenus, respecting role filters from the route config.[source: ui/src/layout/RootLayout.tsx L168-L228][source: ui/src/config/routes.ts L85-L395]
- Surface unread indicators (notifications, pending approvals) inline with nav items, hydrated from the planned notification store.
- Introduce a secondary utility rail for tenant picker, command palette toggle, and help center access to reduce header congestion.[source: ui/src/layout/RootLayout.tsx L118-L252]

**Mobile Shell**
- Replace the current slide-over with a tabbed drawer: primary nav, search, and notifications, each honoring the same role-based filters.[source: ui/src/layout/RootLayout.tsx L168-L228]
- Cache disclosure state in session storage to avoid re-expanding all groups on every route change.

**Governance Alignment**
- Document the redesigned IA in the control-plane architecture index and update diagrams to reflect the new shell, keeping the docs in sync with the AdapterOS master plan.[source: docs/ARCHITECTURE_INDEX.md L1-L112]
- Ensure every new nav element references policy ownership (e.g., Administration → ITAR enforcement) so compliance reviews trace back to the canonical documentation set.

### 5. Next Steps

1. Stand up `@/state` with auth, tenant, and navigation stores alongside adapter contexts.
2. Instrument undo/redo telemetry and disclosure logging.
3. Prototype the redesigned sidebar/drawer in storybook (or a sandbox route) before cutting over.
4. Update architecture docs and onboarding guides once the new shell ships.

