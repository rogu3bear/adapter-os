//! Update Center page
//!
//! Dedicated route for adapter promotion: Draft → Reviewed → Production.
//! Lists skills with promotion state and links to adapter detail for
//! Run Promote and Run Checkout controls.

use crate::api::ApiClient;
use crate::components::layout::nav_group_label_for_route;
use crate::components::{
    AdapterDetailPanel, AsyncBoundary, Badge, BadgeVariant, Button, ButtonVariant, Card,
    EmptyState, EmptyStateVariant, ErrorDisplay, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, PageScaffoldInspector, PageScaffoldPrimaryAction, SkeletonTable,
    SplitPanel, SplitRatio, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::contexts::use_in_flight;
use crate::hooks::{use_cached_api_resource, CacheTtl, LoadingState};
use crate::signals::refetch::{use_refetch_signal, RefetchTopic};
use crate::signals::{try_use_route_context, use_ui_profile, SelectedEntity};
use crate::utils::{chat_path_with_adapter, humanize};
use adapteros_api_types::{AdapterResponse, LifecycleState};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use std::sync::Arc;

const NEW_ADAPTER_PATH: &str = "/training?open_wizard=1";
const PAGE_SIZE: usize = 25;

/// Update Center page - promotion workflow for skills
#[component]
pub fn UpdateCenter() -> impl IntoView {
    let nav_label = nav_group_label_for_route(use_ui_profile().get_untracked(), "/update-center")
        .unwrap_or("Versions");
    let selected_id = RwSignal::new(None::<String>);
    let command_focus = RwSignal::new(None::<String>);
    let query_consumed = RwSignal::new(false);
    let query = use_query_map();

    Effect::new(move || {
        if query_consumed.try_get().unwrap_or(false) {
            return;
        }
        let Some(params) = query.try_get() else {
            return;
        };
        let mut consumed = false;
        if let Some(adapter_id) = params.get("adapter_id") {
            selected_id.set(Some(adapter_id.clone()));
            consumed = true;
        }
        if let Some(command) = params.get("command") {
            command_focus.set(Some(command.clone()));
            consumed = true;
        }
        if consumed {
            query_consumed.set(true);
        }
    });

    let (adapters, refetch) = use_cached_api_resource(
        "adapters_list",
        CacheTtl::LIST,
        |client: Arc<ApiClient>| async move { client.list_adapters().await },
    );

    let refetch_signal = StoredValue::new(refetch);
    let adapter_refetch_counter = use_refetch_signal(RefetchTopic::Adapters);

    Effect::new(move || {
        let Some(counter) = adapter_refetch_counter.try_get() else {
            return;
        };
        if counter > 0 {
            refetch.run(());
        }
    });

    let selected_adapter = Memo::new(move |_| {
        let id = selected_id.try_get()??;
        match adapters.try_get()? {
            LoadingState::Loaded(data) => data.iter().find(|a| a.id == id).cloned(),
            _ => None,
        }
    });

    Effect::new(move || {
        if let Some(route_ctx) = try_use_route_context() {
            if let Some(id) = selected_id.try_get().flatten() {
                if let LoadingState::Loaded(data) = adapters.try_get().unwrap_or(LoadingState::Idle)
                {
                    if let Some(adapter) = data.iter().find(|a| a.id == id) {
                        route_ctx.set_selected(SelectedEntity::with_status(
                            "adapter",
                            id.clone(),
                            adapter.name.clone(),
                            lifecycle_stage_label(adapter.lifecycle_state).to_string(),
                        ));
                    } else {
                        route_ctx.set_selected(SelectedEntity::new("adapter", id.clone(), id));
                    }
                } else {
                    route_ctx.set_selected(SelectedEntity::new("adapter", id.clone(), id));
                }
            } else {
                route_ctx.clear_selected();
            }
        }
    });

    let is_loading = Signal::derive(move || {
        matches!(
            adapters.try_get().unwrap_or(LoadingState::Idle),
            LoadingState::Idle | LoadingState::Loading
        )
    });

    let has_selection = Signal::derive(move || selected_id.try_get().flatten().is_some());

    let on_select = Callback::new(move |id: String| {
        selected_id.set(Some(id));
    });

    let on_close_detail = Callback::new(move |_: ()| {
        selected_id.set(None);
    });

    let on_refetch_detail = Callback::new(move |_: ()| {
        refetch.run(());
    });

    view! {
        <PageScaffold
            title="Versions"
            subtitle="Manage adapter version history and continue dataset updates while preserving lineage."
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/update-center"),
                PageBreadcrumbItem::current("Versions"),
            ]
            full_width=true
        >
            <PageScaffoldPrimaryAction slot>
                {
                    let navigate = use_navigate();
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            aria_label="Create a new adapter in training wizard"
                            on_click=Callback::new(move |_| {
                                navigate(NEW_ADAPTER_PATH, Default::default());
                            })
                        >
                            "Create Adapter"
                        </Button>
                    }
                }
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    aria_label="Refresh update center adapter list"
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>
            <PageScaffoldInspector slot>
                <div class="context-rail">
                    <section class="context-rail-section">
                        <p class="context-rail-eyebrow">"Context"</p>
                        <h2 class="context-rail-title">"Versions"</h2>
                        <p class="context-rail-copy">
                            "Track adapter lifecycle stage and keep update decisions visible while browsing versions."
                        </p>
                    </section>
                    <section class="context-rail-section">
                        <h3 class="context-rail-heading">"Current view"</h3>
                        <dl class="context-rail-kv">
                            <div>
                                <dt>"Adapters"</dt>
                                <dd>
                                    {move || {
                                        match adapters.try_get() {
                                            Some(LoadingState::Loaded(data)) => data.len().to_string(),
                                            _ => "Loading".to_string(),
                                        }
                                    }}
                                </dd>
                            </div>
                            <div>
                                <dt>"Selected"</dt>
                                <dd>
                                    {move || {
                                        selected_adapter
                                            .try_get()
                                            .flatten()
                                            .map(|adapter| adapter.name)
                                            .unwrap_or_else(|| "None".to_string())
                                    }}
                                </dd>
                            </div>
                            <div>
                                <dt>"Stage"</dt>
                                <dd>
                                    {move || {
                                        selected_adapter
                                            .try_get()
                                            .flatten()
                                            .map(|adapter| lifecycle_stage_label(adapter.lifecycle_state).to_string())
                                            .unwrap_or_else(|| "None".to_string())
                                    }}
                                </dd>
                            </div>
                        </dl>
                    </section>
                    <section class="context-rail-section">
                        <h3 class="context-rail-heading">"Next step"</h3>
                        <p class="context-rail-copy">
                            {move || {
                                if selected_id.try_get().flatten().is_some() {
                                    "Use the detail pane to update stage or continue dataset updates."
                                } else {
                                    "Select an adapter to view lifecycle controls and version lineage."
                                }
                            }}
                        </p>
                    </section>
                </div>
            </PageScaffoldInspector>

            {move || {
                match adapters.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonTable rows=5 columns=4 /> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                            />
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let mut adapters_for_list = data.clone();
                        adapters_for_list.sort_by_key(|a| a.lifecycle_state.sort_key());
                        view! {
                            <SplitPanel
                                has_selection=has_selection
                                on_close=on_close_detail
                                back_label="Back to Versions"
                                ratio=SplitRatio::TwoFifthsThreeFifths
                                list_panel=move || {
                                    let data = adapters_for_list.clone();
                                    view! {
                                        <UpdateCenterList
                                            adapters=data
                                            selected_id=selected_id
                                            command_focus=command_focus
                                            on_select=on_select
                                        />
                                    }
                                }
                                detail_panel=move || {
                                    view! {
                                        <AdapterDetailPanel
                                            adapter=selected_adapter
                                            loading=is_loading
                                            on_close=on_close_detail
                                            on_refetch=on_refetch_detail
                                        />
                                    }
                                }
                            />
                        }.into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}

#[component]
fn UpdateCenterList(
    adapters: Vec<AdapterResponse>,
    #[prop(into)] selected_id: RwSignal<Option<String>>,
    #[prop(into)] command_focus: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let in_flight = use_in_flight();
    let navigate = use_navigate();

    if adapters.is_empty() {
        let navigate = navigate.clone();
        return view! {
            <EmptyState
                variant=EmptyStateVariant::Empty
                title="No updates available".to_string()
                description="All adapters are up to date.".to_string()
                action_label="Create Adapter"
                on_action=Callback::new(move |_| {
                    navigate(NEW_ADAPTER_PATH, Default::default());
                })
            />
        }
        .into_any();
    }

    let visible_count = RwSignal::new(PAGE_SIZE.min(adapters.len()));
    let total = adapters.len();
    let show_more = move |_| {
        visible_count.update(|c| *c = (*c + PAGE_SIZE).min(total));
    };

    let in_flight_ids = in_flight.adapter_ids;
    let adapters_for_rows = adapters.clone();
    let nav_stored = StoredValue::new(navigate.clone());

    view! {
        <Card data_testid="update-center-list-card".to_string()>
            {move || command_focus
                .try_get()
                .flatten()
                .map(|command| {
                    let command_label = match command.as_str() {
                        "run-promote" => "Run Promote",
                        "run-checkout" => "Run Checkout",
                        "feed-dataset" => "Feed Dataset",
                        _ => "Open command workflow",
                    };
                    view! {
                        <div class="rounded-md border border-primary/40 bg-primary/5 p-3 mb-4">
                            <p class="text-xs font-semibold uppercase tracking-wide text-primary/80">
                                "Command Palette"
                            </p>
                            <p class="text-sm text-foreground mt-1">
                                {format!(
                                    "Intent received: {}. Select an adapter and continue in the detail panel.",
                                    command_label
                                )}
                            </p>
                        </div>
                    }
                })
            }
            <div class="rounded-md border border-border/50 bg-muted/20 p-3 mb-4">
                <p class="text-sm font-medium">"Promotion Path"</p>
                <div class="flex flex-wrap items-center gap-2 text-xs">
                    <Badge variant=BadgeVariant::Secondary>"Draft"</Badge>
                    <span class="text-muted-foreground">"→"</span>
                    <Badge variant=BadgeVariant::Warning>"Reviewed"</Badge>
                    <span class="text-muted-foreground">"→"</span>
                    <Badge variant=BadgeVariant::Success>"Production"</Badge>
                </div>
                <p class="text-xs text-muted-foreground mt-1">
                    "Select an adapter, resolve a version, then run a command."
                </p>
                <details class="mt-3 rounded border border-border/50 bg-background/50 px-3 py-2">
                    <summary class="cursor-pointer text-xs font-medium text-muted-foreground">
                        "Advanced command map"
                    </summary>
                    <div class="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                        <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"checkout <branch>@<version>"</span>
                        <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"promote <version> --to production"</span>
                        <span class="rounded border border-border/60 bg-background/70 px-2 py-0.5 font-mono">"feed-dataset --branch <branch> --from <version>"</span>
                    </div>
                    <p class="text-xs text-muted-foreground mt-2">
                        "Recommended default: select an adapter, resolve a version, run checkout or promote, then feed-dataset."
                    </p>
                </details>
            </div>

            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Stage"</TableHead>
                        <TableHead>"Tier"</TableHead>
                        <TableHead>""</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {move || {
                        let count = visible_count.try_get().unwrap_or(PAGE_SIZE);
                        let current_selected = selected_id.try_get().flatten();
                        let current_in_flight = in_flight_ids.try_get().unwrap_or_default();
                        adapters_for_rows.iter().take(count).map(|adapter| {
                            let id = adapter.id.clone();
                            let id_for_click = id.clone();
                            let name = adapter.name.clone();
                            let lifecycle = adapter.lifecycle_state;
                            let tier = adapter.tier.clone();
                            let is_selected = current_selected.as_ref() == Some(&id);
                            let is_in_flight = current_in_flight.contains(&id);

                            let lifecycle_variant = lifecycle_badge_variant(lifecycle);
                            let lifecycle_label = lifecycle_stage_label(lifecycle);

                            let id_for_keydown = id_for_click.clone();
                            let row_label = format!("Select adapter {}", name);
                            view! {
                                <tr
                                    class=if is_selected {
                                        "table-row cursor-pointer bg-accent/50 hover:bg-accent"
                                    } else {
                                        "table-row cursor-pointer hover:bg-accent/30"
                                    }
                                    data-state=if is_selected { "selected" } else { "" }
                                    role="button"
                                    tabindex=0
                                    aria-label=row_label
                                    on:click=move |_| on_select.run(id_for_click.clone())
                                    on:keydown=move |e: web_sys::KeyboardEvent| {
                                        let key = e.key();
                                        if key == "Enter" || key == " " || key == "Spacebar" {
                                            e.prevent_default();
                                            e.stop_propagation();
                                            on_select.run(id_for_keydown.clone());
                                        }
                                    }
                                >
                                    <TableCell>
                                        <span class="font-medium">{name}</span>
                                    </TableCell>
                                    <TableCell>
                                        <div class="flex items-center gap-2">
                                            <Badge variant=lifecycle_variant>
                                                {lifecycle_label}
                                            </Badge>
                                            {is_in_flight.then(|| view! {
                                                <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
                                            })}
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{humanize(&tier)}</span>
                                    </TableCell>
                                    <TableCell>
                                        <div on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                                            <Button
                                                variant=crate::components::ButtonVariant::Ghost
                                                size=crate::components::ButtonSize::Sm
                                                aria_label="Open prompt studio with this adapter selected"
                                                on_click=Callback::new(move |_| {
                                                    let path = chat_path_with_adapter(&id);
                                                    nav_stored.with_value(|nav| nav(&path, Default::default()));
                                                })
                                            >
                                                "Open Studio"
                                            </Button>
                                        </div>
                                    </TableCell>
                                </tr>
                            }
                        }).collect::<Vec<_>>()
                    }}
                </TableBody>
            </Table>

            {move || {
                let count = visible_count.try_get().unwrap_or(PAGE_SIZE);
                let remaining = total.saturating_sub(count);
                if remaining > 0 {
                    let aria_label = if remaining == 1 {
                        "Show 1 more skill".to_string()
                    } else {
                        format!("Show {} more skills", remaining)
                    };
                    view! {
                        <div class="flex items-center justify-center py-4 border-t">
                            <button
                                class="text-sm text-primary hover:underline"
                                aria-label=aria_label
                                on:click=show_more
                            >
                                {format!("Show more ({} remaining)", remaining)}
                            </button>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </Card>
    }
    .into_any()
}

fn lifecycle_badge_variant(state: LifecycleState) -> BadgeVariant {
    match state {
        LifecycleState::Active => BadgeVariant::Success,
        LifecycleState::Staging => BadgeVariant::Warning,
        LifecycleState::Deprecated => BadgeVariant::Warning,
        LifecycleState::Retired => BadgeVariant::Destructive,
        LifecycleState::Draft => BadgeVariant::Secondary,
        _ => BadgeVariant::Secondary,
    }
}

fn lifecycle_stage_label(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::Draft => "Draft",
        LifecycleState::Staging => "Reviewed",
        LifecycleState::Active => "Production",
        LifecycleState::Deprecated => "Paused",
        LifecycleState::Retired => "Retired",
        _ => "Unknown",
    }
}
