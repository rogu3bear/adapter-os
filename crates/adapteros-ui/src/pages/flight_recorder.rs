//! Runs page - Canonical provenance viewer
//!
//! The Run Detail hub provides unified access to:
//! - Overview (summary, status, timing)
//! - Trace (full trace visualization via TraceViewer)
//! - Receipt (cryptographic verification)
//! - Routing (K-sparse routing decisions)
//! - Tokens (token accounting and cache stats)
//! - Diff (compare with another run)

use crate::api::types::{ExecuteReplayRequest, ExecuteReplayResponse, ReceiptVerificationResult};
use crate::api::{use_api_client, ApiClient, UiInferenceTraceDetailResponse};
use crate::components::{
    ActionCard, ActionCardVariant, AsyncBoundary, Badge, BadgeVariant, Button, ButtonVariant, Card,
    Checkbox, CopyableId, Dialog, DiffResults, EmptyState, EmptyStateVariant, Link,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Select, SkeletonDetailSection, Spinner,
    SplitPanel, SplitRatio, StatusVariant, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow, TokenDecisionsPaged, TraceViewerWithData, VirtualTableBody,
};
use crate::components::{ButtonSize, Input};
use crate::constants::pagination::TOKEN_DECISIONS_PAGE_SIZE;
use crate::constants::ui_language;
use crate::hooks::{use_api_resource, use_list_controls, use_polling, LoadingState};
use crate::pages::diff::RunSelector;
use crate::signals::{perf_logging_enabled, use_notifications, use_ui_profile, NotificationAction};
use adapteros_api_types::diagnostics::{
    DiagDiffRequest, DiagDiffResponse, DiagEventResponse, DiagExportResponse, DiagRunResponse,
    ListDiagRunsQuery, ListDiagRunsResponse, StageTiming,
};
use adapteros_api_types::errors::ErrorInstance;
use adapteros_api_types::UiProfile;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use std::sync::Arc;
use web_time::Instant;

/// Runs list page - shows all diagnostic runs
#[component]
pub fn FlightRecorder() -> impl IntoView {
    // Selected run ID for split panel detail
    let selected_run_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());

    // Fetch diagnostic runs with filtering
    let (runs, refetch_runs) = use_api_resource(move |client: Arc<ApiClient>| {
        let filter = status_filter.try_get().unwrap_or_default();
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

    view! {
        <PageScaffold
            title=ui_language::SYSTEM_RESTORE_POINTS
            subtitle="Timeline of execution states, signed logs, and exact restore opportunities."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Observe", "/runs"),
                PageBreadcrumbItem::current(ui_language::SYSTEM_RESTORE_POINTS),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    size=ButtonSize::Sm
                    on_click=Callback::new(move |_| refetch_runs.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <div class="mb-3 flex items-center gap-2 rounded-lg border border-border/60 bg-card/60 px-3 py-2">
                <span class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    "Status"
                </span>
                <Select
                    value=status_filter
                    options=vec![
                        ("".to_string(), "All".to_string()),
                        ("running".to_string(), "Running".to_string()),
                        ("completed".to_string(), "Completed".to_string()),
                        ("failed".to_string(), "Failed".to_string()),
                        ("cancelled".to_string(), "Cancelled".to_string()),
                    ]
                    class="w-36".to_string()
                />
            </div>

            <SplitPanel
                has_selection=Signal::derive(move || selected_run_id.try_get().flatten().is_some())
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Restore Points"
                ratio=SplitRatio::Half
                list_panel=move || {
                    view! {
                        <AsyncBoundary
                            state=runs
                            on_retry=Callback::new(move |_| refetch_runs.run(()))
                            render=move |response: ListDiagRunsResponse| {
                                if response.runs.is_empty() {
                                    view! {
                                        <Card>
                                            <EmptyState
                                                title="No restore points yet"
                                                description="Execution snapshots appear here after prompts complete."
                                                variant=EmptyStateVariant::Empty
                                            />
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
                        />
                    }
                }
                detail_panel=move || {
                    view! {
                        {move || {
                            selected_run_id.try_get().flatten().map(|run_id| {
                                view! {
                                    <RunDetailHub
                                        run_id=run_id
                                        on_close=Callback::new(move |_| on_close_detail())
                                    />
                                }
                            })
                        }}
                    }
                }
            />
        </PageScaffold>
    }
}

/// Runs detail page - accessed via /runs/:id route
/// This is the canonical Run Detail hub for provenance
#[component]
pub fn FlightRecorderDetail() -> impl IntoView {
    view! {
        <crate::components::DetailPageShell
            title="Restore Point Detail"
            section="Observe"
            section_href="/runs"
            entity_plural="Restore Points"
            list_href="/runs"
        >
            {move || {
                let id = expect_context::<crate::components::DetailEntityId>().get();
                view! {
                    <RunDetailHub
                        run_id=id
                        on_close=Callback::new(|_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window.history().and_then(|h| h.back());
                            }
                        })
                    />
                }
            }}
        </crate::components::DetailPageShell>
    }
}

/// Client-side page size for runs table
const RUNS_PAGE_SIZE: usize = 25;

/// Runs table component with search and client-side pagination
#[component]
fn RunsTable(
    runs: Vec<DiagRunResponse>,
    selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let items = Signal::derive({
        let runs = runs.clone();
        move || runs.clone()
    });

    let controls = use_list_controls(
        items,
        |run: &DiagRunResponse, query: &str| {
            run.id.to_lowercase().contains(query)
                || run.trace_id.to_lowercase().contains(query)
                || run.status.to_lowercase().contains(query)
        },
        RUNS_PAGE_SIZE,
    );
    let visible_runs = Signal::derive(move || controls.visible_items.try_get().unwrap_or_default());

    view! {
        <Card>
            // Search bar
            <div class="flex items-center justify-between gap-3 pb-3 border-b border-border mb-0">
                <Input
                    value=controls.search
                    placeholder="Search by run ID, trace ID, or status..."
                    input_type="text".to_string()
                />
                <span class="text-xs text-muted-foreground whitespace-nowrap">
                    {move || {
                        let filtered = controls.filtered_count.try_get().unwrap_or_default();
                        let total = controls.total_count.try_get().unwrap_or_default();
                        if filtered == total {
                            format!("{} runs", total)
                        } else {
                            format!("{} of {} runs", filtered, total)
                        }
                    }}
                </span>
            </div>

            <VirtualTableBody
                items=visible_runs
                row_height=56
                max_visible_rows=10
                overscan=4
                header={view! {
                    <TableHeader>
                        <TableRow>
                            <TableHead>"Run ID"</TableHead>
                            <TableHead>"Status"</TableHead>
                            <TableHead>"Duration"</TableHead>
                            <TableHead>"Events"</TableHead>
                            <TableHead>"Started"</TableHead>
                        </TableRow>
                    </TableHeader>
                }.into_any()}
                render_row=move |run, _| {
                    let run_id_for_row_click = run.id.clone();
                    let run_id_for_row_keydown = run.id.clone();
                    let run_id_for_link = run.id.clone();
                    let run_id_for_display = truncate_id(&run.id);
                    let row_label = format!("Open run {}", run.id);
                    let rid = run.id.clone();
                    let on_select_click = on_select.clone();
                    let on_select_keydown = on_select.clone();

                    view! {
                        <tr
                            class="table-row table-row-interactive cursor-pointer hover:bg-muted/50"
                            role="button"
                            tabindex=0
                            aria-label=row_label
                            class:bg-accent=move || selected_id.try_get().flatten().as_ref() == Some(&rid)
                            on:click=move |_| on_select_click.run(run_id_for_row_click.clone())
                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                let key = e.key();
                                if key == "Enter" || key == " " || key == "Spacebar" {
                                    e.prevent_default();
                                    on_select_keydown.run(run_id_for_row_keydown.clone());
                                }
                            }
                        >
                            <TableCell>
                                <a
                                    href=format!("/runs/{}", run_id_for_link)
                                    class="font-mono text-sm text-primary hover:underline"
                                    on:click=move |e: web_sys::MouseEvent| {
                                        // Ctrl/Cmd+click or middle-click: let browser open link
                                        if e.ctrl_key() || e.meta_key() || e.button() != 0 {
                                            e.stop_propagation();
                                        } else {
                                            // Plain left click: prevent navigation, let row handler select
                                            e.prevent_default();
                                        }
                                    }
                                >
                                    {run_id_for_display}
                                </a>
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
                        </tr>
                    }
                }
                debug_label="runs-table".to_string()
            />

            // Pagination
            {move || {
                let total_pages = controls.total_pages.try_get().unwrap_or_default();
                (total_pages > 1).then(|| {
                    let current = controls.page.try_get().unwrap_or_default();
                    view! {
                        <div class="flex items-center justify-center gap-2 py-3 border-t border-border">
                            <Button
                                variant=ButtonVariant::Outline
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || !controls.has_prev.try_get().unwrap_or(false))
                                on_click=Callback::new({
                                    let controls = controls.clone();
                                    move |_| controls.prev_page()
                                })
                            >
                                "Previous"
                            </Button>
                            <span class="text-sm text-muted-foreground">
                                {format!("Page {} of {}", current, total_pages)}
                            </span>
                            <Button
                                variant=ButtonVariant::Outline
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || !controls.has_next.try_get().unwrap_or(false))
                                on_click=Callback::new({
                                    let controls = controls.clone();
                                    move |_| controls.next_page()
                                })
                            >
                                "Next"
                            </Button>
                        </div>
                    }
                })
            }}
        </Card>
    }
}

/// Status badge component
#[component]
fn StatusBadge(status: String) -> impl IntoView {
    let variant = StatusVariant::from_status(&status).to_badge_variant();
    view! {
        <Badge variant=variant>{status}</Badge>
    }
}

// ============================================================================
// Run Detail Hub - Canonical provenance viewer
// ============================================================================

/// Tab enum for the Run Detail hub
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunDetailTab {
    #[default]
    Overview,
    Trace,
    Receipt,
    Routing,
    Errors,
    Tokens,
    Diff,
    Events,
    Replay,
}

impl RunDetailTab {
    fn from_str(s: &str) -> Self {
        match s {
            "trace" => Self::Trace,
            "receipt" => Self::Receipt,
            "routing" => Self::Routing,
            "errors" => Self::Errors,
            "tokens" => Self::Tokens,
            "diff" => Self::Diff,
            "events" => Self::Events,
            "replay" => Self::Replay,
            _ => Self::Overview,
        }
    }
}

/// Canonical Run Detail hub - unified provenance viewer
///
/// This is the single source of truth for "what happened in this run?"
/// Unifies: trace, receipt, routing decisions, token accounting, events
#[component]
fn RunDetailHub(run_id: String, on_close: Callback<()>) -> impl IntoView {
    // Get initial tab from URL query param
    let query = use_query_map();
    let initial_tab = query
        .get()
        .get("tab")
        .map(|t| RunDetailTab::from_str(&t))
        .unwrap_or_default();
    let compare_trace = query.get().get("compare");
    let ui_profile = use_ui_profile();

    // Tab state
    let active_tab = RwSignal::new(initial_tab);
    let receipt_digest = RwSignal::new(None::<String>);
    let trace_detail_cache = RwSignal::new(None::<UiInferenceTraceDetailResponse>);
    let trace_detail_cache_id = RwSignal::new(None::<String>);
    let trace_detail_started_at = RwSignal::new(None::<Instant>);
    let trace_detail_ready_logged = RwSignal::new(false);
    let perf_enabled = perf_logging_enabled();

    // Fetch run export (includes events and timing)
    let run_id_clone = run_id.clone();
    let (export_data, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = run_id_clone.clone();
        async move {
            match client.export_diag_run(&id).await {
                Ok(export) => Ok(export),
                Err(primary_err) => {
                    // Log fallback attempt
                    web_sys::console::warn_1(
                        &format!(
                            "Primary export lookup failed: {}, trying trace_id search",
                            primary_err
                        )
                        .into(),
                    );
                    let runs = client
                        .list_diag_runs(&ListDiagRunsQuery {
                            limit: Some(200),
                            ..Default::default()
                        })
                        .await?;
                    if let Some(run) = runs.runs.into_iter().find(|r| r.trace_id == id) {
                        web_sys::console::log_1(
                            &format!(
                                "Found run via fallback: trace_id={} -> run_id={}",
                                id, run.id
                            )
                            .into(),
                        );
                        client.export_diag_run(&run.id).await
                    } else {
                        Err(primary_err)
                    }
                }
            }
        }
    });
    let run_id_for_diff_link = run_id.clone();

    view! {
        <div class="space-y-4">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <div class="flex items-center gap-2">
                        <h2 class="heading-3">"Restore Point Detail"</h2>
                        <a
                            href=format!("/runs/{}", run_id)
                            class="text-xs text-muted-foreground hover:text-primary"
                            title="Open in full page"
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/>
                            </svg>
                        </a>
                    </div>
                    <CopyableId id=run_id.clone() truncate=28 />
                </div>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| on_close.run(()))
                >
                    "Close"
                </Button>
            </div>

            // Quick Actions bar
            <div class="flex items-center gap-2 flex-wrap">
                <QuickActionButton
                    icon="📋"
                    label="Copy Restore Point ID"
                    action=QuickAction::CopyText(run_id.clone())
                />
                <QuickActionButton
                    icon="🔗"
                    label="Copy Signed Log Fingerprint"
                    action=QuickAction::CopyReceiptHash(receipt_digest.read_only())
                />
                <QuickActionButton
                    icon="📥"
                    label="Export"
                    action=QuickAction::Export(run_id.clone())
                />
                <QuickActionButton
                    icon="🔏"
                    label="Download Tamper-Proof Receipt"
                    action=QuickAction::DownloadSignature(run_id.clone())
                />
                {move || {
                    if ui_profile.try_get().unwrap_or(UiProfile::Full) == UiProfile::Full {
                        Some(view! {
                            <a
                                href=format!("/diff?run={}", run_id_for_diff_link.clone())
                                class="inline-flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors"
                            >
                                <span>"↔"</span>
                                <span>"Compare Restore Points"</span>
                            </a>
                        })
                    } else {
                        None
                    }
                }}
            </div>

            // Tabs navigation
            <div class="border-b border-border">
                <nav class="flex gap-1">
                    <RunDetailTabButton tab=RunDetailTab::Overview active=active_tab label="Overview"/>
                    <RunDetailTabButton tab=RunDetailTab::Trace active=active_tab label="Trace"/>
                    <RunDetailTabButton tab=RunDetailTab::Receipt active=active_tab label=ui_language::SIGNED_SYSTEM_LOGS/>
                    <RunDetailTabButton tab=RunDetailTab::Routing active=active_tab label="Routing"/>
                    <RunDetailTabButton tab=RunDetailTab::Errors active=active_tab label="Errors"/>
                    {move || {
                        if ui_profile.try_get().unwrap_or(UiProfile::Full) == UiProfile::Full {
                            Some(view! {
                                <RunDetailTabButton tab=RunDetailTab::Tokens active=active_tab label="Tokens"/>
                                <RunDetailTabButton tab=RunDetailTab::Diff active=active_tab label="Diff"/>
                                <RunDetailTabButton tab=RunDetailTab::Events active=active_tab label=ui_language::EVENT_VIEWER/>
                                <RunDetailTabButton tab=RunDetailTab::Replay active=active_tab label=ui_language::SYSTEM_RESTORE_POINTS/>
                            })
                        } else {
                            None
                        }
                    }}
                </nav>
            </div>

            // Tab content
            {move || match export_data.try_get().unwrap_or_default() {
                LoadingState::Idle | LoadingState::Loading => view! {
                    <SkeletonDetailSection rows=6 has_title=true/>
                }.into_any(),
                LoadingState::Loaded(export) => {
                    let trace_id = export.run.trace_id.clone();

                    // Single fetch for trace detail - shared by all tabs that need it.
                    // Cache to avoid redundant fetches on tab switches or re-renders.
                    let (trace_detail, refetch_trace_detail) = use_api_resource({
                        let tid = trace_id.clone();
                        let cache = trace_detail_cache;
                        let cache_id = trace_detail_cache_id;
                        let perf_enabled = perf_enabled;
                        move |client: Arc<ApiClient>| {
                            let tid = tid.clone();
                            async move {
                                if cache_id.get_untracked().as_deref() == Some(tid.as_str()) {
                                    if let Some(cached) = cache.get_untracked() {
                                        return Ok(cached);
                                    }
                                }
                                trace_detail_started_at.set(Some(Instant::now()));
                                let started_at = Instant::now();
                                let detail = client
                                    .get_inference_trace_detail(
                                        &tid,
                                        Some(TOKEN_DECISIONS_PAGE_SIZE),
                                        None,
                                    )
                                    .await?;
                                if perf_enabled {
                                    let elapsed_ms = started_at.elapsed().as_millis();
                                    web_sys::console::log_1(
                                        &format!(
                                            "[perf] run detail trace load: {}ms (trace_id={})",
                                            elapsed_ms, tid
                                        )
                                        .into(),
                                    );
                                }
                                cache_id.set(Some(tid.clone()));
                                cache.set(Some(detail.clone()));
                                Ok(detail)
                            }
                        }
                    });

                    // Force a fresh fetch (bypass the local cache) for actions like "Verify receipt".
                    let refetch_trace_detail_fresh = {
                        let cache = trace_detail_cache;
                        let cache_id = trace_detail_cache_id;
                        Callback::new(move |_| {
                            let _ = cache.try_set(None);
                            let _ = cache_id.try_set(None);
                            refetch_trace_detail.run(());
                        })
                    };

                    view! {
                        <TabContent
                            export=export
                            active_tab=active_tab
                            trace_detail=trace_detail
                            refetch_trace_detail=refetch_trace_detail_fresh
                            compare_trace=compare_trace.clone()
                            receipt_digest=receipt_digest.write_only()
                            trace_detail_started_at=trace_detail_started_at
                            trace_detail_ready_logged=trace_detail_ready_logged
                            perf_enabled=perf_enabled
                        />
                    }.into_any()
                }
                LoadingState::Error(err) => {
                    // /runs/:id is used both for diagnostic-run IDs and inference trace IDs.
                    // If the diagnostic export path fails (common for pure trace IDs),
                    // degrade gracefully to a trace-only viewer instead of hard erroring the page.
                    if run_id.starts_with("trc-") || err.is_not_found() {
                        let fallback_message = if err.is_not_found() {
                            "Run export not found; attempting trace-only fallback."
                        } else {
                            "Diagnostic export unavailable; showing trace-only view."
                        };
                        view! {
                            <div class="space-y-3">
                                <div class="text-sm text-muted-foreground">
                                    {fallback_message}
                                </div>
                                <a href="/runs" class="inline-flex text-sm text-primary hover:underline">
                                    "Back to Restore Points"
                                </a>
                                <crate::components::trace_viewer::TraceViewer trace_id=run_id.clone() compact=false/>
                            </div>
                        }.into_any()
                    } else {
                        let err = err.user_message();
                        view! {
                            <div class="bg-destructive/10 border border-destructive/20 rounded-lg p-4">
                                <div class="flex items-center gap-2 text-destructive">
                                    <span class="font-medium">"Error loading run"</span>
                                </div>
                                <p class="text-sm text-muted-foreground mt-2">{err}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Tab content router - renders the appropriate tab based on active selection
#[component]
fn TabContent(
    export: DiagExportResponse,
    active_tab: RwSignal<RunDetailTab>,
    trace_detail: ReadSignal<crate::hooks::LoadingState<UiInferenceTraceDetailResponse>>,
    refetch_trace_detail: Callback<()>,
    compare_trace: Option<String>,
    receipt_digest: WriteSignal<Option<String>>,
    trace_detail_started_at: RwSignal<Option<Instant>>,
    trace_detail_ready_logged: RwSignal<bool>,
    perf_enabled: bool,
) -> impl IntoView {
    Effect::new(move |_| {
        let Some(LoadingState::Loaded(detail)) = trace_detail.try_get() else {
            return;
        };
        let digest = detail.receipt.map(|receipt| receipt.receipt_digest);
        let _ = receipt_digest.try_set(digest);
        if perf_enabled && !trace_detail_ready_logged.get_untracked() {
            if let Some(started_at) = trace_detail_started_at.get_untracked() {
                let elapsed_ms = started_at.elapsed().as_millis();
                web_sys::console::log_1(
                    &format!("[perf] run detail ready: {}ms", elapsed_ms).into(),
                );
            }
            let _ = trace_detail_ready_logged.try_set(true);
        }
    });

    view! {
        {move || {
            let export = export.clone();
            match active_tab.try_get().unwrap_or_default() {
                RunDetailTab::Overview => {
                    view! { <OverviewTab export=export trace_detail=trace_detail/> }.into_any()
                }
                RunDetailTab::Trace => {
                    view! { <TraceTab trace_detail=trace_detail/> }.into_any()
                }
                RunDetailTab::Receipt => {
                    view! { <ReceiptsTab export=export trace_detail=trace_detail refetch_trace_detail=refetch_trace_detail/> }.into_any()
                }
                RunDetailTab::Routing => {
                    view! { <RoutingTab export=export trace_detail=trace_detail/> }.into_any()
                }
                RunDetailTab::Errors => {
                    view! { <ErrorsTab trace_id=export.run.trace_id.clone()/> }.into_any()
                }
                RunDetailTab::Tokens => {
                    view! { <TokensTab export=export trace_detail=trace_detail/> }.into_any()
                }
                RunDetailTab::Diff => {
                    view! { <DiffTab export=export compare_trace=compare_trace.clone()/> }.into_any()
                }
                RunDetailTab::Events => {
                    view! { <EventsTab export=export/> }.into_any()
                }
                RunDetailTab::Replay => {
                    view! { <ReplayTab trace_id=export.run.trace_id.clone()/> }.into_any()
                }
            }
        }}
    }
}

/// Tab button for the Run Detail hub
#[component]
fn RunDetailTabButton(
    tab: RunDetailTab,
    active: RwSignal<RunDetailTab>,
    label: &'static str,
) -> impl IntoView {
    let is_active = move || active.try_get().unwrap_or_default() == tab;
    view! {
        <button
            class=move || {
                if is_active() {
                    "py-2 px-3 border-b-2 border-primary text-primary font-medium text-sm"
                } else {
                    "py-2 px-3 border-b-2 border-transparent text-muted-foreground hover:text-foreground text-sm"
                }
            }
            on:click=move |_| active.set(tab)
        >
            {label}
        </button>
    }
}

/// Quick action type for Run Detail buttons
#[derive(Clone)]
enum QuickAction {
    /// Copy text to clipboard
    CopyText(String),
    /// Copy receipt hash (requires fetching from export)
    CopyReceiptHash(ReadSignal<Option<String>>),
    /// Export run data
    Export(String),
    /// Download signature file (requires bundle creation)
    DownloadSignature(String),
    /// Download receipt JSON file (fetches from API and triggers browser download)
    DownloadReceipt(ReadSignal<Option<String>>),
}

/// Quick action button component
#[component]
fn QuickActionButton(
    icon: &'static str,
    label: &'static str,
    action: QuickAction,
) -> impl IntoView {
    let (copied, set_copied) = signal(false);
    let notifications = use_notifications();
    let client = use_api_client();

    let on_click = move |_| {
        match action.clone() {
            QuickAction::CopyText(text) => {
                copy_to_clipboard(&text, set_copied, notifications.clone(), "Run ID");
            }
            QuickAction::CopyReceiptHash(run_id) => {
                let Some(digest) = run_id.try_get().flatten() else {
                    notifications.error(
                        "Signed log fingerprint unavailable",
                        "Signed log fingerprint is not available for this restore point yet.",
                    );
                    return;
                };
                copy_to_clipboard(
                    &digest,
                    set_copied,
                    notifications.clone(),
                    "Signed log fingerprint",
                );
            }
            QuickAction::Export(run_id) => {
                // Export diagnostic run as JSON (API is mounted at root, not /api).
                let notifs = notifications.clone();
                let client = client.clone();
                spawn_local(async move {
                    match client.export_diag_run(&run_id).await {
                        Ok(export) => match serde_json::to_string_pretty(&export) {
                            Ok(json_content) => {
                                let filename =
                                    format!("diag-export-{}.json", &run_id[..16.min(run_id.len())]);
                                if let Err(e) = trigger_download(&filename, &json_content) {
                                    notifs.error(
                                        "Download failed",
                                        &format!("Could not download export: {}", e),
                                    );
                                } else {
                                    notifs.success(
                                        "Download started",
                                        "Diagnostic export download initiated.",
                                    );
                                }
                            }
                            Err(e) => {
                                notifs.error(
                                    "Export failed",
                                    &format!("Could not serialize export: {}", e),
                                );
                            }
                        },
                        Err(e) => {
                            notifs.error("Export failed", &format!("Could not export run: {}", e));
                        }
                    }
                });
            }
            QuickAction::DownloadSignature(trace_id) => {
                // Create bundle and download signature
                let notifs = notifications.clone();
                let client = client.clone();
                spawn_local(async move {
                    // Create bundle export (this generates the signature)
                    match client.create_bundle_export(&trace_id).await {
                        Ok(bundle) => {
                            // Open signature download URL in new tab
                            let sig_url = client.signature_download_url(&bundle.export_id);
                            if let Some(window) = web_sys::window() {
                                let _ = window.open_with_url_and_target(&sig_url, "_blank");
                            }
                            notifs.success("Signature ready", "Signature file download started.");
                        }
                        Err(e) => {
                            notifs.error(
                                "Signature download failed",
                                &format!("Could not generate signature: {}", e),
                            );
                        }
                    }
                });
            }
            QuickAction::DownloadReceipt(digest_signal) => {
                // Download receipt JSON file
                let Some(digest) = digest_signal.try_get().flatten() else {
                    notifications.error(
                        "Signed log receipt unavailable",
                        "Signed log fingerprint is not available for this restore point yet.",
                    );
                    return;
                };
                let notifs = notifications.clone();
                let client = client.clone();
                spawn_local(async move {
                    match client.get_receipt_json(&digest).await {
                        Ok(json_content) => {
                            let filename =
                                format!("receipt-{}.json", &digest[..16.min(digest.len())]);
                            if let Err(e) = trigger_download(&filename, &json_content) {
                                notifs.error(
                                    "Download failed",
                                    &format!("Could not download signed log receipt: {}", e),
                                );
                            } else {
                                notifs.success(
                                    "Download started",
                                    "Signed log receipt download initiated.",
                                );
                            }
                        }
                        Err(e) => {
                            notifs.error(
                                "Signed log receipt download failed",
                                &format!("Could not fetch signed log receipt: {}", e),
                            );
                        }
                    }
                });
            }
        }
    };

    view! {
        <button
            class="inline-flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors"
            on:click=on_click
            title=label
        >
            <span>{icon}</span>
            <span>{move || if copied.try_get().unwrap_or(false) { "Copied!" } else { label }}</span>
        </button>
    }
}

/// Copy text to clipboard and reset copied state after timeout
fn copy_to_clipboard(
    text: &str,
    set_copied: WriteSignal<bool>,
    notifications: NotificationAction,
    label: &str,
) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let text = text.to_string();
    let label = label.to_string();
    spawn_local(async move {
        let success = async {
            let window = web_sys::window()?;
            let navigator = window.navigator();

            // Get clipboard from navigator using JS reflection
            let clipboard =
                js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
                    .ok()
                    .filter(|v| !v.is_undefined())?;

            // Call writeText method
            let write_text_fn =
                js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText"))
                    .ok()?;
            let write_text_fn = write_text_fn.dyn_ref::<js_sys::Function>()?;
            let promise = write_text_fn
                .call1(&clipboard, &wasm_bindgen::JsValue::from_str(&text))
                .ok()?;
            let promise = promise.dyn_into::<js_sys::Promise>().ok()?;

            JsFuture::from(promise).await.ok()?;
            Some(())
        }
        .await;

        if success.is_some() {
            let _ = set_copied.try_set(true);
            notifications.success(
                "Copied to clipboard",
                &format!("{} copied to clipboard.", label),
            );

            // Reset after 2 seconds
            let window = web_sys::window();
            if let Some(window) = window {
                let callback = Closure::once(Box::new(move || {
                    let _ = set_copied.try_set(false);
                }) as Box<dyn FnOnce()>);

                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    2000,
                );
                callback.forget();
            }
        } else {
            notifications.error(
                "Clipboard copy failed",
                &format!("Could not copy {} to clipboard.", label),
            );
            // Log clipboard failure for debugging
            web_sys::console::warn_1(
                &"Clipboard copy failed - API unavailable or permission denied".into(),
            );
        }
    });
}

/// Trigger a browser file download with the given content.
///
/// Creates a Blob from the content, generates an object URL, creates a hidden
/// anchor element to trigger the download, then revokes the URL.
fn trigger_download(filename: &str, content: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or("No window available")?;
    let document = window.document().ok_or("No document available")?;

    // Create a Blob from the content with JSON mime type
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&wasm_bindgen::JsValue::from_str(content));

    let blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type("application/json");

    let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &blob_options)
        .map_err(|e| format!("Failed to create Blob: {:?}", e))?;

    // Create object URL from blob
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Failed to create object URL: {:?}", e))?;

    // Create hidden anchor element
    let anchor = document
        .create_element("a")
        .map_err(|e| format!("Failed to create anchor: {:?}", e))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "Failed to cast to HtmlAnchorElement")?;

    anchor.set_href(&url);
    anchor.set_download(filename);

    // Append to document, click, then remove
    let body = document.body().ok_or("No document body")?;
    body.append_child(&anchor)
        .map_err(|e| format!("Failed to append anchor: {:?}", e))?;
    anchor.click();
    body.remove_child(&anchor)
        .map_err(|e| format!("Failed to remove anchor: {:?}", e))?;

    // Revoke the object URL to free memory
    let _ = web_sys::Url::revoke_object_url(&url);

    Ok(())
}

// ============================================================================
// Tab Content Components
// ============================================================================

/// Overview tab - run summary, status, timing, adapters
#[component]
fn OverviewTab(
    export: DiagExportResponse,
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
) -> impl IntoView {
    let timing = export.timing_summary.clone().unwrap_or_default();
    let status = export.run.status.clone();
    let duration_str = format_duration_ms(export.run.duration_ms);
    let events_count = export.run.total_events_count;
    let dropped_count = export.run.dropped_events_count;
    let started_at = format_timestamp_ms(export.run.started_at_unix_ms);

    // Extract backend info from events (look for inference-related events)
    let events = export.events.clone().unwrap_or_default();
    let reasoning_mode = extract_reasoning_mode_from_events(&events);
    let ui_profile = use_ui_profile();

    // Calculate total duration for percentage bars
    let total_us: i64 = timing.iter().filter_map(|s| s.duration_us).sum();

    view! {
        <div class="space-y-4">
            // Summary card
            <Card title="Run Summary".to_string()>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Status"</p>
                        <StatusBadge status=status/>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Duration"</p>
                        <p class="font-medium">{duration_str}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">{ui_language::EVENT_VIEWER}</p>
                        <p class="font-medium">
                            {events_count}
                            {if dropped_count > 0 {
                                format!(" ({} dropped)", dropped_count)
                            } else {
                                String::new()
                            }}
                        </p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Started"</p>
                        <p class="text-sm">{started_at}</p>
                    </div>
                </div>
            </Card>

            // Inputs (hash-only by default)
            <Card title="Inputs".to_string()>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <CopyableId
                        id=export.run.request_hash.clone()
                        label="Prompt hash".to_string()
                        truncate=28
                    />
                    {export.run.manifest_hash.clone().map(|hash| view! {
                        <CopyableId
                            id=hash
                            label="Manifest hash".to_string()
                            truncate=28
                        />
                    })}
                </div>
            </Card>

            // Configuration section - shows stack/model/policy/backend used
            <Card title="Configuration".to_string()>
                <div class="grid grid-cols-2 md:grid-cols-6 gap-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Stack"</p>
                        {move || {
                            match trace_detail.try_get().unwrap_or_default() {
                                LoadingState::Loaded(detail) => {
                                    if let Some(stack_id) = detail.stack_id.clone() {
                                        view! { <p class="font-medium text-sm">{stack_id}</p> }.into_any()
                                    } else {
                                        view! { <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p> }.into_any()
                                    }
                                }
                                _ => view! {
                                    <span class="inline-flex items-center gap-2 text-muted-foreground/80">
                                        <Spinner/>
                                        <span class="text-xs">"Loading details"</span>
                                    </span>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Model"</p>
                        {move || {
                            match trace_detail.try_get().unwrap_or_default() {
                                LoadingState::Loaded(detail) => {
                                    if let Some(model_id) = detail.model_id.clone() {
                                        view! { <p class="font-medium text-sm">{model_id}</p> }.into_any()
                                    } else {
                                        view! { <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p> }.into_any()
                                    }
                                }
                                _ => view! {
                                    <span class="inline-flex items-center gap-2 text-muted-foreground/80">
                                        <Spinner/>
                                        <span class="text-xs">"Loading details"</span>
                                    </span>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Policy"</p>
                        {move || {
                            match trace_detail.try_get().unwrap_or_default() {
                                LoadingState::Loaded(detail) => {
                                    if let Some(policy_id) = detail.policy_id.clone() {
                                        view! { <p class="font-medium text-sm">{policy_id}</p> }.into_any()
                                    } else {
                                        view! { <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p> }.into_any()
                                    }
                                }
                                _ => view! {
                                    <span class="inline-flex items-center gap-2 text-muted-foreground/80">
                                        <Spinner/>
                                        <span class="text-xs">"Loading details"</span>
                                    </span>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Backend"</p>
                        {move || {
                            match trace_detail.try_get().unwrap_or_default() {
                                LoadingState::Loaded(detail) => {
                                    if let Some(backend) = detail.backend_id.clone() {
                                        view! {
                                            <p class="font-medium">
                                                <Badge variant=BadgeVariant::Secondary>{backend}</Badge>
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p>
                                        }.into_any()
                                    }
                                }
                                _ => view! {
                                    <span class="inline-flex items-center gap-2 text-muted-foreground/80">
                                        <Spinner/>
                                        <span class="text-xs">"Loading details"</span>
                                    </span>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Adapters"</p>
                        {move || {
                            match trace_detail.try_get().unwrap_or_default() {
                                LoadingState::Loaded(detail) => {
                                    // Clone to avoid lifetime issues with view rendering
                                    let adapters = detail.adapters_used.clone();
                                    if adapters.is_empty() {
                                        view! { <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p> }.into_any()
                                    } else {
                                        view! {
                                            <div class="flex flex-wrap gap-1.5">
                                                {adapters.into_iter().map(|adapter_id| {
                                                    let href = format!("/adapters/{}", adapter_id);
                                                    view! {
                                                        <a href=href class="no-underline">
                                                            <Badge variant=BadgeVariant::Secondary>{adapter_id}</Badge>
                                                        </a>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                                _ => view! {
                                    <span class="inline-flex items-center gap-2 text-muted-foreground/80">
                                        <Spinner/>
                                        <span class="text-xs">"Loading details"</span>
                                    </span>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Thinking Mode"</p>
                        {match reasoning_mode {
                            Some(true) => view! {
                                <Badge variant=BadgeVariant::Success>"Enabled"</Badge>
                            }.into_any(),
                            Some(false) => view! {
                                <Badge variant=BadgeVariant::Secondary>"Disabled"</Badge>
                            }.into_any(),
                            None => view! {
                                <p class="font-medium text-sm text-muted-foreground/70 italic">"Unknown"</p>
                            }.into_any(),
                        }}
                    </div>
                </div>
                {move || {
                    match trace_detail.try_get().unwrap_or_default() {
                        LoadingState::Loaded(detail) => {
                            let missing_ids = detail.stack_id.is_none()
                                && detail.model_id.is_none()
                                && detail.policy_id.is_none();
                            missing_ids.then(|| view! {
                                <p class="text-xs text-muted-foreground mt-3">
                                    "Stack, model, and policy are not included in the trace payload."
                                </p>
                            })
                        }
                        _ => None,
                    }
                }}

                <details class="text-xs text-muted-foreground mt-3">
                    <summary class="cursor-pointer hover:text-foreground">"Decode params"</summary>
                    <div class="grid grid-cols-2 md:grid-cols-4 gap-2 mt-2">
                        <div>"Temperature: " <span class="italic">"Unknown"</span></div>
                        <div>"Top-p: " <span class="italic">"Unknown"</span></div>
                        <div>"Top-k: " <span class="italic">"Unknown"</span></div>
                        <div>"Max tokens: " <span class="italic">"Unknown"</span></div>
                    </div>
                </details>
            </Card>

            // Provenance summary (UI-only receipt fields)
            <Card title="Provenance Summary".to_string()>
                <div data-testid="run-provenance-summary">
                    {move || match trace_detail.try_get().unwrap_or_default() {
                        LoadingState::Loaded(detail) => {
                            if let Some(receipt) = detail.receipt.clone() {
                                let (verified_label, verified_variant, verified_note) =
                                    match receipt.verified {
                                        Some(true) => (
                                            "Verified",
                                            BadgeVariant::Success,
                                            "Server recomputation matched the stored signed log fingerprint.",
                                        ),
                                        Some(false) => (
                                            "Mismatch",
                                            BadgeVariant::Destructive,
                                            "Signed log mismatch: stored fingerprint does not match recomputation.",
                                        ),
                                        None => (
                                            "Pending",
                                            BadgeVariant::Warning,
                                            "Signed log has not been recomputed by the server yet.",
                                        ),
                                    };
                                let backend_label = detail
                                    .backend_id
                                    .clone()
                                    .unwrap_or_else(|| "Unknown".to_string());
                                view! {
                                    <div class="space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <p class="text-sm text-muted-foreground">"Signed log status"</p>
                                                <Badge variant=verified_variant>{verified_label}</Badge>
                                            </div>
                                            <div class="text-right">
                                                <p class="text-sm text-muted-foreground">"Backend"</p>
                                                <Badge variant=BadgeVariant::Secondary>{backend_label}</Badge>
                                            </div>
                                        </div>
                                        <p class="text-xs text-muted-foreground">{verified_note}</p>
                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                            <ProvenanceField label="Input digest" value=receipt.input_digest_b3.clone()/>
                                            <ProvenanceField label="Output digest" value=Some(receipt.output_digest.clone())/>
                                            <ProvenanceField label="Signed log fingerprint" value=Some(receipt.receipt_digest.clone())/>
                                            <ProvenanceField label="Run head hash" value=Some(receipt.run_head_hash.clone())/>
                                            <ProvenanceField label="Seed lineage hash" value=receipt.seed_lineage_hash.clone()/>
                                            <ProvenanceField label="Backend attestation hash" value=receipt.backend_attestation_b3.clone()/>
                                        </div>
                                        // Training digests section
                                        {receipt.adapter_training_digests.clone().and_then(|digests| {
                                            if digests.is_empty() {
                                                None
                                            } else {
                                                Some(view! {
                                                    <div class="border-t border-border pt-3 mt-3">
                                                        <p class="text-xs text-muted-foreground mb-2">"Training Lineage"</p>
                                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                                            {digests.into_iter().enumerate().map(|(idx, digest)| {
                                                                let label = format!("Adapter {} training digest", idx + 1);
                                                                view! {
                                                                    <CopyableId
                                                                        id=digest
                                                                        label=label
                                                                        truncate=24
                                                                    />
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                    </div>
                                                })
                                            }
                                        })}
                                    </div>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <div class="text-sm text-muted-foreground italic">
                                        "Signed log details are not available for this restore point yet."
                                    </div>
                                }
                                .into_any()
                            }
                        }
                        LoadingState::Error(err) => view! {
                            <div class="text-sm text-muted-foreground">{format!("Failed to load provenance: {}", err)}</div>
                        }
                        .into_any(),
                        _ => view! {
                            <div class="text-sm text-muted-foreground italic">"Loading provenance..."</div>
                        }
                        .into_any(),
                    }}
                </div>
            </Card>

            // Stage timeline (if available)
            {if !timing.is_empty() {
                Some(view! {
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
                })
            } else {
                None
            }}

            // Quick links to other tabs
            <Card title="Provenance".to_string()>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
                    <ActionCard
                        href="?tab=trace"
                        icon="🔍"
                        title="Trace"
                        description="Timeline & metrics"
                        variant=ActionCardVariant::Subtle
                        centered=true
                    />
                    <ActionCard
                        href="?tab=receipt"
                        icon="✓"
                        title=ui_language::SIGNED_SYSTEM_LOGS
                        description="Verify fingerprints"
                        variant=ActionCardVariant::Subtle
                        centered=true
                    />
                    {move || {
                        if ui_profile.try_get().unwrap_or(UiProfile::Full) == UiProfile::Full {
                            Some(view! {
                                <ActionCard
                                    href="?tab=routing"
                                    icon="⚡"
                                    title="Routing"
                                    description="K-sparse decisions"
                                    variant=ActionCardVariant::Subtle
                                    centered=true
                                />
                                <ActionCard
                                    href="?tab=diff"
                                    icon="↔"
                                    title="Diff"
                                    description="Compare runs"
                                    variant=ActionCardVariant::Subtle
                                    centered=true
                                />
                            })
                        } else {
                            None
                        }
                    }}
                </div>
            </Card>
        </div>
    }
}

/// Provenance field with copy affordance.
#[component]
fn ProvenanceField(label: &'static str, value: Option<String>) -> impl IntoView {
    match value {
        Some(value) => {
            view! { <CopyableId id=value label=label.to_string() truncate=24/> }.into_any()
        }
        None => view! {
            <div class="flex flex-col gap-1">
                <span class="text-xs text-muted-foreground">{label}</span>
                <span class="text-xs text-muted-foreground italic">"Unavailable"</span>
            </div>
        }
        .into_any(),
    }
}

/// Stage row with duration bar
#[component]
fn StageRow(stage: StageTiming, pct: f64) -> impl IntoView {
    let status_class = if stage.success {
        "bg-status-success"
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

/// Trace tab - uses TraceViewerWithData to display pre-loaded trace data
#[component]
fn TraceTab(
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
) -> impl IntoView {
    view! {
        <div class="space-y-4">
            <p class="text-sm text-muted-foreground">
                "Full inference trace with timeline visualization, latency breakdown, and token-level routing decisions."
            </p>
            <TraceViewerWithData trace_detail=trace_detail compact=false/>
        </div>
    }
}

/// Receipts tab - shows hashes and verification status
#[component]
fn ReceiptsTab(
    export: DiagExportResponse,
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
    refetch_trace_detail: Callback<()>,
) -> impl IntoView {
    #[derive(serde::Serialize)]
    struct TraceVerifyRequest {
        trace_id: String,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct TraceVerifyResponse {
        pass: bool,
        #[serde(default)]
        reasons: Vec<String>,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ReceiptPanelTab {
        Summary,
        Details,
        Export,
    }

    let active_tab = RwSignal::new(ReceiptPanelTab::Summary);
    let receipt_digest = RwSignal::new(Option::<String>::None);
    let notifications = use_notifications();
    let verifying = RwSignal::new(false);
    let client = use_api_client();

    Effect::new(move |_| {
        let Some(LoadingState::Loaded(detail)) = trace_detail.try_get() else {
            return;
        };
        let _ = receipt_digest.try_set(detail.receipt.map(|r| r.receipt_digest));
    });

    let run_id = export.run.id.clone();
    let trace_id = export.run.trace_id.clone();
    let request_hash = export.run.request_hash.clone();
    let request_hash_verified = export.run.request_hash_verified;
    let manifest_hash = export.run.manifest_hash.clone();
    let manifest_hash_verified = export.run.manifest_hash_verified;
    let events = export.events.clone().unwrap_or_default();
    let determinism_label = match extract_reasoning_mode_from_events(&events) {
        Some(true) => "Verified",
        Some(false) => "Fast",
        None => "Fast",
    };

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
        <div class="space-y-4">
            <div class="border-b border-border">
                <nav class="flex gap-1">
                    <button
                        class=move || {
                            if active_tab.try_get().unwrap_or(ReceiptPanelTab::Summary) == ReceiptPanelTab::Summary {
                                "px-3 py-1.5 text-xs rounded-md bg-muted text-foreground".to_string()
                            } else {
                                "px-3 py-1.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50".to_string()
                            }
                        }
                        on:click=move |_| active_tab.set(ReceiptPanelTab::Summary)
                        type="button"
                    >
                        "Summary"
                    </button>
                    <button
                        class=move || {
                            if active_tab.try_get().unwrap_or(ReceiptPanelTab::Summary) == ReceiptPanelTab::Details {
                                "px-3 py-1.5 text-xs rounded-md bg-muted text-foreground".to_string()
                            } else {
                                "px-3 py-1.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50".to_string()
                            }
                        }
                        on:click=move |_| active_tab.set(ReceiptPanelTab::Details)
                        type="button"
                    >
                        "Details"
                    </button>
                    <button
                        class=move || {
                            if active_tab.try_get().unwrap_or(ReceiptPanelTab::Summary) == ReceiptPanelTab::Export {
                                "px-3 py-1.5 text-xs rounded-md bg-muted text-foreground".to_string()
                            } else {
                                "px-3 py-1.5 text-xs rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50".to_string()
                            }
                        }
                        on:click=move |_| active_tab.set(ReceiptPanelTab::Export)
                        type="button"
                    >
                        "Export"
                    </button>
                </nav>
            </div>

            {move || {
                // Clone captured variables for each reactive call to allow FnMut
                let client = client.clone();
                let run_id = run_id.clone();
                let trace_id = trace_id.clone();
                let request_hash = request_hash.clone();
                let manifest_hash = manifest_hash.clone();
                let exported_at = exported_at.clone();
                let events_str = events_str.clone();
                let notifications = notifications.clone();
                match active_tab.try_get().unwrap_or(ReceiptPanelTab::Summary) {
                    ReceiptPanelTab::Summary => {
                        view! {
                            <div class="space-y-4">
                                <p class="text-sm text-muted-foreground">
                                    "Signed Logs & Fingerprints"
                                </p>
                                <Card title="Signed Log Summary".to_string()>
                                    {move || match trace_detail.try_get().unwrap_or_default() {
                                        LoadingState::Loaded(detail) => {
                                            if let Some(receipt) = detail.receipt.clone() {
                                                let (verified_label, verified_variant) =
                                                    match receipt.verified {
                                                        Some(true) => {
                                                            ("Verified", BadgeVariant::Success)
                                                        }
                                                        Some(false) => {
                                                            ("Mismatch", BadgeVariant::Destructive)
                                                        }
                                                        None => ("Pending", BadgeVariant::Warning),
                                                    };
                                                let cache_label = match receipt.prefix_cache_hit {
                                                    Some(true) => "Cache credit applied",
                                                    Some(false) => "No cache credit",
                                                    None => "Cache credit unknown",
                                                };
                                                view! {
                                                    <div class="space-y-3">
                                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                                            <CopyableId
                                                                id=receipt.receipt_digest.clone()
                                                                label="Signed log fingerprint".to_string()
                                                                truncate=28
                                                            />
                                                            <div>
                                                                <p class="text-sm text-muted-foreground">"Verification status"</p>
                                                                <Badge variant=verified_variant>{verified_label}</Badge>
                                                            </div>
                                                        </div>
                                                        <div class="flex flex-wrap items-center gap-2">
                                                            <Button
                                                                variant=ButtonVariant::Outline
                                                                on_click=Callback::new({
                                                                    let trace_id = trace_id.clone();
                                                                    let notifications = notifications.clone();
                                                                    let refetch = refetch_trace_detail;
                                                                    let client = client.clone();
                                                                    move |_| {
                                                                        if verifying.get_untracked() {
                                                                            return;
                                                                        }
                                                                        let _ = verifying.try_set(true);
                                                                        let trace_id =
                                                                            trace_id.clone();
                                                                        let notifications =
                                                                            notifications.clone();
                                                                        let client = client.clone();
                                                                        spawn_local(async move {
                                                                            let req = TraceVerifyRequest { trace_id };
                                                                            let res: Result<TraceVerifyResponse, _> =
                                                                                client.post("/v1/replay/verify/trace", &req).await;
                                                                            match res {
                                                                                Ok(report) => {
                                                                                    if report.pass {
                                                                                        notifications.success(
                                                                                            "Signed log verified",
                                                                                            "Stored signed log matches canonical recomputation.",
                                                                                        );
                                                                                    } else if report.reasons.is_empty() {
                                                                                        notifications.warning(
                                                                                            "Signed log mismatch",
                                                                                            "Verification failed (no reasons returned).",
                                                                                        );
                                                                                    } else {
                                                                                        let reason = report.reasons[0].clone();
                                                                                        notifications.warning(
                                                                                            "Signed log mismatch",
                                                                                            &format!("First reason: {reason}"),
                                                                                        );
                                                                                    }
                                                                                    refetch.run(());
                                                                                }
                                                                                Err(err) => {
                                                                                    notifications.error(
                                                                                        "Signed log verification failed",
                                                                                        &format!("{err}"),
                                                                                    );
                                                                                }
                                                                            }
                                                                            let _ = verifying.try_set(false);
                                                                        });
                                                                    }
                                                                })
                                                                disabled=verifying
                                                            >
                                                                <Show when=move || verifying.try_get().unwrap_or(false) fallback=move || view! { <span>"Verify on server"</span> }>
                                                                    <span class="inline-flex items-center gap-2">
                                                                        <Spinner/>
                                                                        <span>"Verifying"</span>
                                                                    </span>
                                                                </Show>
                                                            </Button>
                                                        </div>
                                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                                                            <div>
                                                                <p class="text-muted-foreground">{ui_language::REPRODUCIBLE_MODE}</p>
                                                                <p>{determinism_label}</p>
                                                            </div>
                                                            <div>
                                                                <p class="text-muted-foreground">"Cache credit"</p>
                                                                <p>{cache_label}</p>
                                                            </div>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="space-y-3">
                                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                                            <div>
                                                                <p class="text-sm text-muted-foreground">"Signed log fingerprint"</p>
                                                                <p class="text-sm text-muted-foreground italic">"Unknown"</p>
                                                            </div>
                                                            <div>
                                                                <p class="text-sm text-muted-foreground">"Verification status"</p>
                                                                <Badge variant=BadgeVariant::Warning>"Not verified"</Badge>
                                                            </div>
                                                        </div>
                                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                                                            <div>
                                                                <p class="text-muted-foreground">{ui_language::REPRODUCIBLE_MODE}</p>
                                                                <p>{determinism_label}</p>
                                                            </div>
                                                            <div>
                                                                <p class="text-muted-foreground">"Cache credit"</p>
                                                                <p>"Unknown"</p>
                                                            </div>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                        LoadingState::Error(err) => view! {
                                            <div class="text-sm text-muted-foreground">{format!("Failed to load: {}", err)}</div>
                                        }.into_any(),
                                        _ => view! {
                                            <div class="text-sm text-muted-foreground italic">"Loading signed log summary..."</div>
                                        }.into_any(),
                                    }}
                                </Card>
                            </div>
                        }.into_any()
                    }
                    ReceiptPanelTab::Details => {
                        view! {
                            <div class="space-y-4">
                                <Card title="Signed Logs & Fingerprints".to_string()>
                                    <div class="space-y-4">
                                        <div class="grid grid-cols-2 gap-4">
                                            <CopyableId id=run_id.clone() label="Run ID".to_string() truncate=28 />
                                            <CopyableId id=trace_id.clone() label="Trace ID".to_string() truncate=28 />
                                        </div>

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

                                <Card title="Training Lineage".to_string()>
                                    {move || match trace_detail.try_get().unwrap_or_default() {
                                        LoadingState::Loaded(detail) => {
                                            if let Some(receipt) = detail.receipt.clone() {
                                                if let Some(digests) = receipt.adapter_training_digests {
                                                    if digests.is_empty() {
                                                        view! {
                                                            <div class="text-sm text-muted-foreground italic">
                                                                "No training digests recorded for this run."
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="space-y-3">
                                                                <p class="text-xs text-muted-foreground">
                                                                    "Training dataset digests for adapters used in this inference. Each digest is a BLAKE3 hash of the training data that produced the adapter."
                                                                </p>
                                                                <div class="space-y-2">
                                                                    {digests.into_iter().enumerate().map(|(idx, digest)| {
                                                                        view! {
                                                                            <TrainingDigestRow
                                                                                index=idx
                                                                                digest=digest
                                                                            />
                                                                        }
                                                                    }).collect::<Vec<_>>()}
                                                                </div>
                                                            </div>
                                                        }.into_any()
                                                    }
                                                } else {
                                                    view! {
                                                        <div class="text-sm text-muted-foreground italic">
                                                            "Training lineage not available for this run."
                                                        </div>
                                                    }.into_any()
                                                }
                                            } else {
                                                view! {
                                                    <div class="text-sm text-muted-foreground italic">
                                                        "Signed log is not available for this restore point."
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                        LoadingState::Error(err) => view! {
                                            <div class="text-sm text-muted-foreground">{format!("Failed to load: {}", err)}</div>
                                        }.into_any(),
                                        _ => view! {
                                            <div class="text-sm text-muted-foreground italic">"Loading training lineage..."</div>
                                        }.into_any(),
                                    }}
                                </Card>
                            </div>
                        }.into_any()
                    }
                    ReceiptPanelTab::Export => {
                        view! {
                            <Card title="Export".to_string()>
                                <div class="flex flex-wrap gap-2">
                                    <QuickActionButton
                                        icon="🔗"
                                        label="Copy hash"
                                        action=QuickAction::CopyReceiptHash(receipt_digest.read_only())
                                    />
                                    <QuickActionButton
                                        icon="⬇"
                                        label="Download signed log receipt"
                                        action=QuickAction::DownloadReceipt(receipt_digest.read_only())
                                    />
                                </div>
                            </Card>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Hash row with verification badge
#[component]
fn HashRow(label: &'static str, hash: String, verified: Option<bool>) -> impl IntoView {
    let hash_display = adapteros_id::format_hash_short(&hash);
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

/// Training digest row with index and copy affordance
#[component]
fn TrainingDigestRow(index: usize, digest: String) -> impl IntoView {
    let label = format!("Adapter {} training digest", index + 1);
    view! {
        <CopyableId
            id=digest
            label=label
            truncate=24
        />
    }
}

/// Routing tab - K-sparse routing decisions with TokenDecisions visualization
#[component]
fn RoutingTab(
    export: DiagExportResponse,
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
) -> impl IntoView {
    // Expandable state for TokenDecisions
    let expanded = RwSignal::new(true);
    let ui_profile = use_ui_profile();

    // Extract routing-related events as fallback
    let events = export.events.clone().unwrap_or_default();
    let routing_events: Vec<_> = events
        .iter()
        .filter(|e| e.event_type.contains("routing") || e.event_type.contains("adapter"))
        .cloned()
        .collect();

    view! {
        <div class="space-y-4">
            <p class="text-sm text-muted-foreground">
                "K-sparse routing decisions showing which adapters were selected and their gate values."
            </p>

            // Token decisions from trace (primary view)
            <AsyncBoundary
                state=trace_detail
                loading_message="Loading token decisions...".to_string()
                render={
                    let routing_events = routing_events.clone();
                    move |detail: UiInferenceTraceDetailResponse| {
                        if detail.token_decisions.is_empty() {
                            // Fall back to showing routing events
                            if routing_events.is_empty() {
                                view! {
                                    <Card>
                                        <div class="text-center py-8 text-muted-foreground">
                                            <p>"No routing decisions recorded for this run."</p>
                                            <p class="text-xs mt-2">"Routing data is captured when adapters are used during inference."</p>
                                        </div>
                                    </Card>
                                }.into_any()
                            } else {
                                let events_clone = routing_events.clone();
                                view! {
                                    <Card title="Routing Timeline".to_string()>
                                        <div class="space-y-2">
                                            {events_clone.into_iter().map(|event| {
                                                view! { <RoutingEventRow event=event/> }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </Card>
                                }.into_any()
                            }
                        } else {
                            // Show paged TokenDecisions component
                            view! {
                                <TokenDecisionsPaged
                                    trace_id=detail.trace_id.clone()
                                    initial_decisions=detail.token_decisions.clone()
                                    initial_next_cursor=detail.token_decisions_next_cursor
                                    initial_has_more=detail.token_decisions_has_more
                                    expanded=expanded.read_only()
                                    set_expanded=expanded.write_only()
                                    compact=false
                                />
                            }.into_any()
                        }
                    }
                }
            />

            // Link to full routing workspace
            {move || {
                if ui_profile.try_get().unwrap_or(UiProfile::Full) == UiProfile::Full {
                    Some(view! {
                        <Card>
                            <div class="text-center py-4">
                                <Link href="/routing" class="text-sm">
                                    "Open Routing Workspace →"
                                </Link>
                            </div>
                        </Card>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

/// Routing event row
#[component]
fn RoutingEventRow(event: DiagEventResponse) -> impl IntoView {
    let payload_str =
        serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| "{}".to_string());
    let payload_truncated = if payload_str.len() > 300 {
        format!("{}...", &payload_str[..300])
    } else {
        payload_str
    };

    view! {
        <div class="border-l-2 border-primary pl-3 py-2">
            <div class="flex items-center gap-2 text-sm">
                <Badge variant=BadgeVariant::Outline>{event.event_type.clone()}</Badge>
                <span class="text-muted-foreground font-mono text-xs">"+{event.mono_us}us"</span>
            </div>
            <pre class="text-xs text-muted-foreground mt-1 overflow-x-auto whitespace-pre-wrap">{payload_truncated}</pre>
        </div>
    }
}

/// Tokens tab - token accounting and cache stats with backend data
#[component]
fn TokensTab(
    export: DiagExportResponse,
    trace_detail: ReadSignal<LoadingState<UiInferenceTraceDetailResponse>>,
) -> impl IntoView {
    // Extract token-related info from events as fallback
    let events = export.events.clone().unwrap_or_default();
    let token_events: Vec<_> = events
        .iter()
        .filter(|e| e.event_type.contains("token") || e.event_type.contains("cache"))
        .cloned()
        .collect();

    view! {
        <div class="space-y-4">
            <p class="text-sm text-muted-foreground">
                "Token accounting, cache hit rates, and billing information for this run."
            </p>

            // Token summary from trace receipt
            <AsyncBoundary
                state=trace_detail
                loading_message="Loading token summary...".to_string()
                render=move |detail: UiInferenceTraceDetailResponse| {
                    if let Some(receipt) = &detail.receipt {
                        let prompt_tokens = receipt.logical_prompt_tokens;
                        let output_tokens = receipt.logical_output_tokens;
                        let logical_tokens = prompt_tokens + output_tokens;
                        let cache_hit = receipt.prefix_cache_hit;

                        let cached_display = match cache_hit {
                            Some(true) => "Cache hit (tokens unknown)".to_string(),
                            Some(false) => "0".to_string(),
                            None => "Unknown".to_string(),
                        };

                        let billed_display = match cache_hit {
                            Some(true) => format!("≤ {}", logical_tokens),
                            Some(false) => logical_tokens.to_string(),
                            None => "Unknown".to_string(),
                        };

                        view! {
                            <Card title="Token Summary".to_string()>
                                <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                                    <div class="text-center p-4 bg-muted/30 rounded-lg">
                                        <div class="text-2xl font-bold text-primary">{logical_tokens.to_string()}</div>
                                        <div class="text-sm text-muted-foreground">"Logical Tokens"</div>
                                    </div>
                                    <div class="text-center p-4 bg-muted/30 rounded-lg">
                                        <div class="text-lg font-semibold">{cached_display}</div>
                                        <div class="text-sm text-muted-foreground">"Cached Tokens"</div>
                                    </div>
                                    <div class="text-center p-4 bg-muted/30 rounded-lg">
                                        <div class="text-lg font-semibold">{billed_display}</div>
                                        <div class="text-sm text-muted-foreground">"Billed Tokens"</div>
                                    </div>
                                </div>
                                <div class="mt-3 text-xs text-muted-foreground">
                                    {format!("Prompt: {} · Completion: {}", prompt_tokens, output_tokens)}
                                </div>
                            </Card>
                        }.into_any()
                    } else {
                        // No receipt data available
                        view! {
                            <Card title="Token Summary".to_string()>
                                <div class="text-center py-4 text-muted-foreground text-sm">
                                    "Token data not available for this run"
                                </div>
                            </Card>
                        }.into_any()
                    }
                }
            />

            // Token events
            {if !token_events.is_empty() {
                let events_clone = token_events.clone();
                Some(view! {
                    <Card title="Token Timeline".to_string()>
                        <div class="space-y-2">
                            {events_clone.into_iter().map(|event| {
                                view! { <EventRow event=event/> }
                            }).collect::<Vec<_>>()}
                        </div>
                    </Card>
                })
            } else {
                Some(view! {
                    <Card>
                        <div class="text-center py-4 text-muted-foreground text-sm">
                            "No token events recorded"
                        </div>
                    </Card>
                })
            }}
        </div>
    }
}

/// Diff tab - compare current run against another run
#[component]
fn DiffTab(export: DiagExportResponse, compare_trace: Option<String>) -> impl IntoView {
    let run_a_trace_id = export.run.trace_id.clone();
    let run_a_id = export.run.id.clone();
    let compare_trace_value = compare_trace.clone().unwrap_or_default();

    let run_b_trace_id = RwSignal::new(compare_trace_value.clone());
    let diff_result: RwSignal<Option<DiagDiffResponse>> = RwSignal::new(None);
    let diff_loading = RwSignal::new(false);
    let diff_error: RwSignal<Option<String>> = RwSignal::new(None);
    let auto_compare_done = RwSignal::new(false);
    let client = use_api_client();

    let (runs, refetch_runs) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .list_diag_runs(&ListDiagRunsQuery {
                limit: Some(50),
                ..Default::default()
            })
            .await
    });

    let start_compare = {
        let run_a_trace_id = run_a_trace_id.clone();
        let client = client.clone();
        Callback::new(move |trace_b: String| {
            if trace_b.is_empty() {
                let _ = diff_error.try_set(Some("Select a run to compare".to_string()));
                return;
            }

            let _ = diff_loading.try_set(true);
            let _ = diff_error.try_set(None);
            let _ = diff_result.try_set(None);

            let trace_a = run_a_trace_id.clone();
            let client = client.clone();
            spawn_local(async move {
                let request = DiagDiffRequest {
                    trace_id_a: trace_a,
                    trace_id_b: trace_b,
                    include_timing: true,
                    include_events: true,
                    include_router_steps: true,
                };

                match client.diff_diag_runs(&request).await {
                    Ok(result) => {
                        let _ = diff_result.try_set(Some(result));
                        let _ = diff_loading.try_set(false);
                    }
                    Err(e) => {
                        let _ = diff_error.try_set(Some(e.user_message()));
                        let _ = diff_loading.try_set(false);
                    }
                }
            });
        })
    };

    let start_compare_for_effect = start_compare;
    Effect::new(move |_| {
        if auto_compare_done.try_get().unwrap_or(true) || compare_trace_value.is_empty() {
            return;
        }
        let _ = auto_compare_done.try_set(true);
        start_compare_for_effect.run(compare_trace_value.clone());
    });

    let selected_run_id = Signal::derive(move || {
        let selected_trace = run_b_trace_id.try_get().unwrap_or_default();
        match runs.try_get().unwrap_or_default() {
            LoadingState::Loaded(ref data) => data
                .runs
                .iter()
                .find(|run| run.trace_id == selected_trace)
                .map(|run| run.id.clone()),
            _ => None,
        }
    });
    let run_a_trace_id_for_link = run_a_trace_id.clone();

    view! {
        <div class="space-y-4">
            <p class="text-sm text-muted-foreground">
                "Compare this run against another trace to diagnose determinism drift."
            </p>

            <Card title="Run Diff".to_string()>
                <div class="space-y-4">
                    <div class="grid gap-4 md:grid-cols-2">
                        <div>
                            <p class="text-xs text-muted-foreground uppercase tracking-wide mb-1">"Run A (current)"</p>
                            <div class="rounded-md border border-border p-3 bg-muted/20">
                                <div class="text-sm font-medium">{"Run ID"}</div>
                                <div class="text-xs font-mono text-muted-foreground break-all">{run_a_id.clone()}</div>
                                <div class="mt-2 text-sm font-medium">{"Trace ID"}</div>
                                <div class="text-xs font-mono text-muted-foreground break-all">{run_a_trace_id.clone()}</div>
                            </div>
                        </div>
                        <div>
                            <p class="text-xs text-muted-foreground uppercase tracking-wide mb-1">"Run B (comparison)"</p>
                            <RunSelector
                                runs=runs
                                selected=run_b_trace_id
                                exclude=Signal::derive({
                                    let run_a_trace_id = run_a_trace_id.clone();
                                    move || run_a_trace_id.clone()
                                })
                            />
                        </div>
                    </div>
                    <div class="flex flex-wrap items-center gap-3">
                        <Button
                            variant=ButtonVariant::Primary
                            disabled=Signal::derive(move || diff_loading.try_get().unwrap_or(false) || run_b_trace_id.try_get().unwrap_or_default().is_empty())
                            on_click=Callback::new(move |_| start_compare.run(run_b_trace_id.try_get().unwrap_or_default()))
                        >
                            {move || if diff_loading.try_get().unwrap_or(false) { "Comparing..." } else { "Compare Restore Points" }}
                        </Button>
                        <Button
                            variant=ButtonVariant::Secondary
                            on_click=Callback::new(move |_| refetch_runs.run(()))
                        >
                            "Refresh Restore Points"
                        </Button>
                        {move || diff_error.try_get().flatten().map(|e| view! {
                            <span class="text-destructive text-sm">{e}</span>
                        })}
                        {move || {
                            let compare_id = run_b_trace_id.try_get().unwrap_or_default();
                            if compare_id.is_empty() {
                                return view! {}.into_any();
                            }
                            let run_id = selected_run_id.try_get().flatten().unwrap_or(compare_id.clone());
                            let href = format!("/runs/{}?tab=diff&compare={}", run_id, run_a_trace_id_for_link);
                            view! {
                                <Link href=href class="text-sm">
                                    "Open restore point detail"
                                </Link>
                            }.into_any()
                        }}
                    </div>
                </div>
            </Card>

            {move || {
                if diff_loading.try_get().unwrap_or(false) {
                    view! {
                        <SkeletonDetailSection rows=5 has_title=true/>
                    }.into_any()
                } else if let Some(result) = diff_result.try_get().flatten() {
                    view! { <DiffResults result=result/> }.into_any()
                } else {
                    view! {
                        <Card>
                            <div class="text-center py-10 text-muted-foreground text-sm">
                                "Select a comparison run to see differences."
                            </div>
                        </Card>
                    }.into_any()
                }
            }}
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

#[component]
fn ErrorsTab(trace_id: String) -> impl IntoView {
    let trace_id_for_fetch = trace_id.clone();
    let (errors, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let diag_trace_id = trace_id_for_fetch.clone();
        async move {
            client
                .list_errors_by_diag_trace_id(&diag_trace_id, Some(100))
                .await
        }
    });

    view! {
        <Card title="Persisted Errors".to_string()>
            <div class="flex items-center justify-between gap-2 pb-3 border-b border-border mb-3">
                <span class="text-sm text-muted-foreground">
                    {format!("Trace ID: {}", truncate_id(&trace_id))}
                </span>
                <Button
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Sm
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </div>
            <AsyncBoundary
                state=errors
                on_retry=Callback::new(move |_| refetch.run(()))
                render=move |response: adapteros_api_types::errors::ListErrorsResponse| {
                    if response.items.is_empty() {
                        view! {
                            <div class="text-center py-8 text-muted-foreground">
                                "No persisted errors recorded for this trace"
                            </div>
                        }.into_any()
                    } else {
                        let mut items = response.items;
                        items.sort_by(|a, b| b.created_at_unix_ms.cmp(&a.created_at_unix_ms));

                        view! {
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>"Debug ID"</TableHead>
                                        <TableHead>"Code"</TableHead>
                                        <TableHead>"Kind"</TableHead>
                                        <TableHead>"Severity"</TableHead>
                                        <TableHead>"Timestamp"</TableHead>
                                        <TableHead>"Request ID"</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {items.into_iter().map(|item| {
                                        let error_link = format!("/v1/errors/{}", item.id);
                                        let run_pivot_id = run_pivot_id(&item);
                                        let request_id_display = item.request_id.clone();
                                        view! {
                                            <TableRow>
                                                <TableCell>
                                                    <a
                                                        href=error_link
                                                        class="font-mono text-xs text-primary hover:underline"
                                                        target="_blank"
                                                        rel="noopener noreferrer"
                                                        title="Open error detail"
                                                    >
                                                        {truncate_id(&item.id)}
                                                    </a>
                                                </TableCell>
                                                <TableCell>
                                                    <span class="font-mono text-xs">{item.error_code}</span>
                                                </TableCell>
                                                <TableCell>{format!("{:?}", item.kind).to_lowercase()}</TableCell>
                                                <TableCell>{format!("{:?}", item.severity).to_lowercase()}</TableCell>
                                                <TableCell>
                                                    <span class="text-xs text-muted-foreground">
                                                        {format_timestamp_ms(item.created_at_unix_ms)}
                                                    </span>
                                                </TableCell>
                                                <TableCell>
                                                    {run_pivot_id.clone().map(|pivot_id| {
                                                        let pivot_href = pivot_id.clone();
                                                        let link_text = request_id_display
                                                            .clone()
                                                            .unwrap_or_else(|| pivot_id.clone());
                                                        view! {
                                                            <a
                                                                href=format!("/runs/{}", pivot_href)
                                                                class="font-mono text-xs text-primary hover:underline"
                                                                title="Pivot to run detail"
                                                            >
                                                                {link_text}
                                                            </a>
                                                        }
                                                        .into_any()
                                                    }).unwrap_or_else(|| view! {
                                                        <span class="font-mono text-xs text-muted-foreground">
                                                            {request_id_display.unwrap_or_else(|| "-".to_string())}
                                                        </span>
                                                    }.into_any())}
                                                </TableCell>
                                            </TableRow>
                                        }
                                    }).collect::<Vec<_>>()}
                                </TableBody>
                            </Table>
                        }.into_any()
                    }
                }
            />
        </Card>
    }
}

fn run_pivot_id(item: &ErrorInstance) -> Option<String> {
    item.diag_trace_id.clone().or_else(|| item.run_id.clone())
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
                        class=move || format!("w-4 h-4 transition-transform {}", if expanded.try_get().unwrap_or(false) { "rotate-90" } else { "" })
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
                if expanded.try_get().unwrap_or(false) {
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
        "warn" => "text-status-warning",
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
                <span class=severity_class>{severity.to_uppercase()}</span>
                <span class="text-muted-foreground font-mono text-xs">{mono_us}</span>
            </div>
            <pre class="text-xs text-muted-foreground mt-1 overflow-x-auto">{payload_truncated}</pre>
        </div>
    }
}

// ============================================================================
// Replay Tab - Execute Replay & Bundle Verify
// ============================================================================

/// Replay tab - execute replay and verify bundles
#[component]
fn ReplayTab(trace_id: String) -> impl IntoView {
    let show_replay_dialog = RwSignal::new(false);

    view! {
        <div class="space-y-4">
            <p class="text-sm text-muted-foreground">
                "Replay this response exactly or verify a saved signed evidence bundle."
            </p>

            // Execute Replay section
            <Card title="Replay Exact Response".to_string()>
                <div class="space-y-3">
                    <p class="text-sm text-muted-foreground">
                        "Re-run with the same inputs and locked settings. Leave the prompt empty for a strict exact replay."
                    </p>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| show_replay_dialog.set(true))
                    >
                        "Replay Exactly"
                    </Button>
                </div>
            </Card>

            // Bundle Verify section
            <BundleVerifySection/>

            // Replay dialog
            <ExecuteReplayDialog
                session_id=trace_id
                open=show_replay_dialog
            />
        </div>
    }
}

/// Dialog for executing a replay session
#[component]
fn ExecuteReplayDialog(session_id: String, open: RwSignal<bool>) -> impl IntoView {
    let use_original_rag = RwSignal::new(false);
    let prompt_override = RwSignal::new(String::new());
    let max_tokens = RwSignal::new(512u32);
    let executing = RwSignal::new(false);
    let result = RwSignal::new(None::<ExecuteReplayResponse>);
    let error = RwSignal::new(None::<String>);
    let notifications = use_notifications();
    let client = use_api_client();

    // Reset form state when dialog opens
    Effect::new(move || {
        if open.try_get().unwrap_or(false) {
            use_original_rag.set(false);
            prompt_override.set(String::new());
            max_tokens.set(512);
            result.set(None);
            error.set(None);
        }
    });

    let on_submit = {
        let session_id = session_id.clone();
        let client = client.clone();
        move |_| {
            if executing.get_untracked() {
                return;
            }
            let _ = executing.try_set(true);
            let _ = error.try_set(None);
            let _ = result.try_set(None);

            let session_id = session_id.clone();
            let prompt_val = prompt_override.get_untracked();
            let prompt = if prompt_val.trim().is_empty() {
                None
            } else {
                Some(prompt_val)
            };
            let request = ExecuteReplayRequest {
                use_original_rag_docs: use_original_rag.get_untracked(),
                prompt,
                max_tokens: max_tokens.get_untracked(),
            };
            let notifications = notifications.clone();
            let client = client.clone();
            spawn_local(async move {
                match client.execute_replay_session(&session_id, &request).await {
                    Ok(resp) => {
                        let _ = result.try_set(Some(resp));
                        notifications.success(
                            "Replay complete",
                            "Output was reproduced and verification metadata is available.",
                        );
                    }
                    Err(e) => {
                        let msg = e.user_message();
                        let _ = error.try_set(Some(msg.clone()));
                        notifications.error("Replay failed", &msg);
                    }
                }
                let _ = executing.try_set(false);
            });
        }
    };

    view! {
        <Dialog open=open title="Replay Exact Response".to_string()>
            <div class="space-y-4">
                // Configuration
                <div class="space-y-3">
                    <Checkbox
                        checked=Signal::derive(move || use_original_rag.try_get().unwrap_or(false))
                        on_change=Callback::new(move |v: bool| use_original_rag.set(v))
                        label="Reuse original source context".to_string()
                    />

                    <div>
                        <label class="text-sm text-muted-foreground block mb-1" for="replay-prompt-override">
                            "Optional: run a what-if prompt"
                        </label>
                        <textarea
                            id="replay-prompt-override"
                            class="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm min-h-[80px] resize-y"
                            placeholder="Leave blank for an exact replay..."
                            prop:value=move || prompt_override.try_get().unwrap_or_default()
                            on:input=move |ev| prompt_override.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="text-sm text-muted-foreground block mb-1" for="replay-max-tokens">
                            "Response length limit"
                        </label>
                        <input
                            id="replay-max-tokens"
                            type="number"
                            class="flex h-10 w-32 rounded-md border border-input bg-background px-3 py-2 text-sm"
                            prop:value=move || max_tokens.try_get().unwrap_or(512).to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                    max_tokens.set(v);
                                }
                            }
                            min="1"
                            max="8192"
                        />
                    </div>
                </div>

                // Actions
                <div class="flex items-center gap-2 pt-2 border-t border-border">
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=Signal::derive(move || executing.try_get().unwrap_or(false))
                        on_click=Callback::new(on_submit.clone())
                    >
                        <Show when=move || executing.try_get().unwrap_or(false) fallback=move || view! { <span>"Replay"</span> }>
                            <span class="inline-flex items-center gap-2">
                                <Spinner/>
                                <span>"Replaying..."</span>
                            </span>
                        </Show>
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| open.set(false))
                    >
                        "Cancel"
                    </Button>
                </div>

                // Error display
                {move || error.try_get().flatten().map(|e| view! {
                    <div class="bg-destructive/10 border border-destructive/20 rounded-lg p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Result display
                {move || result.try_get().flatten().map(|resp| {
                    view! {
                        <Card title="Replay Verification".to_string()>
                            <div class="space-y-3">
                                <div class="flex items-center gap-2">
                                    {if resp.degraded {
                                        view! { <Badge variant=BadgeVariant::Warning>"Needs review"</Badge> }.into_any()
                                    } else {
                                        view! { <Badge variant=BadgeVariant::Success>"Exact mode"</Badge> }.into_any()
                                    }}
                                    <span class="text-sm text-muted-foreground font-mono">
                                        {format!("{}ms", resp.latency_ms)}
                                    </span>
                                </div>
                                <p class="text-xs text-muted-foreground">
                                    {if resp.degraded {
                                        "Replay completed with safety fallbacks. Review evidence before relying on this output.".to_string()
                                    } else {
                                        "Replay completed under locked reproducibility safeguards.".to_string()
                                    }}
                                </p>
                                {resp.verified_at.clone().map(|verified| view! {
                                    <p class="text-xs text-muted-foreground">
                                        {format!("Verified at {}", verified)}
                                    </p>
                                })}
                                {(!resp.missing_doc_ids.is_empty()).then(|| view! {
                                    <div class="rounded-md border border-status-warning/40 bg-status-warning/10 p-2">
                                        <p class="text-xs text-status-warning">
                                            {format!(
                                                "Replay completed with missing source items ({}). Review before promotion.",
                                                resp.missing_doc_ids.len()
                                            )}
                                        </p>
                                    </div>
                                })}
                                <div>
                                    <p class="text-xs text-muted-foreground mb-1">"Replayed output"</p>
                                    <pre class="text-sm bg-muted/30 rounded-lg p-3 overflow-x-auto whitespace-pre-wrap max-h-64 overflow-y-auto">
                                        {resp.output.clone()}
                                    </pre>
                                </div>
                                <CopyableId
                                    id=resp.session_id.clone()
                                    label="Replay run ID".to_string()
                                    truncate=28
                                />
                                <div class="flex flex-wrap items-center gap-2">
                                    <Link
                                        href=format!("/runs/{}", resp.session_id)
                                        class="inline-flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors"
                                    >
                                        "Open Restore Point"
                                    </Link>
                                    <Link
                                        href=format!("/runs/{}?tab=receipt", resp.session_id)
                                        class="inline-flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-border hover:bg-muted transition-colors"
                                    >
                                        "Open Signed Log"
                                    </Link>
                                </div>
                            </div>
                        </Card>
                    }
                })}
            </div>
        </Dialog>
    }
}

/// Bundle verification section with file upload
#[component]
fn BundleVerifySection() -> impl IntoView {
    let file_ref = NodeRef::<leptos::html::Input>::new();
    let verifying = RwSignal::new(false);
    let result = RwSignal::new(None::<ReceiptVerificationResult>);
    let error = RwSignal::new(None::<String>);
    let notifications = use_notifications();
    let client = use_api_client();

    let on_file_change = move |_ev: web_sys::Event| {
        let Some(input) = file_ref.get() else {
            return;
        };
        let input_el: &web_sys::HtmlInputElement = &input;
        let Some(files) = input_el.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        if verifying.get_untracked() {
            return;
        }
        let _ = verifying.try_set(true);
        let _ = error.try_set(None);
        let _ = result.try_set(None);

        let notifications = notifications.clone();
        let _client = client.clone();
        #[cfg(target_arch = "wasm32")]
        spawn_local(async move {
            match _client.verify_bundle_receipt(&file).await {
                Ok(verification) => {
                    if verification.pass {
                        notifications.success("Bundle verified", "Signed log verification passed.");
                    } else {
                        notifications.warning(
                            "Bundle verification failed",
                            "Signed log verification did not pass.",
                        );
                    }
                    let _ = result.try_set(Some(verification));
                }
                Err(e) => {
                    let msg = e.user_message();
                    let _ = error.try_set(Some(msg.clone()));
                    notifications.error("Verification failed", &msg);
                }
            }
            let _ = verifying.try_set(false);
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (file, notifications);
            let _ = verifying.try_set(false);
        }
    };

    view! {
        <Card title="Verify Signed Log Bundle".to_string()>
            <div class="space-y-3">
                <p class="text-sm text-muted-foreground">
                    "Upload a bundle file to verify its signed system log. The server will validate all digests and signatures."
                </p>

                <div class="flex items-center gap-3">
                    <input
                        type="file"
                        node_ref=file_ref
                        accept=".bundle,.json,.tar.gz"
                        class="flex h-10 w-full max-w-sm text-sm file:mr-4 file:py-2 file:px-4 file:rounded-md file:border file:border-input file:bg-background file:text-sm file:font-medium hover:file:bg-muted"
                        aria_label="Upload verification bundle"
                        on:change=on_file_change
                    />
                    <Show when=move || verifying.try_get().unwrap_or(false) fallback=|| view! {}>
                        <span class="inline-flex items-center gap-2 text-sm text-muted-foreground">
                            <Spinner/>
                            <span>"Verifying signed log..."</span>
                        </span>
                    </Show>
                </div>

                // Error display
                {move || error.try_get().flatten().map(|e| view! {
                    <div class="bg-destructive/10 border border-destructive/20 rounded-lg p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Verification result
                {move || result.try_get().flatten().map(|res| {
                    let pass_variant = if res.pass {
                        BadgeVariant::Success
                    } else {
                        BadgeVariant::Destructive
                    };
                    let pass_label = if res.pass { "PASS" } else { "FAIL" };

                    view! {
                        <div class="space-y-3 border-t border-border pt-3">
                            <div class="flex items-center gap-3">
                                <Badge variant=pass_variant>{pass_label}</Badge>
                                {if res.signature_checked {
                                    if res.signature_valid {
                                        view! { <Badge variant=BadgeVariant::Success>"Signature valid"</Badge> }.into_any()
                                    } else {
                                        view! { <Badge variant=BadgeVariant::Destructive>"Signature invalid"</Badge> }.into_any()
                                    }
                                } else {
                                    view! { <Badge variant=BadgeVariant::Secondary>"Signature not checked"</Badge> }.into_any()
                                }}
                            </div>

                            // Reasons (if any)
                            {if !res.reasons.is_empty() {
                                let reasons = res.reasons.clone();
                                Some(view! {
                                    <div>
                                        <p class="text-xs text-muted-foreground mb-1">"Reasons"</p>
                                        <ul class="list-disc list-inside space-y-1">
                                            {reasons.into_iter().map(|reason| {
                                                view! { <li class="text-sm">{reason}</li> }
                                            }).collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                })
                            } else {
                                None
                            }}

                            // Digests
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                {res.context_digest.clone().map(|d| view! {
                                    <CopyableId id=d label="Context digest".to_string() truncate=24/>
                                })}
                                {res.run_head_hash.clone().map(|d| view! {
                                    <CopyableId id=d label="Run head hash".to_string() truncate=24/>
                                })}
                                {res.output_digest.clone().map(|d| view! {
                                    <CopyableId id=d label="Output digest".to_string() truncate=24/>
                                })}
                                {res.receipt_digest.clone().map(|d| view! {
                                    <CopyableId id=d label="Signed log fingerprint".to_string() truncate=24/>
                                })}
                                {res.trace_id.clone().map(|d| view! {
                                    <CopyableId id=d label="Trace ID".to_string() truncate=24/>
                                })}
                            </div>
                        </div>
                    }
                })}
            </div>
        </Card>
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Truncate an ID for display.
fn truncate_id(id: &str) -> String {
    adapteros_id::short_id(id)
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

/// Extract reasoning mode from diagnostic events
/// Looks for "reasoning_mode" or "thinking_mode" in event payloads
fn extract_reasoning_mode_from_events(events: &[DiagEventResponse]) -> Option<bool> {
    for event in events {
        // Check for reasoning_mode in payload
        if let Some(reasoning) = event.payload.get("reasoning_mode") {
            if let Some(b) = reasoning.as_bool() {
                return Some(b);
            }
        }
        // Check for thinking_mode as an alternative key
        if let Some(thinking) = event.payload.get("thinking_mode") {
            if let Some(b) = thinking.as_bool() {
                return Some(b);
            }
        }
        // Check in run_envelope if present
        if let Some(envelope) = event.payload.get("run_envelope") {
            if let Some(reasoning) = envelope.get("reasoning_mode") {
                if let Some(b) = reasoning.as_bool() {
                    return Some(b);
                }
            }
        }
    }
    None
}
