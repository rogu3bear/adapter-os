//! Policies page
//!
//! Policy pack management with list view and detail panel.

use crate::api::client::{ApiClient, PolicyPackResponse, PolicyValidationResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, EmptyState, EmptyStateVariant, ErrorDisplay,
    Spinner, SplitPanel, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Policies management page
#[component]
pub fn Policies() -> impl IntoView {
    // Selected policy CPID for detail panel
    let selected_cpid = RwSignal::new(None::<String>);

    // Fetch policies
    let (policies, refetch_policies) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_policies().await });

    let on_policy_select = move |cpid: String| {
        selected_cpid.set(Some(cpid));
    };

    let on_close_detail = move || {
        selected_cpid.set(None);
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_cpid.get().is_some());

    view! {
        <div class="p-6 space-y-6">
            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Policies"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Header
                            <div class="flex items-center justify-between">
                                <div>
                                    <h1 class="text-3xl font-bold tracking-tight">"Policy Packs"</h1>
                                    <p class="text-muted-foreground mt-1">"Manage inference policies and enforcement rules"</p>
                                </div>
                                <div class="flex items-center gap-2">
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=Callback::new(move |_| refetch_policies.run(()))
                                    >
                                        "Refresh"
                                    </Button>
                                </div>
                            </div>

                            // Policy list
                            {move || {
                                match policies.get() {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <div class="flex items-center justify-center py-12">
                                                <Spinner/>
                                            </div>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(data) => {
                                        view! {
                                            <PolicyList
                                                policies=data
                                                selected_cpid=selected_cpid
                                                on_select=on_policy_select
                                            />
                                        }.into_any()
                                    }
                                    LoadingState::Error(e) => {
                                        view! {
                                            <ErrorDisplay
                                                error=e
                                                on_retry=Callback::new(move |_| refetch_policies.run(()))
                                            />
                                        }.into_any()
                                    }
                                }
                            }}
                        </div>
                    }
                }
                detail_panel=move || {
                    let cpid = selected_cpid.get().unwrap_or_default();
                    view! {
                        <PolicyDetail
                            cpid=cpid
                            on_close=on_close_detail
                        />
                    }
                }
            />
        </div>
    }
}

/// Policy list component
#[component]
fn PolicyList(
    policies: Vec<PolicyPackResponse>,
    selected_cpid: RwSignal<Option<String>>,
    on_select: impl Fn(String) + Copy + Send + 'static,
) -> impl IntoView {
    if policies.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No policy packs found"
                    description="Policy packs define enforcement rules for inference. Create a policy pack to control model behavior."
                    secondary_label="Learn about Policies"
                    secondary_href="/docs/policies"
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
                        <TableHead>"CPID"</TableHead>
                        <TableHead>"Hash"</TableHead>
                        <TableHead>"Created"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {policies
                        .into_iter()
                        .map(|policy| {
                            let cpid = policy.cpid.clone();
                            let cpid_for_click = cpid.clone();

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_cpid.get().as_ref() == Some(&cpid)
                                    on:click=move |_| on_select(cpid_for_click.clone())
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium font-mono text-sm">{policy.cpid.clone()}</p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-xs font-mono text-muted-foreground">
                                            {policy.hash_b3.chars().take(12).collect::<String>()}"..."
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_date(&policy.created_at)}
                                        </span>
                                    </TableCell>
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Policy detail panel
#[component]
fn PolicyDetail(cpid: String, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
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
                <h2 class="text-xl font-semibold">"Policy Details"</h2>
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
                match policy.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <PolicyDetailContent policy=data/>
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
fn PolicyDetailContent(policy: PolicyPackResponse) -> impl IntoView {
    // Clone all fields upfront to avoid partial move issues
    let cpid = policy.cpid.clone();
    let hash_b3 = policy.hash_b3.clone();
    let hash_b3_title = policy.hash_b3.clone();
    let created_at = policy.created_at.clone();
    let content = policy.content.clone();

    view! {
        // Metadata
        <Card title="Metadata".to_string()>
            <div class="grid gap-3 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"CPID"</span>
                    <span class="font-mono text-xs">{cpid}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Hash (BLAKE3)"</span>
                    <span class="font-mono text-xs truncate max-w-truncate" title=hash_b3_title>
                        {hash_b3}
                    </span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Created"</span>
                    <span>{format_date(&created_at)}</span>
                </div>
            </div>
        </Card>

        // Policy content
        <Card title="Policy Content".to_string() class="mt-4".to_string()>
            <div class="bg-zinc-950 rounded-md p-4 overflow-auto max-h-96">
                <pre class="text-xs text-status-success font-mono whitespace-pre-wrap">
                    {content}
                </pre>
            </div>
        </Card>

        // Actions (role-gated via backend)
        <PolicyActionsCard policy=policy/>
    }
}

/// Policy actions card with validation and apply functionality
#[component]
fn PolicyActionsCard(policy: PolicyPackResponse) -> impl IntoView {
    let (validating, set_validating) = signal(false);
    let (validation_result, set_validation_result) =
        signal(None::<Result<PolicyValidationResponse, String>>);

    let content = policy.content.clone();
    let cpid = policy.cpid.clone();

    // Validate handler
    let on_validate = move |_| {
        let content = content.clone();
        set_validating.set(true);
        set_validation_result.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let result = client.validate_policy(&content).await;
            set_validation_result.set(Some(result.map_err(|e| e.to_string())));
            set_validating.set(false);
        });
    };

    // Apply handler (reapply with current content)
    let content_for_apply = policy.content.clone();
    let (applying, set_applying) = signal(false);
    let (apply_result, set_apply_result) = signal(None::<Result<(), String>>);

    let on_apply = move |_| {
        let cpid = cpid.clone();
        let content = content_for_apply.clone();
        set_applying.set(true);
        set_apply_result.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let result = client.apply_policy(&cpid, &content, None).await;
            set_apply_result.set(Some(result.map(|_| ()).map_err(|e| e.to_string())));
            set_applying.set(false);
        });
    };

    view! {
        <Card title="Actions".to_string() class="mt-4".to_string()>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(on_validate)
                    disabled=Signal::derive(move || validating.get())
                >
                    {move || if validating.get() {
                        view! { <Spinner/> }.into_any()
                    } else {
                        view! { "Validate" }.into_any()
                    }}
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(on_apply)
                    disabled=Signal::derive(move || applying.get())
                >
                    {move || if applying.get() {
                        view! { <Spinner/> }.into_any()
                    } else {
                        view! { "Apply" }.into_any()
                    }}
                </Button>
            </div>

            // Validation result
            {move || validation_result.get().map(|result| {
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
            {move || apply_result.get().map(|result| {
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

// ============================================================================
// Utility functions
// ============================================================================

/// Format a date string for display
fn format_date(date_str: &str) -> String {
    if date_str.len() >= 16 {
        format!("{} {}", &date_str[0..10], &date_str[11..16])
    } else {
        date_str.to_string()
    }
}
