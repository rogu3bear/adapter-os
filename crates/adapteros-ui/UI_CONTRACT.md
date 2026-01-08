# AdapterOS UI Contract

This document codifies the UI system design standards established during the UI stabilization work (PRs 1-6).

## 1. Browser-Light Architecture

The UI follows a strict browser-light / server-heavy boundary:

**UI (Browser) Responsibility:**
- Rendering and layout
- User input handling
- API calls and SSE streaming
- Client-side filtering/pagination for display

**Server Responsibility (never in UI):**
- Cryptographic operations
- Policy evaluation
- Receipt generation
- Determinism enforcement
- All business logic

## 2. Layout Primitives

### SplitPanel

Standard component for list/detail layouts. Use for any page with selectable items showing details.

```rust
<SplitPanel
    has_selection=has_selection
    on_close=Callback::new(move |_| on_close_detail())
    back_label="Back to Items"
    list_panel=move || view! { /* list content */ }
    detail_panel=move || view! { /* detail content */ }
/>
```

**Behavior:**
- Desktop (≥768px): Side-by-side split layout
- Mobile (<768px): Stacked with back button navigation

**Used by:** Training, Models, Policies

## 3. Scale Primitives

### When to use each pattern:

| Pattern | Use When | Example |
|---------|----------|---------|
| **Direct render** | <50 items, static | Status cards |
| **Client-side cap** | 50-200 items, rarely exceeds | Audit logs (`PAGE_SIZE = 25`) |
| **VirtualList** | >200 items possible | Large worker lists |
| **Server pagination** | Unbounded, API supports it | Search results |

### VirtualList Usage

```rust
<VirtualList
    items=items_signal
    item_height=48
    render_item=move |item| view! { <ItemRow item=item/> }
/>
```

### CappedList Usage (simple progressive disclosure)

Shows initial batch with "Show more" button:
```rust
let visible_count = RwSignal::new(PAGE_SIZE);
// ... render items.take(visible_count.get())
// ... "Show more" button increments visible_count
```

## 4. Error / Empty / Loading Primitives

### ErrorDisplay

Standard error component with optional retry:

```rust
// With retry
<ErrorDisplay
    error=e
    on_retry=Callback::new(move |_| refetch())
/>

// Without retry (read-only views)
<ErrorDisplay error=e/>
```

**Rule:** Always provide `on_retry` when a refetch function is available.

### Loading State

Use `Spinner` centered in container:
```rust
<div class="flex items-center justify-center py-12">
    <Spinner/>
</div>
```

### Empty State

Use muted text centered in card:
```rust
<div class="text-center py-8 text-muted-foreground">
    "No items found"
</div>
```

## 5. Performance Rules

### Avoid repeated `.get()` in render loops

```rust
// BAD: .get() called per iteration
{items.get().iter().map(|item| {
    let data = some_signal.get(); // Called N times!
    view! { ... }
})}

// GOOD: Extract once before loop
{move || {
    let data = some_signal.get();
    items.get().iter().map(|item| {
        view! { ... }
    }).collect::<Vec<_>>()
}}
```

### Use Memo for derived chart data

```rust
// BAD: Signal::derive recomputes on every access
let chart_data = Signal::derive(move || expensive_computation());

// GOOD: Memo caches until dependencies change
let chart_data = Memo::new(move |_| expensive_computation());
```

### Pre-compute expensive operations

For charts and visualizations, compute paths/scales once:
```rust
#[derive(Clone, PartialEq)]
struct ChartData {
    path: String,
    x_scale: LinearScale,
    y_scale: LinearScale,
}

let chart_data = Memo::new(move |_| {
    let data = raw_data.get();
    ChartData {
        path: compute_path(&data),
        x_scale: compute_x_scale(&data),
        y_scale: compute_y_scale(&data),
    }
});
```

## 6. Responsive Breakpoints

Standard Tailwind breakpoints used consistently:

| Prefix | Min Width | Usage |
|--------|-----------|-------|
| `sm:` | 640px | 2-column grids |
| `md:` | 768px | SplitPanel switch, 2-4 col grids |
| `lg:` | 1024px | 4+ column grids |

Grid pattern for metric cards:
```rust
<div class="grid gap-4 sm:grid-cols-2 md:grid-cols-2 lg:grid-cols-4">
```

---

*Last updated: PR6 (UI System Design Closeout)*
