## AdapterOS UI Layout Standards

### 1. Shell & Header

- **Use `FeatureLayout` for every routed page** to inherit safe-area padding, typography tokens, and breadcrumb rendering. Configure action controls with `headerActions` so buttons align consistently across desktop and mobile shells.[source: ui/src/layout/FeatureLayout.tsx L91-L233]
- **Keep content widths deliberate** by selecting an explicit `maxWidth` (`md`, `lg`, `xl`, or `full`) and `contentPadding` profile when mounting the layout. The defaults (`xl`, `default`) match the control plane dashboard grid.[source: ui/src/layout/FeatureLayout.tsx L55-L105]
- **Breadcrumbs** render automatically from the URL; override with the `breadcrumbs` prop only for multi-step wizards or deeply nested flows.[source: ui/src/layout/FeatureLayout.tsx L107-L257]

### 2. Panels & Resizing

- **Progressive disclosure**: place optional inspector panels in the `left`/`right` slots. When `resizable` is true, pair with a stable `storageKey` so panel widths persist per user/device.[source: ui/src/layout/FeatureLayout.tsx L124-L217]
- **Panel padding**: child panels already include `p-[var(--space-4)]` and enforce `min-w-0 / min-h-0`. Avoid adding extra wrappers unless you need domain-specific framing.[source: ui/src/layout/FeatureLayout.tsx L210-L214]

### 3. Surface States

- **Loading**: use `LoadingState` for initial fetches or long-lived background tasks. Pick `size="sm"` for inline cards and `size="md"` for full-surface placeholders.[source: ui/src/components/ui/loading-state.tsx L1-L60]
- **Empty**: render `EmptyState` when API responses succeed but no data matches filters. Provide a primary remediation action when possible (e.g., “Register Adapter”).[source: ui/src/components/ui/empty-state.tsx L1-L37]
- **Errors**: favor `ErrorRecovery` (or one of its templates) so users receive actionable follow-ups and we capture telemetry for incident triage.[source: ui/src/components/ui/error-recovery.tsx L1-L191]

### 4. Keyboard & Accessibility

- Register global shortcuts through `useKeyboardShortcuts`, reserving `/` for search, `?` for help, and `Cmd/Ctrl+Shift+N` for notifications. The hook automatically ignores text inputs and contentEditable regions.[source: ui/src/utils/accessibility.ts L80-L133]
- Ensure custom headers keep a 44 px hit target on touch devices (FeatureLayout + MobileNavigation already enforce this for nav drawers).[source: ui/src/components/MobileNavigation.tsx L27-L69]

### 5. Implementation Checklist

1. Wrap each routed page with `FeatureLayout`, passing `headerActions` for toolbar controls.
2. Choose `contentPadding` and `maxWidth` suitable for the feature density.
3. Use `LoadingState`, `EmptyState`, and `ErrorRecovery` for async/empty/error branches.
4. Supply deterministic `storageKey` values when enabling resizable panels.
5. Register keyboard toggles through `useKeyboardShortcuts` (command palette, help, notifications) and avoid redefining them ad hoc.

