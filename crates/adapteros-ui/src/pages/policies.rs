//! Policies page
//!
//! Policy pack management with list view and create dialog.

use crate::api::client::{ApiClient, PolicyPackResponse, PolicyValidationResponse};
use crate::api::use_api_client;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Column, DataTable, Input, Link,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner, Textarea,
};
use crate::constants::urls::docs_link;
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use crate::utils::format_datetime;
use leptos::prelude::*;
use std::sync::Arc;

/// Policies management page
#[component]
pub fn Policies() -> impl IntoView {
    let show_create = RwSignal::new(false);
    let new_cpid = RwSignal::new(String::new());
    let new_description = RwSignal::new(String::new());
    let new_content = RwSignal::new(String::new());

    // Fetch policies
    let (policies, refetch_policies) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_policies().await });

    let on_policy_created = {
        Callback::new(move |_| {
            show_create.set(false);
            new_cpid.set(String::new());
            new_description.set(String::new());
            new_content.set(String::new());
            refetch_policies.run(());
        })
    };

    view! {
        <PageScaffold
            title="Policies"
            subtitle="Manage inference policies and enforcement rules".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::label("System"),
                PageBreadcrumbItem::current("Policies"),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| {
                        refetch_policies.run(());
                    })
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

            <div class="space-y-6">
                // Create policy pack form
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
                                    <CreatePolicyActions
                                        cpid=new_cpid
                                        content=new_content
                                        description=new_description
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

                    view! {
                        <div class="space-y-2">
                            <DataTable
                                data=policies
                                columns=columns
                                on_retry=Callback::new(move |_| refetch_policies.run(()))
                                empty_title="No policy packs found"
                                empty_description="Policy packs define enforcement rules for inference. Create a policy pack to control model behavior."
                            />
                            {move || match policies.try_get().unwrap_or(LoadingState::Loading) {
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
        </PageScaffold>
    }
}

/// Create policy actions: validate + apply
#[component]
fn CreatePolicyActions(
    cpid: RwSignal<String>,
    content: RwSignal<String>,
    description: RwSignal<String>,
    on_applied: Callback<()>,
) -> impl IntoView {
    let alive = use_scope_alive();
    let client = use_api_client();
    let (validating, set_validating) = signal(false);
    let (validation_result, set_validation_result) =
        signal(None::<Result<PolicyValidationResponse, String>>);

    // Validate handler
    let on_validate = {
        let client = client.clone();
        move |_| {
            let content = content.try_get().unwrap_or_default();
            let _ = set_validating.try_set(true);
            let _ = set_validation_result.try_set(None);

            let client = client.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = client.validate_policy(&content).await;
                let _ = set_validation_result.try_set(Some(result.map_err(|e| e.user_message())));
                let _ = set_validating.try_set(false);
            });
        }
    };

    // Apply handler
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

        let client = client.clone();
        wasm_bindgen_futures::spawn_local(async move {
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
                <Show when=move || applying.try_get().unwrap_or(false) fallback=move || view! { "Create & Apply" }>
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
    }
}
