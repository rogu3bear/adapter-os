//! Trace Viewer Component
//!
//! Visualizes inference traces with timeline, token-level breakdown,
//! and latency metrics for debugging and analysis.

use leptos::prelude::*;

use crate::api::{
    ApiClient, InferenceTraceResponse, TimingBreakdown, TokenDecision,
    UiInferenceTraceDetailResponse, UiTraceReceiptSummary,
};
use crate::components::async_state::ErrorDisplay;
use crate::components::Spinner;
use crate::constants::pagination::{TOKEN_DECISIONS_DOM_CAP, TOKEN_DECISIONS_PAGE_SIZE};
use crate::hooks::LoadingState;
use crate::signals::{perf_logging_enabled, try_use_notifications};
use leptos::task::spawn_local;
use std::time::Instant;

/// State for the trace viewer
#[derive(Clone, Debug)]
pub enum TraceViewState {
    /// Initial state, no trace selected
    Empty,
    /// Loading trace data
    Loading,
    /// Loaded trace summary list
    List(Vec<InferenceTraceResponse>),
    /// Loaded detailed trace
    Detail(Box<UiInferenceTraceDetailResponse>),
    /// Error loading trace
    Error(String),
}

/// Trace viewer component for visualizing inference traces
#[component]
pub fn TraceViewer(
    #[prop(optional)] request_id: Option<String>,
    #[prop(optional)] trace_id: Option<String>,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let (state, set_state) = signal(TraceViewState::Empty);
    let (selected_trace_id, set_selected_trace_id) = signal::<Option<String>>(trace_id.clone());
    let (expanded_tokens, set_expanded_tokens) = signal(false);

    // Clone for initial load check
    let initial_trace_id = trace_id.clone();
    let initial_request_id = request_id.clone();

    // Load traces on mount or when request_id/trace_id changes
    let api = ApiClient::new();

    // Effect to load trace list or detail
    Effect::new(move |_prev| {
        let api = api.clone();
        let request_id = initial_request_id.clone();
        let has_initial_trace = initial_trace_id.is_some();
        let selected = selected_trace_id.get();

        wasm_bindgen_futures::spawn_local(async move {
            if let Some(tid) = selected {
                // Load detailed trace
                set_state.set(TraceViewState::Loading);
                match api
                    .get_inference_trace_detail(&tid, Some(TOKEN_DECISIONS_PAGE_SIZE), None)
                    .await
                {
                    Ok(detail) => set_state.set(TraceViewState::Detail(Box::new(detail))),
                    Err(e) => set_state.set(TraceViewState::Error(e.to_string())),
                }
            } else if request_id.is_some() || has_initial_trace {
                // Load trace list
                set_state.set(TraceViewState::Loading);
                match api
                    .list_inference_traces(request_id.as_deref(), Some(20))
                    .await
                {
                    Ok(traces) => set_state.set(TraceViewState::List(traces)),
                    Err(e) => set_state.set(TraceViewState::Error(e.to_string())),
                }
            }
        });
    });

    let container_class = if compact {
        "bg-card border border-border rounded-lg p-3 text-sm"
    } else {
        "bg-card border border-border rounded-lg p-6"
    };

    view! {
        <div class=container_class data-testid="receipt-verification">
            {move || match state.get() {
                TraceViewState::Empty => view! {
                    <div class="text-muted-foreground text-center py-8">
                        <svg class="w-12 h-12 mx-auto mb-3 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/>
                        </svg>
                        <p>"No trace data available"</p>
                        <p class="text-xs mt-1">"Run an inference to generate traces"</p>
                    </div>
                }.into_any(),

                TraceViewState::Loading => view! {
                    <div class="flex items-center justify-center py-8">
                        <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
                        <span class="ml-3 text-muted-foreground">"Loading trace data..."</span>
                    </div>
                }.into_any(),

                TraceViewState::List(traces) => view! {
                    <TraceList
                        traces=traces
                        on_select=move |id| set_selected_trace_id.set(Some(id))
                        compact=compact
                    />
                }.into_any(),

                TraceViewState::Detail(detail) => view! {
                    <TraceDetail
                        trace=(*detail).clone()
                        expanded_tokens=expanded_tokens
                        set_expanded_tokens=set_expanded_tokens
                        on_back=move || set_selected_trace_id.set(None)
                        compact=compact
                    />
                }.into_any(),

                TraceViewState::Error(err) => view! {
                    <div class="bg-destructive/10 border border-destructive/20 rounded-lg p-4">
                        <div class="flex items-center gap-2 text-destructive">
                            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                            </svg>
                            <span class="font-medium">"Error loading trace"</span>
                        </div>
                        <p class="text-sm text-muted-foreground mt-2">{err}</p>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

/// List of traces for selection
#[component]
fn TraceList(
    traces: Vec<InferenceTraceResponse>,
    on_select: impl Fn(String) + 'static + Clone,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let heading_class = if compact {
        "heading-4 mb-2"
    } else {
        "heading-3 mb-4"
    };

    view! {
        <div>
            <h3 class=heading_class>"Recent Traces"</h3>
            <div class="space-y-2">
                {traces.into_iter().map(|trace| {
                    let on_select = on_select.clone();
                    let trace_id = trace.trace_id.clone();
                    view! {
                        <TraceListItem
                            trace=trace
                            on_click=move || on_select(trace_id.clone())
                            compact=compact
                        />
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Single trace list item
#[component]
fn TraceListItem(
    trace: InferenceTraceResponse,
    on_click: impl Fn() + 'static,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let item_class = if compact {
        "flex items-center justify-between p-2 rounded-md hover:bg-accent cursor-pointer transition-colors"
    } else {
        "flex items-center justify-between p-3 rounded-lg border border-border hover:bg-accent cursor-pointer transition-colors"
    };

    let status_class = match trace.finish_reason.as_deref() {
        Some("stop") | Some("end_turn") => "bg-status-success/20 text-status-success",
        Some("length") => "bg-status-warning/20 text-status-warning",
        Some("error") => "bg-status-error/20 text-status-error",
        _ => "bg-muted text-muted-foreground",
    };

    let trace_id_short = trace.trace_id.chars().take(8).collect::<String>();
    let finish_reason_display = trace
        .finish_reason
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    view! {
        <div class=item_class on:click=move |_| on_click()>
            <div class="flex items-center gap-3">
                <div class="flex flex-col">
                    <span class="font-mono text-xs text-muted-foreground">
                        {trace_id_short}"..."
                    </span>
                    <span class="text-xs text-muted-foreground">{trace.created_at}</span>
                </div>
            </div>
            <div class="flex items-center gap-4">
                <div class="text-right">
                    <div class="text-sm font-medium">{trace.latency_ms}"ms"</div>
                    <div class="text-xs text-muted-foreground">{trace.token_count}" tokens"</div>
                </div>
                <span class={format!("px-2 py-0.5 rounded text-xs font-medium {}", status_class)}>
                    {finish_reason_display}
                </span>
            </div>
        </div>
    }
}

/// Detailed trace view with timeline
#[component]
fn TraceDetail(
    trace: UiInferenceTraceDetailResponse,
    expanded_tokens: ReadSignal<bool>,
    set_expanded_tokens: WriteSignal<bool>,
    on_back: impl Fn() + 'static,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let heading_class = if compact {
        "heading-4"
    } else {
        "heading-3"
    };

    view! {
        <div class="space-y-4">
            // Header with back button
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-3">
                    <button
                        class="p-1 rounded hover:bg-accent transition-colors"
                        on:click=move |_| on_back()
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7"/>
                        </svg>
                    </button>
                    <div>
                        <h3 class=heading_class>"Trace Details"</h3>
                        <span class="font-mono text-xs text-muted-foreground">{trace.trace_id.clone()}</span>
                    </div>
                </div>
                <div class="text-right text-sm text-muted-foreground">
                    {trace.created_at.clone()}
                </div>
            </div>

            // Latency metrics
            <LatencyMetrics breakdown=trace.timing_breakdown.clone() compact=compact/>

            // Timeline visualization
            <TimelineVisualization breakdown=trace.timing_breakdown.clone() compact=compact/>

            // Adapters used
            <AdaptersList adapters=trace.adapters_used.clone() compact=compact/>

            // Token decisions (expandable, paged)
            {if !trace.token_decisions.is_empty() {
                Some(view! {
                    <TokenDecisionsPaged
                        trace_id=trace.trace_id.clone()
                        initial_decisions=trace.token_decisions.clone()
                        initial_next_cursor=trace.token_decisions_next_cursor
                        initial_has_more=trace.token_decisions_has_more
                        expanded=expanded_tokens
                        set_expanded=set_expanded_tokens
                        compact=compact
                    />
                })
            } else {
                None
            }}

            // Receipt verification
            {trace.receipt.clone().map(|r| view! {
                <ReceiptVerification receipt=r compact=compact/>
            })}
        </div>
    }
}

/// Latency metrics display
#[component]
fn LatencyMetrics(breakdown: TimingBreakdown, #[prop(optional)] compact: bool) -> impl IntoView {
    let grid_class = if compact {
        "grid grid-cols-4 gap-2"
    } else {
        "grid grid-cols-4 gap-4"
    };

    let card_class = if compact {
        "bg-muted/50 rounded p-2 text-center"
    } else {
        "bg-muted/50 rounded-lg p-3 text-center"
    };

    view! {
        <div class=grid_class>
            <div class=card_class>
                <div class="text-2xl font-bold text-primary">{breakdown.total_ms}</div>
                <div class="text-xs text-muted-foreground">"Total (ms)"</div>
            </div>
            {breakdown.prefill_ms.map(|ms| view! {
                <div class=card_class>
                    <div class="text-xl font-semibold">{ms}</div>
                    <div class="text-xs text-muted-foreground">"Prefill"</div>
                </div>
            })}
            {breakdown.decode_ms.map(|ms| view! {
                <div class=card_class>
                    <div class="text-xl font-semibold">{ms}</div>
                    <div class="text-xs text-muted-foreground">"Decode"</div>
                </div>
            })}
            <div class=card_class>
                <div class="text-xl font-semibold">{breakdown.routing_ms}</div>
                <div class="text-xs text-muted-foreground">"Routing"</div>
            </div>
        </div>
    }
}

/// Timeline visualization of trace phases
#[component]
fn TimelineVisualization(
    breakdown: TimingBreakdown,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let total = breakdown.total_ms.max(1) as f64;
    let routing_pct = (breakdown.routing_ms as f64 / total * 100.0) as u32;
    let inference_pct = (breakdown.inference_ms as f64 / total * 100.0) as u32;
    let policy_pct = (breakdown.policy_ms as f64 / total * 100.0) as u32;
    let other_pct = 100_u32.saturating_sub(routing_pct + inference_pct + policy_pct);

    let bar_height = if compact { "h-4" } else { "h-6" };

    view! {
        <div class="space-y-2">
            <div class="flex items-center justify-between text-xs text-muted-foreground">
                <span>"0ms"</span>
                <span>{breakdown.total_ms}"ms"</span>
            </div>
            <div class={format!("flex rounded-full overflow-hidden {}", bar_height)}>
                <div
                    class="bg-status-info transition-all"
                    style=format!("width: {}%", routing_pct)
                    title=format!("Routing: {}ms", breakdown.routing_ms)
                ></div>
                <div
                    class="bg-status-success transition-all"
                    style=format!("width: {}%", inference_pct)
                    title=format!("Inference: {}ms", breakdown.inference_ms)
                ></div>
                <div
                    class="bg-primary transition-all"
                    style=format!("width: {}%", policy_pct)
                    title=format!("Policy: {}ms", breakdown.policy_ms)
                ></div>
                {if other_pct > 0 {
                    Some(view! {
                        <div
                            class="bg-muted transition-all"
                            style=format!("width: {}%", other_pct)
                            title="Other"
                        ></div>
                    })
                } else {
                    None
                }}
            </div>
            <div class="flex items-center justify-center gap-4 text-xs">
                <div class="flex items-center gap-1">
                    <div class="w-3 h-3 rounded bg-status-info"></div>
                    <span>"Routing"</span>
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-3 h-3 rounded bg-status-success"></div>
                    <span>"Inference"</span>
                </div>
                <div class="flex items-center gap-1">
                    <div class="w-3 h-3 rounded bg-primary"></div>
                    <span>"Policy"</span>
                </div>
            </div>
        </div>
    }
}

/// List of adapters used
#[component]
fn AdaptersList(adapters: Vec<String>, #[prop(optional)] compact: bool) -> impl IntoView {
    let label_class = if compact { "text-xs" } else { "text-sm" };

    view! {
        <div>
            <h4 class={format!("{} font-medium mb-2", label_class)}>"Adapters Used"</h4>
            <div class="flex flex-wrap gap-2">
                {adapters.into_iter().map(|adapter| {
                    view! {
                        <span class="px-2 py-1 bg-primary/10 text-primary rounded text-xs font-mono">
                            {adapter}
                        </span>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Token-level routing decisions
#[component]
pub fn TokenDecisions(
    decisions: ReadSignal<Vec<TokenDecision>>,
    expanded: ReadSignal<bool>,
    set_expanded: WriteSignal<bool>,
    #[prop(optional)] compact: bool,
    has_more: ReadSignal<bool>,
    loading_more: ReadSignal<bool>,
    on_load_more: Callback<()>,
) -> impl IntoView {
    let label_class = if compact { "text-xs" } else { "text-sm" };

    view! {
        <div class="border border-border rounded-lg p-3" data-testid="token-decisions">
            <button
                class="w-full flex items-center justify-between"
                on:click=move |_| set_expanded.update(|e| *e = !*e)
            >
                <h4 class={format!("{} font-medium", label_class)}>
                    "Token Routing Decisions ("{move || decisions.get().len()}")"
                </h4>
                <svg
                    class={move || format!("w-4 h-4 transition-transform {}", if expanded.get() { "rotate-180" } else { "" })}
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                </svg>
            </button>

            {move || if expanded.get() {
                let decisions = decisions.get();
                let total = decisions.len();
                let cap = TOKEN_DECISIONS_DOM_CAP;
                let truncated = total > cap;
                let visible = if truncated {
                    decisions.into_iter().skip(total.saturating_sub(cap)).collect::<Vec<_>>()
                } else {
                    decisions
                };
                Some(view! {
                    <div class="mt-3 space-y-2" data-testid="token-decisions-list">
                        {truncated.then(|| view! {
                            <p class="text-xs text-muted-foreground">
                                {format!("Showing last {} of {} token decisions.", cap, total)}
                            </p>
                        })}
                        {visible.into_iter().map(|d| {
                            view! {
                                <TokenDecisionRow decision=d compact=compact/>
                            }
                        }).collect::<Vec<_>>()}
                        {move || {
                            if has_more.get() {
                                let loading = loading_more.get();
                                let on_load_more = on_load_more.clone();
                                Some(view! {
                                    <div class="flex justify-center pt-2">
                                        <button
                                            class=move || {
                                                format!(
                                                    "text-xs px-3 py-1 rounded border border-border hover:bg-muted transition-colors {}",
                                                    if loading { "opacity-60 cursor-not-allowed" } else { "" }
                                                )
                                            }
                                            disabled=move || loading
                                            data-testid="token-decisions-show-more"
                                            on:click=move |_| {
                                                on_load_more.run(());
                                            }
                                        >
                                            {move || if loading { "Loading..." } else { "Show more" }}
                                        </button>
                                    </div>
                                })
                            } else {
                                None
                            }
                        }}
                    </div>
                })
            } else {
                None
            }}
        </div>
    }
}

/// Token decisions with cursor-based paging.
#[component]
pub fn TokenDecisionsPaged(
    trace_id: String,
    initial_decisions: Vec<TokenDecision>,
    initial_next_cursor: Option<u32>,
    initial_has_more: bool,
    expanded: ReadSignal<bool>,
    set_expanded: WriteSignal<bool>,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let decisions = RwSignal::new(initial_decisions);
    let next_cursor = RwSignal::new(initial_next_cursor);
    let has_more = RwSignal::new(initial_has_more);
    let loading_more = RwSignal::new(false);
    let notifications = try_use_notifications();
    let perf_enabled = perf_logging_enabled();

    let on_load_more = Callback::new(move |_| {
        if loading_more.get() || !has_more.get() {
            return;
        }
        let Some(after) = next_cursor.get() else {
            return;
        };
        loading_more.set(true);
        let trace_id = trace_id.clone();
        let notifications = notifications.clone();
        let perf_enabled = perf_enabled;

        spawn_local(async move {
            let started_at = Instant::now();
            let client = ApiClient::new();
            match client
                .get_inference_trace_detail(&trace_id, Some(TOKEN_DECISIONS_PAGE_SIZE), Some(after))
                .await
            {
                Ok(detail) => {
                    decisions.update(|items| items.extend(detail.token_decisions));
                    next_cursor.set(detail.token_decisions_next_cursor);
                    has_more.set(detail.token_decisions_has_more);
                    if perf_enabled {
                        let elapsed_ms = started_at.elapsed().as_millis();
                        web_sys::console::log_1(
                            &format!(
                                "[perf] token decisions page: {}ms (trace_id={})",
                                elapsed_ms, trace_id
                            )
                            .into(),
                        );
                    }
                }
                Err(err) => {
                    if let Some(notifications) = notifications {
                        notifications.error("Token decisions fetch failed", &err.to_string());
                    } else {
                        web_sys::console::warn_1(
                            &format!("Token decisions fetch failed: {}", err).into(),
                        );
                    }
                }
            }
            loading_more.set(false);
        });
    });

    view! {
        <TokenDecisions
            decisions=decisions.read_only()
            expanded=expanded
            set_expanded=set_expanded
            compact=compact
            has_more=has_more.read_only()
            loading_more=loading_more.read_only()
            on_load_more=on_load_more
        />
    }
}

/// Single token decision row
#[component]
fn TokenDecisionRow(decision: TokenDecision, #[prop(optional)] compact: bool) -> impl IntoView {
    let row_class = if compact {
        "flex items-center gap-2 p-1 bg-muted/30 rounded text-xs"
    } else {
        "flex items-center gap-3 p-2 bg-muted/30 rounded"
    };

    // Format gates as percentages (Q15 -> percentage)
    let adapter_gates: Vec<(String, String)> = decision
        .adapter_ids
        .iter()
        .zip(decision.gates_q15.iter())
        .map(|(id, g)| {
            let id_short = id.chars().take(8).collect::<String>();
            let gate_pct = format!("{:.1}%", (*g as f64 / 327.67));
            (id_short, gate_pct)
        })
        .collect();

    // Entropy indicator color
    let entropy_color = if decision.entropy < 0.5 {
        "text-status-success"
    } else if decision.entropy < 1.0 {
        "text-status-warning"
    } else {
        "text-status-error"
    };

    let entropy_display = format!("{:.2}", decision.entropy);

    view! {
        <div class=row_class data-testid="token-decision-row">
            <span class="font-mono text-muted-foreground w-8">{"#"}{decision.token_index}</span>
            <div class="flex-1 flex items-center gap-2">
                <span class="text-xs">"Adapters:"</span>
                {adapter_gates.into_iter().map(|(id_short, gate_pct)| {
                    let display = format!("{} ({})", id_short, gate_pct);
                    view! {
                        <span class="px-1.5 py-0.5 bg-primary/10 rounded text-xs">
                            {display}
                        </span>
                    }
                }).collect::<Vec<_>>()}
            </div>
            <div class={format!("text-xs {}", entropy_color)} title="Routing entropy">
                "H="{entropy_display}
            </div>
        </div>
    }
}

/// Receipt verification display
#[component]
fn ReceiptVerification(
    receipt: UiTraceReceiptSummary,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let container_class = if compact {
        "border border-border rounded p-2"
    } else {
        "border border-border rounded-lg p-4"
    };

    let verified_class = if receipt.verified {
        "bg-status-success/10 border-status-success/20 text-status-success"
    } else {
        "bg-status-warning/10 border-status-warning/20 text-status-warning"
    };

    let verified_text = if receipt.verified {
        "Verified"
    } else {
        "Unverified"
    };
    let verified_help = if receipt.verified {
        "Verified: receipt digest matches recorded inputs and outputs."
    } else {
        "Unverified: receipt digest has not been validated for this run."
    };
    let receipt_short = format!(
        "{}...",
        receipt.receipt_digest.chars().take(16).collect::<String>()
    );
    let output_short = format!(
        "{}...",
        receipt.output_digest.chars().take(16).collect::<String>()
    );
    let cache_hit = receipt.prefix_cache_hit.unwrap_or(false);

    view! {
        <div class=container_class data-testid="receipt-verification">
            <div class="flex items-center justify-between mb-3">
                <h4 class="text-sm font-medium">"Inference Receipt"</h4>
                <div class="flex gap-2">
                    {receipt.stop_reason_code.map(|code| view! {
                        <span class="px-2 py-0.5 rounded text-xs font-mono bg-muted border border-border">
                            {code}
                        </span>
                    })}
                    <span class={format!("px-2 py-0.5 rounded text-xs font-medium border {}", verified_class)}>
                        {verified_text}
                    </span>
                </div>
            </div>
            <p class="text-xs text-muted-foreground mb-2">{verified_help}</p>
            <div class="grid grid-cols-2 gap-2 text-xs">
                <div>
                    <span class="text-muted-foreground">"Prompt tokens: "</span>
                    <span class="font-mono">{receipt.logical_prompt_tokens}</span>
                    {if cache_hit {
                        Some(view! { <span class="ml-1 text-[10px] text-status-success">"(Cache Hit)"</span> })
                    } else {
                        None
                    }}
                </div>
                <div>
                    <span class="text-muted-foreground">"Output tokens: "</span>
                    <span class="font-mono">{receipt.logical_output_tokens}</span>
                </div>

                {receipt.processor_id.as_ref().map(|id: &String| {
                    let id = id.clone();
                    view! {
                        <div class="col-span-2 flex justify-between border-t border-border/50 pt-1 mt-1">
                            <span class="text-muted-foreground">"Hardware: "</span>
                            <span class="font-mono text-[10px]">{id}</span>
                        </div>
                    }
                })}

                {let engine = receipt.engine_version.clone();
                 let ane = receipt.ane_version.clone();
                 (engine.is_some() || ane.is_some()).then(|| {
                    let engine_display = engine.unwrap_or_else(|| "Unknown".into());
                    let ane_display = ane.unwrap_or_else(|| "N/A".into());
                    let display_text = format!("{} / {}", engine_display, ane_display);
                    view! {
                        <div class="col-span-2 flex justify-between text-[10px]">
                            <span class="text-muted-foreground">"Engine/ANE: "</span>
                            <span class="font-mono">{display_text}</span>
                        </div>
                    }
                })}

                <div class="col-span-2 border-t border-border/50 pt-1 mt-1">
                    <span class="text-muted-foreground">"Receipt digest: "</span>
                    <span class="font-mono text-[10px] break-all">{receipt_short}</span>
                </div>
                <div class="col-span-2">
                    <span class="text-muted-foreground">"Output hash: "</span>
                    <span class="font-mono text-[10px] break-all">{output_short}</span>
                </div>
            </div>
        </div>
    }
}

/// Compact trace button for embedding in chat messages
#[component]
pub fn TraceButton(
    trace_id: String,
    latency_ms: u64,
    #[prop(optional)] on_click: Option<Callback<String>>,
    #[prop(optional, into)] data_testid: Option<String>,
) -> impl IntoView {
    let tid = trace_id.clone();
    let trace_id_short = trace_id.chars().take(8).collect::<String>();

    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <button
            class="inline-flex items-center gap-1.5 px-2 py-1 bg-muted/50 hover:bg-muted rounded text-xs transition-colors"
            data-testid=move || data_testid.clone()
            on:click=move |_| {
                if let Some(ref cb) = on_click {
                    cb.run(tid.clone());
                }
            }
        >
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/>
            </svg>
            <span class="font-mono">{trace_id_short}</span>
            <span class="text-muted-foreground">{latency_ms}"ms"</span>
        </button>
    }
}

/// Trace panel for showing in a modal or side panel
#[component]
pub fn TracePanel(
    trace_id: String,
    #[prop(optional)] on_close: Option<Callback<()>>,
) -> impl IntoView {
    let full_page_url = format!("/runs/{}?tab=trace", trace_id);

    view! {
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
            <div class="bg-card border border-border rounded-xl shadow-xl w-full max-w-2xl max-h-[80vh] overflow-auto">
                <div class="sticky top-0 bg-card border-b border-border p-4 flex items-center justify-between">
                    <div class="flex items-center gap-3">
                        <h2 class="heading-4">"Trace Viewer"</h2>
                        <a
                            href=full_page_url
                            class="text-xs text-primary hover:underline flex items-center gap-1"
                            title="Open in Run Detail page"
                        >
                            "Open Full View"
                            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/>
                            </svg>
                        </a>
                    </div>
                    <button
                        class="p-2 rounded-lg hover:bg-muted transition-colors"
                        on:click=move |_| {
                            if let Some(ref cb) = on_close {
                                cb.run(());
                            }
                        }
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>
                <div class="p-4">
                    <TraceViewerInner trace_id=trace_id.clone() compact=false/>
                </div>
            </div>
        </div>
    }
}

/// Internal trace viewer that takes a required trace_id
#[component]
fn TraceViewerInner(trace_id: String, #[prop(optional)] compact: bool) -> impl IntoView {
    let (state, set_state) = signal(TraceViewState::Loading);
    let (expanded_tokens, set_expanded_tokens) = signal(false);

    // Load trace on mount
    let api = ApiClient::new();
    let tid = trace_id.clone();

    Effect::new(move |_prev| {
        let api = api.clone();
        let trace_id = tid.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match api
                .get_inference_trace_detail(&trace_id, Some(TOKEN_DECISIONS_PAGE_SIZE), None)
                .await
            {
                Ok(detail) => set_state.set(TraceViewState::Detail(Box::new(detail))),
                Err(e) => set_state.set(TraceViewState::Error(e.to_string())),
            }
        });
    });

    let container_class = if compact {
        "bg-card border border-border rounded-lg p-3 text-sm"
    } else {
        "bg-card border border-border rounded-lg p-6"
    };

    view! {
        <div class=container_class>
            {move || match state.get() {
                TraceViewState::Empty | TraceViewState::Loading => view! {
                    <div class="flex items-center justify-center py-8">
                        <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
                        <span class="ml-3 text-muted-foreground">"Loading trace data..."</span>
                    </div>
                }.into_any(),

                TraceViewState::List(_) => view! {
                    <div class="text-muted-foreground text-center py-8">
                        <p>"Unexpected state"</p>
                    </div>
                }.into_any(),

                TraceViewState::Detail(detail) => view! {
                    <TraceDetailStandalone
                        trace=(*detail).clone()
                        expanded_tokens=expanded_tokens
                        set_expanded_tokens=set_expanded_tokens
                        compact=compact
                    />
                }.into_any(),

                TraceViewState::Error(err) => view! {
                    <div class="bg-destructive/10 border border-destructive/20 rounded-lg p-4">
                        <div class="flex items-center gap-2 text-destructive">
                            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                            </svg>
                            <span class="font-medium">"Error loading trace"</span>
                        </div>
                        <p class="text-sm text-muted-foreground mt-2">{err}</p>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

/// Trace detail without back button (for modal use)
#[component]
pub fn TraceDetailStandalone(
    trace: UiInferenceTraceDetailResponse,
    expanded_tokens: ReadSignal<bool>,
    set_expanded_tokens: WriteSignal<bool>,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let heading_class = if compact {
        "heading-4"
    } else {
        "heading-3"
    };

    view! {
        <div class="space-y-4">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <h3 class=heading_class>"Trace Details"</h3>
                    <span class="font-mono text-xs text-muted-foreground">{trace.trace_id.clone()}</span>
                </div>
                <div class="text-right text-sm text-muted-foreground">
                    {trace.created_at.clone()}
                </div>
            </div>

            // Latency metrics
            <LatencyMetrics breakdown=trace.timing_breakdown.clone() compact=compact/>

            // Timeline visualization
            <TimelineVisualization breakdown=trace.timing_breakdown.clone() compact=compact/>

            // Adapters used
            <AdaptersList adapters=trace.adapters_used.clone() compact=compact/>

            // Token decisions (expandable, paged)
            {if !trace.token_decisions.is_empty() {
                Some(view! {
                    <TokenDecisionsPaged
                        trace_id=trace.trace_id.clone()
                        initial_decisions=trace.token_decisions.clone()
                        initial_next_cursor=trace.token_decisions_next_cursor
                        initial_has_more=trace.token_decisions_has_more
                        expanded=expanded_tokens
                        set_expanded=set_expanded_tokens
                        compact=compact
                    />
                })
            } else {
                None
            }}

            // Receipt verification
            {trace.receipt.clone().map(|r| view! {
                <ReceiptVerification receipt=r compact=compact/>
            })}
        </div>
    }
}

/// Trace viewer that accepts pre-loaded data (no internal fetch)
///
/// Use this component when trace data is already fetched at a parent level
/// to avoid duplicate API calls.
#[component]
pub fn TraceViewerWithData(
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let (expanded_tokens, set_expanded_tokens) = signal(false);

    let container_class = if compact {
        "bg-card border border-border rounded-lg p-3 text-sm"
    } else {
        "bg-card border border-border rounded-lg p-6"
    };

    view! {
        <div class=container_class>
            {move || match trace_detail.get() {
                LoadingState::Idle | LoadingState::Loading => view! {
                    <div class="flex items-center justify-center py-8">
                        <Spinner/>
                        <span class="ml-3 text-muted-foreground">"Loading trace data..."</span>
                    </div>
                }.into_any(),
                LoadingState::Loaded(detail) => view! {
                    <TraceDetailStandalone
                        trace=detail.clone()
                        expanded_tokens=expanded_tokens
                        set_expanded_tokens=set_expanded_tokens
                        compact=compact
                    />
                }.into_any(),
                LoadingState::Error(err) => view! {
                    <ErrorDisplay error=err.clone()/>
                }.into_any(),
            }}
        </div>
    }
}
