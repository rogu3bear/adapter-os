//! Update Center page
//!
//! Dedicated route for adapter promotion: Draft → Reviewed → Production.
//! Lists skills with promotion state and links to adapter detail for
//! Move to Production and Restore Version controls.

use crate::api::ApiClient;
use crate::components::{
    AdapterDetailPanel, AsyncBoundary, Badge, BadgeVariant, Button, ButtonVariant, Card,
    EmptyStateVariant, ListEmptyCard, PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    PageScaffoldPrimaryAction, SplitPanel, SplitRatio, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::contexts::use_in_flight;
use crate::hooks::{use_cached_api_resource, CacheTtl, LoadingState};
use crate::signals::refetch::{use_refetch_signal, RefetchTopic};
use crate::signals::{try_use_route_context, SelectedEntity};
use crate::utils::{chat_path_with_adapter, humanize};
use adapteros_api_types::{AdapterResponse, LifecycleState};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use std::sync::Arc;

const NEW_ADAPTER_PATH: &str = "/training?open_wizard=1";
const PAGE_SIZE: usize = 25;

/// Update Center page - promotion workflow for skills
#[component]
pub fn UpdateCenter() -> impl IntoView {
    let selected_id = RwSignal::new(None::<String>);

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
            title="Update Center"
            subtitle="Move versions from Draft to Reviewed to Production with rollback controls."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Deploy", "/adapters"),
                PageBreadcrumbItem::current("Update Center"),
            ]
        >
            <PageScaffoldPrimaryAction slot>
                {
                    let navigate = use_navigate();
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| {
                                navigate(NEW_ADAPTER_PATH, Default::default());
                            })
                        >
                            "Teach New Skill"
                        </Button>
                    }
                }
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <AsyncBoundary
                state=adapters
                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                render=move |data| {
                    let mut adapters_for_list = data.clone();
                    adapters_for_list.sort_by_key(|a| a.lifecycle_state.sort_key());
                    view! {
                        <SplitPanel
                            has_selection=has_selection
                            on_close=on_close_detail
                            back_label="Back to skills"
                            ratio=SplitRatio::TwoFifthsThreeFifths
                            list_panel=move || {
                                let data = adapters_for_list.clone();
                                view! {
                                    <UpdateCenterList
                                        adapters=data
                                        selected_id=selected_id
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
                    }
                }
            />
        </PageScaffold>
    }
}

#[component]
fn UpdateCenterList(
    adapters: Vec<AdapterResponse>,
    #[prop(into)] selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let in_flight = use_in_flight();
    let navigate = use_navigate();

    if adapters.is_empty() {
        let navigate = navigate.clone();
        return view! {
            <ListEmptyCard
                variant=EmptyStateVariant::Empty
                title="No skills yet"
                description="Teach your first skill to get started. Then promote through Draft, Reviewed, and Production."
                action_label="Teach New Skill"
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
                    "Select a skill to move versions to Production or restore a previous version."
                </p>
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
                            let row_label = format!("Select skill {}", name);
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
