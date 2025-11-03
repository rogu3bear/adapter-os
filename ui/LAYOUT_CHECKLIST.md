# Feature Layout Checklist

Use this checklist when shipping or reviewing feature pages so that layout basics stay consistent across desktop and mobile.

- **Spacing tokens**  
  - Outer padding: `p-[var(--space-6)]` on the layout container.  
  - Section gap: `gap-[var(--space-4)]` between major blocks.  
  - Header spacing: `mb-[var(--section-gap)]` below the page header.  
  - Split-panels: wrap each pane contents with `p-[var(--space-4)]` if the panel needs its own padding.

- **Breadcrumbs**  
  - Auto-generated from the route path by default; override via `breadcrumbs` prop when copy needs to diverge.  
  - Hide breadcrumbs on narrow screens (`sm` and down) to keep the header compact.  
  - Always begin with the “Home” crumb for top-level navigation.

- **Header actions**  
  - Pass buttons, selectors, or status badges through the `headerActions` prop so they align to the right edge.  
  - Keep control clusters inside a `div` with `flex`, `items-center`, and `gap-2`.

- **Split-panels**  
  - Provide `left`, `children`, and/or `right` props when a page needs more than one column.  
  - Set `resizable` to `true` for adjustable columns and provide a unique `storageKey` when layout persistence is desired.  
  - Default sizes come from `defaultLayout`; if omitted, panels split evenly.  
  - Each panel should manage its own scroll boundaries to avoid double scroll bars.

- **Consistency guardrails**  
  - Keep primary content wrapped in `<main>` with `max-w-[1440px] mx-auto` to match the global shell.  
  - Use shared UI primitives from `@/components/ui` for skeletons, empty states, and error UI so spacing stays aligned with the layout.
