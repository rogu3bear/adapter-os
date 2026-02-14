//! Policies page
//!
//! Policy pack management with list view and detail panel.

use crate::api::client::{ApiClient, PolicyPackResponse, PolicyValidationResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Column, DataTable, ErrorDisplay, Input, Link,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner, SplitPanel, Textarea,
};
use crate::constants::urls::docs_link;
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use crate::utils::format_datetime;
use leptos::prelude::*;
use std::sync::Arc;

/// Policies management page
#[component]
pub fn Policies() -> impl IntoView {
    // Selected policy CPID for detail panel
    let selected_cpid = RwSignal::new(None::<String>);
    let show_create = RwSignal::new(false);
    let new_cpid = RwSignal::new(String::new());
    let new_description = RwSignal::new(String::new());
    let new_content = RwSignal::new(String::new());

    // Fetch policies
    let (policies, refetch_policies) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_policies().await });

    let on_policy_select = move |cpid: String| {
        selected_cpid.set(Some(cpid));
    };

    let on_close_detail = move || {
        selected_cpid.set(None);
    };

    let on_policy_created = {
        Callback::new(move |_| {
            show_create.set(false);
            new_cpid.set(String::new());
            new_description.set(String::new());
            new_content.set(String::new());
            refetch_policies.run(());
        })
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_cpid.try_get().flatten().is_some());

    view! {
        <PageScaffold
            title="Policy Packs"
            subtitle="Manage inference policies and enforcement rules".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Govern"),
                PageBreadcrumbItem::current("Policy Packs"),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| refetch_policies.run(()))
                >
                    "Refresh"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| {
                        if show_create.try_get().unwrap_or(false) {
                            show_create.set(false);
                            new_cpid.set(String::new());
                            new_description.set(String::new());
                            new_content.set(String::new());
                        } else {
                            show_create.set(true);
                        }
                    })
                >
                    {move || if show_create.try_get().unwrap_or(false) { "Cancel" } else { "New Policy Pack" }}
                </Button>
            </PageScaffoldActions>

            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Policies"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Create policy pack
                            {move || {
                                if show_create.try_get().unwrap_or(false) {
                                    view! {
                                        <Card
                                            title="Create Policy Pack".to_string()
                                            description="Create a new policy pack and activate it for enforcement.".to_string()
                                        >
                                            <div class="space-y-4">
                                                <Input
                                                    value=new_cpid
                                                    label="CPID".to_string()
                                                    placeholder="e.g., policy-pack-001".to_string()
                                                    required=true
                                                />
                                                <Input
                                                    value=new_description
                                                    label="Description (optional)".to_string()
                                                    placeholder="Short description of this policy pack".to_string()
                                                />
                                                <Textarea
                                                    value=new_content
                                                    label="Policy JSON".to_string()
                                                    aria_label="Policy JSON".to_string()
                                                    rows=14
                                                    class="font-mono text-xs bg-muted text-status-success min-h-48".to_string()
                                                />
                                                <PolicyActionsCard
                                                    cpid=new_cpid
                                                    content=new_content
                                                    description=new_description
                                                    apply_label="Create & Apply".to_string()
                                                    on_applied=on_policy_created
                                                />
                                            </div>
                                        </Card>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }
                            }}

                            // Policy list
                            {
                                let columns: Vec<Column<PolicyPackResponse>> = vec![
                                    Column::custom("CPID", |p: &PolicyPackResponse| {
                                        let cpid = p.cpid.clone();
                                        view! {
                                            <p class="font-medium font-mono text-sm">{cpid}</p>
                                        }
                                    }),
                                    Column::custom("Hash", |p: &PolicyPackResponse| {
                                        let hash = p.hash_b3.chars().take(12).collect::<String>();
                                        view! {
                                            <span class="text-xs font-mono text-muted-foreground">
                                                {hash}"..."
                                            </span>
                                        }
                                    }),
                                    Column::custom("Created", |p: &PolicyPackResponse| {
                                        let created = format_datetime(&p.created_at);
                                        view! {
                                            <span class="text-sm text-muted-foreground">{created}</span>
                                        }
                                    }),
                                ];

                                let row_class = {
                                    let selected_cpid = selected_cpid;
                                    Arc::new(move |p: &PolicyPackResponse| {
                                        if selected_cpid.try_get().flatten().as_ref() == Some(&p.cpid) {
                                            "bg-muted".to_string()
                                        } else {
                                            String::new()
                                        }
                                    })
                                };

                                let on_row_click = Callback::new(move |p: PolicyPackResponse| {
                                    on_policy_select(p.cpid);
                                });

                                view! {
                                    <div class="space-y-2">
                                        <DataTable
                                            data=policies
                                            columns=columns
                                            on_retry=Callback::new(move |_| refetch_policies.run(()))
                                            empty_title="No policy packs found"
                                            empty_description="Policy packs define enforcement rules for inference. Create a policy pack to control model behavior."
                                            on_row_click=on_row_click
                                            row_class=row_class
                                        />
                                        {move || match policies.get() {
                                            LoadingState::Loaded(items) if items.is_empty() => view! {
                                                <div class="text-sm text-muted-foreground text-center">
                                                    <Link
                                                        href=docs_link("policies")
                                                        target="_blank"
                                                        rel="noopener noreferrer"
                                                    >
                                                        "Learn about Policies"
                                                    </Link>
                                                </div>
                                            }.into_any(),
                                            _ => view! {}.into_any(),
                                        }}
                                    </div>
                                }
                            }
                        </div>
                    }
                }
                detail_panel=move || {
                    let cpid = selected_cpid.try_get().flatten().unwrap_or_default();
                    view! {
                        <PolicyDetail
                            cpid=cpid
                            on_close=on_close_detail
                            on_updated=refetch_policies.as_callback()
                        />
                    }
                }
            />
        </PageScaffold>
    }
}

/// Policy detail panel
#[component]
fn PolicyDetail(
    cpid: String,
    on_close: impl Fn() + Copy + 'static,
    on_updated: Callback<()>,
) -> impl IntoView {
    let cpid_for_fetch = cpid.clone();

    // Fetch policy details
    let (policy, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = cpid_for_fetch.clone();
        async move { client.get_policy(&id).await }
    });

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"Policy Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                    aria-label="Close"
                    type="button"
                    on:click=move |_| on_close()
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            {move || {
                match policy.try_get().unwrap_or_default() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let refetch_detail = refetch;
                        let on_updated = on_updated;
                        let on_applied = Callback::new(move |_| {
                            refetch_detail.run(());
                            on_updated.run(());
                        });
                        view! {
                            <PolicyDetailContent policy=data on_applied=on_applied/>
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

/// Policy detail content
#[component]
fn PolicyDetailContent(policy: PolicyPackResponse, on_applied: Callback<()>) -> impl IntoView {
    // Clone all fields upfront to avoid partial move issues
    let cpid = policy.cpid.clone();
    let hash_b3 = policy.hash_b3.clone();
    let hash_b3_title = policy.hash_b3.clone();
    let created_at = policy.created_at.clone();
    let original_content = policy.content.clone();
    let content_signal = RwSignal::new(policy.content.clone());
    let cpid_signal = RwSignal::new(cpid.clone());
    let description_signal = RwSignal::new(String::new());
    let original_for_reset = original_content.clone();
    let has_changes =
        Signal::derive(move || content_signal.try_get().unwrap_or_default() != original_content);
    view! {
        <div class="flex flex-col gap-4">
            // Metadata
            <Card title="Metadata".to_string() class="order-1".to_string()>
                <div class="grid gap-3 text-sm">
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"CPID"</span>
                        <span class="font-mono text-xs">{cpid}</span>
                    </div>
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Hash (BLAKE3)"</span>
                        <span class="font-mono text-xs truncate max-w-60" title=hash_b3_title>
                            {hash_b3}
                        </span>
                    </div>
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Created"</span>
                        <span>{format_datetime(&created_at)}</span>
                    </div>
                </div>
            </Card>

            // Actions (role-gated via backend)
            <div class="order-2 md:order-3">
                <PolicyActionsCard
                    cpid=cpid_signal
                    content=content_signal
                    description=description_signal
                    apply_label="Apply".to_string()
                    on_applied=on_applied
                />
            </div>

            // Policy content
            <Card
                title="Policy Content".to_string()
                description="Edit policy JSON and apply changes to update enforcement.".to_string()
                class="order-3 md:order-2".to_string()
            >
                <div class="flex items-center justify-between mb-2">
                    <p class="text-xs text-muted-foreground">
                        {move || if has_changes.try_get().unwrap_or(false) { "Unsaved changes" } else { "No local changes" }}
                    </p>
                    <Button
                        variant=ButtonVariant::Ghost
                        on_click=Callback::new(move |_| content_signal.set(original_for_reset.clone()))
                        disabled=Signal::derive(move || !has_changes.try_get().unwrap_or(false))
                    >
                        "Reset"
                    </Button>
                </div>
                <Textarea
                    value=content_signal
                    label="Policy JSON".to_string()
                    aria_label="Policy JSON".to_string()
                    rows=16
                    class="font-mono text-xs bg-muted text-status-success min-h-56".to_string()
                />
            </Card>
        </div>
    }
}

/// Policy actions card with validation and apply functionality
#[component]
fn PolicyActionsCard(
    cpid: RwSignal<String>,
    content: RwSignal<String>,
    description: RwSignal<String>,
    apply_label: String,
    on_applied: Callback<()>,
) -> impl IntoView {
    let alive = use_scope_alive();
    let (validating, set_validating) = signal(false);
    let (validation_result, set_validation_result) =
        signal(None::<Result<PolicyValidationResponse, String>>);

    // Validate handler
    let on_validate = move |_| {
        let content = content.try_get().unwrap_or_default();
        let _ = set_validating.try_set(true);
        let _ = set_validation_result.try_set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let result = client.validate_policy(&content).await;
            let _ = set_validation_result.try_set(Some(result.map_err(|e| e.user_message())));
            let _ = set_validating.try_set(false);
        });
    };

    // Apply handler (reapply with current content)
    let (applying, set_applying) = signal(false);
    let (apply_result, set_apply_result) = signal(None::<Result<(), String>>);

    Effect::new(move |_| {
        let Some(_) = content.try_get() else { return };
        let Some(_) = cpid.try_get() else { return };
        let _ = set_validation_result.try_set(None);
        let _ = set_apply_result.try_set(None);
    });

    let on_apply = move |_| {
        let cpid_value = cpid.try_get().unwrap_or_default();
        let content_value = content.try_get().unwrap_or_default();
        let description_value = description.try_get().unwrap_or_default();
        let on_applied = on_applied;
        let alive = alive.clone();
        if cpid_value.trim().is_empty() {
            let _ = set_apply_result.try_set(Some(Err("CPID is required".to_string())));
            return;
        }
        if content_value.trim().is_empty() {
            let _ = set_apply_result.try_set(Some(Err("Policy content is required".to_string())));
            return;
        }
        let _ = set_applying.try_set(true);
        let _ = set_apply_result.try_set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let description = if description_value.trim().is_empty() {
                None
            } else {
                Some(description_value)
            };
            let result = client
                .apply_policy(&cpid_value, &content_value, description)
                .await;
            match result {
                Ok(_) => {
                    let _ = set_apply_result.try_set(Some(Ok(())));
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        on_applied.run(());
                    }
                }
                Err(e) => {
                    let _ = set_apply_result.try_set(Some(Err(e.user_message())));
                }
            }
            let _ = set_applying.try_set(false);
        });
    };

    let apply_disabled = Signal::derive(move || {
        applying.try_get().unwrap_or(false)
            || cpid.try_get().unwrap_or_default().trim().is_empty()
            || content.try_get().unwrap_or_default().trim().is_empty()
    });

    view! {
        <Card title="Actions".to_string()>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(on_validate)
                    disabled=Signal::derive(move || validating.try_get().unwrap_or(false) || content.try_get().unwrap_or_default().trim().is_empty())
                >
                    <Show when=move || validating.try_get().unwrap_or(false) fallback=move || view! { "Validate" }>
                        <Spinner/>
                    </Show>
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(on_apply)
                    disabled=apply_disabled
                >
                    <Show when=move || applying.try_get().unwrap_or(false) fallback={
                        let apply_label = apply_label.clone();
                        move || view! { {apply_label.clone()} }
                    }>
                        <Spinner/>
                    </Show>
                </Button>
            </div>

            // Validation result
            {move || validation_result.try_get().flatten().map(|result| {
                match result {
                    Ok(resp) if resp.valid => view! {
                        <div class="mt-3 flex items-center gap-2">
                            <Badge variant=BadgeVariant::Success>"Valid"</Badge>
                            {resp.hash_b3.map(|hash| view! {
                                <span class="text-xs font-mono text-muted-foreground">
                                    {format!("Hash: {}...", &hash[..12.min(hash.len())])}
                                </span>
                            })}
                        </div>
                    }.into_any(),
                    Ok(resp) => view! {
                        <div class="mt-3">
                            <Badge variant=BadgeVariant::Destructive>"Invalid"</Badge>
                            <ul class="mt-2 text-xs text-destructive list-disc pl-4">
                                {resp.errors.into_iter().map(|e| view! { <li>{e}</li> }).collect::<Vec<_>>()}
                            </ul>
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="mt-3 text-xs text-destructive">{format!("Error: {}", e)}</div>
                    }.into_any(),
                }
            })}

            // Apply result
            {move || apply_result.try_get().flatten().map(|result| {
                match result {
                    Ok(()) => view! {
                        <div class="mt-3">
                            <Badge variant=BadgeVariant::Success>"Applied"</Badge>
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="mt-3 text-xs text-destructive">{format!("Error: {}", e)}</div>
                    }.into_any(),
                }
            })}

            <p class="text-xs text-muted-foreground mt-2">
                "Validation and enforcement actions require appropriate permissions."
            </p>
        </Card>
    }
}
