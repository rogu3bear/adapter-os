//! Routing Rules management component

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, Card, EmptyState, ErrorDisplay, Input, LoadingDisplay, PageHeader,
    RefreshButton, Select, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api, use_api_resource, LoadingState};
use adapteros_api_types::{CreateRoutingRuleRequest, RoutingRuleResponse};
use leptos::prelude::*;
use std::sync::Arc;
#[component]
pub fn RoutingRules() -> impl IntoView {
    // Fetch identity datasets for the dropdown
    let (identity_datasets, _) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.list_datasets(Some("identity")).await
    });

    // Fetch all active adapters for mapping
    let (adapters, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    let selected_dataset_id = RwSignal::new(Option::<String>::None);

    // Fetch rules for selected dataset
    let (rules, refetch_rules) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = selected_dataset_id.get();
        async move {
            match id {
                Some(id) => client.list_routing_rules(&id).await.map(|r| r.rules),
                None => Ok(Vec::new()),
            }
        }
    });

    let refetch_trigger = RwSignal::new(0usize);

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch_rules.run(());
    });

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Routing Rules"
                subtitle="Map Identity Set outcomes to target adapters"
            >
                <RefreshButton on_click=Callback::new(move |_| refetch_trigger.update(|n| *n = n.wrapping_add(1)))/>
            </PageHeader>

            <div class="grid gap-6 md:grid-cols-4">
                <Card class="md:col-span-1">
                    <h3 class="text-sm font-semibold mb-4">"Select Identity Set"</h3>
                    {move || match identity_datasets.get() {
                        LoadingState::Loaded(data) => {
                            if data.datasets.is_empty() {
                                view! { <p class="text-xs text-muted-foreground">"No Identity Sets found. Mark a dataset as 'Identity' first."</p> }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-2">
                                        {data.datasets.into_iter().map(|ds| {
                                            let id = ds.id.clone();
                                            let name = ds.name.clone();
                                            let is_selected = Signal::derive(move || selected_dataset_id.get() == Some(id.clone()));

                                            let ds_id = ds.id.clone();
                                            view! {
                                                <button
                                                    class=move || format!(
                                                        "w-full text-left px-3 py-2 rounded-md text-sm transition-colors {}",
                                                        if is_selected.get() { "bg-primary text-primary-foreground" } else { "hover:bg-muted text-foreground" }
                                                    )
                                                    on:click=move |_| selected_dataset_id.set(Some(ds_id.clone()))
                                                >
                                                    {name}
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Loading | LoadingState::Idle => view! { <Spinner/> }.into_any(),
                        LoadingState::Error(_) => view! { <p class="text-destructive text-xs">"Failed to load identity sets"</p> }.into_any(),
                    }}
                </Card>

                <div class="md:col-span-3 space-y-6">
                    {move || match selected_dataset_id.get() {
                        None => view! {
                            <Card>
                                <EmptyState
                                    title="No identity set selected"
                                    description="Select an identity set from the left to manage its routing rules."
                                />
                            </Card>
                        }.into_any(),
                        Some(_) => {
                            match rules.get() {
                                LoadingState::Loading | LoadingState::Idle => view! { <LoadingDisplay message="Loading rules..."/> }.into_any(),
                                LoadingState::Error(e) => {
                                    view! { <ErrorDisplay error=e on_retry=refetch_rules/> }.into_any()
                                }
                                LoadingState::Loaded(rule_list) => {
                                    view! {
                                        <div class="space-y-6">
                                            <CreateRuleForm
                                                dataset_id=selected_dataset_id.get().unwrap_or_default()
                                                adapters=adapters.get()
                                                on_success=refetch_rules
                                            />
                                            <RulesTable
                                                rules=rule_list
                                                on_delete=refetch_rules
                                            />
                                        </div>
                                    }.into_any()
                                }
                            }
                        }
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
fn CreateRuleForm(
    dataset_id: String,
    adapters: LoadingState<Vec<adapteros_api_types::AdapterResponse>>,
    on_success: Callback<()>,
) -> impl IntoView {
    let client = use_api();
    let condition = RwSignal::new(String::new());
    let target_adapter_id = RwSignal::new(String::new());
    let priority = RwSignal::new(1i64);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    let handle_submit = move |_| {
        let cond = condition.get();
        let target = target_adapter_id.get();
        if cond.is_empty() || target.is_empty() {
            error.set(Some("Condition and target adapter are required".into()));
            return;
        }

        // JSON validation for condition logic
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&cond) {
            error.set(Some(format!("Invalid JSON in condition: {}", e)));
            return;
        }

        saving.set(true);
        error.set(None);

        let client = Arc::clone(&client);
        let ds_id = dataset_id.clone();
        let p = priority.get();

        wasm_bindgen_futures::spawn_local(async move {
            let req = CreateRoutingRuleRequest {
                identity_dataset_id: ds_id,
                condition_logic: cond,
                target_adapter_id: target,
                priority: p,
            };

            match client.create_routing_rule(&req).await {
                Ok(_) => {
                    condition.set(String::new());
                    target_adapter_id.set(String::new());
                    on_success.run(());
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    };

    view! {
        <Card>
            <h3 class="text-sm font-semibold mb-4">"Add New Rule"</h3>
            <div class="grid gap-4 md:grid-cols-4 items-end">
                <div class="space-y-2 md:col-span-2">
                    <label class="text-xs font-medium">"Condition (Outcome)"</label>
                    <input
                        type="text"
                        class="w-full px-3 py-2 bg-background border rounded-md text-sm"
                        placeholder="e.g. sentiment == 'negative'"
                        prop:value=condition
                        on:input=move |e| condition.set(event_target_value(&e))
                    />
                </div>
                <div class="space-y-2">
                    <label class="text-xs font-medium">"Target Adapter"</label>
                    <select
                        class="w-full px-3 py-2 bg-background border rounded-md text-sm"
                        on:change=move |e| target_adapter_id.set(event_target_value(&e))
                    >
                        <option value="">"Select Adapter"</option>
                        {match adapters {
                            LoadingState::Loaded(list) => {
                                list.into_iter().map(|a| {
                                    view! { <option value=a.adapter_id.clone()>{a.name}</option> }
                                }).collect::<Vec<_>>().into_any()
                            }
                            _ => Vec::<AnyView>::new().into_any(),
                        }}
                    </select>
                </div>
                <div class="space-y-2">
                    <label class="text-xs font-medium">"Priority"</label>
                    <input
                        type="number"
                        class="w-full px-3 py-2 bg-background border rounded-md text-sm"
                        prop:value=priority
                        on:input=move |e| {
                            if let Ok(p) = event_target_value(&e).parse::<i64>() {
                                priority.set(p);
                            }
                        }
                    />
                </div>
                <Button
                    variant=ButtonVariant::Primary
                    loading=saving
                    on_click=Callback::new(handle_submit)
                >
                    "Add Rule"
                </Button>
            </div>
            {move || error.get().map(|e| view! { <p class="mt-2 text-xs text-destructive">{e}</p> })}
        </Card>
    }
}

#[component]
fn RulesTable(rules: Vec<RoutingRuleResponse>, on_delete: Callback<()>) -> impl IntoView {
    let client = use_api();
    let deleting = RwSignal::new(false);

    if rules.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    title="No rules defined"
                    description="Condition-based routing rules will appear here."
                />
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Condition"</TableHead>
                        <TableHead>"Target Adapter"</TableHead>
                        <TableHead>"Priority"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rules.into_iter().map(|rule| {
                        let id = rule.id.clone();
                        let client = Arc::clone(&client);

                        view! {
                            <TableRow>
                                <TableCell class="font-mono text-xs">{rule.condition_logic}</TableCell>
                                <TableCell>{rule.target_adapter_id}</TableCell>
                                <TableCell>{rule.priority}</TableCell>
                                <TableCell class="text-right">
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        on_click=Callback::new(move |_| {
                                            let id = id.clone();
                                            let client = Arc::clone(&client);
                                            deleting.set(true);
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let _ = client.delete_routing_rule(&id).await;
                                                deleting.set(false);
                                                on_delete.run(());
                                            });
                                        })
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-destructive" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                        </svg>
                                    </Button>
                                </TableCell>
                            </TableRow>
                        }
                    }).collect_view()}
                </TableBody>
            </Table>
        </Card>
    }.into_any()
}
