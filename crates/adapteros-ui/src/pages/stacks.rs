//! Adapter Stacks management page
//!
//! Provides UI for managing adapter stacks - compositions of adapters
//! that can be activated together for inference.

use crate::api::{ApiClient, CreateStackRequest, StackResponse, UpdateStackRequest, WorkflowType};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, EmptyState, ErrorDisplay, Input, LoadingDisplay, PageHeader,
    RefreshButton, Select, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
    Textarea,
};
use crate::hooks::{use_api, use_api_resource, LoadingState};
use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Stacks list page
#[component]
pub fn Stacks() -> impl IntoView {
    let (stacks, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_stacks().await });

    let show_create_dialog = RwSignal::new(false);
    let refetch_trigger = RwSignal::new(0u32);

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch.run(());
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Adapter Stacks"
                subtitle="Manage adapter compositions for inference"
            >
                <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                >
                    "Create Stack"
                </Button>
            </PageHeader>

            {move || {
                match stacks.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading stacks..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <StacksList stacks=data refetch_trigger=refetch_trigger/> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
                            />
                        }.into_any()
                    }
                }
            }}

            <CreateStackDialog
                open=show_create_dialog
                refetch_trigger=refetch_trigger
            />
        </div>
    }
}

/// List of stacks component
#[component]
fn StacksList(stacks: Vec<StackResponse>, refetch_trigger: RwSignal<u32>) -> impl IntoView {
    if stacks.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    title="No adapter stacks"
                    description="Create a stack to compose multiple adapters for inference."
                />
            </Card>
        }
        .into_any();
    }

    let client = use_api();

    // Delete confirmation dialog state
    let show_delete_confirm = RwSignal::new(false);
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let pending_delete_name = RwSignal::new(String::new());
    let deleting = RwSignal::new(false);
    let delete_error = RwSignal::new(Option::<String>::None);

    // Reset dialog state
    let reset_delete_state = move || {
        pending_delete_id.set(None);
        pending_delete_name.set(String::new());
        delete_error.set(None);
    };

    // Handle cancel/close of delete dialog
    let on_cancel_delete = Callback::new(move |_| {
        reset_delete_state();
    });

    // Handle confirmed deletion
    let on_confirm_delete = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            if let Some(id) = pending_delete_id.get() {
                deleting.set(true);
                delete_error.set(None);
                let client = Arc::clone(&client);
                wasm_bindgen_futures::spawn_local(async move {
                    match client.delete_stack(&id).await {
                        Ok(_) => {
                            refetch_trigger.update(|n| *n = n.wrapping_add(1));
                            show_delete_confirm.set(false);
                            reset_delete_state();
                        }
                        Err(e) => {
                            delete_error.set(Some(format!("Failed to delete: {}", e)));
                        }
                    }
                    deleting.set(false);
                });
            }
        })
    };

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Adapters"</TableHead>
                        <TableHead>"Workflow"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {stacks
                        .into_iter()
                        .map(|stack| {
                            let client = Arc::clone(&client);
                            view! {
                                <StackRow
                                    stack=stack
                                    client=client
                                    refetch_trigger=refetch_trigger
                                    show_delete_confirm=show_delete_confirm
                                    pending_delete_id=pending_delete_id
                                    pending_delete_name=pending_delete_name
                                />
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>

        {move || {
            let name = pending_delete_name.get();
            let error = delete_error.get();
            let description = if let Some(ref err) = error {
                format!(
                    "This will permanently delete the adapter stack '{}'. This action cannot be undone.\n\nError: {}",
                    name,
                    err
                )
            } else {
                format!("This will permanently delete the adapter stack '{}'. This action cannot be undone.", name)
            };
            view! {
                <ConfirmationDialog
                    open=show_delete_confirm
                    title="Delete Stack"
                    description=description
                    severity=ConfirmationSeverity::Destructive
                    confirm_text="Delete"
                    typed_confirmation=name.clone()
                    on_confirm=on_confirm_delete
                    on_cancel=on_cancel_delete
                    loading=Signal::derive(move || deleting.get())
                />
            }
        }}
    }
    .into_any()
}

/// Individual stack row component
#[component]
fn StackRow(
    stack: StackResponse,
    client: Arc<ApiClient>,
    refetch_trigger: RwSignal<u32>,
    show_delete_confirm: RwSignal<bool>,
    pending_delete_id: RwSignal<Option<String>>,
    pending_delete_name: RwSignal<String>,
) -> impl IntoView {
    let id = stack.id.clone();
    let id_link = id.clone();
    let id_activate = id.clone();
    let id_delete = id.clone();
    let name = stack.name.clone();
    let name_for_delete = name.clone();
    let adapter_count = stack.adapter_ids.len();
    let workflow_label = workflow_type_label(&stack.workflow_type);
    let is_active = stack.is_active;
    let is_default = stack.is_default;
    let lifecycle_state = stack.lifecycle_state.clone();

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <TableRow>
            <TableCell>
                <div class="flex flex-col">
                    <a
                        href=format!("/stacks/{}", id_link)
                        class="font-medium hover:underline"
                    >
                        {name}
                    </a>
                    {is_default.then(|| view! {
                        <span class="text-xs text-muted-foreground">"(default)"</span>
                    })}
                </div>
            </TableCell>
            <TableCell>
                <Badge variant=BadgeVariant::Secondary>
                    {format!("{} adapter{}", adapter_count, if adapter_count == 1 { "" } else { "s" })}
                </Badge>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{workflow_label}</span>
            </TableCell>
            <TableCell>
                <div class="flex items-center gap-2">
                    <Badge variant=lifecycle_badge_variant(&lifecycle_state)>
                        {lifecycle_state}
                    </Badge>
                    {is_active.then(|| view! {
                        <Badge variant=BadgeVariant::Success>"Active"</Badge>
                    })}
                </div>
            </TableCell>
            <TableCell>
                <div class="flex items-center gap-2">
                    <a
                        href=format!("/stacks/{}", id)
                        class="text-sm text-primary hover:underline"
                    >
                        "View"
                    </a>
                    {if is_active {
                        let client = Arc::clone(&client);
                        view! {
                            <button
                                class="text-sm text-status-warning hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    let client = Arc::clone(&client);
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if client.deactivate_stack().await.is_ok() {
                                            trigger_refresh();
                                        }
                                    });
                                }
                            >
                                "Deactivate"
                            </button>
                        }.into_any()
                    } else {
                        let client = Arc::clone(&client);
                        let id_for_activate = id_activate.clone();
                        view! {
                            <button
                                class="text-sm text-status-success hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    let client = Arc::clone(&client);
                                    let id = id_for_activate.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if client.activate_stack(&id).await.is_ok() {
                                            trigger_refresh();
                                        }
                                    });
                                }
                            >
                                "Activate"
                            </button>
                        }.into_any()
                    }}
                    {
                        let id_for_delete = id_delete.clone();
                        let name_for_delete = name_for_delete.clone();
                        view! {
                            <button
                                class="text-sm text-destructive hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    pending_delete_id.set(Some(id_for_delete.clone()));
                                    pending_delete_name.set(name_for_delete.clone());
                                    show_delete_confirm.set(true);
                                }
                            >
                                "Delete"
                            </button>
                        }
                    }
                </div>
            </TableCell>
        </TableRow>
    }
}

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
    let refetch_trigger = RwSignal::new(0u32);

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch.run(());
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <a href="/stacks" class="text-muted-foreground hover:text-foreground">
                        "< Stacks"
                    </a>
                    <h1 class="text-3xl font-bold tracking-tight">"Stack Details"</h1>
                </div>
                <div class="flex items-center gap-2">
                    <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
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
                                refetch_trigger=refetch_trigger
                            />
                            <EditStackDialog
                                open=show_edit_dialog
                                stack=data
                                refetch_trigger=refetch_trigger
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
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
fn StackDetailContent(
    stack: StackResponse,
    all_adapters: Vec<AdapterResponse>,
    refetch_trigger: RwSignal<u32>,
) -> impl IntoView {
    let client = use_api();
    let stack_id = stack.id.clone();
    let stack_id_activate = stack_id.clone();
    let is_active = stack.is_active;

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

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
                    <div>
                        <p class="text-sm text-muted-foreground">"Stack ID"</p>
                        <p class="font-mono text-sm">{stack.id.clone()}</p>
                    </div>
                    {stack.description.clone().map(|desc| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Description"</p>
                            <p class="text-sm">{desc}</p>
                        </div>
                    })}
                    <div>
                        <p class="text-sm text-muted-foreground">"Tenant ID"</p>
                        <p class="font-mono text-sm">{stack.tenant_id.clone()}</p>
                    </div>
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
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if client.deactivate_stack().await.is_ok() {
                                                trigger_refresh();
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
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if client.activate_stack(&id).await.is_ok() {
                                                trigger_refresh();
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

/// Create stack dialog
#[component]
fn CreateStackDialog(open: RwSignal<bool>, refetch_trigger: RwSignal<u32>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let workflow_type = RwSignal::new("parallel".to_string());
    let determinism_mode = RwSignal::new("strict".to_string());
    let creating = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    // Fetch available adapters
    let (adapters, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    let selected_adapter_ids = RwSignal::new(Vec::<String>::new());

    let client = use_api();

    let on_submit = {
        let client = Arc::clone(&client);
        move |_| {
            let name_val = name.get();
            if name_val.is_empty() {
                error.set(Some("Stack name is required".to_string()));
                return;
            }

            creating.set(true);
            error.set(None);

            let client = Arc::clone(&client);
            let adapter_ids = selected_adapter_ids.get();
            let desc = description.get();
            let wf_type = workflow_type.get();
            let det_mode = determinism_mode.get();

            wasm_bindgen_futures::spawn_local(async move {
                let workflow = match wf_type.as_str() {
                    "parallel" => Some(WorkflowType::Parallel),
                    "sequential" => Some(WorkflowType::Sequential),
                    "upstream_downstream" => Some(WorkflowType::UpstreamDownstream),
                    _ => None,
                };

                let request = CreateStackRequest {
                    name: name_val,
                    description: if desc.is_empty() { None } else { Some(desc) },
                    adapter_ids,
                    workflow_type: workflow,
                    metadata: None,
                    determinism_mode: Some(det_mode),
                    routing_determinism_mode: None,
                };

                match client.create_stack(&request).await {
                    Ok(_) => {
                        creating.set(false);
                        name.set(String::new());
                        description.set(String::new());
                        selected_adapter_ids.set(vec![]);
                        open.set(false);
                        refetch_trigger.update(|n| *n = n.wrapping_add(1));
                    }
                    Err(e) => {
                        creating.set(false);
                        error.set(Some(e.to_string()));
                    }
                }
            });
        }
    };

    view! {
        {move || {
            if open.get() {
                view! {
                    // Backdrop
                    <div
                        class="fixed inset-0 z-50 bg-black/80"
                        on:click=move |_| open.set(false)
                    />

                    // Dialog content
                    <div class="dialog-content dialog-scrollable">
                        <div class="flex flex-col space-y-1.5">
                            <h2 class="text-lg font-semibold">"Create Adapter Stack"</h2>
                            <p class="text-sm text-muted-foreground">
                                "Create a new stack to compose multiple adapters for inference."
                            </p>
                        </div>

                        <div class="space-y-4 py-4">
                            <Input
                                value=name
                                label="Name".to_string()
                                placeholder="my-stack".to_string()
                            />

                            <Textarea
                                value=description
                                label="Description"
                                placeholder="Optional description for this stack".to_string()
                            />

                            <Select
                                value=workflow_type
                                label="Workflow Type"
                                options=vec![
                                    ("parallel".to_string(), "Parallel".to_string()),
                                    ("sequential".to_string(), "Sequential".to_string()),
                                    ("upstream_downstream".to_string(), "Upstream/Downstream".to_string()),
                                ]
                            />

                            <Select
                                value=determinism_mode
                                label="Determinism Mode"
                                options=vec![
                                    ("strict".to_string(), "Strict".to_string()),
                                    ("besteffort".to_string(), "Best Effort".to_string()),
                                    ("relaxed".to_string(), "Relaxed".to_string()),
                                ]
                            />

                            // Adapter selection
                            <div class="space-y-2">
                                <label class="text-sm font-medium">"Select Adapters"</label>
                                {move || {
                                    match adapters.get() {
                                        LoadingState::Loading | LoadingState::Idle => {
                                            view! { <Spinner/> }.into_any()
                                        }
                                        LoadingState::Loaded(adapter_list) => {
                                            if adapter_list.is_empty() {
                                                view! {
                                                    <p class="text-sm text-muted-foreground">
                                                        "No adapters available"
                                                    </p>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <AdapterCheckboxList
                                                        adapters=adapter_list
                                                        selected=selected_adapter_ids
                                                    />
                                                }.into_any()
                                            }
                                        }
                                        LoadingState::Error(_) => {
                                            view! {
                                                <p class="text-sm text-destructive">
                                                    "Failed to load adapters"
                                                </p>
                                            }.into_any()
                                        }
                                    }
                                }}
                                <p class="text-xs text-muted-foreground">
                                    {move || format!("{} adapter(s) selected", selected_adapter_ids.get().len())}
                                </p>
                            </div>

                            {move || error.get().map(|e| view! {
                                <div class="text-sm text-destructive p-2 bg-destructive/10 rounded">
                                    {e}
                                </div>
                            })}
                        </div>

                        <div class="flex justify-end gap-2">
                            <Button
                                variant=ButtonVariant::Outline
                                on_click=Callback::new(move |_| open.set(false))
                            >
                                "Cancel"
                            </Button>
                            <Button
                                variant=ButtonVariant::Primary
                                loading=creating.get()
                                disabled=creating.get()
                                on_click=Callback::new(on_submit.clone())
                            >
                                "Create Stack"
                            </Button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }
        }}
    }
}

/// Adapter checkbox list component
#[component]
fn AdapterCheckboxList(
    adapters: Vec<AdapterResponse>,
    selected: RwSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        <div class="max-h-48 overflow-y-auto border rounded-md p-2 space-y-1">
            {adapters.into_iter().map(|adapter| {
                let adapter_id = adapter.id.clone();
                let adapter_id_check = adapter_id.clone();
                let adapter_id_toggle = adapter_id.clone();
                let adapter_name = adapter.name.clone();

                view! {
                    <label class="flex items-center gap-2 p-2 hover:bg-muted rounded cursor-pointer">
                        <input
                            type="checkbox"
                            class="rounded border-input"
                            checked=move || selected.get().contains(&adapter_id_check)
                            on:change=move |_| {
                                let id = adapter_id_toggle.clone();
                                selected.update(|ids| {
                                    if ids.contains(&id) {
                                        ids.retain(|x| x != &id);
                                    } else {
                                        ids.push(id);
                                    }
                                });
                            }
                        />
                        <span class="text-sm">{adapter_name}</span>
                    </label>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Edit stack dialog
#[component]
fn EditStackDialog(
    open: RwSignal<bool>,
    stack: StackResponse,
    refetch_trigger: RwSignal<u32>,
) -> impl IntoView {
    let name = RwSignal::new(stack.name.clone());
    let description = RwSignal::new(stack.description.clone().unwrap_or_default());
    let workflow_type = RwSignal::new(
        stack
            .workflow_type
            .as_ref()
            .map(|w| match w {
                WorkflowType::Parallel => "parallel",
                WorkflowType::Sequential => "sequential",
                WorkflowType::UpstreamDownstream => "upstream_downstream",
            })
            .unwrap_or("parallel")
            .to_string(),
    );
    let updating = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    // Fetch available adapters
    let (adapters, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    let selected_adapter_ids = RwSignal::new(stack.adapter_ids.clone());
    let stack_id = stack.id.clone();

    let client = use_api();

    let on_submit = {
        let client = Arc::clone(&client);
        let stack_id = stack_id.clone();
        move |_| {
            let name_val = name.get();
            if name_val.is_empty() {
                error.set(Some("Stack name is required".to_string()));
                return;
            }

            updating.set(true);
            error.set(None);

            let client = Arc::clone(&client);
            let adapter_ids = selected_adapter_ids.get();
            let desc = description.get();
            let wf_type = workflow_type.get();
            let id = stack_id.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let workflow = match wf_type.as_str() {
                    "parallel" => Some(WorkflowType::Parallel),
                    "sequential" => Some(WorkflowType::Sequential),
                    "upstream_downstream" => Some(WorkflowType::UpstreamDownstream),
                    _ => None,
                };

                let request = UpdateStackRequest {
                    name: Some(name_val),
                    description: Some(if desc.is_empty() { String::new() } else { desc }),
                    adapter_ids: Some(adapter_ids),
                    workflow_type: workflow,
                    metadata: None,
                    determinism_mode: None,
                    routing_determinism_mode: None,
                };

                match client.update_stack(&id, &request).await {
                    Ok(_) => {
                        updating.set(false);
                        open.set(false);
                        refetch_trigger.update(|n| *n = n.wrapping_add(1));
                    }
                    Err(e) => {
                        updating.set(false);
                        error.set(Some(e.to_string()));
                    }
                }
            });
        }
    };

    view! {
        {move || {
            if open.get() {
                view! {
                    // Backdrop
                    <div
                        class="fixed inset-0 z-50 bg-black/80"
                        on:click=move |_| open.set(false)
                    />

                    // Dialog content
                    <div class="dialog-content dialog-scrollable">
                        <div class="flex flex-col space-y-1.5">
                            <h2 class="text-lg font-semibold">"Edit Adapter Stack"</h2>
                            <p class="text-sm text-muted-foreground">
                                "Update the stack configuration and adapters."
                            </p>
                        </div>

                        <div class="space-y-4 py-4">
                            <Input
                                value=name
                                label="Name".to_string()
                                placeholder="my-stack".to_string()
                            />

                            <Textarea
                                value=description
                                label="Description"
                                placeholder="Optional description for this stack".to_string()
                            />

                            <Select
                                value=workflow_type
                                label="Workflow Type"
                                options=vec![
                                    ("parallel".to_string(), "Parallel".to_string()),
                                    ("sequential".to_string(), "Sequential".to_string()),
                                    ("upstream_downstream".to_string(), "Upstream/Downstream".to_string()),
                                ]
                            />

                            // Adapter selection
                            <div class="space-y-2">
                                <label class="text-sm font-medium">"Select Adapters"</label>
                                <p class="text-xs text-muted-foreground mb-2">
                                    "Select adapters to include in this stack"
                                </p>
                                {move || {
                                    match adapters.get() {
                                        LoadingState::Loading | LoadingState::Idle => {
                                            view! { <Spinner/> }.into_any()
                                        }
                                        LoadingState::Loaded(adapter_list) => {
                                            if adapter_list.is_empty() {
                                                view! {
                                                    <p class="text-sm text-muted-foreground">
                                                        "No adapters available"
                                                    </p>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <AdapterCheckboxList
                                                        adapters=adapter_list
                                                        selected=selected_adapter_ids
                                                    />
                                                }.into_any()
                                            }
                                        }
                                        LoadingState::Error(_) => {
                                            view! {
                                                <p class="text-sm text-destructive">
                                                    "Failed to load adapters"
                                                </p>
                                            }.into_any()
                                        }
                                    }
                                }}
                                <p class="text-xs text-muted-foreground">
                                    {move || format!("{} adapter(s) selected", selected_adapter_ids.get().len())}
                                </p>
                            </div>

                            {move || error.get().map(|e| view! {
                                <div class="text-sm text-destructive p-2 bg-destructive/10 rounded">
                                    {e}
                                </div>
                            })}
                        </div>

                        <div class="flex justify-end gap-2">
                            <Button
                                variant=ButtonVariant::Outline
                                on_click=Callback::new(move |_| open.set(false))
                            >
                                "Cancel"
                            </Button>
                            <Button
                                variant=ButtonVariant::Primary
                                loading=updating.get()
                                disabled=updating.get()
                                on_click=Callback::new(on_submit.clone())
                            >
                                "Save Changes"
                            </Button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }
        }}
    }
}

// Helper functions

fn workflow_type_label(wf: &Option<WorkflowType>) -> &'static str {
    match wf {
        Some(WorkflowType::Parallel) => "Parallel",
        Some(WorkflowType::Sequential) => "Sequential",
        Some(WorkflowType::UpstreamDownstream) => "Upstream/Downstream",
        None => "Default",
    }
}

fn lifecycle_badge_variant(state: &str) -> BadgeVariant {
    match state {
        "active" => BadgeVariant::Success,
        "deprecated" => BadgeVariant::Warning,
        "retired" => BadgeVariant::Destructive,
        "draft" => BadgeVariant::Secondary,
        _ => BadgeVariant::Secondary,
    }
}
