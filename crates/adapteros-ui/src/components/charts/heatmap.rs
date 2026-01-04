//! Status heatmap component for worker health visualization.

use leptos::prelude::*;

use crate::components::charts::types::{HeatmapData, WorkerStatus};

/// Status heatmap for visualizing worker health over time.
///
/// Displays a grid where rows are workers and columns are time buckets.
/// Cell color indicates health status.
#[component]
pub fn StatusHeatmap(
    /// Heatmap data
    #[prop(into)]
    data: Signal<HeatmapData>,
    /// Chart title
    #[prop(optional, into)]
    title: Option<String>,
    /// Cell size in pixels
    #[prop(default = 24)]
    cell_size: u32,
    /// Gap between cells
    #[prop(default = 2)]
    gap: u32,
    /// Show column labels (times)
    #[prop(default = true)]
    show_column_labels: bool,
    /// Show row labels (worker IDs)
    #[prop(default = true)]
    show_row_labels: bool,
    /// Row label width
    #[prop(default = 100)]
    label_width: u32,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    // Tooltip state
    let hovered_cell = RwSignal::new(Option::<(usize, usize, String)>::None);

    view! {
        <div class={format!("heatmap-container glass-panel {}", class)} data-elevation="2">
            // Title
            {title.map(|t| view! {
                <h3 class="chart-title">{t}</h3>
            })}

            // Heatmap grid
            <div
                class="heatmap-scroll"
                role="img"
                aria-label="Worker status heatmap"
            >
                {move || {
                    let d = data.get();

                    if !d.has_data() {
                        return view! {
                            <div class="heatmap-empty">
                                <p class="text-muted-foreground">"No worker data available"</p>
                            </div>
                        }.into_any();
                    }

                    let (_rows, _cols) = d.dimensions();
                    let cell_with_gap = cell_size + gap;

                    view! {
                        <div class="heatmap-grid">
                            // Column labels (time)
                            {show_column_labels.then(|| view! {
                                <div
                                    class="heatmap-column-labels"
                                    style:margin-left={format!("{}px", label_width)}
                                >
                                    {d.column_labels.iter().enumerate().map(|(i, label)| {
                                        let left = i as u32 * cell_with_gap;
                                        view! {
                                            <span
                                                class="heatmap-column-label"
                                                style:left={format!("{}px", left)}
                                                style:width={format!("{}px", cell_size)}
                                            >
                                                {label.clone()}
                                            </span>
                                        }
                                    }).collect_view()}
                                </div>
                            })}

                            // Rows
                            <div class="heatmap-rows">
                                {d.rows.iter().enumerate().map(|(row_idx, row)| {
                                    view! {
                                        <div class="heatmap-row">
                                            // Row label
                                            {show_row_labels.then(|| view! {
                                                <span
                                                    class="heatmap-row-label"
                                                    style:width={format!("{}px", label_width)}
                                                    title={row.label.clone()}
                                                >
                                                    {truncate_label(&row.label, 12)}
                                                </span>
                                            })}

                                            // Cells
                                            <div
                                                class="heatmap-cells"
                                                style:gap={format!("{}px", gap)}
                                            >
                                                {row.cells.iter().enumerate().map(|(col_idx, cell)| {
                                                    let color = cell.status.color();
                                                    let tooltip = cell.tooltip.clone().unwrap_or_else(|| {
                                                        format!("{:.0}%", cell.value * 100.0)
                                                    });
                                                    let row_label = row.label.clone();
                                                    let col_label = d.column_labels.get(col_idx)
                                                        .cloned()
                                                        .unwrap_or_default();

                                                    let tooltip_for_enter = tooltip.clone();
                                                    let tooltip_for_focus = tooltip.clone();

                                                    view! {
                                                        <div
                                                            class="heatmap-cell"
                                                            style:width={format!("{}px", cell_size)}
                                                            style:height={format!("{}px", cell_size)}
                                                            style:background-color={color}
                                                            title={format!("{} @ {}: {}", row_label, col_label, tooltip)}
                                                            tabindex="0"
                                                            role="gridcell"
                                                            aria-label={format!("{} at {}: {}", row_label, col_label, tooltip)}
                                                            on:mouseenter=move |_| {
                                                                hovered_cell.set(Some((row_idx, col_idx, tooltip_for_enter.clone())));
                                                            }
                                                            on:mouseleave=move |_| {
                                                                hovered_cell.set(None);
                                                            }
                                                            on:focus=move |_| {
                                                                hovered_cell.set(Some((row_idx, col_idx, tooltip_for_focus.clone())));
                                                            }
                                                            on:blur=move |_| {
                                                                hovered_cell.set(None);
                                                            }
                                                        />
                                                    }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    }.into_any()
                }}
            </div>

            // Legend
            <div class="heatmap-legend">
                <HeatmapLegendItem status={WorkerStatus::Healthy} label="Healthy" />
                <HeatmapLegendItem status={WorkerStatus::Degraded} label="Degraded" />
                <HeatmapLegendItem status={WorkerStatus::Draining} label="Draining" />
                <HeatmapLegendItem status={WorkerStatus::Down} label="Down" />
            </div>
        </div>
    }
}

/// Legend item for heatmap.
#[component]
fn HeatmapLegendItem(status: WorkerStatus, #[prop(into)] label: String) -> impl IntoView {
    view! {
        <div class="heatmap-legend-item">
            <span
                class="heatmap-legend-color"
                style:background-color={status.color()}
            />
            <span class="heatmap-legend-label">{label}</span>
        </div>
    }
}

/// Compact status heatmap for dashboard cards.
#[component]
pub fn MiniHeatmap(
    /// Heatmap data
    #[prop(into)]
    data: Signal<HeatmapData>,
    /// Cell size
    #[prop(default = 12)]
    cell_size: u32,
    /// Gap between cells
    #[prop(default = 1)]
    gap: u32,
    /// Maximum rows to show
    #[prop(default = 5)]
    max_rows: usize,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    view! {
        <div class={format!("mini-heatmap {}", class)}>
            {move || {
                let d = data.get();

                if !d.has_data() {
                    return view! {
                        <span class="text-muted-foreground text-xs">"No data"</span>
                    }.into_any();
                }

                let rows_to_show = d.rows.len().min(max_rows);

                view! {
                    <div class="mini-heatmap-grid" style:gap={format!("{}px", gap)}>
                        {d.rows.iter().take(rows_to_show).map(|row| {
                            view! {
                                <div class="mini-heatmap-row" style:gap={format!("{}px", gap)}>
                                    {row.cells.iter().map(|cell| {
                                        view! {
                                            <div
                                                class="mini-heatmap-cell"
                                                style:width={format!("{}px", cell_size)}
                                                style:height={format!("{}px", cell_size)}
                                                style:background-color={cell.status.color()}
                                                title={cell.tooltip.clone().unwrap_or_default()}
                                            />
                                        }
                                    }).collect_view()}
                                </div>
                            }
                        }).collect_view()}

                        // Show "+N more" if truncated
                        {(d.rows.len() > max_rows).then(|| {
                            let remaining = d.rows.len() - max_rows;
                            view! {
                                <span class="mini-heatmap-more">
                                    {format!("+{} more", remaining)}
                                </span>
                            }
                        })}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

/// Truncate a label with ellipsis if too long.
fn truncate_label(label: &str, max_len: usize) -> String {
    if label.len() <= max_len {
        label.to_string()
    } else {
        format!("{}…", &label[..max_len - 1])
    }
}
