//! Adapters page
//!
//! Displays the list of registered adapters with an expand/collapse detail panel.
//! Automatically refreshes when the global `RefetchTopic::Adapters` signal is
//! triggered (e.g., after training job completion).
//!
//! ## Layout
//!
//! Uses a split panel layout:
//! - Left: Paginated adapter list with click-to-select
//! - Right: AdapterDetailPanel showing full details for selected adapter
//!
//! ## Drawer Behavior
//!
//! - Click adapter row to open detail panel
//! - Click close button or press Escape to close
//! - Mobile: Full-screen overlay with back button

use crate::api::ApiClient;
use crate::components::{
    AdapterDetailPanel, AsyncBoundary, AsyncBoundaryWithErrorRender, Badge, BadgeVariant,
    BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, Card, CopyableId, EmptyState,
    EmptyStateVariant, ErrorDisplay, Link, SplitPanel, SplitRatio, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::constants::urls::docs_link;
use crate::contexts::use_in_flight;
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::refetch::{use_refetch_signal, RefetchTopic};
use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Adapters list page with split-panel detail drawer
#[component]
pub fn Adapters() -> impl IntoView {
    // State: selected adapter ID (None = detail panel closed)
    let selected_id = RwSignal::new(None::<String>);

    let (adapters, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    let refetch_signal = StoredValue::new(refetch);

    // Subscribe to global adapter refetch topic (triggered on training completion)
    let adapter_refetch_counter = use_refetch_signal(RefetchTopic::Adapters);

    // Refetch when the global counter increments
    Effect::new(move || {
        let _ = adapter_refetch_counter.get();
        // Skip initial effect run (counter starts at 0)
        if adapter_refetch_counter.get() > 0 {
            refetch.run(());
        }
    });

    // Derive selected adapter from list
    let selected_adapter = Signal::derive(move || {
        let id = selected_id.get()?;
        match adapters.get() {
            LoadingState::Loaded(data) => data.iter().find(|a| a.id == id).cloned(),
            _ => None,
        }
    });

    // Loading state
    let is_loading = Signal::derive(move || {
        matches!(adapters.get(), LoadingState::Idle | LoadingState::Loading)
    });

    // Has selection for split panel
    let has_selection = Signal::derive(move || selected_id.get().is_some());

    // Callbacks
    let on_select = Callback::new(move |id: String| {
        selected_id.set(Some(id));
    });

    let on_close_detail = Callback::new(move |_: ()| {
        selected_id.set(None);
    });

    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold tracking-tight">"Adapters"</h1>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </div>

            <AsyncBoundary
                state=adapters
                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                render=move |data| {
                    let adapters_for_list = data.clone();
                    view! {
                        <SplitPanel
                            has_selection=has_selection
                            on_close=on_close_detail
                            back_label="Back to Adapters"
                            ratio=SplitRatio::TwoFifthsThreeFifths
                            list_panel=move || {
                                let data = adapters_for_list.clone();
                                view! {
                                    <AdaptersListInteractive
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
                                    />
                                }
                            }
                        />
                    }
                }
            />
        </div>
    }
}

/// Page size for client-side pagination (reduces initial DOM nodes)
const PAGE_SIZE: usize = 25;

/// Interactive adapter list with selection support for split panel layout.
#[component]
fn AdaptersListInteractive(
    adapters: Vec<AdapterResponse>,
    #[prop(into)] selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let total = adapters.len();
    let in_flight = use_in_flight();

    if adapters.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No adapters found"
                    description="Adapters enable specialized inference capabilities. Train your first adapter to get started."
                    action_label="Train Adapter"
                    on_action=Callback::new(|_| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href("/training");
                        }
                    })
                    secondary_label="View Documentation"
                    secondary_href=docs_link("adapters")
                />
            </Card>
        }
        .into_any();
    }

    // Client-side pagination to reduce DOM nodes
    let visible_count = RwSignal::new(PAGE_SIZE.min(total));

    let show_more = move |_| {
        visible_count.update(|c| *c = (*c + PAGE_SIZE).min(total));
    };

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Lifecycle"</TableHead>
                        <TableHead>"Tier"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {move || {
                        let count = visible_count.get();
                        let current_selected = selected_id.get();
                        adapters.iter().take(count).map(|adapter| {
                            let id = adapter.id.clone();
                            let id_for_click = id.clone();
                            let id_for_in_flight = id.clone();
                            let name = adapter.name.clone();
                            let lifecycle = adapter.lifecycle_state.clone();
                            let tier = adapter.tier.clone();
                            let is_selected = current_selected.as_ref() == Some(&id);
                            let on_select = on_select.clone();
                            let in_flight = in_flight.clone();

                            // Lifecycle badge variant
                            let lifecycle_variant = match lifecycle.as_str() {
                                "active" => BadgeVariant::Success,
                                "deprecated" => BadgeVariant::Warning,
                                "retired" => BadgeVariant::Destructive,
                                _ => BadgeVariant::Secondary,
                            };

                            // Check if adapter is in-flight
                            let is_in_flight = Signal::derive(move || {
                                in_flight.is_in_flight(&id_for_in_flight)
                            });

                            view! {
                                <tr
                                    class=if is_selected {
                                        "table-row cursor-pointer bg-accent/50 hover:bg-accent"
                                    } else {
                                        "table-row cursor-pointer hover:bg-accent/30"
                                    }
                                    data-state=if is_selected { "selected" } else { "" }
                                    on:click=move |_| {
                                        on_select.run(id_for_click.clone());
                                    }
                                >
                                    <TableCell>
                                        <span class="font-medium">{name}</span>
                                    </TableCell>
                                    <TableCell>
                                        <div class="flex items-center gap-2">
                                            <Badge variant=lifecycle_variant>
                                                {lifecycle}
                                            </Badge>
                                            {move || is_in_flight.get().then(|| view! {
                                                <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
                                            })}
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{tier}</span>
                                    </TableCell>
                                </tr>
                            }
                        }).collect::<Vec<_>>()
                    }}
                </TableBody>
            </Table>

            // Show more button if there are hidden items
            {move || {
                let count = visible_count.get();
                let remaining = total.saturating_sub(count);
                if remaining > 0 {
                    view! {
                        <div class="flex items-center justify-center py-4 border-t">
                            <button
                                class="text-sm text-primary hover:underline"
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

/// Validate adapter ID format
/// Valid IDs: alphanumeric with hyphens/underscores, 1-128 chars
fn validate_adapter_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("Adapter ID is required");
    }
    if id.len() > 128 {
        return Err("Adapter ID exceeds maximum length");
    }
    // Allow alphanumeric, hyphens, underscores
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Adapter ID contains invalid characters");
    }
    Ok(())
}

/// Adapter detail page
#[component]
pub fn AdapterDetail() -> impl IntoView {
    // Log component mount
    web_sys::console::log_1(&"[AdapterDetail] Component mounted".into());

    let params = use_params_map();

    // Extract adapter ID from URL - must be called unconditionally
    let adapter_id = Memo::new(move |_| {
        let params_map = params.get();
        let id = params_map.get("id").unwrap_or_default();

        // Log parameter extraction
        if id.is_empty() {
            web_sys::console::warn_1(&"[AdapterDetail] No 'id' parameter in route params".into());
        } else {
            web_sys::console::log_1(
                &format!("[AdapterDetail] Param extracted: id='{}'", id).into(),
            );
        }
        id
    });

    // Always call use_api_resource (hooks must be called unconditionally)
    // Use get_untracked() to avoid reactive tracking warnings
    let (adapter, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = adapter_id.get_untracked();
        async move {
            // Log validation step
            web_sys::console::log_1(&format!("[AdapterDetail] Validating ID: '{}'", id).into());

            if let Err(validation_err) = validate_adapter_id(&id) {
                web_sys::console::error_1(
                    &format!("[AdapterDetail] Validation failed: {}", validation_err).into(),
                );
                return Err(crate::api::ApiError::Validation(validation_err.to_string()));
            }

            // Log API call initiation
            web_sys::console::log_1(&format!("[AdapterDetail] Fetching adapter: '{}'", id).into());

            let result = client.get_adapter(&id).await;

            // Log API result
            match &result {
                Ok(adapter) => {
                    web_sys::console::log_1(
                        &format!(
                            "[AdapterDetail] Loaded: '{}' ({})",
                            adapter.name, adapter.id
                        )
                        .into(),
                    );
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[AdapterDetail] API error: {:?}", e).into(),
                    );
                }
            }
            result
        }
    });

    let refetch_signal = StoredValue::new(refetch);

    // Derive adapter name for breadcrumb (shows ID until loaded)
    let adapter_name_for_breadcrumb = Signal::derive(move || adapter_id.get());

    view! {
        <div class="p-6 space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Adapters", "/adapters"),
                BreadcrumbItem::current(adapter_name_for_breadcrumb.get()),
            ]/>

            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold tracking-tight">"Adapter Details"</h1>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </div>

            <AsyncBoundaryWithErrorRender
                state=adapter
                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                render=move |data| view! { <AdapterDetailContent adapter=data /> }
                render_error=move |e, retry| {
                    if let crate::api::ApiError::Validation(msg) = &e {
                        let error_msg = msg.clone();
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <div class="text-center">
                                    <h2 class="text-xl font-semibold mb-2 text-destructive">"Invalid Adapter ID"</h2>
                                    <p class="text-muted-foreground mb-4">{error_msg}</p>
                                    <Link href="/adapters">
                                        "← Back to Adapters"
                                    </Link>
                                </div>
                            </div>
                        }.into_any()
                    } else if let Some(retry_cb) = retry {
                        view! { <ErrorDisplay error=e on_retry=retry_cb /> }.into_any()
                    } else {
                        view! { <ErrorDisplay error=e /> }.into_any()
                    }
                }
            />
        </div>
    }
}

/// Validate adapter response data before rendering
/// Returns error message if validation fails
fn validate_adapter_data(adapter: &AdapterResponse) -> Result<(), String> {
    let mut missing = Vec::new();

    if adapter.id.is_empty() {
        missing.push("id");
    }
    if adapter.adapter_id.is_empty() {
        missing.push("adapter_id");
    }
    if adapter.name.is_empty() {
        missing.push("name");
    }
    if adapter.hash_b3.is_empty() {
        missing.push("hash_b3");
    }
    if adapter.tier.is_empty() {
        missing.push("tier");
    }
    if adapter.lifecycle_state.is_empty() {
        missing.push("lifecycle_state");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("Missing required fields: {}", missing.join(", ")))
    }
}

#[component]
fn AdapterDetailContent(adapter: AdapterResponse) -> impl IntoView {
    // Log component render
    web_sys::console::log_1(
        &format!(
            "[AdapterDetailContent] Rendering: '{}' ({})",
            adapter.name, adapter.id
        )
        .into(),
    );

    // Validate adapter data before rendering
    web_sys::console::log_1(&"[AdapterDetailContent] Validating data...".into());
    if let Err(validation_error) = validate_adapter_data(&adapter) {
        web_sys::console::error_1(
            &format!(
                "[AdapterDetailContent] Validation failed: {}",
                validation_error
            )
            .into(),
        );
        return view! {
            <div class="flex items-center justify-center py-12">
                <div class="text-center">
                    <h2 class="text-xl font-semibold mb-2 text-destructive">"Invalid Adapter Data"</h2>
                    <p class="text-muted-foreground mb-2">{validation_error}</p>
                    <Link href="/adapters">
                        "← Back to Adapters"
                    </Link>
                </div>
            </div>
        }.into_any();
    }
    web_sys::console::log_1(&"[AdapterDetailContent] Validation passed".into());

    // Defensive: Handle potential invalid lifecycle_state values
    let lifecycle_variant = match adapter.lifecycle_state.as_str() {
        "active" => BadgeVariant::Success,
        "deprecated" => BadgeVariant::Warning,
        "retired" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    };

    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{adapter.name.clone()}</p>
                    </div>
                    <CopyableId
                        id=adapter.adapter_id.clone()
                        label="Adapter ID".to_string()
                        truncate=28
                    />
                    <div>
                        <p class="text-sm text-muted-foreground">"Hash (BLAKE3)"</p>
                        <p class="font-mono text-sm truncate">{adapter.hash_b3.clone()}</p>
                    </div>
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="flex items-center gap-2 mb-3">
                    <Badge variant=lifecycle_variant>
                        {adapter.lifecycle_state.clone()}
                    </Badge>
                    {adapter.runtime_state.clone().map(|state| view! {
                        <Badge variant=BadgeVariant::Secondary>
                            {state}
                        </Badge>
                    })}
                </div>
                <div class="space-y-2 text-sm">
                    <div>
                        <span class="text-muted-foreground">"Tier: "</span>
                        <span class="font-medium">{adapter.tier.clone()}</span>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Category: "</span>
                        <span class="font-medium">{adapter.category.clone().unwrap_or_else(|| "N/A".to_string())}</span>
                    </div>
                </div>
            </Card>

        </div>

        // Languages
        <Card title="Languages".to_string() class="mt-6".to_string()>
            <div class="flex flex-wrap gap-2">
                {if adapter.languages.is_empty() {
                    view! { <span class="text-muted-foreground">"No languages specified"</span> }.into_any()
                } else {
                    view! {
                        {adapter.languages.clone().into_iter().map(|lang| view! {
                            <Badge variant=BadgeVariant::Secondary>{lang}</Badge>
                        }).collect::<Vec<_>>()}
                    }.into_any()
                }}
            </div>
        </Card>

        // Metadata
        <Card title="Metadata".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Rank"</p>
                    <p class="font-medium">{adapter.rank}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Version"</p>
                    <p class="font-medium">{adapter.version.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Created At"</p>
                    <p class="font-medium">{adapter.created_at.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Updated At"</p>
                    <p class="font-medium">{adapter.updated_at.clone().unwrap_or_else(|| "N/A".to_string())}</p>
                </div>
            </div>
        </Card>

        // Framework (if available)
        {adapter.framework.clone().map(|fw| view! {
            <Card title="Framework".to_string() class="mt-6".to_string()>
                <div class="grid gap-4 md:grid-cols-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Framework"</p>
                        <p class="font-medium">{fw}</p>
                    </div>
                    {adapter.framework_id.clone().map(|fid| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Framework ID"</p>
                            <p class="font-mono text-sm">{fid}</p>
                        </div>
                    })}
                    {adapter.framework_version.clone().map(|fv| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Framework Version"</p>
                            <p class="font-medium">{fv}</p>
                        </div>
                    })}
                </div>
            </Card>
        })}

        // Stats (if available)
        {adapter.stats.clone().map(|stats| view! {
            <Card title="Statistics".to_string() class="mt-6".to_string()>
                <div class="grid gap-4 md:grid-cols-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Total Activations"</p>
                        <p class="text-2xl font-bold">{stats.total_activations}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selected Count"</p>
                        <p class="text-2xl font-bold">{stats.selected_count}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selection Rate"</p>
                        <p class="text-2xl font-bold">{format!("{:.1}%", stats.selection_rate * 100.0)}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Avg Gate Value"</p>
                        <p class="text-2xl font-bold">{format!("{:.3}", stats.avg_gate_value)}</p>
                    </div>
                </div>
            </Card>
        })}
    }.into_any()
}
