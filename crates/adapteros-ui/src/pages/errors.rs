//! Error Monitor page
//!
//! Real-time error monitoring with live feed, history, analytics, alerts, and crashes.
//! Uses SSE for real-time error streaming via `/v1/stream/client-errors`.

use crate::api::{
    use_sse_json, ApiClient, CreateErrorAlertRuleRequest, ErrorAlertHistoryListResponse,
    ErrorAlertHistoryResponse, ErrorAlertRuleResponse, ProcessCrashDumpResponse, SseState,
    UpdateErrorAlertRuleRequest,
};
use crate::components::{
    AsyncBoundary, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Dialog, EmptyState,
    Input, LoadingDisplay, Select, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
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
                    <h1 class="text-3xl font-bold tracking-tight">"Incidents"</h1>
                    <p class="text-muted-foreground mt-1">"Real-time error monitoring and analysis"</p>
                </div>
            </div>

            // Tab navigation
            <div class="border-b">
                <nav class="-mb-px flex space-x-8" role="tablist">
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
                    <TabButton
                        tab="crashes".to_string()
                        label="Crashes".to_string()
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
                        "crashes" => view! { <CrashesSection/> }.into_any(),
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
            class={
                let is_active = is_active.clone();
                move || {
                    let base = "whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded-t-sm";
                    if is_active() {
                        format!("{} border-primary text-primary", base)
                    } else {
                        format!("{} border-transparent text-muted-foreground hover:text-foreground hover:border-muted", base)
                    }
                }
            }
            type="button"
            role="tab"
            aria-selected=is_active
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

    // Refetch on filter changes
    Effect::new(move || {
        let _ = error_type_filter.get();
        let _ = http_status_filter.get();
        refetch.run(());
    });

    view! {
        <div class="space-y-4">
            // Filters
            <div class="flex items-center gap-4">
                <Select
                    value=error_type_filter
                    label="Type".to_string()
                    options=vec![
                        ("".to_string(), "All".to_string()),
                        ("Network".to_string(), "Network".to_string()),
                        ("Http".to_string(), "HTTP".to_string()),
                        ("Server".to_string(), "Server".to_string()),
                        ("Validation".to_string(), "Validation".to_string()),
                    ]
                    class="w-32".to_string()
                />
                <Select
                    value=http_status_filter
                    label="Status".to_string()
                    options=vec![
                        ("".to_string(), "All".to_string()),
                        ("400".to_string(), "400".to_string()),
                        ("401".to_string(), "401".to_string()),
                        ("403".to_string(), "403".to_string()),
                        ("404".to_string(), "404".to_string()),
                        ("500".to_string(), "500".to_string()),
                        ("502".to_string(), "502".to_string()),
                        ("503".to_string(), "503".to_string()),
                    ]
                    class="w-24".to_string()
                />
                <Button
                    variant=ButtonVariant::Outline
                    on:click=move |_| refetch.run(())
                >
                    "Refresh"
                </Button>
            </div>

            // Errors table
            <Card>
                <AsyncBoundary
                    state=errors
                    on_retry=Callback::new(move |_| refetch.run(()))
                    render=move |data| {
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
                />
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

    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-end">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </div>

            <AsyncBoundary
                state=stats
                on_retry=Callback::new(move |_| refetch.run(()))
                render=move |data| view! { <StatsDisplay stats=data /> }
            />
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
                                        "bg-status-warning"
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

/// Alert history panel - recent alert triggers
#[component]
fn AlertHistoryPanel() -> impl IntoView {
    let status_filter = RwSignal::new("unresolved".to_string());

    let (history, refetch_history) = use_api_resource(move |client: Arc<ApiClient>| {
        let unresolved_only = if status_filter.get() == "unresolved" {
            Some(true)
        } else {
            None
        };
        async move { client.list_error_alert_history(unresolved_only, Some(50)).await }
    });

    Effect::new(move || {
        let _ = status_filter.get();
        refetch_history.run(());
    });

    view! {
        <Card>
            <div class="flex items-center justify-between mb-4">
                <div>
                    <h3 class="text-lg font-semibold">"Triggered Alerts"</h3>
                    <p class="text-sm text-muted-foreground">
                        "Recent alert rule activations and acknowledgements"
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <Select
                        value=status_filter
                        label="Status".to_string()
                        options=vec![
                            ("unresolved".to_string(), "Unresolved".to_string()),
                            ("all".to_string(), "All".to_string()),
                        ]
                        class="w-36".to_string()
                    />
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        on_click=Callback::new(move |_| refetch_history.run(()))
                    >
                        "Refresh"
                    </Button>
                </div>
            </div>

            <AsyncBoundary
                state=history
                on_retry=Callback::new(move |_| refetch_history.run(()))
                render=move |data: ErrorAlertHistoryListResponse| {
                    if data.alerts.is_empty() {
                        view! {
                            <EmptyState
                                title="No alerts triggered"
                                description="Alert history will appear here when rules fire."
                                icon="bell"
                            />
                        }.into_any()
                    } else {
                        let shown = data.alerts.len();
                        let total = data.total;
                        let rows: Vec<_> = data.alerts
                            .into_iter()
                            .map(|alert| view! { <AlertHistoryRow alert=alert/> })
                            .collect();
                        view! {
                            <div>
                                <div class="px-4 py-2 text-sm text-muted-foreground border-b">
                                    {format!("Showing {} of {} alerts", shown, total)}
                                </div>
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Triggered"</TableHead>
                                            <TableHead>"Rule"</TableHead>
                                            <TableHead>"Errors"</TableHead>
                                            <TableHead>"Status"</TableHead>
                                            <TableHead>"Acknowledged"</TableHead>
                                            <TableHead>"Resolved"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {rows}
                                    </TableBody>
                                </Table>
                            </div>
                        }.into_any()
                    }
                }
            />
        </Card>
    }
}

/// Alert history row
#[component]
fn AlertHistoryRow(alert: ErrorAlertHistoryResponse) -> impl IntoView {
    let (status_label, status_variant) = if alert.resolved_at.is_some() {
        ("Resolved", BadgeVariant::Success)
    } else if alert.acknowledged_at.is_some() {
        ("Acknowledged", BadgeVariant::Secondary)
    } else {
        ("Active", BadgeVariant::Warning)
    };

    let rule_label = alert
        .rule_name
        .clone()
        .unwrap_or_else(|| truncate_message(&alert.rule_id, 12));
    let rule_title = alert.rule_id.clone();
    let acknowledged = alert
        .acknowledged_at
        .as_deref()
        .map(format_date_time)
        .unwrap_or_else(|| "-".to_string());
    let resolved = alert
        .resolved_at
        .as_deref()
        .map(format_date_time)
        .unwrap_or_else(|| "-".to_string());
    let resolution_note = alert.resolution_note.clone().unwrap_or_default();

    view! {
        <TableRow>
            <TableCell>
                <span class="text-xs text-muted-foreground font-mono">
                    {format_date_time(&alert.triggered_at)}
                </span>
            </TableCell>
            <TableCell>
                <span class="text-sm font-mono" title=rule_title>{rule_label}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm font-medium">{alert.error_count.to_string()}</span>
            </TableCell>
            <TableCell>
                <Badge variant=status_variant>{status_label}</Badge>
            </TableCell>
            <TableCell>
                <span class="text-xs text-muted-foreground">{acknowledged}</span>
            </TableCell>
            <TableCell>
                <span class="text-xs text-muted-foreground" title=resolution_note>{resolved}</span>
            </TableCell>
        </TableRow>
    }
}

/// Alerts section - manages error alert rules
#[component]
fn AlertsSection() -> impl IntoView {
    // State for alert rules
    let (rules, refetch_rules) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_error_alert_rules().await.map(|r| r.rules)
    });

    // State for create dialog
    let show_create_dialog = RwSignal::new(false);

    view! {
        <div class="space-y-6">
            <AlertHistoryPanel/>

            <div class="space-y-4">
                // Header with create button
                <div class="flex items-center justify-between">
                    <div>
                        <h3 class="text-lg font-semibold">"Alert Rules"</h3>
                        <p class="text-sm text-muted-foreground">
                            "Configure threshold-based alerts for error monitoring"
                        </p>
                    </div>
                    <div class="flex gap-2">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| refetch_rules.run(()))
                        >
                            "Refresh"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| show_create_dialog.set(true))
                        >
                            "+ New Rule"
                        </Button>
                    </div>
                </div>

                // Rules list
                <AsyncBoundary
                    state=rules
                    on_retry={Callback::new(move |_| refetch_rules.run(()))}
                    render={move |rules_list: Vec<ErrorAlertRuleResponse>| {
                        if rules_list.is_empty() {
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
                                        <p class="text-muted-foreground">
                                            "No alert rules configured. Create your first rule to get notified when errors exceed thresholds."
                                        </p>
                                    </div>
                                </Card>
                            }.into_any()
                        } else {
                            view! {
                                <AlertRulesList rules=rules_list on_update={Callback::new(move |_| refetch_rules.run(()))}/>
                            }.into_any()
                        }
                    }}
                />

                // Create dialog
                <CreateAlertRuleDialog
                    open=show_create_dialog
                    on_created=Callback::new(move |_| {
                        show_create_dialog.set(false);
                        refetch_rules.run(());
                    })
                />
            </div>
        </div>
    }
}

/// Crash dumps section - worker crash data
#[component]
fn CrashesSection() -> impl IntoView {
    let selected_worker_id = RwSignal::new(String::new());

    let (workers, refetch_workers) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_workers().await });

    let (crashes, refetch_crashes) = use_api_resource(move |client: Arc<ApiClient>| {
        let worker_id = selected_worker_id.get();
        async move {
            if worker_id.is_empty() {
                Ok(Vec::new())
            } else {
                client.get_worker_crashes(&worker_id).await
            }
        }
    });

    // Auto-select a crashed worker (or first worker) once list loads
    Effect::new(move || {
        if !selected_worker_id.get().is_empty() {
            return;
        }
        if let LoadingState::Loaded(ref list) = workers.get() {
            if let Some(worker) = list
                .iter()
                .find(|w| w.status == "crashed")
                .or_else(|| list.first())
            {
                selected_worker_id.set(worker.id.clone());
            }
        }
    });

    // Refetch crashes when selection changes
    Effect::new(move || {
        let id = selected_worker_id.get();
        if !id.is_empty() {
            refetch_crashes.run(());
        }
    });

    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="text-lg font-semibold">"Crash Dumps"</h3>
                    <p class="text-sm text-muted-foreground">
                        "Worker crash reports and recovery details"
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    {move || {
                        match workers.get() {
                            LoadingState::Loaded(list) if !list.is_empty() => {
                                let options: Vec<(String, String)> = list
                                    .iter()
                                    .map(|worker| {
                                        let label = format!("{} ({})", truncate_message(&worker.id, 12), worker.status);
                                        (worker.id.clone(), label)
                                    })
                                    .collect();
                                view! {
                                    <Select
                                        value=selected_worker_id
                                        label="Worker".to_string()
                                        options=options
                                        class="w-56".to_string()
                                    />
                                }.into_any()
                            }
                            LoadingState::Loaded(_) => view! {
                                <span class="text-xs text-muted-foreground">"No workers"</span>
                            }.into_any(),
                            LoadingState::Error(_) => view! {
                                <span class="text-xs text-destructive">"Failed to load workers"</span>
                            }.into_any(),
                            LoadingState::Idle | LoadingState::Loading => view! {
                                <span class="text-xs text-muted-foreground">"Loading workers..."</span>
                            }.into_any(),
                        }
                    }}
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        on_click=Callback::new(move |_| {
                            refetch_workers.run(());
                            refetch_crashes.run(());
                        })
                    >
                        "Refresh"
                    </Button>
                </div>
            </div>

            {move || match workers.get() {
                LoadingState::Idle | LoadingState::Loading => {
                    view! { <LoadingDisplay message="Loading workers..."/> }.into_any()
                }
                LoadingState::Error(e) => {
                    view! {
                        <Card>
                            <div class="p-4 text-sm text-destructive">
                                {format!("Failed to load workers: {}", e)}
                            </div>
                        </Card>
                    }.into_any()
                }
                LoadingState::Loaded(list) => {
                    if list.is_empty() {
                        view! {
                            <Card>
                                <EmptyState
                                    title="No workers available"
                                    description="Crash dumps will appear once workers are registered."
                                />
                            </Card>
                        }.into_any()
                    } else {
                        view! {
                            <Card>
                                <AsyncBoundary
                                    state=crashes
                                    on_retry=Callback::new(move |_| refetch_crashes.run(()))
                                    loading_message="Loading crash dumps...".to_string()
                                    render=move |data| {
                                        let data: Vec<ProcessCrashDumpResponse> = data;
                                        if selected_worker_id.get().is_empty() {
                                            view! {
                                                <EmptyState
                                                    title="Select a worker"
                                                    description="Choose a worker to view crash dumps."
                                                />
                                            }.into_any()
                                        } else if data.is_empty() {
                                            view! {
                                                <EmptyState
                                                    title="No crash dumps"
                                                    description="No crash data recorded for this worker."
                                                />
                                            }.into_any()
                                        } else {
                                            let rows: Vec<_> = data
                                                .into_iter()
                                                .map(|crash| view! { <CrashRow crash=crash/> })
                                                .collect();
                                            view! {
                                                <Table>
                                                    <TableHeader>
                                                        <TableRow>
                                                            <TableHead>"Time"</TableHead>
                                                            <TableHead>"Worker"</TableHead>
                                                            <TableHead>"Type"</TableHead>
                                                            <TableHead>"Recovery"</TableHead>
                                                            <TableHead>"Details"</TableHead>
                                                        </TableRow>
                                                    </TableHeader>
                                                    <TableBody>
                                                        {rows}
                                                    </TableBody>
                                                </Table>
                                            }.into_any()
                                        }
                                    }
                                />
                            </Card>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Crash dump row
#[component]
fn CrashRow(crash: ProcessCrashDumpResponse) -> impl IntoView {
    let stack = crash
        .stack_trace
        .clone()
        .unwrap_or_else(|| "No stack trace".to_string());
    let stack_preview = truncate_message(&stack, 80);
    let worker_label = truncate_message(&crash.worker_id, 12);
    let recovery_label = crash
        .recovery_action
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let recovered_at = crash
        .recovered_at
        .as_deref()
        .map(format_date_time)
        .unwrap_or_else(|| "-".to_string());

    view! {
        <TableRow>
            <TableCell>
                <span class="text-xs text-muted-foreground font-mono">
                    {format_date_time(&crash.crash_timestamp)}
                </span>
            </TableCell>
            <TableCell>
                <span class="text-xs font-mono" title=crash.worker_id.clone()>{worker_label}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm font-medium">{crash.crash_type.clone()}</span>
            </TableCell>
            <TableCell>
                <div class="space-y-1">
                    <span class="text-sm">{recovery_label}</span>
                    <span class="text-xs text-muted-foreground">
                        {"Recovered: "}{recovered_at}
                    </span>
                </div>
            </TableCell>
            <TableCell>
                <span class="text-xs text-muted-foreground" title=stack>{stack_preview}</span>
            </TableCell>
        </TableRow>
    }
}

/// Alert rules list component
#[component]
fn AlertRulesList(rules: Vec<ErrorAlertRuleResponse>, on_update: Callback<()>) -> impl IntoView {
    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Pattern"</TableHead>
                        <TableHead>"Threshold"</TableHead>
                        <TableHead>"Severity"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rules.into_iter().map(|rule| {
                        view! {
                            <AlertRuleRow rule=rule on_update=on_update/>
                        }
                    }).collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
}

/// Single alert rule row
#[component]
fn AlertRuleRow(rule: ErrorAlertRuleResponse, on_update: Callback<()>) -> impl IntoView {
    let rule_id_for_toggle = rule.id.clone();
    let rule_id_for_delete = rule.id.clone();
    let is_active = rule.is_active;

    let (toggling, set_toggling) = signal(false);
    let (deleting, set_deleting) = signal(false);

    // Toggle active state
    let on_toggle = move |_| {
        let id = rule_id_for_toggle.clone();
        set_toggling.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let request = UpdateErrorAlertRuleRequest {
                is_active: Some(!is_active),
                ..Default::default()
            };
            match client.update_error_alert_rule(&id, &request).await {
                Ok(_) => on_update.run(()),
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to toggle rule: {}", e).into());
                }
            }
            set_toggling.set(false);
        });
    };

    // Delete rule
    let on_delete = move |_| {
        let id = rule_id_for_delete.clone();
        set_deleting.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.delete_error_alert_rule(&id).await {
                Ok(_) => on_update.run(()),
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to delete rule: {}", e).into());
                }
            }
            set_deleting.set(false);
        });
    };

    let severity_variant = match rule.severity.as_str() {
        "critical" => BadgeVariant::Destructive,
        "warning" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    };

    let pattern_display = rule
        .error_type_pattern
        .clone()
        .or_else(|| rule.http_status_pattern.clone())
        .or_else(|| rule.page_pattern.clone())
        .unwrap_or_else(|| "Any".to_string());

    view! {
        <TableRow>
            <TableCell>
                <div>
                    <p class="font-medium">{rule.name.clone()}</p>
                    {rule.description.clone().map(|d| view! {
                        <p class="text-xs text-muted-foreground">{d}</p>
                    })}
                </div>
            </TableCell>
            <TableCell>
                <code class="text-xs bg-muted px-1 py-0.5 rounded">{pattern_display}</code>
            </TableCell>
            <TableCell>
                <span class="text-sm">
                    {format!("{} in {}m", rule.threshold_count, rule.threshold_window_minutes)}
                </span>
            </TableCell>
            <TableCell>
                <Badge variant=severity_variant>{rule.severity.clone()}</Badge>
            </TableCell>
            <TableCell>
                <Badge variant=if rule.is_active { BadgeVariant::Success } else { BadgeVariant::Secondary }>
                    {if rule.is_active { "Active" } else { "Inactive" }}
                </Badge>
            </TableCell>
            <TableCell class="text-right">
                <div class="flex justify-end gap-1">
                    <Button
                        variant=ButtonVariant::Ghost
                        size=ButtonSize::Sm
                        on_click=Callback::new(on_toggle)
                        disabled=Signal::derive(move || toggling.get())
                    >
                        {move || if toggling.get() { "..." } else if is_active { "Disable" } else { "Enable" }}
                    </Button>
                    <Button
                        variant=ButtonVariant::Ghost
                        size=ButtonSize::Sm
                        on_click=Callback::new(on_delete)
                        disabled=Signal::derive(move || deleting.get())
                    >
                        {move || if deleting.get() { "..." } else { "Delete" }}
                    </Button>
                </div>
            </TableCell>
        </TableRow>
    }
}

/// Create alert rule dialog
#[component]
fn CreateAlertRuleDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let error_pattern = RwSignal::new(String::new());
    let threshold_count = RwSignal::new("5".to_string());
    let threshold_window = RwSignal::new("5".to_string());
    let severity = RwSignal::new("warning".to_string());
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |_| {
        let name_val = name.get();
        if name_val.trim().is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        let threshold_count_val = threshold_count.get().parse::<i32>().unwrap_or(5).max(1);
        let threshold_window_val = threshold_window.get().parse::<i32>().unwrap_or(5).max(1);

        submitting.set(true);
        error.set(None);

        let request = CreateErrorAlertRuleRequest {
            name: name_val,
            description: if description.get().is_empty() {
                None
            } else {
                Some(description.get())
            },
            error_type_pattern: if error_pattern.get().is_empty() {
                None
            } else {
                Some(error_pattern.get())
            },
            http_status_pattern: None,
            page_pattern: None,
            threshold_count: threshold_count_val,
            threshold_window_minutes: threshold_window_val,
            cooldown_minutes: 15,
            severity: severity.get(),
            notification_channels: None,
        };

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.create_error_alert_rule(&request).await {
                Ok(_) => on_created.run(()),
                Err(e) => {
                    error.set(Some(e.to_string()));
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <Dialog
            open=open
            title="Create Alert Rule"
            description="Configure a new alert rule for error monitoring"
        >
            {move || error.get().map(|e| view! {
                <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                    {e}
                </div>
            })}

            <div class="space-y-4">
                <Input
                    value=name
                    label="Name".to_string()
                    placeholder="High error rate alert".to_string()
                    required=true
                />

                <Input
                    value=description
                    label="Description".to_string()
                    placeholder="Optional description".to_string()
                />

                <div>
                    <Input
                        value=error_pattern
                        label="Error Type Pattern".to_string()
                        placeholder="e.g., NetworkError, *Timeout* (optional)".to_string()
                    />
                    <p class="text-xs text-muted-foreground mt-1">
                        "Leave empty to match all error types"
                    </p>
                </div>

                <div class="grid grid-cols-2 gap-4">
                    <Input
                        value=threshold_count
                        label="Threshold Count".to_string()
                        input_type="number".to_string()
                    />
                    <Input
                        value=threshold_window
                        label="Window (minutes)".to_string()
                        input_type="number".to_string()
                    />
                </div>

                <Select
                    value=severity
                    label="Severity".to_string()
                    options=vec![
                        ("info".to_string(), "Info".to_string()),
                        ("warning".to_string(), "Warning".to_string()),
                        ("critical".to_string(), "Critical".to_string()),
                    ]
                />
            </div>

            <div class="flex justify-end gap-2 pt-4">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| open.set(false))
                >
                    "Cancel"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(on_submit)
                    disabled=Signal::derive(move || submitting.get())
                >
                    {move || if submitting.get() { "Creating..." } else { "Create Rule" }}
                </Button>
            </div>
        </Dialog>
    }
}

/// SSE connection indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    let (color, label) = match state.get() {
        SseState::Connected => ("bg-status-success", "Connected"),
        SseState::Connecting => ("bg-status-warning animate-pulse", "Connecting..."),
        SseState::Disconnected => ("bg-muted", "Disconnected"),
        SseState::Error => ("bg-status-error", "Error"),
        SseState::CircuitOpen => ("bg-status-warning", "Circuit Open"),
    };

    view! {
        <div class="flex items-center gap-2">
            <div class=format!("w-2 h-2 rounded-full {}", color)/>
            <span class="text-xs text-muted-foreground">{label}</span>
        </div>
    }
}

/// Format date/time for display
fn format_date_time(ts: &str) -> String {
    if ts.len() >= 16 {
        format!("{} {}", &ts[0..10], &ts[11..16])
    } else {
        ts.to_string()
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
