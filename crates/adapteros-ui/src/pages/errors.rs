//! Error Monitor page
//!
//! Real-time error monitoring with live feed, history, analytics, and alerts.
//! Uses SSE for real-time error streaming via `/v1/stream/client-errors`.

use crate::api::{
    use_sse_json, ApiClient, CreateErrorAlertRuleRequest, ErrorAlertRuleResponse, SseState,
    UpdateErrorAlertRuleRequest,
};
use crate::components::{
    AsyncBoundary, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Dialog, Input,
    Select, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::use_api_resource;
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
                            refetch.run(());
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
                            refetch.run(());
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
    }
}

/// Alert rules list component
#[component]
fn AlertRulesList(
    rules: Vec<ErrorAlertRuleResponse>,
    on_update: Callback<()>,
) -> impl IntoView {
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
fn CreateAlertRuleDialog(on_close: Callback<()>, on_created: Callback<()>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let error_pattern = RwSignal::new(String::new());
    let threshold_count = RwSignal::new(5);
    let threshold_window = RwSignal::new(5);
    let severity = RwSignal::new("warning".to_string());
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_submit = move |_| {
        let name_val = name.get();
        if name_val.trim().is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

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
            threshold_count: threshold_count.get(),
            threshold_window_minutes: threshold_window.get(),
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
        <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
            <Card class="w-full max-w-lg mx-4">
                <div class="p-6 space-y-4">
                    <div class="flex items-center justify-between">
                        <h2 class="text-lg font-semibold">"Create Alert Rule"</h2>
                        <button
                            type="button"
                            class="text-muted-foreground hover:text-foreground"
                            on:click=move |_| on_close.run(())
                        >
                            "✕"
                        </button>
                    </div>

                    {move || error.get().map(|e| view! {
                        <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                            {e}
                        </div>
                    })}

                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium mb-1">"Name"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 border rounded-md bg-background"
                                placeholder="High error rate alert"
                                prop:value=move || name.get()
                                on:input=move |ev| set_name.set(event_target_value(&ev))
                            />
                        </div>

                        <div>
                            <label class="block text-sm font-medium mb-1">"Description"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 border rounded-md bg-background"
                                placeholder="Optional description"
                                prop:value=move || description.get()
                                on:input=move |ev| set_description.set(event_target_value(&ev))
                            />
                        </div>

                        <div>
                            <label class="block text-sm font-medium mb-1">"Error Type Pattern"</label>
                            <input
                                type="text"
                                class="w-full px-3 py-2 border rounded-md bg-background"
                                placeholder="e.g., NetworkError, *Timeout* (optional)"
                                prop:value=move || error_pattern.get()
                                on:input=move |ev| set_error_pattern.set(event_target_value(&ev))
                            />
                            <p class="text-xs text-muted-foreground mt-1">
                                "Leave empty to match all error types"
                            </p>
                        </div>

                        <div class="grid grid-cols-2 gap-4">
                            <div>
                                <label class="block text-sm font-medium mb-1">"Threshold Count"</label>
                                <input
                                    type="number"
                                    min="1"
                                    class="w-full px-3 py-2 border rounded-md bg-background"
                                    prop:value=move || threshold_count.get()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                            set_threshold_count.set(v.max(1));
                                        }
                                    }
                                />
                            </div>
                            <div>
                                <label class="block text-sm font-medium mb-1">"Window (minutes)"</label>
                                <input
                                    type="number"
                                    min="1"
                                    class="w-full px-3 py-2 border rounded-md bg-background"
                                    prop:value=move || threshold_window.get()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                            set_threshold_window.set(v.max(1));
                                        }
                                    }
                                />
                            </div>
                        </div>

                        <div>
                            <label class="block text-sm font-medium mb-1">"Severity"</label>
                            <select
                                class="w-full px-3 py-2 border rounded-md bg-background"
                                on:change=move |ev| set_severity.set(event_target_value(&ev))
                            >
                                <option value="info" selected=move || severity.get() == "info">"Info"</option>
                                <option value="warning" selected=move || severity.get() == "warning">"Warning"</option>
                                <option value="critical" selected=move || severity.get() == "critical">"Critical"</option>
                            </select>
                        </div>
                    </div>

                    <div class="flex justify-end gap-2 pt-4">
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(move |_| on_close.run(()))
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
                </div>
            </Card>
        </div>
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
