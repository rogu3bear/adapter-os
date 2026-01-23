//! Flight Recorder page
//!
//! Diagnostic run viewer with list and detail panels for inspecting
//! inference runs, stages, router decisions, and receipts.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Spinner, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use adapteros_api_types::diagnostics::{
    DiagEventResponse, DiagExportResponse, DiagRunResponse, ListDiagRunsQuery, StageTiming,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Flight Recorder list page - shows all diagnostic runs
#[component]
pub fn FlightRecorder() -> impl IntoView {
    // Selected run ID for split panel detail
    let selected_run_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());

    // Fetch diagnostic runs with filtering
    let (runs, refetch_runs) = use_api_resource(move |client: Arc<ApiClient>| {
        let filter = status_filter.get();
        async move {
            let query = ListDiagRunsQuery {
                status: if filter.is_empty() {
                    None
                } else {
                    Some(filter)
                },
                limit: Some(50),
                ..Default::default()
            };
            client.list_diag_runs(&query).await
        }
    });

    // Polling for live updates (every 10 seconds)
    let _ = use_polling(10_000, move || async move {
        refetch_runs.run(());
    });

    let on_run_select = move |run_id: String| {
        selected_run_id.set(Some(run_id));
    };

    let on_close_detail = move || {
        selected_run_id.set(None);
    };

    // Dynamic class for left panel width
    let left_panel_class = move || {
        if selected_run_id.get().is_some() {
            "w-1/2 space-y-6 pr-4 overflow-auto"
        } else {
            "flex-1 space-y-6 pr-4"
        }
    };

    view! {
        <div class="p-6 flex h-full">
            // Left panel: Run list
            <div class=left_panel_class>
                <div class="flex items-center justify-between">
                    <h1 class="text-3xl font-bold tracking-tight">"Flight Recorder"</h1>
                    <StatusFilter filter=status_filter/>
                </div>

                {move || {
                    match runs.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(response) => {
                            if response.runs.is_empty() {
                                view! {
                                    <Card>
                                        <div class="text-center py-8 text-muted-foreground">
                                            "No diagnostic runs found"
                                        </div>
                                    </Card>
                                }.into_any()
                            } else {
                                view! {
                                    <RunsTable
                                        runs=response.runs
                                        selected_id=selected_run_id
                                        on_select=Callback::new(on_run_select)
                                    />
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <Card>
                                    <div class="text-center py-8 text-destructive">
                                        {format!("Error loading runs: {}", e)}
                                    </div>
                                </Card>
                            }.into_any()
                        }
                    }
                }}
            </div>

            // Right panel: Run detail (when selected)
            {move || {
                selected_run_id.get().map(|run_id| {
                    view! {
                        <div class="w-1/2 border-l border-border pl-4 overflow-auto h-full">
                            <RunDetailPanel
                                run_id=run_id
                                on_close=Callback::new(move |_| on_close_detail())
                            />
                        </div>
                    }
                })
            }}
        </div>
    }
}

/// Flight Recorder detail page - accessed via /runs/:id route
#[component]
pub fn FlightRecorderDetail() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").unwrap_or_default();

    view! {
        <div class="p-6 h-full overflow-auto">
            <RunDetailPanel
                run_id=run_id()
                on_close=Callback::new(|_| {
                    // Navigate back to list
                    if let Some(window) = web_sys::window() {
                        let _ = window.history().and_then(|h| h.back());
                    }
                })
            />
        </div>
    }
}

/// Status filter dropdown
#[component]
fn StatusFilter(filter: RwSignal<String>) -> impl IntoView {
    view! {
        <select
            class="input h-9 w-40"
            on:change=move |ev| {
                let value = event_target_value(&ev);
                filter.set(value);
            }
            prop:value=move || filter.get()
        >
            <option value="">"All Statuses"</option>
            <option value="running">"Running"</option>
            <option value="completed">"Completed"</option>
            <option value="failed">"Failed"</option>
            <option value="cancelled">"Cancelled"</option>
        </select>
    }
}

/// Runs table component
#[component]
fn RunsTable(
    runs: Vec<DiagRunResponse>,
    selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Run ID"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Duration"</TableHead>
                        <TableHead>"Events"</TableHead>
                        <TableHead>"Started"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {runs.into_iter().map(|run| {
                        let run_id_for_click = run.id.clone();
                        let run_id_for_display = truncate_id(&run.id);
                        let is_selected = {
                            let rid = run.id.clone();
                            move || selected_id.get().as_ref() == Some(&rid)
                        };
                        let row_class = {
                            let is_sel = is_selected.clone();
                            move || if is_sel() { "bg-accent".to_string() } else { String::new() }
                        };
                        view! {
                            <TableRow class=row_class()>
                                <TableCell>
                                    <button
                                        class="text-left font-mono text-sm text-primary hover:underline cursor-pointer"
                                        on:click={
                                            let id = run_id_for_click.clone();
                                            move |_| on_select.run(id.clone())
                                        }
                                    >
                                        {run_id_for_display}
                                    </button>
                                </TableCell>
                                <TableCell>
                                    <StatusBadge status=run.status.clone()/>
                                </TableCell>
                                <TableCell>
                                    {format_duration_ms(run.duration_ms)}
                                </TableCell>
                                <TableCell>
                                    <span class="font-mono text-sm">
                                        {run.total_events_count}
                                        {if run.dropped_events_count > 0 {
                                            format!(" ({} dropped)", run.dropped_events_count)
                                        } else {
                                            String::new()
                                        }}
                                    </span>
                                </TableCell>
                                <TableCell>
                                    {format_timestamp_ms(run.started_at_unix_ms)}
                                </TableCell>
                            </TableRow>
                        }
                    }).collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
}

/// Status badge component
#[component]
fn StatusBadge(status: String) -> impl IntoView {
    let variant = match status.as_str() {
        "completed" => BadgeVariant::Success,
        "running" => BadgeVariant::Default,
        "failed" => BadgeVariant::Destructive,
        "cancelled" => BadgeVariant::Warning,
        _ => BadgeVariant::Outline,
    };
    view! {
        <Badge variant=variant>{status}</Badge>
    }
}

/// Run detail panel - shows stages, events, and receipts
#[component]
fn RunDetailPanel(run_id: String, on_close: Callback<()>) -> impl IntoView {
    // Fetch run export (includes events and timing)
    let run_id_clone = run_id.clone();
    let (export_data, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = run_id_clone.clone();
        async move { client.export_diag_run(&id).await }
    });

    // Tab state
    let active_tab = RwSignal::new(DetailTab::Timeline);
    let run_id_display = truncate_id(&run_id);

    view! {
        <div class="space-y-4">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <h2 class="text-xl font-semibold">"Run Details"</h2>
                    <p class="text-sm text-muted-foreground font-mono">{run_id_display}</p>
                </div>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| on_close.run(()))
                >
                    "Close"
                </Button>
            </div>

            // Tabs
            <div class="border-b border-border">
                <nav class="flex gap-4">
                    <TabButton
                        tab=DetailTab::Timeline
                        active=active_tab
                        label="Timeline"
                    />
                    <TabButton
                        tab=DetailTab::Events
                        active=active_tab
                        label="Events"
                    />
                    <TabButton
                        tab=DetailTab::Receipts
                        active=active_tab
                        label="Receipts"
                    />
                </nav>
            </div>

            // Tab content
            {move || {
                match export_data.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(export) => {
                        match active_tab.get() {
                            DetailTab::Timeline => {
                                view! { <TimelineTab export=export.clone()/> }.into_any()
                            }
                            DetailTab::Events => {
                                view! { <EventsTab export=export.clone()/> }.into_any()
                            }
                            DetailTab::Receipts => {
                                view! { <ReceiptsTab export=export.clone()/> }.into_any()
                            }
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="text-center py-8 text-destructive">
                                {format!("Error loading run: {}", e)}
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Tab enum for detail panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Timeline,
    Events,
    Receipts,
}

/// Tab button component
#[component]
fn TabButton(tab: DetailTab, active: RwSignal<DetailTab>, label: &'static str) -> impl IntoView {
    let is_active = move || active.get() == tab;
    view! {
        <button
            class=move || {
                if is_active() {
                    "py-2 px-1 border-b-2 border-primary text-primary font-medium"
                } else {
                    "py-2 px-1 border-b-2 border-transparent text-muted-foreground hover:text-foreground"
                }
            }
            on:click=move |_| active.set(tab)
        >
            {label}
        </button>
    }
}

/// Timeline tab - shows stages with durations
#[component]
fn TimelineTab(export: DiagExportResponse) -> impl IntoView {
    let timing = export.timing_summary.unwrap_or_default();

    if timing.is_empty() {
        return view! {
            <Card>
                <div class="text-center py-8 text-muted-foreground">
                    "No timing data available"
                </div>
            </Card>
        }
        .into_any();
    }

    // Calculate total duration for percentage bars
    let total_us: i64 = timing.iter().filter_map(|s| s.duration_us).sum();
    let status = export.run.status.clone();
    let duration_str = format_duration_ms(export.run.duration_ms);
    let events_count = export.run.total_events_count;

    view! {
        <div class="space-y-4">
            // Summary card
            <Card title="Run Summary".to_string()>
                <div class="grid grid-cols-3 gap-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Status"</p>
                        <StatusBadge status=status/>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Total Duration"</p>
                        <p class="font-medium">{duration_str}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Events"</p>
                        <p class="font-medium">{events_count}</p>
                    </div>
                </div>
            </Card>

            // Timeline visualization
            <Card title="Stage Timeline".to_string()>
                <div class="space-y-3">
                    {timing.into_iter().map(|stage| {
                        let pct = if total_us > 0 {
                            (stage.duration_us.unwrap_or(0) as f64 / total_us as f64 * 100.0).min(100.0)
                        } else {
                            0.0
                        };
                        view! {
                            <StageRow stage=stage pct=pct/>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </Card>
        </div>
    }
    .into_any()
}

/// Stage row with duration bar
#[component]
fn StageRow(stage: StageTiming, pct: f64) -> impl IntoView {
    let status_class = if stage.success {
        "bg-success"
    } else {
        "bg-destructive"
    };
    let stage_name = stage.stage.clone();
    let duration_str = format_duration_us(stage.duration_us);
    let bar_style = format!("width: {}%", pct.max(1.0));
    let bar_class = format!("h-full {} transition-all", status_class);

    view! {
        <div class="space-y-1">
            <div class="flex justify-between text-sm">
                <span class="font-medium">{stage_name}</span>
                <span class="text-muted-foreground font-mono">{duration_str}</span>
            </div>
            <div class="h-2 bg-muted rounded-full overflow-hidden">
                <div class=bar_class style=bar_style/>
            </div>
        </div>
    }
}

/// Events tab - shows events with collapsible details
#[component]
fn EventsTab(export: DiagExportResponse) -> impl IntoView {
    let events = export.events.unwrap_or_default();

    if events.is_empty() {
        return view! {
            <Card>
                <div class="text-center py-8 text-muted-foreground">
                    "No events available"
                </div>
            </Card>
        }
        .into_any();
    }

    // Group events by type for collapsible sections
    let mut grouped: std::collections::HashMap<String, Vec<DiagEventResponse>> =
        std::collections::HashMap::new();
    for event in events {
        grouped
            .entry(event.event_type.clone())
            .or_default()
            .push(event);
    }

    // Sort groups by first event's sequence number
    let mut groups: Vec<_> = grouped.into_iter().collect();
    groups.sort_by_key(|(_, events)| events.first().map(|e| e.seq).unwrap_or(0));

    view! {
        <div class="space-y-2">
            {groups.into_iter().map(|(event_type, events)| {
                view! {
                    <EventGroup event_type=event_type events=events/>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
    .into_any()
}

/// Collapsible event group
#[component]
fn EventGroup(event_type: String, events: Vec<DiagEventResponse>) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let count = events.len();
    let event_type_display = event_type.clone();

    view! {
        <Card>
            <button
                class="w-full flex items-center justify-between p-2 hover:bg-accent rounded cursor-pointer"
                on:click=move |_| expanded.update(|v| *v = !*v)
            >
                <div class="flex items-center gap-2">
                    <svg
                        class=move || format!("w-4 h-4 transition-transform {}", if expanded.get() { "rotate-90" } else { "" })
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    <span class="font-medium">{event_type_display}</span>
                </div>
                <Badge variant=BadgeVariant::Outline>{count.to_string()}</Badge>
            </button>

            {move || {
                if expanded.get() {
                    view! {
                        <div class="mt-2 space-y-1 pl-6">
                            {events.iter().map(|event| {
                                view! {
                                    <EventRow event=event.clone()/>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </Card>
    }
}

/// Single event row
#[component]
fn EventRow(event: DiagEventResponse) -> impl IntoView {
    let severity_class = match event.severity.as_str() {
        "error" => "text-destructive",
        "warn" => "text-warning",
        "debug" => "text-muted-foreground",
        "trace" => "text-muted-foreground/70",
        _ => "",
    };

    // Format payload as JSON (truncated)
    let payload_str =
        serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| "{}".to_string());
    let payload_truncated = if payload_str.len() > 200 {
        format!("{}...", &payload_str[..200])
    } else {
        payload_str
    };
    let seq = event.seq;
    let severity = event.severity.clone();
    let mono_us = format!("+{}us", event.mono_us);

    view! {
        <div class="border-l-2 border-border pl-3 py-1">
            <div class="flex items-center gap-2 text-sm">
                <span class="font-mono text-muted-foreground">{"#"}{seq}</span>
                <span class=severity_class>{severity}</span>
                <span class="text-muted-foreground font-mono text-xs">{mono_us}</span>
            </div>
            <pre class="text-xs text-muted-foreground mt-1 overflow-x-auto">{payload_truncated}</pre>
        </div>
    }
}

/// Receipts tab - shows hashes and verification status
#[component]
fn ReceiptsTab(export: DiagExportResponse) -> impl IntoView {
    let run_id = export.run.id.clone();
    let trace_id = export.run.trace_id.clone();
    let request_hash = export.run.request_hash.clone();
    let request_hash_verified = export.run.request_hash_verified;
    let manifest_hash = export.run.manifest_hash.clone();
    let manifest_hash_verified = export.run.manifest_hash_verified;

    let exported_at = export
        .metadata
        .as_ref()
        .map(|m| m.exported_at.clone())
        .unwrap_or_default();
    let events_exported = export
        .metadata
        .as_ref()
        .map(|m| m.events_exported)
        .unwrap_or(0);
    let events_total = export
        .metadata
        .as_ref()
        .map(|m| m.events_total)
        .unwrap_or(0);
    let truncated = export
        .metadata
        .as_ref()
        .map(|m| m.truncated)
        .unwrap_or(false);
    let has_metadata = export.metadata.is_some();

    let events_str = if truncated {
        format!("{} / {} (truncated)", events_exported, events_total)
    } else {
        format!("{} / {}", events_exported, events_total)
    };

    view! {
        <Card title="Receipts & Hashes".to_string()>
            <div class="space-y-4">
                // Run ID and Trace ID
                <div class="grid grid-cols-2 gap-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Run ID"</p>
                        <p class="font-mono text-sm break-all">{run_id}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Trace ID"</p>
                        <p class="font-mono text-sm break-all">{trace_id}</p>
                    </div>
                </div>

                // Hashes
                <div class="space-y-3">
                    <HashRow
                        label="Request Hash"
                        hash=request_hash
                        verified=request_hash_verified
                    />
                    {manifest_hash.map(|hash| {
                        view! {
                            <HashRow
                                label="Manifest Hash"
                                hash=hash
                                verified=manifest_hash_verified
                            />
                        }
                    })}
                </div>

                // Metadata
                {if has_metadata {
                    Some(view! {
                        <div class="border-t border-border pt-4 mt-4">
                            <p class="text-sm text-muted-foreground mb-2">"Export Metadata"</p>
                            <div class="grid grid-cols-2 gap-2 text-sm">
                                <div>
                                    <span class="text-muted-foreground">"Exported: "</span>
                                    {exported_at}
                                </div>
                                <div>
                                    <span class="text-muted-foreground">"Events: "</span>
                                    {events_str}
                                </div>
                            </div>
                        </div>
                    })
                } else {
                    None
                }}
            </div>
        </Card>
    }
}

/// Hash row with verification badge
///
/// Displays hash verification status:
/// - `Some(true)` - Verified (green badge)
/// - `Some(false)` - Invalid (red badge)
/// - `None` - Pending verification (secondary badge)
#[component]
fn HashRow(label: &'static str, hash: String, verified: Option<bool>) -> impl IntoView {
    let hash_display = truncate_hash(&hash);
    let (variant, text) = match verified {
        Some(true) => (BadgeVariant::Success, "Verified"),
        Some(false) => (BadgeVariant::Destructive, "Invalid"),
        None => (BadgeVariant::Secondary, "Pending"),
    };
    view! {
        <div class="flex items-center justify-between">
            <div>
                <p class="text-sm text-muted-foreground">{label}</p>
                <p class="font-mono text-sm break-all">{hash_display}</p>
            </div>
            <Badge variant=variant>
                {text}
            </Badge>
        </div>
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Truncate an ID for display
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

/// Truncate a hash for display
fn truncate_hash(hash: &str) -> String {
    if hash.len() > 16 {
        format!("{}...{}", &hash[..8], &hash[hash.len() - 8..])
    } else {
        hash.to_string()
    }
}

/// Format duration in milliseconds
fn format_duration_ms(ms: Option<i64>) -> String {
    match ms {
        Some(ms) if ms < 1000 => format!("{}ms", ms),
        Some(ms) => format!("{:.2}s", ms as f64 / 1000.0),
        None => "-".to_string(),
    }
}

/// Format duration in microseconds
fn format_duration_us(us: Option<i64>) -> String {
    match us {
        Some(us) if us < 1000 => format!("{}us", us),
        Some(us) if us < 1_000_000 => format!("{:.2}ms", us as f64 / 1000.0),
        Some(us) => format!("{:.2}s", us as f64 / 1_000_000.0),
        None => "-".to_string(),
    }
}

/// Format Unix timestamp in milliseconds with both relative and absolute time
fn format_timestamp_ms(ms: i64) -> String {
    let now_ms = js_sys::Date::now() as i64;
    let diff_ms = now_ms - ms;

    // Calculate relative time
    let relative = if diff_ms < 0 {
        "in the future".to_string()
    } else if diff_ms < 60_000 {
        "just now".to_string()
    } else if diff_ms < 3_600_000 {
        format!("{}m ago", diff_ms / 60_000)
    } else if diff_ms < 86_400_000 {
        format!("{}h ago", diff_ms / 3_600_000)
    } else {
        format!("{}d ago", diff_ms / 86_400_000)
    };

    // Format absolute time using js_sys::Date
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    let hours = date.get_hours();
    let minutes = date.get_minutes();
    let month = date.get_month() + 1; // 0-indexed
    let day = date.get_date();

    // Format as "5m ago (Jan 23, 2:45 PM)"
    let month_name = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    };

    let (hour12, ampm) = if hours == 0 {
        (12, "AM")
    } else if hours < 12 {
        (hours, "AM")
    } else if hours == 12 {
        (12, "PM")
    } else {
        (hours - 12, "PM")
    };

    format!(
        "{} ({} {}, {}:{:02} {})",
        relative, month_name, day, hour12, minutes, ampm
    )
}
