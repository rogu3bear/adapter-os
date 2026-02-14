//! Routing Decisions page
//!
//! View and debug K-sparse routing decisions with filtering and chain visualization.

use crate::api::{
    AdapterScoreResponse, ApiClient, RoutingCandidateResponse, RoutingDebugRequest,
    RoutingDebugResponse, RoutingDecisionResponse, RoutingDecisionsQuery, RoutingDecisionsResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Checkbox, CopyableId, EmptyState,
    EmptyStateVariant, ErrorDisplay, Input, Spinner, SplitPanel, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow, Textarea,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::utils::format_datetime;
use leptos::prelude::*;
use std::sync::Arc;

/// Routing Decisions page with list view and detail panel
#[component]
pub fn RoutingDecisions() -> impl IntoView {
    // Selected decision ID for detail panel
    let selected_decision_id = RwSignal::new(None::<String>);

    // Filter state
    let filter_anomalies = RwSignal::new(false);
    let filter_stack = RwSignal::new(String::new());

    // Debug panel state
    let show_debug_panel = RwSignal::new(false);

    // Build query from filters
    let query = Signal::derive(move || {
        let mut q = RoutingDecisionsQuery::default();
        if filter_anomalies.try_get().unwrap_or(false) {
            q.anomalies_only = Some(true);
        }
        let stack = filter_stack.try_get().unwrap_or_default();
        if !stack.is_empty() {
            q.stack_id = Some(stack);
        }
        q.limit = Some(100);
        q
    });

    // Fetch routing decisions
    let (decisions, refetch_decisions) = use_api_resource(move |client: Arc<ApiClient>| {
        let q = query.try_get().unwrap_or_default();
        async move { client.get_routing_decisions(&q).await }
    });

    let on_decision_select = move |decision_id: String| {
        selected_decision_id.set(Some(decision_id));
    };

    let on_close_detail = move || {
        selected_decision_id.set(None);
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_decision_id.try_get().flatten().is_some());

    view! {
        <div class="space-y-6">
            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Decisions"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Header
                            <div class="flex items-center justify-between">
                                <div>
                                    <h2 class="heading-2">"Routing Decisions"</h2>
                                    <p class="text-muted-foreground mt-1">
                                        "K-sparse adapter routing decisions and debugging"
                                    </p>
                                </div>
                                <div class="flex items-center gap-2">
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=Callback::new(move |_| show_debug_panel.update(|v| *v = !*v))
                                    >
                                        {move || if show_debug_panel.try_get().unwrap_or(false) { "Hide Debug" } else { "Debug Router" }}
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=refetch_decisions.as_callback()
                                    >
                                        "Refresh"
                                    </Button>
                                </div>
                            </div>

                            // Debug panel (collapsible)
                            {move || {
                                if show_debug_panel.try_get().unwrap_or(false) {
                                    Some(view! {
                                        <DebugPanel />
                                    })
                                } else {
                                    None
                                }
                            }}

                            // Filters
                            <FilterBar
                                filter_anomalies=filter_anomalies
                                filter_stack=filter_stack
                                on_filter_change=refetch_decisions.as_callback()
                            />

                            // Summary stats
                            <SummaryStats decisions=decisions />

                            // Decisions list
                            {move || {
                                match decisions.try_get().unwrap_or(LoadingState::Idle) {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <div class="flex items-center justify-center py-12">
                                                <Spinner/>
                                            </div>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(data) => {
                                        view! {
                                            <DecisionsList
                                                decisions=data
                                                selected_id=selected_decision_id
                                                on_select=on_decision_select
                                            />
                                        }.into_any()
                                    }
                                    LoadingState::Error(e) => {
                                        view! {
                                            <ErrorDisplay
                                                error=e
                                                on_retry=refetch_decisions.as_callback()
                                            />
                                        }.into_any()
                                    }
                                }
                            }}
                        </div>
                    }
                }
                detail_panel=move || {
                    let decision_id = selected_decision_id.try_get().flatten().unwrap_or_default();
                    view! {
                        <DecisionDetail
                            decision_id=decision_id
                            on_close=on_close_detail
                        />
                    }
                }
            />
        </div>
    }
}

/// Filter bar component
#[component]
fn FilterBar(
    filter_anomalies: RwSignal<bool>,
    filter_stack: RwSignal<String>,
    on_filter_change: Callback<()>,
) -> impl IntoView {
    // Watch for filter_stack changes and trigger refetch
    Effect::new(move || {
        let _ = filter_stack.try_get();
        on_filter_change.run(());
    });

    view! {
        <Card>
            <div class="flex items-center gap-4 flex-wrap">
                // Anomalies only toggle
                <Checkbox
                    checked=filter_anomalies
                    on_change=Callback::new(move |checked| {
                        filter_anomalies.set(checked);
                        on_filter_change.run(());
                    })
                    label="High Entropy Only"
                />

                // Stack filter
                <div class="flex items-center gap-2">
                    <Input
                        value=filter_stack
                        placeholder="Filter by stack ID".to_string()
                        class="h-8 w-48".to_string()
                    />
                </div>

                // Clear filters
                <Button
                    variant=ButtonVariant::Ghost
                    size=ButtonSize::Sm
                    on_click=Callback::new(move |_| {
                        filter_anomalies.set(false);
                        filter_stack.set(String::new());
                        on_filter_change.run(());
                    })
                >
                    "Clear Filters"
                </Button>
            </div>
        </Card>
    }
}

/// Summary stats row
#[component]
fn SummaryStats(decisions: ReadSignal<LoadingState<RoutingDecisionsResponse>>) -> impl IntoView {
    let total = Signal::derive(
        move || match decisions.try_get().unwrap_or(LoadingState::Idle) {
            LoadingState::Loaded(ref d) => d.total,
            _ => 0,
        },
    );

    let avg_entropy =
        Signal::derive(
            move || match decisions.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(ref d) if !d.decisions.is_empty() => {
                    let sum: f64 = d.decisions.iter().map(|x| x.entropy).sum();
                    sum / d.decisions.len() as f64
                }
                _ => 0.0,
            },
        );

    let avg_k = Signal::derive(
        move || match decisions.try_get().unwrap_or(LoadingState::Idle) {
            LoadingState::Loaded(ref d) if !d.decisions.is_empty() => {
                let sum: i32 = d.decisions.iter().map(|x| x.k_value).sum();
                sum as f64 / d.decisions.len() as f64
            }
            _ => 0.0,
        },
    );

    let high_entropy_count =
        Signal::derive(
            move || match decisions.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(ref d) => {
                    d.decisions.iter().filter(|x| x.entropy > 1.5).count()
                }
                _ => 0,
            },
        );

    let total_str = Signal::derive(move || total.try_get().unwrap_or(0).to_string());
    let entropy_str =
        Signal::derive(move || format!("{:.3}", avg_entropy.try_get().unwrap_or(0.0)));
    let k_str = Signal::derive(move || format!("{:.1}", avg_k.try_get().unwrap_or(0.0)));
    let high_entropy_str =
        Signal::derive(move || high_entropy_count.try_get().unwrap_or(0).to_string());

    view! {
        <div class="grid gap-4 md:grid-cols-4">
            <StatCard label="Total Decisions" value=total_str />
            <StatCard label="Avg Entropy" value=entropy_str />
            <StatCard label="Avg K-Value" value=k_str />
            <StatCard
                label="High Entropy"
                value=high_entropy_str
                variant=BadgeVariant::Warning
            />
        </div>
    }
}

/// Stat card component
#[component]
fn StatCard(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(default = BadgeVariant::Secondary)] variant: BadgeVariant,
) -> impl IntoView {
    view! {
        <Card>
            <div class="flex items-center justify-between">
                <span class="text-sm font-medium text-muted-foreground">{label}</span>
                <Badge variant=variant>{move || value.try_get().unwrap_or_default()}</Badge>
            </div>
        </Card>
    }
}

/// Decisions list table
#[component]
fn DecisionsList(
    decisions: RoutingDecisionsResponse,
    selected_id: RwSignal<Option<String>>,
    on_select: impl Fn(String) + Copy + Send + 'static,
) -> impl IntoView {
    if decisions.decisions.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    title="No routing decisions found"
                    description="Run some inference requests to see routing decisions."
                    variant=EmptyStateVariant::Empty
                />
            </Card>
        }
        .into_any();
    }

    // Extract values before view! macro to avoid borrow/move issues
    let shown_count = decisions.decisions.len();
    let total_count = decisions.total;
    let decision_rows: Vec<_> = decisions.decisions.into_iter().map(|decision| {
        let decision_id = decision.id.clone();
        let decision_id_for_click = decision_id.clone();

        // Determine entropy severity
        let entropy_variant = if decision.entropy > 2.0 {
            BadgeVariant::Destructive
        } else if decision.entropy > 1.5 {
            BadgeVariant::Warning
        } else {
            BadgeVariant::Success
        };

        view! {
            <tr
                class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&decision_id)
                on:click=move |_| on_select(decision_id_for_click.clone())
            >
                <TableCell>
                    <span class="text-sm font-mono">{format_timestamp(&decision.timestamp)}</span>
                </TableCell>
                <TableCell>
                    <span class="text-sm font-mono">
                        {decision.stack_id.clone().map(|s| truncate(&s, 12)).unwrap_or_else(|| "-".to_string())}
                    </span>
                </TableCell>
                <TableCell>
                    <Badge variant=entropy_variant>
                        {format!("{:.3}", decision.entropy)}
                    </Badge>
                </TableCell>
                <TableCell>
                    <span class="text-sm font-medium">{decision.k_value.to_string()}</span>
                </TableCell>
                <TableCell>
                    <span class="text-sm text-muted-foreground">
                        {decision.total_inference_latency_us.map(|l| format!("{:.1}ms", l as f64 / 1000.0)).unwrap_or_else(|| "-".to_string())}
                    </span>
                </TableCell>
                <TableCell>
                    <span class="text-sm text-muted-foreground">
                        {decision.overhead_pct.map(|o| format!("{:.1}%", o)).unwrap_or_else(|| "-".to_string())}
                    </span>
                </TableCell>
            </tr>
        }
    }).collect();

    view! {
        <Card>
            <div class="overflow-x-auto">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>"Timestamp"</TableHead>
                            <TableHead>"Stack ID"</TableHead>
                            <TableHead>"Entropy"</TableHead>
                            <TableHead>"K-Value"</TableHead>
                            <TableHead>"Latency"</TableHead>
                            <TableHead>"Overhead"</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {decision_rows}
                    </TableBody>
                </Table>
            </div>

            // Pagination info
            <div class="flex items-center justify-between mt-4 text-sm text-muted-foreground">
                <span>
                    "Showing "{shown_count}" of "{total_count}" decisions"
                </span>
            </div>
        </Card>
    }
    .into_any()
}

/// Decision detail panel
#[component]
fn DecisionDetail(decision_id: String, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let decision_id_for_fetch = decision_id.clone();

    // Fetch decision detail
    let (decision, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = decision_id_for_fetch.clone();
        async move { client.get_routing_decision(&id).await }
    });

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"Decision Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            {move || {
                match decision.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <DecisionDetailContent decision=data/>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=refetch.as_callback()
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Decision detail content
#[component]
fn DecisionDetailContent(decision: RoutingDecisionResponse) -> impl IntoView {
    let entropy_variant = if decision.entropy > 2.0 {
        BadgeVariant::Destructive
    } else if decision.entropy > 1.5 {
        BadgeVariant::Warning
    } else {
        BadgeVariant::Success
    };

    view! {
        // Overview
        <Card title="Overview".to_string()>
            <div class="grid gap-3 text-sm">
                <CopyableId id=decision.id.clone() label="Decision ID".to_string() truncate=24 />
                <CopyableId id=decision.request_id.clone() label="Request ID".to_string() truncate=24 />
                <CopyableId id=decision.tenant_id.clone() label="Tenant".to_string() truncate=16 />
                {decision.stack_id.clone().map(|s| view! {
                    <CopyableId id=s label="Stack ID".to_string() truncate=16 />
                })}
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Timestamp"</span>
                    <span>{format_datetime(&decision.timestamp)}</span>
                </div>
            </div>
        </Card>

        // Routing Metrics
        <Card title="Routing Metrics".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <div class="flex justify-between items-center">
                    <span class="text-muted-foreground">"Entropy"</span>
                    <Badge variant=entropy_variant>{format!("{:.4}", decision.entropy)}</Badge>
                </div>
                <div class="flex justify-between items-center">
                    <span class="text-muted-foreground">"K-Value"</span>
                    <Badge variant=BadgeVariant::Default>{decision.k_value.to_string()}</Badge>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Tau (τ)"</span>
                    <span class="font-mono">{format!("{:.4}", decision.tau)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Step"</span>
                    <span>{decision.step.to_string()}</span>
                </div>
                {decision.overhead_pct.map(|o| view! {
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Overhead"</span>
                        <span class="font-mono">{format!("{:.2}%", o)}</span>
                    </div>
                })}
                {decision.total_inference_latency_us.map(|l| view! {
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Latency"</span>
                        <span class="font-mono">{format!("{:.2}ms", l as f64 / 1000.0)}</span>
                    </div>
                })}
            </div>
        </Card>

        // Selected Adapters
        <Card title="Selected Adapters".to_string() class="mt-4".to_string()>
            <div class="space-y-2">
                {if decision.selected_adapter_ids.is_empty() {
                    view! {
                        <p class="text-sm text-muted-foreground">"No adapters selected"</p>
                    }.into_any()
                } else {
                    let adapter_badges: Vec<_> = decision.selected_adapter_ids.iter()
                        .map(|id| {
                            let truncated = truncate(id, 16);
                            view! {
                                <Badge variant=BadgeVariant::Success>
                                    {truncated}
                                </Badge>
                            }
                        })
                        .collect();
                    view! {
                        <div class="flex flex-wrap gap-2">
                            {adapter_badges}
                        </div>
                    }.into_any()
                }}
            </div>
        </Card>

        // Candidates
        {if !decision.candidates.is_empty() {
            let candidate_rows: Vec<_> = decision.candidates.iter()
                .map(|c| view! { <CandidateRow candidate=c.clone()/> })
                .collect();
            Some(view! {
                <Card title="Candidates".to_string() class="mt-4".to_string()>
                    <div class="space-y-2">
                        {candidate_rows}
                    </div>
                </Card>
            })
        } else {
            None
        }}
    }
}

/// Candidate row
#[component]
fn CandidateRow(candidate: RoutingCandidateResponse) -> impl IntoView {
    let status_variant = if candidate.selected {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };

    view! {
        <div class="flex items-center justify-between p-2 rounded-lg border border-border">
            <div class="flex items-center gap-2">
                <span class="text-sm font-medium">"#"{candidate.rank.to_string()}</span>
                <span class="text-sm font-mono">{truncate(&candidate.adapter_id, 20)}</span>
            </div>
            <div class="flex items-center gap-2">
                <span class="text-sm text-muted-foreground">
                    "gate: "{format!("{:.4}", candidate.gate_value)}
                </span>
                <Badge variant=status_variant>
                    {if candidate.selected { "Selected" } else { "Candidate" }}
                </Badge>
            </div>
        </div>
    }
}

/// Debug panel for testing routing
#[component]
fn DebugPanel() -> impl IntoView {
    let prompt = RwSignal::new(String::new());
    let context = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let result = RwSignal::new(None::<RoutingDebugResponse>);
    let error = RwSignal::new(None::<String>);

    let on_debug = move |_| {
        let prompt_val = prompt.try_get().unwrap_or_default();
        if prompt_val.is_empty() {
            return;
        }

        loading.set(true);
        error.set(None);
        result.set(None);

        let context_val = context.try_get().unwrap_or_default();
        let request = RoutingDebugRequest {
            prompt: prompt_val,
            context: if context_val.is_empty() {
                None
            } else {
                Some(context_val)
            },
            stack_id: None,
        };

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.debug_routing(&request).await {
                Ok(response) => {
                    result.set(Some(response));
                }
                Err(e) => {
                    error.set(Some(e.user_message()));
                }
            }
            loading.set(false);
        });
    };

    view! {
        <Card title="Debug Router".to_string()>
            <div class="space-y-4">
                // Prompt input
                <Textarea
                    value=prompt
                    label="Test Prompt".to_string()
                    placeholder="Enter a prompt to test routing...".to_string()
                    rows=4
                />

                // Context input
                <Input
                    value=context
                    label="Context (optional)".to_string()
                    placeholder="Additional context...".to_string()
                />

                // Submit button
                {move || {
                    let is_disabled = prompt.try_get().unwrap_or_default().is_empty() || loading.try_get().unwrap_or(false);
                    let is_loading = loading.try_get().unwrap_or(false);
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            loading=is_loading
                            disabled=is_disabled
                            on_click=Callback::new(on_debug)
                        >
                            "Debug Routing"
                        </Button>
                    }
                }}

                // Error display
                {move || error.try_get().flatten().map(|e| view! {
                    <div class="rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Result display
                {move || result.try_get().flatten().map(|r| view! {
                    <DebugResult response=r/>
                })}
            </div>
        </Card>
    }
}

/// Debug result display
#[component]
fn DebugResult(response: RoutingDebugResponse) -> impl IntoView {
    let entropy_variant = if response.entropy > 2.0 {
        BadgeVariant::Destructive
    } else if response.entropy > 1.5 {
        BadgeVariant::Warning
    } else {
        BadgeVariant::Success
    };

    // Extract values before view! macro to avoid lifetime issues
    let entropy_str = format!("{:.4}", response.entropy);
    let k_value_str = response.k_value.to_string();
    let lang_badge = response.detected_features.language.clone();
    let domain_badge = response.detected_features.domain.clone();
    let verb_badge = response.detected_features.verb.clone();
    let frameworks: Vec<_> = response
        .detected_features
        .frameworks
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|fw| view! { <Badge variant=BadgeVariant::Outline>{fw}</Badge> })
        .collect();
    let selected_badges: Vec<_> = response
        .selected_adapters
        .iter()
        .map(|a| {
            let truncated = truncate(a, 16);
            view! { <Badge variant=BadgeVariant::Success>{truncated}</Badge> }
        })
        .collect();
    let score_rows: Vec<_> = response
        .adapter_scores
        .iter()
        .map(|s| view! { <AdapterScoreRow score=s.clone()/> })
        .collect();
    let explanation = response.explanation.clone();

    view! {
        <div class="space-y-4 border-t border-border pt-4">
            // Metrics
            <div class="flex items-center gap-4">
                <div class="flex items-center gap-2">
                    <span class="text-sm text-muted-foreground">"Entropy:"</span>
                    <Badge variant=entropy_variant>{entropy_str}</Badge>
                </div>
                <div class="flex items-center gap-2">
                    <span class="text-sm text-muted-foreground">"K-Value:"</span>
                    <Badge variant=BadgeVariant::Secondary>{k_value_str}</Badge>
                </div>
            </div>

            // Detected features
            <div>
                <h4 class="text-sm font-medium mb-2">"Detected Features"</h4>
                <div class="flex flex-wrap gap-2">
                    {lang_badge.map(|l| view! {
                        <Badge variant=BadgeVariant::Outline>{"lang: "}{l}</Badge>
                    })}
                    {domain_badge.map(|d| view! {
                        <Badge variant=BadgeVariant::Outline>{"domain: "}{d}</Badge>
                    })}
                    {verb_badge.map(|v| view! {
                        <Badge variant=BadgeVariant::Outline>{"verb: "}{v}</Badge>
                    })}
                    {frameworks}
                </div>
            </div>

            // Selected adapters
            <div>
                <h4 class="text-sm font-medium mb-2">"Selected Adapters"</h4>
                <div class="flex flex-wrap gap-2">
                    {selected_badges}
                </div>
            </div>

            // Adapter scores
            <div>
                <h4 class="text-sm font-medium mb-2">"Adapter Scores"</h4>
                <div class="space-y-2">
                    {score_rows}
                </div>
            </div>

            // Explanation
            <div>
                <h4 class="text-sm font-medium mb-2">"Explanation"</h4>
                <p class="text-sm text-muted-foreground">{explanation}</p>
            </div>
        </div>
    }
}

/// Adapter score row
#[component]
fn AdapterScoreRow(score: AdapterScoreResponse) -> impl IntoView {
    let status_variant = if score.selected {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };

    view! {
        <div class="flex items-center justify-between p-2 rounded-lg border border-border">
            <div class="flex items-center gap-2">
                <span class="text-sm font-mono">{truncate(&score.adapter_id, 16)}</span>
                {score.reason.clone().map(|r| view! {
                    <span class="text-xs text-muted-foreground">"("{r}")"</span>
                })}
            </div>
            <div class="flex items-center gap-3">
                <span class="text-xs text-muted-foreground">
                    "score: "{format!("{:.3}", score.score)}
                </span>
                <span class="text-xs text-muted-foreground">
                    "gate: "{format!("{:.4}", score.gate_value)}
                </span>
                <Badge variant=status_variant>
                    {if score.selected { "Selected" } else { "Not selected" }}
                </Badge>
            </div>
        </div>
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Format a timestamp for display
fn format_timestamp(ts: &str) -> String {
    if ts.len() >= 19 {
        // Extract time portion: HH:MM:SS
        ts[11..19].to_string()
    } else {
        ts.to_string()
    }
}

/// Truncate a string with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
