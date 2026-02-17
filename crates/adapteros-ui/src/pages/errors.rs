//! Error Monitor page
//!
//! Real-time error monitoring with live feed, history, analytics, alerts, and crashes.
//! Uses SSE for real-time error streaming via `/v1/stream/client-errors`.

use crate::api::{
    report_error_with_toast, use_sse_json, ApiClient, CreateErrorAlertRuleRequest,
    ErrorAlertHistoryResponse, ErrorAlertRuleResponse, ProcessCrashDumpResponse, SseState,
    UpdateErrorAlertRuleRequest,
};
use crate::components::{
    loaded_signal, AsyncBoundary, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card,
    Column, ConfirmationDialog, ConfirmationSeverity, DataTable, Dialog, EmptyState, Input,
    PageBreadcrumbItem, PageScaffold, Select, SkeletonTable, TabNav, TabPanel, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use crate::utils::humanize;
use adapteros_api_types::telemetry::{ClientErrorItem, ClientErrorStatsResponse};
use leptos::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Maximum number of errors to keep in the live feed ring buffer
const LIVE_FEED_MAX_ERRORS: usize = 100;

/// Client-side pagination chunk for the history table.
const ERROR_HISTORY_PAGE_SIZE: usize = 25;

/// Error Monitor page
#[component]
pub fn Errors() -> impl IntoView {
    // Active tab
    let active_tab = RwSignal::new("live");

    view! {
        <PageScaffold
            title="Incidents"
            subtitle="Real-time error monitoring and analysis"
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Observe"),
                PageBreadcrumbItem::current("Errors"),
            ]
        >
            <TabNav
                tabs=vec![
                    ("live", "Live Feed"),
                    ("history", "History"),
                    ("analytics", "Analytics"),
                    ("alerts", "Alerts"),
                    ("crashes", "Crashes"),
                ]
                active=active_tab
            />

            <TabPanel tab="live" active=active_tab>
                <LiveFeedSection/>
            </TabPanel>

            <TabPanel tab="history" active=active_tab>
                <HistorySection/>
            </TabPanel>

            <TabPanel tab="analytics" active=active_tab>
                <AnalyticsSection/>
            </TabPanel>

            <TabPanel tab="alerts" active=active_tab>
                <AlertsSection/>
            </TabPanel>

            <TabPanel tab="crashes" active=active_tab>
                <CrashesSection/>
            </TabPanel>
        </PageScaffold>
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
    // Use try_get/try_update to avoid panics when signals are disposed on unmount
    let (sse_status, _reconnect) =
        use_sse_json::<ClientErrorItem, _>("/v1/stream/client-errors", move |error| {
            if !is_paused.try_get().unwrap_or(true) {
                live_errors.try_update(|errors| {
                    errors.push_front(error);
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
                        {move || format!("{} errors in buffer", live_errors.try_get().unwrap_or_default().len())}
                    </span>
                </div>
                <div class="flex items-center gap-2">
                    <Button
                        variant=if is_paused.try_get().unwrap_or(false) { ButtonVariant::Primary } else { ButtonVariant::Outline }
                        on:click=move |_| is_paused.update(|p| *p = !*p)
                    >
                        {move || if is_paused.try_get().unwrap_or(false) { "Resume" } else { "Pause" }}
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
            {
                let live_vec = Signal::derive(move || {
                    live_errors.try_get().unwrap_or_default().iter().cloned().collect::<Vec<_>>()
                });
                let live_data = loaded_signal(live_vec);

                let columns: Vec<Column<ClientErrorItem>> = vec![
                    Column::custom("Time", |e: &ClientErrorItem| {
                        let ts = format_timestamp(&e.client_timestamp);
                        view! {
                            <span class="text-xs text-muted-foreground font-mono">{ts}</span>
                        }
                    }),
                    Column::custom("Type", |e: &ClientErrorItem| {
                        let variant = match e.error_type.as_str() {
                            "Network" | "Server" | "Panic" => BadgeVariant::Destructive,
                            "Http" | "JsBootError" => BadgeVariant::Warning,
                            "Validation" => BadgeVariant::Secondary,
                            _ => BadgeVariant::Outline,
                        };
                        let label = e.error_type.clone();
                        view! { <Badge variant=variant>{label}</Badge> }
                    }),
                    Column::custom("Message", |e: &ClientErrorItem| {
                        let title = e.message.clone();
                        let display = truncate_message(&e.message, 80);
                        view! {
                            <span class="text-sm truncate max-w-md block" title=title>{display}</span>
                        }
                    }),
                    Column::custom("Status", |e: &ClientErrorItem| {
                        if let Some(status) = e.http_status {
                            let variant = if status >= 500 {
                                BadgeVariant::Destructive
                            } else if status >= 400 {
                                BadgeVariant::Warning
                            } else {
                                BadgeVariant::Secondary
                            };
                            view! { <Badge variant=variant>{status.to_string()}</Badge> }.into_any()
                        } else {
                            view! { <span /> }.into_any()
                        }
                    }),
                    Column::custom("Page", |e: &ClientErrorItem| {
                        let page = e.page.clone().unwrap_or_else(|| "-".to_string());
                        view! {
                            <span class="text-xs text-muted-foreground font-mono">{page}</span>
                        }
                    }),
                ];

                view! {
                    <DataTable
                        data=live_data
                        columns=columns
                        empty_title="No incidents detected"
                        empty_description="Errors will appear here in real-time when they occur"
                    />
                }
            }
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
        "Panic" => BadgeVariant::Destructive,
        "JsBootError" => BadgeVariant::Warning,
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
            let val = error_type_filter.try_get().unwrap_or_default();
            if val.is_empty() {
                None
            } else {
                Some(val)
            }
        };
        let http_status = {
            let val = http_status_filter.try_get().unwrap_or_default();
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
        let _ = error_type_filter.try_get();
        let _ = http_status_filter.try_get();
        refetch.run(());
    });

    view! {
        <div class="space-y-4">
            // Filters
            <div class="flex flex-wrap items-center gap-4">
                <Select
                    value=error_type_filter
                    label="Type".to_string()
                    options=vec![
                        ("".to_string(), "All".to_string()),
                        ("Network".to_string(), "Network".to_string()),
                        ("Http".to_string(), "HTTP".to_string()),
                        ("Server".to_string(), "Server".to_string()),
                        ("Panic".to_string(), "Panic".to_string()),
                        ("JsBootError".to_string(), "JS Boot".to_string()),
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

            // Errors table with client-side pagination
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
                            let error_count = data.errors.len();
                            let total = data.total;
                            let errors_data = data.errors;
                            let visible_count = RwSignal::new(ERROR_HISTORY_PAGE_SIZE);

                            view! {
                                <div>
                                    <div class="px-4 py-2 text-sm text-muted-foreground border-b">
                                        {format!("Showing {} of {} errors", error_count, total)}
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
                                            {move || {
                                                let count = visible_count.try_get().unwrap_or(ERROR_HISTORY_PAGE_SIZE).min(error_count);
                                                errors_data
                                                    .iter()
                                                    .take(count)
                                                    .cloned()
                                                    .map(|error| {
                                                        view! { <ErrorRow error=error/> }
                                                    })
                                                    .collect::<Vec<_>>()
                                            }}
                                        </TableBody>
                                    </Table>

                                    // Show more button
                                    {move || {
                                        let count = visible_count.try_get().unwrap_or(ERROR_HISTORY_PAGE_SIZE);
                                        let remaining = error_count.saturating_sub(count);
                                        (remaining > 0).then(|| view! {
                                            <div class="flex items-center justify-center py-3 border-t">
                                                <button
                                                    class="text-sm text-primary hover:underline"
                                                    on:click=move |_| {
                                                        visible_count.update(|c| *c = (*c + ERROR_HISTORY_PAGE_SIZE).min(error_count));
                                                    }
                                                >
                                                    {format!("Show more ({} remaining)", remaining)}
                                                </button>
                                            </div>
                                        })
                                    }}
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
                        <div class="text-2xl font-bold mt-1">{total_count}</div>
                    </div>
                </Card>
                <Card>
                    <div class="p-4">
                        <div class="text-sm font-medium text-muted-foreground">"Error Types"</div>
                        <div class="text-2xl font-bold mt-1">{error_type_count}</div>
                    </div>
                </Card>
                <Card>
                    <div class="p-4">
                        <div class="text-sm font-medium text-muted-foreground">"HTTP Status Codes"</div>
                        <div class="text-2xl font-bold mt-1">{http_status_count}</div>
                    </div>
                </Card>
            </div>

            // Error type breakdown
            <Card>
                <div class="p-4">
                    <h3 class="heading-4 mb-4">"Errors by Type"</h3>
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
                    <h3 class="heading-4 mb-4">"Errors by HTTP Status"</h3>
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
                    <h3 class="heading-4 mb-4">"Errors per Hour (Last 24h)"</h3>
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
        let unresolved_only = if status_filter.try_get().unwrap_or_default() == "unresolved" {
            Some(true)
        } else {
            None
        };
        async move {
            client
                .list_error_alert_history(unresolved_only, Some(50))
                .await
        }
    });

    Effect::new(move || {
        let _ = status_filter.try_get();
        refetch_history.run(());
    });

    view! {
        <Card>
            <div class="flex items-center justify-between mb-4">
                <div>
                    <h3 class="heading-4">"Triggered Alerts"</h3>
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

            {
                let alert_data: ReadSignal<LoadingState<Vec<ErrorAlertHistoryResponse>>> = {
                    let (read, write) = signal(LoadingState::Loading);
                    Effect::new(move || {
                        let state = history.try_get().unwrap_or(LoadingState::Loading);
                        let mapped = match state {
                            LoadingState::Idle => LoadingState::Idle,
                            LoadingState::Loading => LoadingState::Loading,
                            LoadingState::Loaded(resp) => LoadingState::Loaded(resp.alerts),
                            LoadingState::Error(e) => LoadingState::Error(e),
                        };
                        write.set(mapped);
                    });
                    read
                };

                let columns: Vec<Column<ErrorAlertHistoryResponse>> = vec![
                    Column::custom("Triggered", |a: &ErrorAlertHistoryResponse| {
                        let ts = format_date_time(&a.triggered_at);
                        view! {
                            <span class="text-xs text-muted-foreground font-mono">{ts}</span>
                        }
                    }),
                    Column::custom("Rule", |a: &ErrorAlertHistoryResponse| {
                        let label = a.rule_name.clone()
                            .unwrap_or_else(|| truncate_message(&a.rule_id, 12));
                        let title = a.rule_id.clone();
                        view! {
                            <span class="text-sm font-mono" title=title>{label}</span>
                        }
                    }),
                    Column::custom("Errors", |a: &ErrorAlertHistoryResponse| {
                        let count = a.error_count.to_string();
                        view! {
                            <span class="text-sm font-medium">{count}</span>
                        }
                    }),
                    Column::custom("Status", |a: &ErrorAlertHistoryResponse| {
                        let (label, variant) = if a.resolved_at.is_some() {
                            ("Resolved", BadgeVariant::Success)
                        } else if a.acknowledged_at.is_some() {
                            ("Acknowledged", BadgeVariant::Secondary)
                        } else {
                            ("Active", BadgeVariant::Warning)
                        };
                        view! { <Badge variant=variant>{label}</Badge> }
                    }),
                    Column::custom("Acknowledged", |a: &ErrorAlertHistoryResponse| {
                        let ts = a.acknowledged_at.as_deref()
                            .map(format_date_time)
                            .unwrap_or_else(|| "-".to_string());
                        view! {
                            <span class="text-xs text-muted-foreground">{ts}</span>
                        }
                    }),
                    Column::custom("Resolved", |a: &ErrorAlertHistoryResponse| {
                        let note = a.resolution_note.clone().unwrap_or_default();
                        let ts = a.resolved_at.as_deref()
                            .map(format_date_time)
                            .unwrap_or_else(|| "-".to_string());
                        view! {
                            <span class="text-xs text-muted-foreground" title=note>{ts}</span>
                        }
                    }),
                ];

                view! {
                    <DataTable
                        data=alert_data
                        columns=columns
                        on_retry=Callback::new(move |_| refetch_history.run(()))
                        empty_title="No alerts triggered"
                        empty_description="Alert history will appear here when rules fire."
                        card=false
                    />
                }
            }
        </Card>
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
                        <h3 class="heading-4">"Alert Rules"</h3>
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

    let (workers, refetch_workers) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_workers_with_history().await
    });

    let (crashes, refetch_crashes) = use_api_resource(move |client: Arc<ApiClient>| {
        let worker_id = selected_worker_id.try_get().unwrap_or_default();
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
        let Some(sel) = selected_worker_id.try_get() else {
            return;
        };
        if !sel.is_empty() {
            return;
        }
        if let Some(LoadingState::Loaded(ref list)) = workers.try_get() {
            if let Some(worker) = list
                .iter()
                .find(|w| w.status == "crashed")
                .or_else(|| list.first())
            {
                let _ = selected_worker_id.try_set(worker.id.clone());
            }
        }
    });

    // Refetch crashes when selection changes
    Effect::new(move || {
        let Some(id) = selected_worker_id.try_get() else {
            return;
        };
        if !id.is_empty() {
            refetch_crashes.run(());
        }
    });

    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-between">
                <div>
                    <h3 class="heading-4">"Crash Dumps"</h3>
                    <p class="text-sm text-muted-foreground">
                        "Worker crash reports and recovery details"
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    {move || {
                        match workers.try_get().unwrap_or(LoadingState::Loading) {
                            LoadingState::Loaded(list) if !list.is_empty() => {
                                let options: Vec<(String, String)> = list
                                    .iter()
                                    .map(|worker| {
                                        let name = worker.display_name.clone().unwrap_or_else(|| adapteros_id::short_id(&worker.id));
                                        let label = format!("{} ({})", name, humanize(&worker.status));
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

            {move || match workers.try_get().unwrap_or(LoadingState::Loading) {
                LoadingState::Idle | LoadingState::Loading => {
                    view! { <SkeletonTable rows=5 columns=5 /> }.into_any()
                }
                LoadingState::Error(e) => {
                    view! {
                        <Card>
                            <div class="p-6 text-center space-y-2">
                                <p class="text-sm text-muted-foreground">
                                    "Could not load worker list. Crash data requires a connected worker."
                                </p>
                                <p class="text-xs text-muted-foreground">{e.user_message()}</p>
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| refetch_workers.run(()))
                                >
                                    "Retry"
                                </Button>
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
                        let columns: Vec<Column<ProcessCrashDumpResponse>> = vec![
                            Column::custom("Time", |c: &ProcessCrashDumpResponse| {
                                let ts = format_date_time(&c.crash_timestamp);
                                view! {
                                    <span class="text-xs text-muted-foreground font-mono">{ts}</span>
                                }
                            }),
                            Column::custom("Worker", |c: &ProcessCrashDumpResponse| {
                                let title = c.worker_id.clone();
                                let label = adapteros_id::short_id(&c.worker_id);
                                view! {
                                    <span class="text-xs font-mono" title=title>{label}</span>
                                }
                            }),
                            Column::custom("Type", |c: &ProcessCrashDumpResponse| {
                                let crash_type = c.crash_type.clone();
                                view! {
                                    <span class="text-sm font-medium">{crash_type.to_uppercase()}</span>
                                }
                            }),
                            Column::custom("Recovery", |c: &ProcessCrashDumpResponse| {
                                let action = c.recovery_action.clone()
                                    .unwrap_or_else(|| "\u{2014}".to_string());
                                let recovered = c.recovered_at.as_deref()
                                    .map(format_date_time)
                                    .unwrap_or_else(|| "-".to_string());
                                view! {
                                    <div class="space-y-1">
                                        <span class="text-sm">{humanize(&action)}</span>
                                        <span class="text-xs text-muted-foreground">
                                            {"Recovered: "}{recovered}
                                        </span>
                                    </div>
                                }
                            }),
                            Column::custom("Details", |c: &ProcessCrashDumpResponse| {
                                let stack = c.stack_trace.clone()
                                    .unwrap_or_else(|| "No stack trace".to_string());
                                let preview = truncate_message(&stack, 80);
                                view! {
                                    <span class="text-xs text-muted-foreground" title=stack>{preview}</span>
                                }
                            }),
                        ];

                        let empty_title = if selected_worker_id.try_get().unwrap_or_default().is_empty() {
                            "Select a worker"
                        } else {
                            "No crash dumps"
                        };
                        let empty_desc = if selected_worker_id.try_get().unwrap_or_default().is_empty() {
                            "Choose a worker to view crash dumps."
                        } else {
                            "No crash data recorded for this worker."
                        };

                        view! {
                            <DataTable
                                data=crashes
                                columns=columns
                                on_retry=Callback::new(move |_| refetch_crashes.run(()))
                                empty_title=empty_title
                                empty_description=empty_desc
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
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
    let alive = use_scope_alive();
    let rule_id_for_toggle = rule.id.clone();
    let rule_id_for_delete = rule.id.clone();
    let is_active = rule.is_active;

    let (toggling, set_toggling) = signal(false);
    let (deleting, set_deleting) = signal(false);
    let show_delete_confirm = RwSignal::new(false);

    // Toggle active state
    let alive_for_toggle = alive.clone();
    let on_toggle = move |_| {
        let id = rule_id_for_toggle.clone();
        let alive = alive_for_toggle.clone();
        set_toggling.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let request = UpdateErrorAlertRuleRequest {
                is_active: Some(!is_active),
                ..Default::default()
            };
            match client.update_error_alert_rule(&id, &request).await {
                Ok(_) => {
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        on_update.run(());
                    }
                }
                Err(e) => {
                    report_error_with_toast(
                        &e,
                        "Failed to toggle alert rule",
                        Some("/errors"),
                        true,
                    );
                }
            }
            set_toggling.set(false);
        });
    };

    // Delete rule (called after confirmation)
    let on_confirm_delete = {
        let rule_id = rule_id_for_delete.clone();
        let alive = alive.clone();
        Callback::new(move |_| {
            let id = rule_id.clone();
            let alive = alive.clone();
            set_deleting.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::new();
                match client.delete_error_alert_rule(&id).await {
                    Ok(_) => {
                        if alive.load(std::sync::atomic::Ordering::SeqCst) {
                            show_delete_confirm.set(false);
                            on_update.run(());
                        }
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to delete alert rule",
                            Some("/errors"),
                            true,
                        );
                    }
                }
                set_deleting.set(false);
            });
        })
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

    let rule_name_for_dialog = rule.name.clone();

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
                        disabled=Signal::derive(move || toggling.try_get().unwrap_or(false))
                    >
                        {move || if toggling.try_get().unwrap_or(false) { "..." } else if is_active { "Disable" } else { "Enable" }}
                    </Button>
                    <Button
                        variant=ButtonVariant::Ghost
                        size=ButtonSize::Sm
                        on_click=Callback::new(move |_| show_delete_confirm.set(true))
                        disabled=Signal::derive(move || deleting.try_get().unwrap_or(false))
                    >
                        {move || if deleting.try_get().unwrap_or(false) { "..." } else { "Delete" }}
                    </Button>
                </div>
            </TableCell>
        </TableRow>

        <ConfirmationDialog
            open=show_delete_confirm
            title="Delete Alert Rule"
            description=format!(
                "Are you sure you want to delete the alert rule '{}'? This action cannot be undone.",
                rule_name_for_dialog
            )
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            on_confirm=on_confirm_delete
            loading=Signal::derive(move || deleting.try_get().unwrap_or(false))
        />
    }
}

/// Create alert rule dialog
#[component]
fn CreateAlertRuleDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let alive = use_scope_alive();
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let error_pattern = RwSignal::new(String::new());
    let threshold_count = RwSignal::new("5".to_string());
    let threshold_window = RwSignal::new("5".to_string());
    let severity = RwSignal::new("warning".to_string());
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |_| {
        let name_val = name.try_get().unwrap_or_default();
        if name_val.trim().is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        let threshold_count_val = threshold_count
            .try_get()
            .unwrap_or_default()
            .parse::<i32>()
            .unwrap_or(5)
            .max(1);
        let threshold_window_val = threshold_window
            .try_get()
            .unwrap_or_default()
            .parse::<i32>()
            .unwrap_or(5)
            .max(1);

        submitting.set(true);
        error.set(None);

        let request = CreateErrorAlertRuleRequest {
            name: name_val,
            description: {
                let val = description.try_get().unwrap_or_default();
                if val.is_empty() {
                    None
                } else {
                    Some(val)
                }
            },
            error_type_pattern: {
                let val = error_pattern.try_get().unwrap_or_default();
                if val.is_empty() {
                    None
                } else {
                    Some(val)
                }
            },
            http_status_pattern: None,
            page_pattern: None,
            threshold_count: threshold_count_val,
            threshold_window_minutes: threshold_window_val,
            cooldown_minutes: 15,
            severity: severity.try_get().unwrap_or_else(|| "warning".to_string()),
            notification_channels: None,
        };

        let alive = alive.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.create_error_alert_rule(&request).await {
                Ok(_) => {
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        on_created.run(());
                    }
                }
                Err(e) => {
                    error.set(Some(e.user_message()));
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
            {move || error.try_get().flatten().map(|e| view! {
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

                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
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
                    disabled=Signal::derive(move || submitting.try_get().unwrap_or(false))
                >
                    {move || if submitting.try_get().unwrap_or(false) { "Creating..." } else { "Create Rule" }}
                </Button>
            </div>
        </Dialog>
    }
}

/// SSE connection indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2">
            {move || {
                let (color, label) = match state.try_get().unwrap_or(SseState::Disconnected) {
                    SseState::Connected => ("bg-status-success", "Connected"),
                    SseState::Connecting => ("bg-status-warning animate-pulse", "Connecting..."),
                    SseState::Disconnected => ("bg-muted", "Disconnected"),
                    SseState::Error => ("bg-status-error", "Error"),
                    SseState::CircuitOpen => ("bg-status-warning", "Circuit Open"),
                };
                view! {
                    <div class=format!("w-2 h-2 rounded-full {}", color)/>
                    <span class="text-xs text-muted-foreground">{label}</span>
                }
            }}
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
