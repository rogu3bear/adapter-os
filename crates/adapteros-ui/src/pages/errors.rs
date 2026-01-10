//! Error Monitor page
//!
//! Real-time error monitoring with live feed, history, analytics, and alerts.
//! Uses SSE for real-time error streaming via `/v1/stream/client-errors`.

use crate::api::{use_sse_json, ApiClient, SseState};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorDisplay, Spinner, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::telemetry::{ClientErrorItem, ClientErrorStatsResponse};
use leptos::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Maximum number of errors to keep in the live feed ring buffer
const LIVE_FEED_MAX_ERRORS: usize = 100;

/// Error Monitor page
#[component]
pub fn Errors() -> impl IntoView {
    // Active tab
    let active_tab = RwSignal::new("live".to_string());

    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Error Monitor"</h1>
                    <p class="text-muted-foreground mt-1">"Real-time error monitoring and analysis"</p>
                </div>
            </div>

            // Tab navigation
            <div class="border-b">
                <nav class="-mb-px flex space-x-8">
                    <TabButton
                        tab="live".to_string()
                        label="Live Feed".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="history".to_string()
                        label="History".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="analytics".to_string()
                        label="Analytics".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="alerts".to_string()
                        label="Alerts".to_string()
                        active=active_tab
                    />
                </nav>
            </div>

            // Tab content
            <div class="py-4">
                {move || {
                    match active_tab.get().as_str() {
                        "live" => view! { <LiveFeedSection/> }.into_any(),
                        "history" => view! { <HistorySection/> }.into_any(),
                        "analytics" => view! { <AnalyticsSection/> }.into_any(),
                        "alerts" => view! { <AlertsSection/> }.into_any(),
                        _ => view! { <LiveFeedSection/> }.into_any(),
                    }
                }}
            </div>
        </div>
    }
}

/// Tab button component
#[component]
fn TabButton(tab: String, label: String, active: RwSignal<String>) -> impl IntoView {
    let tab_value = tab.clone();
    let is_active = move || active.get() == tab_value;

    view! {
        <button
            class=move || {
                let base = "whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors";
                if is_active() {
                    format!("{} border-primary text-primary", base)
                } else {
                    format!("{} border-transparent text-muted-foreground hover:text-foreground hover:border-muted", base)
                }
            }
            on:click={
                let tab = tab.clone();
                move |_| active.set(tab.clone())
            }
        >
            {label}
        </button>
    }
}

/// Live Feed section - real-time errors via SSE
#[component]
fn LiveFeedSection() -> impl IntoView {
    // Ring buffer for live errors
    let live_errors: RwSignal<VecDeque<ClientErrorItem>> = RwSignal::new(VecDeque::new());

    // Pause state
    let is_paused = RwSignal::new(false);

    // SSE connection for client error stream
    let (sse_status, _reconnect) =
        use_sse_json::<ClientErrorItem, _>("/v1/stream/client-errors", move |error| {
            if !is_paused.get() {
                live_errors.update(|errors| {
                    errors.push_front(error);
                    // Keep only the most recent errors
                    while errors.len() > LIVE_FEED_MAX_ERRORS {
                        errors.pop_back();
                    }
                });
            }
        });

    view! {
        <div class="space-y-4">
            // SSE status and controls
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                    <SseIndicator state=sse_status/>
                    <span class="text-sm text-muted-foreground">
                        {move || format!("{} errors in buffer", live_errors.get().len())}
                    </span>
                </div>
                <div class="flex items-center gap-2">
                    <Button
                        variant=if is_paused.get() { ButtonVariant::Primary } else { ButtonVariant::Outline }
                        on:click=move |_| is_paused.update(|p| *p = !*p)
                    >
                        {move || if is_paused.get() { "Resume" } else { "Pause" }}
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        on:click=move |_| live_errors.set(VecDeque::new())
                    >
                        "Clear"
                    </Button>
                </div>
            </div>

            // Live errors list
            <Card>
                {move || {
                    let errors = live_errors.get();
                    if errors.is_empty() {
                        view! {
                            <div class="py-12 text-center">
                                <div class="text-muted-foreground">"Waiting for errors..."</div>
                                <div class="text-sm text-muted-foreground mt-2">"Errors will appear here in real-time"</div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>"Time"</TableHead>
                                        <TableHead>"Type"</TableHead>
                                        <TableHead>"Message"</TableHead>
                                        <TableHead>"Status"</TableHead>
                                        <TableHead>"Page"</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {errors.iter().cloned().collect::<Vec<_>>().into_iter().map(|error| {
                                        view! {
                                            <ErrorRow error=error/>
                                        }
                                    }).collect::<Vec<_>>()}
                                </TableBody>
                            </Table>
                        }.into_any()
                    }
                }}
            </Card>
        </div>
    }
}

/// Single error row component
#[component]
fn ErrorRow(error: ClientErrorItem) -> impl IntoView {
    let badge_variant = match error.error_type.as_str() {
        "Network" => BadgeVariant::Destructive,
        "Http" => BadgeVariant::Warning,
        "Server" => BadgeVariant::Destructive,
        "Validation" => BadgeVariant::Secondary,
        _ => BadgeVariant::Outline,
    };

    let status_badge = error.http_status.map(|status| {
        let variant = if status >= 500 {
            BadgeVariant::Destructive
        } else if status >= 400 {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Secondary
        };
        (status, variant)
    });

    view! {
        <TableRow>
            <TableCell>
                <span class="text-xs text-muted-foreground font-mono">
                    {format_timestamp(&error.client_timestamp)}
                </span>
            </TableCell>
            <TableCell>
                <Badge variant=badge_variant>{error.error_type.clone()}</Badge>
            </TableCell>
            <TableCell>
                <span class="text-sm truncate max-w-md block" title=error.message.clone()>
                    {truncate_message(&error.message, 80)}
                </span>
            </TableCell>
            <TableCell>
                {status_badge.map(|(status, variant)| view! {
                    <Badge variant=variant>{status.to_string()}</Badge>
                })}
            </TableCell>
            <TableCell>
                <span class="text-xs text-muted-foreground font-mono">
                    {error.page.clone().unwrap_or_else(|| "-".to_string())}
                </span>
            </TableCell>
        </TableRow>
    }
}

/// History section - filtered query of past errors
#[component]
fn HistorySection() -> impl IntoView {
    // Filter state
    let error_type_filter = RwSignal::new(String::new());
    let http_status_filter = RwSignal::new(String::new());

    // Fetch errors from API
    let (errors, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        let error_type = {
            let val = error_type_filter.get();
            if val.is_empty() {
                None
            } else {
                Some(val)
            }
        };
        let http_status = {
            let val = http_status_filter.get();
            val.parse::<i32>().ok()
        };
        client
            .list_client_errors(
                error_type.as_deref(),
                http_status,
                None, // page_pattern
                None, // since
                None, // until
                Some(100),
                None,
            )
            .await
    });

    let refetch_signal = StoredValue::new(refetch);

    view! {
        <div class="space-y-4">
            // Filters
            <div class="flex items-center gap-4">
                <div class="flex items-center gap-2">
                    <label class="text-sm font-medium">"Type:"</label>
                    <select
                        class="rounded-md border border-input bg-background px-3 py-1.5 text-sm"
                        on:change=move |ev| {
                            error_type_filter.set(event_target_value(&ev));
                            refetch_signal.with_value(|f| f());
                        }
                    >
                        <option value="">"All"</option>
                        <option value="Network">"Network"</option>
                        <option value="Http">"HTTP"</option>
                        <option value="Server">"Server"</option>
                        <option value="Validation">"Validation"</option>
                    </select>
                </div>
                <div class="flex items-center gap-2">
                    <label class="text-sm font-medium">"Status:"</label>
                    <select
                        class="rounded-md border border-input bg-background px-3 py-1.5 text-sm"
                        on:change=move |ev| {
                            http_status_filter.set(event_target_value(&ev));
                            refetch_signal.with_value(|f| f());
                        }
                    >
                        <option value="">"All"</option>
                        <option value="400">"400"</option>
                        <option value="401">"401"</option>
                        <option value="403">"403"</option>
                        <option value="404">"404"</option>
                        <option value="500">"500"</option>
                        <option value="502">"502"</option>
                        <option value="503">"503"</option>
                    </select>
                </div>
                <Button
                    variant=ButtonVariant::Outline
                    on:click=move |_| refetch_signal.with_value(|f| f())
                >
                    "Refresh"
                </Button>
            </div>

            // Errors table
            <Card>
                {move || {
                    match errors.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            if data.errors.is_empty() {
                                view! {
                                    <div class="py-12 text-center">
                                        <div class="text-muted-foreground">"No errors found"</div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        <div class="px-4 py-2 text-sm text-muted-foreground border-b">
                                            {format!("Showing {} of {} errors", data.errors.len(), data.total)}
                                        </div>
                                        <Table>
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>"Time"</TableHead>
                                                    <TableHead>"Type"</TableHead>
                                                    <TableHead>"Message"</TableHead>
                                                    <TableHead>"Status"</TableHead>
                                                    <TableHead>"Page"</TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {data.errors.iter().cloned().map(|error| {
                                                    view! { <ErrorRow error=error/> }
                                                }).collect::<Vec<_>>()}
                                            </TableBody>
                                        </Table>
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <div class="p-4">
                                    <ErrorDisplay error=e.clone()/>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </Card>
        </div>
    }
}

/// Analytics section - error statistics
#[component]
fn AnalyticsSection() -> impl IntoView {
    // Fetch stats from API
    let (stats, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.get_client_error_stats(None).await
    });

    let refetch_signal = StoredValue::new(refetch);

    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-end">
                <Button
                    variant=ButtonVariant::Outline
                    on:click=move |_| refetch_signal.with_value(|f| f())
                >
                    "Refresh"
                </Button>
            </div>

            {move || {
                match stats.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <StatsDisplay stats=data/> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="p-4">
                                <ErrorDisplay error=e.clone()/>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Stats display component
#[component]
fn StatsDisplay(stats: ClientErrorStatsResponse) -> impl IntoView {
    // Extract counts to avoid move issues in closures
    let total_count = stats.total_count;
    let error_type_count = stats.error_type_counts.len();
    let http_status_count = stats.http_status_counts.len();

    // Clone vectors for iteration
    let error_types = stats.error_type_counts;
    let http_statuses = stats.http_status_counts;
    let hourly_errors = stats.errors_per_hour;

    // Pre-calculate max for hourly chart
    let hourly_max = hourly_errors.iter().map(|h| h.count).max().unwrap_or(1);

    view! {
        <div class="space-y-6">
            // Summary cards
            <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                <Card>
                    <div class="p-4">
                        <div class="text-sm font-medium text-muted-foreground">"Total Errors (24h)"</div>
                        <div class="text-3xl font-bold mt-1">{total_count}</div>
                    </div>
                </Card>
                <Card>
                    <div class="p-4">
                        <div class="text-sm font-medium text-muted-foreground">"Error Types"</div>
                        <div class="text-3xl font-bold mt-1">{error_type_count}</div>
                    </div>
                </Card>
                <Card>
                    <div class="p-4">
                        <div class="text-sm font-medium text-muted-foreground">"HTTP Status Codes"</div>
                        <div class="text-3xl font-bold mt-1">{http_status_count}</div>
                    </div>
                </Card>
            </div>

            // Error type breakdown
            <Card>
                <div class="p-4">
                    <h3 class="text-lg font-semibold mb-4">"Errors by Type"</h3>
                    {if error_types.is_empty() {
                        view! {
                            <div class="text-muted-foreground text-sm">"No errors recorded"</div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-2">
                                {error_types.iter().map(|tc| {
                                    let percentage = if total_count > 0 {
                                        (tc.count as f64 / total_count as f64 * 100.0) as u32
                                    } else {
                                        0
                                    };
                                    view! {
                                        <div class="flex items-center gap-2">
                                            <div class="w-24 text-sm font-medium">{tc.error_type.clone()}</div>
                                            <div class="flex-1 h-4 bg-muted rounded-full overflow-hidden">
                                                <div
                                                    class="h-full bg-primary transition-all"
                                                    style=format!("width: {}%", percentage)
                                                />
                                            </div>
                                            <div class="w-16 text-sm text-right">{tc.count}</div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </div>
            </Card>

            // HTTP status breakdown
            <Card>
                <div class="p-4">
                    <h3 class="text-lg font-semibold mb-4">"Errors by HTTP Status"</h3>
                    {if http_statuses.is_empty() {
                        view! {
                            <div class="text-muted-foreground text-sm">"No HTTP errors recorded"</div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-2">
                                {http_statuses.iter().map(|hc| {
                                    let percentage = if total_count > 0 {
                                        (hc.count as f64 / total_count as f64 * 100.0) as u32
                                    } else {
                                        0
                                    };
                                    let status_class = if hc.http_status >= 500 {
                                        "bg-destructive"
                                    } else if hc.http_status >= 400 {
                                        "bg-warning"
                                    } else {
                                        "bg-primary"
                                    };
                                    view! {
                                        <div class="flex items-center gap-2">
                                            <div class="w-24 text-sm font-medium">{hc.http_status.to_string()}</div>
                                            <div class="flex-1 h-4 bg-muted rounded-full overflow-hidden">
                                                <div
                                                    class=format!("h-full transition-all {}", status_class)
                                                    style=format!("width: {}%", percentage)
                                                />
                                            </div>
                                            <div class="w-16 text-sm text-right">{hc.count}</div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </div>
            </Card>

            // Hourly breakdown
            <Card>
                <div class="p-4">
                    <h3 class="text-lg font-semibold mb-4">"Errors per Hour (Last 24h)"</h3>
                    {if hourly_errors.is_empty() {
                        view! {
                            <div class="text-muted-foreground text-sm">"No hourly data available"</div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="flex items-end gap-1 h-32">
                                {hourly_errors.iter().map(|h| {
                                    let height_percent = if hourly_max > 0 {
                                        (h.count as f64 / hourly_max as f64 * 100.0) as u32
                                    } else {
                                        0
                                    };
                                    view! {
                                        <div
                                            class="flex-1 bg-primary rounded-t transition-all"
                                            style=format!("height: {}%", height_percent.max(2))
                                            title=format!("{}: {} errors", h.hour, h.count)
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </div>
            </Card>
        </div>
    }
}

/// Alerts section - placeholder for alert rules
#[component]
fn AlertsSection() -> impl IntoView {
    view! {
        <Card>
            <div class="p-8 text-center">
                <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-8 w-8 text-muted-foreground"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Alert Rules"</h3>
                <p class="text-muted-foreground mt-2 max-w-md mx-auto">
                    "Configure threshold-based alerts to be notified when error rates exceed defined limits."
                </p>
                <div class="mt-4">
                    <Badge variant=BadgeVariant::Secondary>"Coming Soon"</Badge>
                </div>
            </div>
        </Card>
    }
}

/// SSE connection indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    let (color, label) = match state.get() {
        SseState::Connected => ("bg-green-500", "Connected"),
        SseState::Connecting => ("bg-yellow-500 animate-pulse", "Connecting..."),
        SseState::Disconnected => ("bg-gray-400", "Disconnected"),
        SseState::Error => ("bg-red-500", "Error"),
        SseState::CircuitOpen => ("bg-orange-500", "Circuit Open"),
    };

    view! {
        <div class="flex items-center gap-2">
            <div class=format!("w-2 h-2 rounded-full {}", color)/>
            <span class="text-xs text-muted-foreground">{label}</span>
        </div>
    }
}

/// Format timestamp for display
fn format_timestamp(ts: &str) -> String {
    // Simple extraction of time portion from ISO 8601 timestamp
    if let Some(time_start) = ts.find('T') {
        let time_part = &ts[time_start + 1..];
        if let Some(end) = time_part.find('.').or_else(|| time_part.find('Z')) {
            return time_part[..end].to_string();
        }
        if time_part.len() >= 8 {
            return time_part[..8].to_string();
        }
    }
    ts.to_string()
}

/// Truncate message for display
fn truncate_message(msg: &str, max_len: usize) -> String {
    if msg.len() <= max_len {
        msg.to_string()
    } else {
        format!("{}...", &msg[..max_len])
    }
}
