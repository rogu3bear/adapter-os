//! Stacks detail components
//!
//! Detail view for individual adapter stacks.

use super::dialogs::EditStackDialog;
use super::helpers::{lifecycle_badge_variant, workflow_type_label};
use crate::api::{ApiClient, StackResponse};
use crate::components::{
    Badge, BadgeVariant, BreadcrumbItem, BreadcrumbTrail, Button, ButtonSize, ButtonVariant, Card,
    CopyableId, ErrorDisplay, LoadingDisplay, RefreshButton,
};
use crate::contexts::use_in_flight;
use crate::hooks::{use_api, use_api_resource, LoadingState, Refetch};
use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Stack detail page
#[component]
pub fn StackDetail() -> impl IntoView {
    let params = use_params_map();

    let stack_id = Memo::new(move |_| params.get().get("id").unwrap_or_default());

    let (stack, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = stack_id.get();
        async move { client.get_stack(&id).await }
    });

    // Fetch adapters for the stack
    let (adapters, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    let show_edit_dialog = RwSignal::new(false);

    view! {
        <div class="space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Stacks", "/stacks"),
                BreadcrumbItem::current(stack_id.get()),
            ]/>

            <div class="flex items-center justify-between">
                <h1 class="heading-1">"Stack Details"</h1>
                <div class="flex items-center gap-2">
                    <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| show_edit_dialog.set(true))
                    >
                        "Edit Stack"
                    </Button>
                </div>
            </div>

            {move || {
                match stack.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading stack details..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let adapter_data = match adapters.get() {
                            LoadingState::Loaded(a) => a,
                            _ => vec![],
                        };
                        view! {
                            <StackDetailContent
                                stack=data.clone()
                                all_adapters=adapter_data
                                refetch=refetch
                            />
                            <EditStackDialog
                                open=show_edit_dialog
                                stack=data
                                refetch=refetch
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch.run(()))
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Stack detail content
#[component]
pub fn StackDetailContent(
    stack: StackResponse,
    all_adapters: Vec<AdapterResponse>,
    refetch: Refetch,
) -> impl IntoView {
    let client = use_api();
    let in_flight = use_in_flight();
    let stack_id = stack.id.clone();
    let stack_id_activate = stack_id.clone();
    let is_active = stack.is_active;

    // Filter adapters that belong to this stack
    let stack_adapter_ids: std::collections::HashSet<_> =
        stack.adapter_ids.iter().cloned().collect();
    let stack_adapters: Vec<_> = all_adapters
        .into_iter()
        .filter(|a| stack_adapter_ids.contains(&a.id) || stack_adapter_ids.contains(&a.adapter_id))
        .collect();

    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{stack.name.clone()}</p>
                    </div>
                    <CopyableId id=stack.id.clone() label="Stack ID".to_string() truncate=24 />
                    {stack.description.clone().map(|desc| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Description"</p>
                            <p class="text-sm">{desc}</p>
                        </div>
                    })}
                    <CopyableId id=stack.tenant_id.clone() label="Tenant ID".to_string() truncate=24 />
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center gap-2">
                        <Badge variant=lifecycle_badge_variant(&stack.lifecycle_state)>
                            {stack.lifecycle_state.clone()}
                        </Badge>
                        {stack.is_active.then(|| view! {
                            <Badge variant=BadgeVariant::Success>"Active"</Badge>
                        })}
                        {stack.is_default.then(|| view! {
                            <Badge variant=BadgeVariant::Secondary>"Default"</Badge>
                        })}
                    </div>

                    <div class="flex gap-2">
                        {if is_active {
                            let client = Arc::clone(&client);
                            view! {
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| {
                                        let client = Arc::clone(&client);
                                        let refetch = refetch;
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if client.deactivate_stack().await.is_ok() {
                                                refetch.run(());
                                            }
                                        });
                                    })
                                >
                                    "Deactivate"
                                </Button>
                            }.into_any()
                        } else {
                            let client = Arc::clone(&client);
                            let id = stack_id_activate.clone();
                            view! {
                                <Button
                                    variant=ButtonVariant::Primary
                                    size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| {
                                        let client = Arc::clone(&client);
                                        let id = id.clone();
                                        let refetch = refetch;
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if client.activate_stack(&id).await.is_ok() {
                                                refetch.run(());
                                            }
                                        });
                                    })
                                >
                                    "Activate Stack"
                                </Button>
                            }.into_any()
                        }}
                    </div>

                    <div class="grid grid-cols-2 gap-4 text-sm pt-2 border-t">
                        <div>
                            <p class="text-muted-foreground">"Version"</p>
                            <p class="font-medium">{stack.version}</p>
                        </div>
                        <div>
                            <p class="text-muted-foreground">"Workflow Type"</p>
                            <p class="font-medium">{workflow_type_label(&stack.workflow_type)}</p>
                        </div>
                    </div>
                </div>
            </Card>
        </div>

        // Determinism Settings
        <Card title="Determinism Settings".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-2">
                <div>
                    <p class="text-sm text-muted-foreground">"Determinism Mode"</p>
                    <p class="font-medium">
                        {stack.determinism_mode.clone().unwrap_or_else(|| "default".to_string())}
                    </p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Routing Determinism Mode"</p>
                    <p class="font-medium">
                        {stack.routing_determinism_mode.clone().unwrap_or_else(|| "deterministic".to_string())}
                    </p>
                </div>
            </div>
        </Card>

        // Adapters in Stack
        <Card title="Adapters in Stack".to_string() class="mt-6".to_string()>
            {if stack_adapters.is_empty() {
                view! {
                    <div class="py-4 text-center text-muted-foreground">
                        "No adapters in this stack"
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-2">
                        {stack.adapter_ids.iter().enumerate().map(|(idx, adapter_id)| {
                            let adapter = stack_adapters.iter().find(|a| &a.id == adapter_id || &a.adapter_id == adapter_id);
                            let in_flight = in_flight.clone();
                            let adapter_id_for_in_flight = adapter_id.clone();
                            let is_in_flight = Signal::derive(move || {
                                in_flight.is_in_flight(&adapter_id_for_in_flight)
                            });
                            view! {
                                <div class="flex items-center gap-3 p-3 rounded-lg border bg-muted/50">
                                    <div class="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary font-medium text-sm">
                                        {idx + 1}
                                    </div>
                                    <div class="flex-1 min-w-0">
                                        {if let Some(a) = adapter {
                                            view! {
                                                <div>
                                                    <a
                                                        href=format!("/adapters/{}", a.id.clone())
                                                        class="font-medium hover:underline"
                                                    >
                                                        {a.name.clone()}
                                                    </a>
                                                    <p class="text-xs text-muted-foreground font-mono truncate">
                                                        {a.adapter_id.clone()}
                                                    </p>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div>
                                                    <p class="font-medium text-muted-foreground">"Unknown Adapter"</p>
                                                    <p class="text-xs text-muted-foreground font-mono truncate">
                                                        {adapter_id.clone()}
                                                    </p>
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>
                                    {adapter.map(|a| {
                                        let tier = a.tier.clone();
                                        let lifecycle_state = a.lifecycle_state.clone();
                                        let badge_variant = lifecycle_badge_variant(&lifecycle_state);
                                        view! {
                                            <div class="flex items-center gap-2">
                                                <Badge variant=BadgeVariant::Secondary>
                                                    {tier}
                                                </Badge>
                                                <Badge variant=badge_variant>
                                                    {lifecycle_state}
                                                </Badge>
                                                {move || is_in_flight.get().then(|| view! {
                                                    <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
                                                })}
                                            </div>
                                        }
                                    })}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </Card>

        // Warnings
        {(!stack.warnings.is_empty()).then(|| view! {
            <Card title="Warnings".to_string() class="mt-6".to_string()>
                <div class="space-y-2">
                    {stack.warnings.iter().map(|warning| {
                        view! {
                            <div class="flex items-start gap-2 p-3 rounded-lg border border-status-warning/50 bg-status-warning/10">
                                <svg
                                    xmlns="http://www.w3.org/2000/svg"
                                    class="h-5 w-5 text-status-warning shrink-0 mt-0.5"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>
                                    <line x1="12" y1="9" x2="12" y2="13"/>
                                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                                </svg>
                                <p class="text-sm text-status-warning">{warning.clone()}</p>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </Card>
        })}

        // Metadata
        <Card title="Timestamps".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-2">
                <div>
                    <p class="text-sm text-muted-foreground">"Created At"</p>
                    <p class="font-medium">{stack.created_at.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Updated At"</p>
                    <p class="font-medium">{stack.updated_at.clone()}</p>
                </div>
            </div>
        </Card>
    }
}
