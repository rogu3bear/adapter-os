//! Stack dialog components
//!
//! Create and edit dialogs for adapter stacks.
//! Uses canonical Dialog component for ARIA compliance and keyboard handling.

use crate::api::{ApiClient, CreateStackRequest, StackResponse, UpdateStackRequest, WorkflowType};
use crate::components::{
    AsyncBoundaryWithEmpty, Button, ButtonVariant, Checkbox, Dialog, FormField, Input, Select,
    Textarea,
};
use crate::hooks::{use_api, use_api_resource, Refetch};
use crate::signals::use_notifications;
use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Create stack dialog
#[component]
pub fn CreateStackDialog(open: RwSignal<bool>, refetch: Refetch) -> impl IntoView {
    let notifications = use_notifications();
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
            let notifications = notifications.clone();
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
                        let _ = creating.try_set(false);
                        let _ = name.try_set(String::new());
                        let _ = description.try_set(String::new());
                        let _ = selected_adapter_ids.try_set(vec![]);
                        let _ = open.try_set(false);
                        notifications
                            .success("Stack created", "Adapter stack created successfully.");
                        refetch.run(());
                    }
                    Err(e) => {
                        let _ = creating.try_set(false);
                        let _ = error.try_set(Some(e.user_message()));
                    }
                }
            });
        }
    };

    view! {
        <Dialog
            open=open
            title="Create Adapter Stack"
            description="Create a new stack to compose multiple adapters for inference."
        >
            <div class="space-y-4 py-4">
                <FormField label="Name" name="stack_name">
                    <Input
                        value=name
                        placeholder="my-stack".to_string()
                    />
                </FormField>

                <FormField label="Description" name="stack_description">
                    <Textarea
                        value=description
                        placeholder="Optional description for this stack".to_string()
                    />
                </FormField>

                <FormField label="Workflow Type" name="workflow_type">
                    <Select
                        value=workflow_type
                        options=vec![
                            ("parallel".to_string(), "Parallel".to_string()),
                            ("sequential".to_string(), "Sequential".to_string()),
                            ("upstream_downstream".to_string(), "Upstream/Downstream".to_string()),
                        ]
                    />
                </FormField>

                <FormField label="Determinism Mode" name="determinism_mode">
                    <Select
                        value=determinism_mode
                        options=vec![
                            ("strict".to_string(), "Strict".to_string()),
                            ("besteffort".to_string(), "Best Effort".to_string()),
                            ("relaxed".to_string(), "Relaxed".to_string()),
                        ]
                    />
                </FormField>

                // Adapter selection
                <div class="space-y-2">
                    <p class="text-sm font-medium">"Select Adapters"</p>
                    <AsyncBoundaryWithEmpty
                        state=adapters
                        is_empty={|list: &Vec<AdapterResponse>| list.is_empty()}
                        empty_title="No adapters available"
                        empty_description="Register adapters to add them to stacks."
                        render={move |adapter_list| view! {
                            <AdapterCheckboxList
                                adapters=adapter_list
                                selected=selected_adapter_ids
                            />
                        }}
                    />
                    <p class="text-xs text-muted-foreground">
                        {move || format!("{} adapter(s) selected", selected_adapter_ids.get().len())}
                    </p>
                </div>

                {move || error.get().map(|e| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
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
                    loading=Signal::from(creating)
                    disabled=Signal::from(creating)
                    on_click=Callback::new(on_submit.clone())
                >
                    "Create Stack"
                </Button>
            </div>
        </Dialog>
    }
}

/// Adapter checkbox list component
#[component]
pub fn AdapterCheckboxList(
    adapters: Vec<AdapterResponse>,
    selected: RwSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        <div class="max-h-48 overflow-y-auto border rounded-md p-2 space-y-1">
            {adapters.into_iter().map(|adapter| {
                let adapter_id = adapter.id.clone();
                let adapter_id_for_check = adapter_id.clone();
                let adapter_id_for_toggle = adapter_id.clone();
                let adapter_name = adapter.name.clone();

                let is_checked = Signal::derive(move || {
                    selected.get().contains(&adapter_id_for_check)
                });

                view! {
                    <Checkbox
                        checked=is_checked
                        on_change=Callback::new(move |_checked| {
                            let id = adapter_id_for_toggle.clone();
                            selected.update(|ids| {
                                if ids.contains(&id) {
                                    ids.retain(|x| x != &id);
                                } else {
                                    ids.push(id);
                                }
                            });
                        })
                        label=adapter_name
                        class="p-2 hover:bg-muted rounded"
                    />
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Edit stack dialog
#[component]
pub fn EditStackDialog(
    open: RwSignal<bool>,
    stack: StackResponse,
    refetch: Refetch,
) -> impl IntoView {
    let notifications = use_notifications();
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
            let notifications = notifications.clone();
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
                        let _ = updating.try_set(false);
                        let _ = open.try_set(false);
                        notifications
                            .success("Stack updated", "Adapter stack updated successfully.");
                        refetch.run(());
                    }
                    Err(e) => {
                        let _ = updating.try_set(false);
                        let _ = error.try_set(Some(e.user_message()));
                    }
                }
            });
        }
    };

    view! {
        <Dialog
            open=open
            title="Edit Adapter Stack"
            description="Update the stack configuration and adapters."
        >
            <div class="space-y-4 py-4">
                <FormField label="Name" name="edit_stack_name">
                    <Input
                        value=name
                        placeholder="my-stack".to_string()
                    />
                </FormField>

                <FormField label="Description" name="edit_stack_description">
                    <Textarea
                        value=description
                        placeholder="Optional description for this stack".to_string()
                    />
                </FormField>

                <FormField label="Workflow Type" name="edit_workflow_type">
                    <Select
                        value=workflow_type
                        options=vec![
                            ("parallel".to_string(), "Parallel".to_string()),
                            ("sequential".to_string(), "Sequential".to_string()),
                            ("upstream_downstream".to_string(), "Upstream/Downstream".to_string()),
                        ]
                    />
                </FormField>

                // Adapter selection
                <div class="space-y-2">
                    <p class="text-sm font-medium">"Select Adapters"</p>
                    <p class="text-xs text-muted-foreground mb-2">
                        "Select adapters to include in this stack"
                    </p>
                    <AsyncBoundaryWithEmpty
                        state=adapters
                        is_empty={|list: &Vec<AdapterResponse>| list.is_empty()}
                        empty_title="No adapters available"
                        empty_description="Register adapters to add them to stacks."
                        render={move |adapter_list| view! {
                            <AdapterCheckboxList
                                adapters=adapter_list
                                selected=selected_adapter_ids
                            />
                        }}
                    />
                    <p class="text-xs text-muted-foreground">
                        {move || format!("{} adapter(s) selected", selected_adapter_ids.get().len())}
                    </p>
                </div>

                {move || error.get().map(|e| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
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
                    loading=Signal::from(updating)
                    disabled=Signal::from(updating)
                    on_click=Callback::new(on_submit.clone())
                >
                    "Save Changes"
                </Button>
            </div>
        </Dialog>
    }
}
