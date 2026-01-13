//! Diff page for comparing two diagnostic runs
//!
//! Shows deterministic anchor comparison and first divergence point.

use crate::api::ApiClient;
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, Spinner};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::diagnostics::{
    AnchorComparison, DiagDiffRequest, DiagDiffResponse, DiagRunResponse, FirstDivergence,
    ListDiagRunsQuery, ListDiagRunsResponse, RouterStepDiff,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::Arc;

/// Diff page for comparing diagnostic runs
#[component]
pub fn Diff() -> impl IntoView {
    // State for selected runs
    let run_a_id = RwSignal::new(String::new());
    let run_b_id = RwSignal::new(String::new());
    let diff_result: RwSignal<Option<DiagDiffResponse>> = RwSignal::new(None);
    let diff_loading = RwSignal::new(false);
    let diff_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Fetch available runs
    let (runs, refetch_runs) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .list_diag_runs(&ListDiagRunsQuery {
                limit: Some(50),
                ..Default::default()
            })
            .await
    });

    // Compare runs action
    let do_compare = move |_| {
        let trace_a = run_a_id.get();
        let trace_b = run_b_id.get();

        if trace_a.is_empty() || trace_b.is_empty() {
            diff_error.set(Some("Please select two runs to compare".to_string()));
            return;
        }

        diff_loading.set(true);
        diff_error.set(None);
        diff_result.set(None);

        spawn_local(async move {
            let client = ApiClient::new();
            let request = DiagDiffRequest {
                trace_id_a: trace_a,
                trace_id_b: trace_b,
                include_timing: true,
                include_events: true,
                include_router_steps: true,
            };

            match client.diff_diag_runs(&request).await {
                Ok(result) => {
                    diff_result.set(Some(result));
                    diff_loading.set(false);
                }
                Err(e) => {
                    diff_error.set(Some(e.to_string()));
                    diff_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="p-6 space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Diff Runs"</h1>
                    <p class="text-muted-foreground mt-1">
                        "Compare two diagnostic runs to find deterministic divergence"
                    </p>
                </div>
                <Button variant=ButtonVariant::Outline on:click=move |_| refetch_runs()>
                    "Refresh Runs"
                </Button>
            </div>

            // Run selectors
            <Card>
                <div class="space-y-4">
                    <h2 class="text-lg font-semibold">"Select Runs to Compare"</h2>
                    <div class="grid gap-4 md:grid-cols-2">
                        // Run A selector
                        <div>
                            <label class="text-sm font-medium mb-2 block">"Run A (Baseline)"</label>
                            <RunSelector
                                runs=runs
                                selected=run_a_id
                                exclude=run_b_id
                            />
                        </div>
                        // Run B selector
                        <div>
                            <label class="text-sm font-medium mb-2 block">"Run B (Comparison)"</label>
                            <RunSelector
                                runs=runs
                                selected=run_b_id
                                exclude=run_a_id
                            />
                        </div>
                    </div>
                    <div class="flex items-center gap-4">
                        <button
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                            disabled=move || diff_loading.get() || run_a_id.get().is_empty() || run_b_id.get().is_empty()
                            on:click=do_compare
                        >
                            {move || if diff_loading.get() { "Comparing..." } else { "Compare Runs" }}
                        </button>
                        {move || diff_error.get().map(|e| view! {
                            <span class="text-destructive text-sm">{e}</span>
                        })}
                    </div>
                </div>
            </Card>

            // Diff results
            {move || {
                if diff_loading.get() {
                    view! {
                        <Card>
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                                <span class="ml-2 text-muted-foreground">"Comparing runs..."</span>
                            </div>
                        </Card>
                    }.into_any()
                } else if let Some(result) = diff_result.get() {
                    view! { <DiffResults result=result/> }.into_any()
                } else {
                    view! {
                        <Card>
                            <div class="text-center py-12 text-muted-foreground">
                                "Select two runs and click Compare to see differences"
                            </div>
                        </Card>
                    }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn RunSelector(
    runs: ReadSignal<LoadingState<ListDiagRunsResponse>>,
    selected: RwSignal<String>,
    exclude: RwSignal<String>,
) -> impl IntoView {
    view! {
        <select
            class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            on:change=move |ev| selected.set(event_target_value(&ev))
            prop:value=move || selected.get()
        >
            <option value="">"-- Select a run --"</option>
            {move || {
                match runs.get() {
                    LoadingState::Loaded(data) => {
                        let exclude_id = exclude.get();
                        data.runs
                            .into_iter()
                            .filter(|r| r.trace_id != exclude_id)
                            .map(|run| {
                                let trace_id = run.trace_id.clone();
                                let label = format!(
                                    "{} - {} ({})",
                                    run.trace_id.chars().take(12).collect::<String>(),
                                    run.status,
                                    run.created_at
                                );
                                view! {
                                    <option value=trace_id.clone()>{label}</option>
                                }.into_any()
                            })
                            .collect::<Vec<_>>()
                    }
                    LoadingState::Loading => vec![view! { <option value="">"Loading..."</option> }.into_any()],
                    _ => vec![view! { <option value="">"No runs available"</option> }.into_any()],
                }
            }}
        </select>
    }
}

#[component]
fn DiffResults(result: DiagDiffResponse) -> impl IntoView {
    let equivalent = result.summary.equivalent;
    let first_divergence = result.first_divergence.clone();

    view! {
        <div class="space-y-6">
            // Summary card
            <Card>
                <div class="space-y-4">
                    <div class="flex items-center justify-between">
                        <h2 class="text-lg font-semibold">"Comparison Summary"</h2>
                        {if equivalent {
                            view! { <Badge variant=BadgeVariant::Success>"Equivalent"</Badge> }.into_any()
                        } else {
                            view! { <Badge variant=BadgeVariant::Destructive>"Divergent"</Badge> }.into_any()
                        }}
                    </div>

                    // Run info
                    <div class="grid gap-4 md:grid-cols-2">
                        <RunInfoCard run=result.run_a.clone() label="Run A (Baseline)"/>
                        <RunInfoCard run=result.run_b.clone() label="Run B (Comparison)"/>
                    </div>
                </div>
            </Card>

            // First divergence block (prominent display)
            {first_divergence.map(|fd| view! {
                <FirstDivergenceCard divergence=fd/>
            })}

            // Anchor comparison
            <AnchorComparisonCard anchors=result.anchor_comparison.clone()/>

            // Router step diffs
            {result.router_step_diffs.map(|steps| view! {
                <RouterStepsCard steps=steps/>
            })}
        </div>
    }
}

#[component]
fn RunInfoCard(run: DiagRunResponse, label: &'static str) -> impl IntoView {
    let status_variant = match run.status.as_str() {
        "completed" => BadgeVariant::Success,
        "running" => BadgeVariant::Warning,
        "failed" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    };

    view! {
        <div class="p-4 bg-muted/30 rounded-lg space-y-2">
            <div class="flex items-center justify-between">
                <span class="text-sm font-medium">{label}</span>
                <Badge variant=status_variant>{run.status.clone()}</Badge>
            </div>
            <div class="text-xs text-muted-foreground space-y-1">
                <p><span class="font-medium">"Trace: "</span><span class="font-mono">{run.trace_id.clone()}</span></p>
                <p><span class="font-medium">"Request Hash: "</span><span class="font-mono">{format!("{}...", run.request_hash.chars().take(16).collect::<String>())}</span></p>
                <p><span class="font-medium">"Events: "</span>{run.total_events_count}</p>
                {run.duration_ms.map(|d| view! {
                    <p><span class="font-medium">"Duration: "</span>{format!("{}ms", d)}</p>
                })}
            </div>
        </div>
    }
}

#[component]
fn FirstDivergenceCard(divergence: FirstDivergence) -> impl IntoView {
    let category_badge = match divergence.category.as_str() {
        "anchor" => BadgeVariant::Warning,
        "router_step" => BadgeVariant::Destructive,
        "stage" => BadgeVariant::Secondary,
        _ => BadgeVariant::Outline,
    };

    view! {
        <Card>
            <div class="border-l-4 border-destructive pl-4 space-y-4">
                <div class="flex items-center gap-3">
                    <svg class="h-6 w-6 text-destructive" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <circle cx="12" cy="12" r="10"/>
                        <line x1="12" y1="8" x2="12" y2="12"/>
                        <line x1="12" y1="16" x2="12.01" y2="16"/>
                    </svg>
                    <div>
                        <h2 class="text-lg font-bold text-destructive">"First Divergence"</h2>
                        <div class="flex items-center gap-2 mt-1">
                            <Badge variant=category_badge>{divergence.category.clone()}</Badge>
                            {divergence.stage.clone().map(|s| view! {
                                <Badge variant=BadgeVariant::Outline>{format!("Stage: {}", s)}</Badge>
                            })}
                            {divergence.router_step.map(|step| view! {
                                <Badge variant=BadgeVariant::Outline>{format!("Step: {}", step)}</Badge>
                            })}
                        </div>
                    </div>
                </div>

                <p class="text-foreground font-medium">{divergence.description.clone()}</p>

                // Value comparison
                <div class="grid gap-4 md:grid-cols-2">
                    {divergence.value_a.map(|v| view! {
                        <div class="p-3 bg-status-error/10 border border-status-error/30 rounded-md">
                            <p class="text-xs text-muted-foreground mb-1">"Run A Value"</p>
                            <pre class="text-xs font-mono whitespace-pre-wrap overflow-x-auto">
                                {serde_json::to_string_pretty(&v).unwrap_or_default()}
                            </pre>
                        </div>
                    })}
                    {divergence.value_b.map(|v| view! {
                        <div class="p-3 bg-status-info/10 border border-status-info/30 rounded-md">
                            <p class="text-xs text-muted-foreground mb-1">"Run B Value"</p>
                            <pre class="text-xs font-mono whitespace-pre-wrap overflow-x-auto">
                                {serde_json::to_string_pretty(&v).unwrap_or_default()}
                            </pre>
                        </div>
                    })}
                </div>
            </div>
        </Card>
    }
}

#[component]
fn AnchorComparisonCard(anchors: AnchorComparison) -> impl IntoView {
    view! {
        <Card>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <h2 class="text-lg font-semibold">"Anchor Comparison"</h2>
                    {if anchors.all_anchors_match {
                        view! { <Badge variant=BadgeVariant::Success>"All Match"</Badge> }.into_any()
                    } else {
                        view! { <Badge variant=BadgeVariant::Warning>"Mismatch"</Badge> }.into_any()
                    }}
                </div>

                <div class="space-y-2">
                    <AnchorRow
                        label="Request Hash"
                        matches=anchors.request_hash_match
                        value_a=Some(anchors.request_hash_a.clone())
                        value_b=Some(anchors.request_hash_b.clone())
                    />
                    <AnchorRow
                        label="Manifest Hash"
                        matches=anchors.manifest_hash_match
                        value_a=None
                        value_b=None
                    />
                    <AnchorRow
                        label="Decision Chain Hash"
                        matches=anchors.decision_chain_hash_match
                        value_a=anchors.decision_chain_hash_a.clone()
                        value_b=anchors.decision_chain_hash_b.clone()
                    />
                    <AnchorRow
                        label="Backend Identity"
                        matches=anchors.backend_identity_hash_match
                        value_a=None
                        value_b=None
                    />
                    <AnchorRow
                        label="Model Identity"
                        matches=anchors.model_identity_hash_match
                        value_a=None
                        value_b=None
                    />
                </div>
            </div>
        </Card>
    }
}

#[component]
fn AnchorRow(
    label: &'static str,
    matches: bool,
    value_a: Option<String>,
    value_b: Option<String>,
) -> impl IntoView {
    let icon_class = if matches {
        "text-status-success"
    } else {
        "text-status-error"
    };
    let bg_class = if matches {
        "bg-status-success/5"
    } else {
        "bg-status-error/5"
    };

    view! {
        <div class=format!("flex items-center justify-between p-3 rounded-md {}", bg_class)>
            <div class="flex items-center gap-3">
                {if matches {
                    view! {
                        <svg class=format!("h-5 w-5 {}", icon_class) viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <path d="M20 6L9 17l-5-5"/>
                        </svg>
                    }.into_any()
                } else {
                    view! {
                        <svg class=format!("h-5 w-5 {}", icon_class) viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                    }.into_any()
                }}
                <span class="font-medium">{label}</span>
            </div>
            <div class="text-right">
                {if matches {
                    view! { <span class="text-status-success text-sm">"Match"</span> }.into_any()
                } else {
                    view! {
                        <div class="text-xs font-mono text-muted-foreground">
                            {value_a.map(|v| view! {
                                <div class="text-status-error">{format!("A: {}...", v.chars().take(12).collect::<String>())}</div>
                            })}
                            {value_b.map(|v| view! {
                                <div class="text-status-info">{format!("B: {}...", v.chars().take(12).collect::<String>())}</div>
                            })}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

#[component]
fn RouterStepsCard(steps: Vec<RouterStepDiff>) -> impl IntoView {
    // Compute values before the view to avoid borrow/move conflicts
    let has_divergence = steps.iter().any(|s| !s.matches);
    let first_divergent_step = steps
        .iter()
        .find(|s| s.is_first_divergence)
        .map(|s| s.step_idx);
    let step_count = steps.len();

    let summary_text = match first_divergent_step {
        Some(step_idx) => {
            format!(
                "First divergence at step {}. Click to expand all steps.",
                step_idx
            )
        }
        None => "Click to view all router steps".to_string(),
    };

    view! {
        <Card>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <h2 class="text-lg font-semibold">"Router Steps"</h2>
                    <div class="flex items-center gap-2">
                        <span class="text-sm text-muted-foreground">{format!("{} steps", step_count)}</span>
                        {if has_divergence {
                            view! { <Badge variant=BadgeVariant::Warning>"Has Divergence"</Badge> }.into_any()
                        } else {
                            view! { <Badge variant=BadgeVariant::Success>"All Match"</Badge> }.into_any()
                        }}
                    </div>
                </div>

                // Expandable step list
                <details class="group">
                    <summary class="cursor-pointer text-sm text-muted-foreground hover:text-foreground">
                        {summary_text}
                    </summary>
                    <div class="mt-4 space-y-2 max-h-96 overflow-y-auto">
                        {steps.into_iter().map(|step| view! { <RouterStepRow step=step/> }).collect::<Vec<_>>()}
                    </div>
                </details>
            </div>
        </Card>
    }
}

#[component]
fn RouterStepRow(step: RouterStepDiff) -> impl IntoView {
    let bg_class = if step.is_first_divergence {
        "bg-destructive/10 border-destructive"
    } else if step.matches {
        "bg-status-success/5 border-status-success/30"
    } else {
        "bg-status-warning/5 border-status-warning/30"
    };

    view! {
        <div class=format!("p-3 rounded-md border {}", bg_class)>
            <div class="flex items-center justify-between mb-2">
                <span class="font-medium">{format!("Step {}", step.step_idx)}</span>
                <div class="flex items-center gap-2">
                    {if step.is_first_divergence {
                        view! { <Badge variant=BadgeVariant::Destructive>"First Divergence"</Badge> }.into_any()
                    } else if step.matches {
                        view! { <Badge variant=BadgeVariant::Success>"Match"</Badge> }.into_any()
                    } else {
                        view! { <Badge variant=BadgeVariant::Warning>"Differs"</Badge> }.into_any()
                    }}
                </div>
            </div>

            <div class="grid gap-2 md:grid-cols-2 text-xs font-mono">
                <div>
                    <p class="text-muted-foreground mb-1">"Run A - selected_ids"</p>
                    <p>{format!("{:?}", step.selected_ids_a)}</p>
                    <p class="text-muted-foreground mt-1 mb-1">"scores_q15"</p>
                    <p>{format!("{:?}", step.scores_q15_a)}</p>
                </div>
                <div>
                    <p class="text-muted-foreground mb-1">"Run B - selected_ids"</p>
                    <p>{format!("{:?}", step.selected_ids_b)}</p>
                    <p class="text-muted-foreground mt-1 mb-1">"scores_q15"</p>
                    <p>{format!("{:?}", step.scores_q15_b)}</p>
                </div>
            </div>
        </div>
    }
}
