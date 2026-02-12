//! Diff page for comparing two diagnostic runs
//!
//! Shows deterministic anchor comparison and first divergence point.

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, Card, DiffResults, EmptyState, EmptyStateVariant, Link,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::diagnostics::{
    DiagDiffRequest, DiagDiffResponse, ListDiagRunsQuery, ListDiagRunsResponse,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Redirect;
use leptos_router::hooks::use_query_map;
use std::sync::Arc;

/// Diff page for comparing diagnostic runs
#[component]
pub fn Diff() -> impl IntoView {
    let query = use_query_map();
    let redirect_path = {
        let map = query.try_get().unwrap_or_default();
        let run_a = map
            .get("run")
            .or_else(|| map.get("run_a"))
            .or_else(|| map.get("trace_id_a"))
            .or_else(|| map.get("a"));
        let run_b = map
            .get("compare")
            .or_else(|| map.get("run_b"))
            .or_else(|| map.get("trace_id_b"))
            .or_else(|| map.get("b"));

        run_a.map(|run| {
            let compare = run_b
                .map(|id| format!("&compare={}", id))
                .unwrap_or_default();
            format!("/runs/{}?tab=diff{}", run, compare)
        })
    };

    if let Some(path) = redirect_path {
        return view! { <Redirect path=path/> }.into_any();
    }

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
        let trace_a = run_a_id.try_get().unwrap_or_default();
        let trace_b = run_b_id.try_get().unwrap_or_default();

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
                    diff_error.set(Some(e.user_message()));
                    diff_loading.set(false);
                }
            }
        });
    };

    view! {
        <PageScaffold
            title="Run Diff"
            subtitle="Compare diagnostic runs and launch into the Run Detail diff tab"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Observe", "/runs"),
                PageBreadcrumbItem::current("Run Diff"),
            ]
        >
            <PageScaffoldActions slot>
                <Button variant=ButtonVariant::Outline on:click=move |_| refetch_runs.run(())>
                    "Refresh Runs"
                </Button>
            </PageScaffoldActions>

            // Run selectors
            <Card>
                <div class="space-y-4">
                    <h2 class="heading-4">"Select Runs to Compare"</h2>
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
                        <Button
                            variant=ButtonVariant::Primary
                            disabled=Signal::derive(move || diff_loading.try_get().unwrap_or(false) || run_a_id.try_get().unwrap_or_default().is_empty() || run_b_id.try_get().unwrap_or_default().is_empty())
                            on_click=Callback::new(do_compare)
                        >
                            {move || if diff_loading.try_get().unwrap_or(false) { "Comparing..." } else { "Compare Runs" }}
                        </Button>
                        {move || {
                            let run_a = run_a_id.try_get().unwrap_or_default();
                            let run_b = run_b_id.try_get().unwrap_or_default();
                            if run_a.is_empty() || run_b.is_empty() {
                                return view! {}.into_any();
                            }
                            let href = format!("/runs/{}?tab=diff&compare={}", run_a, run_b);
                            view! {
                                <Link href=href class="text-sm">
                                    "Open in Run Detail"
                                </Link>
                            }.into_any()
                        }}
                        {move || diff_error.try_get().flatten().map(|e| view! {
                            <span class="text-destructive text-sm">{e}</span>
                        })}
                    </div>
                </div>
            </Card>

            // Diff results
            {move || {
                if diff_loading.try_get().unwrap_or(false) {
                    view! {
                        <Card>
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                                <span class="ml-2 text-muted-foreground">"Comparing runs..."</span>
                            </div>
                        </Card>
                    }.into_any()
                } else if let Some(result) = diff_result.try_get().flatten() {
                    view! { <DiffResults result=result/> }.into_any()
                } else {
                    view! {
                        <Card>
                            <EmptyState
                                title="No comparison selected"
                                description="Select two runs and click Compare to see differences."
                                variant=EmptyStateVariant::Empty
                            />
                        </Card>
                    }.into_any()
                }
            }}
        </PageScaffold>
    }.into_any()
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
            prop:value=move || selected.try_get().unwrap_or_default()
        >
            <option value="">"-- Select a run --"</option>
            {move || {
                match runs.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Loaded(data) => {
                        let exclude_id = exclude.try_get().unwrap_or_default();
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
