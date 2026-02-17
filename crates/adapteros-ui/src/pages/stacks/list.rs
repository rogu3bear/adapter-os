//! Stacks list components
//!
//! List view and row components for adapter stacks.

use super::helpers::{lifecycle_badge_variant, workflow_type_label};
use crate::api::{report_error_with_toast, StackResponse};
use crate::components::{
    AlertBanner, Badge, BadgeVariant, BannerVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, EmptyState, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
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
    let activate_error = RwSignal::new(Option::<String>::None);

    // Deactivate confirmation dialog state
    let show_deactivate_confirm = RwSignal::new(false);
    let pending_deactivate_name = RwSignal::new(String::new());
    let deactivating = RwSignal::new(false);
    let deactivate_error = RwSignal::new(Option::<String>::None);

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
        activate_error.set(None);
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
                activate_error.set(None);
                let client = Arc::clone(&client);
                wasm_bindgen_futures::spawn_local(async move {
                    match client.activate_stack(&id).await {
                        Ok(_) => {
                            refetch.run(());
                            let _ = show_activate_confirm.try_set(false);
                            reset_activate_state();
                        }
                        Err(e) => {
                            let _ = activate_error
                                .try_set(Some(format!("Failed to activate: {}", e.user_message())));
                            report_error_with_toast(
                                &e,
                                "Failed to activate stack",
                                Some("/stacks"),
                                true,
                            );
                        }
                    }
                    let _ = activating.try_set(false);
                });
            }
        })
    };

    // Handle confirmed deactivation
    let on_confirm_deactivate = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            deactivating.set(true);
            deactivate_error.set(None);
            let client = Arc::clone(&client);
            wasm_bindgen_futures::spawn_local(async move {
                match client.deactivate_stack().await {
                    Ok(_) => {
                        refetch.run(());
                        let _ = show_deactivate_confirm.try_set(false);
                        let _ = pending_deactivate_name.try_set(String::new());
                        let _ = deactivate_error.try_set(None);
                    }
                    Err(e) => {
                        let _ = deactivate_error
                            .try_set(Some(format!("Failed to deactivate: {}", e.user_message())));
                        report_error_with_toast(
                            &e,
                            "Failed to deactivate stack",
                            Some("/stacks"),
                            true,
                        );
                    }
                }
                let _ = deactivating.try_set(false);
            });
        })
    };

    let on_cancel_deactivate = Callback::new(move |_| {
        pending_deactivate_name.set(String::new());
        deactivate_error.set(None);
    });

    view! {
        <Card>
            {move || {
                activate_error
                    .try_get()
                    .flatten()
                    .map(|msg| {
                        view! {
                            <AlertBanner
                                title="Unable to activate stack"
                                message=msg
                                variant=BannerVariant::Error
                            />
                        }
                    })
                    .or_else(|| {
                        deactivate_error
                            .try_get()
                            .flatten()
                            .map(|msg| {
                                view! {
                                    <AlertBanner
                                        title="Unable to deactivate stack"
                                        message=msg
                                        variant=BannerVariant::Error
                                    />
                                }
                            })
                    })
            }}
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
                            view! {
                                <StackRow
                                    stack=stack
                                    show_delete_confirm=show_delete_confirm
                                    pending_delete_id=pending_delete_id
                                    pending_delete_name=pending_delete_name
                                    show_activate_confirm=show_activate_confirm
                                    pending_activate_id=pending_activate_id
                                    pending_activate_name=pending_activate_name
                                    show_deactivate_confirm=show_deactivate_confirm
                                    pending_deactivate_name=pending_deactivate_name
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
            let error = activate_error.get();
            let description = if let Some(ref err) = error {
                format!(
                    "Activating '{}' will route inference requests to this adapter stack. This may affect running workloads. Continue?\n\nError: {}",
                    name, err
                )
            } else {
                format!(
                    "Activating '{}' will route inference requests to this adapter stack. This may affect running workloads. Continue?",
                    name
                )
            };
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

        {move || {
            let name = pending_deactivate_name.get();
            let error = deactivate_error.get();
            let description = if let Some(ref err) = error {
                format!(
                    "Deactivate '{}'? Inference requests will no longer route to this stack.\n\nError: {}",
                    name, err
                )
            } else {
                format!(
                    "Deactivate '{}'? Inference requests will no longer route to this stack.",
                    name
                )
            };
            view! {
                <ConfirmationDialog
                    open=show_deactivate_confirm
                    title="Deactivate Stack"
                    description=description
                    severity=ConfirmationSeverity::Warning
                    confirm_text="Deactivate"
                    on_confirm=on_confirm_deactivate
                    on_cancel=on_cancel_deactivate
                    loading=Signal::derive(move || deactivating.get())
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
    show_delete_confirm: RwSignal<bool>,
    pending_delete_id: RwSignal<Option<String>>,
    pending_delete_name: RwSignal<String>,
    show_activate_confirm: RwSignal<bool>,
    pending_activate_id: RwSignal<Option<String>>,
    pending_activate_name: RwSignal<String>,
    show_deactivate_confirm: RwSignal<bool>,
    pending_deactivate_name: RwSignal<String>,
) -> impl IntoView {
    let id = stack.id.clone();
    let id_link = id.clone();
    let id_activate = id.clone();
    let id_delete = id.clone();
    let name = stack.name.clone();
    let name_for_delete = name.clone();
    let name_for_activate = name.clone();
    let name_for_deactivate = name.clone();
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
                        let name_for_deactivate = name_for_deactivate.clone();
                        view! {
                            <button
                                class="text-sm text-status-warning hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    pending_deactivate_name.set(name_for_deactivate.clone());
                                    show_deactivate_confirm.set(true);
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
