//! Virtual list component for efficient rendering of large lists.
//!
//! Only renders visible items plus a small overscan buffer, significantly
//! reducing DOM nodes for lists with 100+ items.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Configuration for the virtual list.
#[derive(Clone, Copy)]
pub struct VirtualListConfig {
    /// Height of each row in pixels (fixed height for simplicity).
    pub row_height: u32,
    /// Number of items to render above/below the visible area.
    pub overscan: usize,
    /// Total height of the container in pixels (CSS value like "400px" or "50vh").
    pub container_height: &'static str,
}

impl Default for VirtualListConfig {
    fn default() -> Self {
        Self {
            row_height: 48,
            overscan: 5,
            container_height: "400px",
        }
    }
}

/// Result of virtual list calculations.
#[derive(Clone, Debug, PartialEq)]
pub struct VirtualRange {
    /// First item index to render (inclusive).
    pub start: usize,
    /// Last item index to render (exclusive).
    pub end: usize,
    /// Padding at top to maintain scroll position.
    pub padding_top: u32,
    /// Padding at bottom to maintain scroll height.
    pub padding_bottom: u32,
}

impl VirtualRange {
    /// Calculate the visible range given scroll position and container dimensions.
    pub fn calculate(
        scroll_top: u32,
        container_height: u32,
        total_items: usize,
        row_height: u32,
        overscan: usize,
    ) -> Self {
        if total_items == 0 || row_height == 0 {
            return Self {
                start: 0,
                end: 0,
                padding_top: 0,
                padding_bottom: 0,
            };
        }

        // Calculate visible range
        let first_visible = (scroll_top / row_height) as usize;
        let visible_count = (container_height / row_height) as usize + 1;

        // Apply overscan
        let start = first_visible.saturating_sub(overscan);
        let end = (first_visible + visible_count + overscan).min(total_items);

        // Calculate padding to maintain scroll position
        let padding_top = (start as u32) * row_height;
        let padding_bottom = ((total_items - end) as u32) * row_height;

        Self {
            start,
            end,
            padding_top,
            padding_bottom,
        }
    }
}

/// Virtual list component that only renders visible items.
///
/// This component wraps a scrollable container and only renders items
/// that are within the visible viewport plus an overscan buffer.
///
/// # Example
/// ```ignore
/// <VirtualList
///     items=items
///     config=VirtualListConfig { row_height: 48, ..Default::default() }
///     render_item=move |item, index| view! { <div>{item.name}</div> }
/// />
/// ```
#[component]
pub fn VirtualList<T, V, F>(
    /// The items to render.
    #[prop(into)]
    items: Signal<Vec<T>>,
    /// Configuration for the virtual list.
    #[prop(default = VirtualListConfig::default())]
    config: VirtualListConfig,
    /// Render function for each item.
    render_item: F,
    /// Optional CSS class for the container.
    #[prop(optional, into)]
    class: String,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T, usize) -> V + Clone + Send + Sync + 'static,
{
    let scroll_top = RwSignal::new(0u32);

    // Calculate container height in pixels (parse from CSS value)
    let container_height_px = parse_height(config.container_height);

    // Calculate the virtual range based on scroll position
    let virtual_range = Memo::new(move |_| {
        let items_vec = items.try_get().unwrap_or_default();
        let total = items_vec.len();
        VirtualRange::calculate(
            scroll_top.try_get().unwrap_or(0),
            container_height_px,
            total,
            config.row_height,
            config.overscan,
        )
    });

    // Log list size in debug builds
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        let Some(items_vec) = items.try_get() else {
            return;
        };
        if !items_vec.is_empty() {
            web_sys::console::log_1(
                &format!("[list] VirtualList: {} items", items_vec.len()).into(),
            );
        }
    });

    let on_scroll = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(element) = target.dyn_into::<web_sys::HtmlElement>() {
                scroll_top.set(element.scroll_top() as u32);
            }
        }
    };

    view! {
        <div
            class={format!("virtual-list-container overflow-y-auto {}", class)}
            style:height={config.container_height}
            on:scroll=on_scroll
        >
            {move || {
                let range = virtual_range.try_get().unwrap_or(VirtualRange { start: 0, end: 0, padding_top: 0, padding_bottom: 0 });
                let items_vec = items.try_get().unwrap_or_default();
                let render = render_item.clone();

                view! {
                    // Top spacer
                    <div style:height={format!("{}px", range.padding_top)}></div>

                    // Visible items
                    {items_vec
                        .into_iter()
                        .enumerate()
                        .skip(range.start)
                        .take(range.end - range.start)
                        .map(|(idx, item)| {
                            view! {
                                <div
                                    class="virtual-list-item"
                                    style:height={format!("{}px", config.row_height)}
                                >
                                    {render(item, idx)}
                                </div>
                            }
                        })
                        .collect_view()}

                    // Bottom spacer
                    <div style:height={format!("{}px", range.padding_bottom)}></div>
                }
            }}
        </div>
    }
}

/// Virtual table body component for use with existing Table components.
///
/// Renders only visible table rows while maintaining proper table structure.
#[component]
pub fn VirtualTableBody<T, V, F>(
    /// The items to render as rows.
    #[prop(into)]
    items: Signal<Vec<T>>,
    /// Height of each row in pixels.
    #[prop(default = 48)]
    row_height: u32,
    /// Maximum visible rows (determines container height).
    #[prop(default = 10)]
    max_visible_rows: usize,
    /// Overscan count.
    #[prop(default = 3)]
    overscan: usize,
    /// Render function for each row.
    render_row: F,
    /// Debug label for logging.
    #[prop(optional, into)]
    debug_label: String,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T, usize) -> V + Clone + Send + Sync + 'static,
{
    let scroll_top = RwSignal::new(0u32);
    let container_height = row_height * max_visible_rows as u32;

    // Calculate the virtual range
    let virtual_range = Memo::new(move |_| {
        let items_vec = items.try_get().unwrap_or_default();
        VirtualRange::calculate(
            scroll_top.try_get().unwrap_or(0),
            container_height,
            items_vec.len(),
            row_height,
            overscan,
        )
    });

    // Log list size in debug builds
    #[cfg(debug_assertions)]
    {
        let label = debug_label.clone();
        Effect::new(move |_| {
            let Some(items_vec) = items.try_get() else {
                return;
            };
            if !items_vec.is_empty() {
                let name = if label.is_empty() {
                    "VirtualTableBody"
                } else {
                    &label
                };
                web_sys::console::log_1(
                    &format!("[list] {}: {} items", name, items_vec.len()).into(),
                );
            }
        });
    }
    #[cfg(not(debug_assertions))]
    let _ = debug_label;

    let on_scroll = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(element) = target.dyn_into::<web_sys::HtmlElement>() {
                scroll_top.set(element.scroll_top() as u32);
            }
        }
    };

    view! {
        <div
            class="virtual-table-scroll overflow-y-auto"
            style:max-height={format!("{}px", container_height)}
            on:scroll=on_scroll
        >
            <table class="w-full">
                <tbody>
                    {move || {
                        let range = virtual_range.try_get().unwrap_or(VirtualRange { start: 0, end: 0, padding_top: 0, padding_bottom: 0 });
                        let items_vec = items.try_get().unwrap_or_default();
                        let render = render_row.clone();

                        view! {
                            // Top spacer row
                            {(range.padding_top > 0).then(|| view! {
                                <tr style:height={format!("{}px", range.padding_top)}>
                                    <td></td>
                                </tr>
                            })}

                            // Visible rows
                            {items_vec
                                .into_iter()
                                .enumerate()
                                .skip(range.start)
                                .take(range.end - range.start)
                                .map(|(idx, item)| render(item, idx))
                                .collect_view()}

                            // Bottom spacer row
                            {(range.padding_bottom > 0).then(|| view! {
                                <tr style:height={format!("{}px", range.padding_bottom)}>
                                    <td></td>
                                </tr>
                            })}
                        }
                    }}
                </tbody>
            </table>
        </div>
    }
}

/// Parse a CSS height value to pixels (simplified).
fn parse_height(height: &str) -> u32 {
    if height.ends_with("px") {
        height.trim_end_matches("px").parse().unwrap_or(400)
    } else if height.ends_with("vh") {
        // Approximate: assume 800px viewport height
        let vh: u32 = height.trim_end_matches("vh").parse().unwrap_or(50);
        vh * 8 // 800px / 100vh = 8px per vh
    } else {
        400 // default
    }
}

/// Simple capped list component that shows first N items with a "show more" message.
///
/// Use this when virtualization is not needed but unbounded rendering is risky.
#[component]
pub fn CappedList<T, V, F>(
    /// The items to render.
    #[prop(into)]
    items: Signal<Vec<T>>,
    /// Maximum items to show.
    #[prop(default = 50)]
    max_items: usize,
    /// Render function for each item.
    render_item: F,
    /// Debug label for logging.
    #[prop(optional, into)]
    debug_label: String,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T, usize) -> V + Clone + Send + Sync + 'static,
{
    // Log list size in debug builds
    #[cfg(debug_assertions)]
    {
        let label = debug_label.clone();
        Effect::new(move |_| {
            let Some(items_vec) = items.try_get() else {
                return;
            };
            if !items_vec.is_empty() {
                let name = if label.is_empty() {
                    "CappedList"
                } else {
                    &label
                };
                web_sys::console::log_1(
                    &format!("[list] {}: {} items", name, items_vec.len()).into(),
                );
            }
        });
    }
    #[cfg(not(debug_assertions))]
    let _ = debug_label;

    view! {
        {move || {
            let items_vec = items.try_get().unwrap_or_default();
            let total = items_vec.len();
            let capped = items_vec.into_iter().take(max_items);
            let render = render_item.clone();

            view! {
                {capped
                    .enumerate()
                    .map(|(idx, item)| render(item, idx))
                    .collect_view()}

                {(total > max_items).then(|| {
                    let remaining = total - max_items;
                    view! {
                        <div class="text-center py-4 text-sm text-muted-foreground border-t">
                            {format!("Showing first {} of {} items ({} more)", max_items, total, remaining)}
                        </div>
                    }
                })}
            }
        }}
    }
}
