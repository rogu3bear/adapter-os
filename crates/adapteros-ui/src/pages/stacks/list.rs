//! Stacks list components
//!
//! List view and row components for adapter stacks.

use super::helpers::{lifecycle_badge_variant, workflow_type_label};
use crate::api::{ApiClient, StackResponse};
use crate::components::{
    Badge, BadgeVariant, Card, ConfirmationDialog, ConfirmationSeverity, EmptyState, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::Refetch;
use leptos::prelude::*;
use std::sync::Arc;

/// List of stacks component
#[component]
pub fn StacksList(stacks: Vec<StackResponse>, refetch: Refetch) -> impl IntoView {
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

    let client = crate::hooks::use_api();

    // Delete confirmation dialog state
    let show_delete_confirm = RwSignal::new(false);
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let pending_delete_name = RwSignal::new(String::new());
    let deleting = RwSignal::new(false);
    let delete_error = RwSignal::new(Option::<String>::None);

    // Activate confirmation dialog state
    let show_activate_confirm = RwSignal::new(false);
    let pending_activate_id = RwSignal::new(Option::<String>::None);
    let pending_activate_name = RwSignal::new(String::new());
    let activating = RwSignal::new(false);

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
                            refetch.run(());
                            let _ = show_delete_confirm.try_set(false);
                            reset_delete_state();
                        }
                        Err(e) => {
                            let _ = delete_error.try_set(Some(format!("Failed to delete: {}", e)));
                        }
                    }
                    let _ = deleting.try_set(false);
                });
            }
        })
    };

    // Reset activate dialog state
    let reset_activate_state = move || {
        pending_activate_id.set(None);
        pending_activate_name.set(String::new());
    };

    // Handle cancel/close of activate dialog
    let on_cancel_activate = Callback::new(move |_| {
        reset_activate_state();
    });

    // Handle confirmed activation
    let on_confirm_activate = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            if let Some(id) = pending_activate_id.get() {
                activating.set(true);
                let client = Arc::clone(&client);
                wasm_bindgen_futures::spawn_local(async move {
                    match client.activate_stack(&id).await {
                        Ok(_) => {
                            refetch.run(());
                            let _ = show_activate_confirm.try_set(false);
                            reset_activate_state();
                        }
                        Err(e) => {
                            tracing::error!("Failed to activate stack: {}", e);
                        }
                    }
                    let _ = activating.try_set(false);
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
                                    refetch=refetch
                                    show_delete_confirm=show_delete_confirm
                                    pending_delete_id=pending_delete_id
                                    pending_delete_name=pending_delete_name
                                    show_activate_confirm=show_activate_confirm
                                    pending_activate_id=pending_activate_id
                                    pending_activate_name=pending_activate_name
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

        {move || {
            let name = pending_activate_name.get();
            let description = format!(
                "Activating '{}' will route inference requests to this adapter stack. This may affect running workloads. Continue?",
                name
            );
            view! {
                <ConfirmationDialog
                    open=show_activate_confirm
                    title="Activate Stack"
                    description=description
                    severity=ConfirmationSeverity::Warning
                    confirm_text="Activate"
                    on_confirm=on_confirm_activate
                    on_cancel=on_cancel_activate
                    loading=Signal::derive(move || activating.get())
                />
            }
        }}
    }
    .into_any()
}

/// Individual stack row component
#[component]
pub fn StackRow(
    stack: StackResponse,
    client: Arc<ApiClient>,
    refetch: Refetch,
    show_delete_confirm: RwSignal<bool>,
    pending_delete_id: RwSignal<Option<String>>,
    pending_delete_name: RwSignal<String>,
    show_activate_confirm: RwSignal<bool>,
    pending_activate_id: RwSignal<Option<String>>,
    pending_activate_name: RwSignal<String>,
) -> impl IntoView {
    let id = stack.id.clone();
    let id_link = id.clone();
    let id_activate = id.clone();
    let id_delete = id.clone();
    let name = stack.name.clone();
    let name_for_delete = name.clone();
    let name_for_activate = name.clone();
    let adapter_count = stack.adapter_ids.len();
    let workflow_label = workflow_type_label(&stack.workflow_type);
    let is_active = stack.is_active;
    let is_default = stack.is_default;
    let lifecycle_state = stack.lifecycle_state.clone();

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
                                            refetch.run(());
                                        }
                                    });
                                }
                            >
                                "Deactivate"
                            </button>
                        }.into_any()
                    } else {
                        let id_for_activate = id_activate.clone();
                        let name_for_activate = name_for_activate.clone();
                        view! {
                            <button
                                class="text-sm text-status-success hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    pending_activate_id.set(Some(id_for_activate.clone()));
                                    pending_activate_name.set(name_for_activate.clone());
                                    show_activate_confirm.set(true);
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
