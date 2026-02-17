//! Policies page
//!
//! Policy pack management with list view and detail panel.

use crate::api::client::{
    ApiClient, PolicyAssignmentResponse, PolicyComparisonRequest, PolicyComparisonResponse,
    PolicyPackResponse, PolicyValidationResponse, PolicyViolationResponse, SignPolicyResponse,
    VerifyPolicyResponse,
};
use crate::api::report_error_with_toast;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Column, DataTable, Dialog, ErrorDisplay,
    Input, Link, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner, SplitPanel,
    TabNav, TabPanel, Textarea,
};
use crate::constants::urls::docs_link;
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState, Refetch};
use crate::utils::{format_datetime, humanize};
use leptos::prelude::*;
use std::sync::Arc;

/// Tab identifier for the policies page
#[derive(Clone, Copy, PartialEq, Eq)]
enum PolicyTab {
    Policies,
    Assignments,
    Violations,
}

impl std::fmt::Display for PolicyTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Policies => write!(f, "policies"),
            Self::Assignments => write!(f, "assignments"),
            Self::Violations => write!(f, "violations"),
        }
    }
}

/// Policies management page
#[component]
pub fn Policies() -> impl IntoView {
    // Selected policy CPID for detail panel
    let selected_cpid = RwSignal::new(None::<String>);
    let show_create = RwSignal::new(false);
    let new_cpid = RwSignal::new(String::new());
    let new_description = RwSignal::new(String::new());
    let new_content = RwSignal::new(String::new());
    let active_tab = RwSignal::new(PolicyTab::Policies);

    // Fetch policies
    let (policies, refetch_policies) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_policies().await });

    // Fetch assignments
    let (assignments, refetch_assignments) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.list_policy_assignments(None, None).await
        });

    // Fetch violations
    let (violations, refetch_violations) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.list_violations(None, None, None, Some(50)).await
        });

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
                    on_click=Callback::new(move |_| {
                        refetch_policies.run(());
                        refetch_assignments.run(());
                        refetch_violations.run(());
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

            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Policies"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Tab navigation
                            <div class="border-b border-border">
                                <TabNav
                                    tabs=vec![
                                        (PolicyTab::Policies, "Policies"),
                                        (PolicyTab::Assignments, "Assignments"),
                                        (PolicyTab::Violations, "Violations"),
                                    ]
                                    active=active_tab
                                    aria_label="Policy management tabs".to_string()
                                />
                            </div>

                            // Policies tab
                            <TabPanel tab=PolicyTab::Policies active=active_tab tab_id="policies".to_string()>
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
                            </TabPanel>

                            // Assignments tab
                            <TabPanel tab=PolicyTab::Assignments active=active_tab tab_id="assignments".to_string()>
                                <PolicyAssignmentsPanel
                                    assignments=assignments
                                    refetch=refetch_assignments
                                />
                            </TabPanel>

                            // Violations tab
                            <TabPanel tab=PolicyTab::Violations active=active_tab tab_id="violations".to_string()>
                                <PolicyViolationsPanel
                                    violations=violations
                                    refetch=refetch_violations
                                />
                            </TabPanel>
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
    let cpid_for_gov = policy.cpid.clone();
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

            // Governance actions
            <div class="order-3 md:order-4">
                <PolicyGovernanceCard cpid=cpid_for_gov/>
            </div>

            // Policy content
            <Card
                title="Policy Content".to_string()
                description="Edit policy JSON and apply changes to update enforcement.".to_string()
                class="order-4 md:order-2".to_string()
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

// =============================================================================
// Governance Actions
// =============================================================================

/// Governance actions card: sign, verify, compare, export
#[component]
fn PolicyGovernanceCard(cpid: String) -> impl IntoView {
    let cpid_sign = cpid.clone();
    let cpid_verify = cpid.clone();
    let cpid_export = cpid.clone();
    let cpid_compare = cpid.clone();

    // Sign state
    let signing = RwSignal::new(false);
    let sign_result = RwSignal::new(None::<Result<SignPolicyResponse, String>>);

    // Verify state
    let verifying = RwSignal::new(false);
    let verify_result = RwSignal::new(None::<Result<VerifyPolicyResponse, String>>);

    // Export state
    let exporting = RwSignal::new(false);

    // Compare dialog
    let show_compare = RwSignal::new(false);

    let on_sign = move |_| {
        let cpid = cpid_sign.clone();
        signing.set(true);
        sign_result.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let result = client.sign_policy(&cpid).await;
            if let Err(ref e) = result {
                report_error_with_toast(e, "Failed to sign policy", Some("/policies"), false);
            }
            let _ = sign_result.try_set(Some(result.map_err(|e| e.user_message())));
            let _ = signing.try_set(false);
        });
    };

    let on_verify = move |_| {
        let cpid = cpid_verify.clone();
        verifying.set(true);
        verify_result.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let result = client.verify_policy_signature(&cpid).await;
            if let Err(ref e) = result {
                report_error_with_toast(e, "Failed to verify policy", Some("/policies"), false);
            }
            let _ = verify_result.try_set(Some(result.map_err(|e| e.user_message())));
            let _ = verifying.try_set(false);
        });
    };

    let on_export = move |_| {
        let cpid = cpid_export.clone();
        exporting.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.export_policy(&cpid).await {
                Ok(resp) => {
                    trigger_json_download(&resp.cpid, &resp.policy_json);
                }
                Err(e) => {
                    report_error_with_toast(&e, "Failed to export policy", Some("/policies"), true);
                }
            }
            let _ = exporting.try_set(false);
        });
    };

    view! {
        <Card title="Governance".to_string()>
            <div class="flex flex-wrap gap-2">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(on_sign)
                    disabled=Signal::derive(move || signing.try_get().unwrap_or(false))
                >
                    <Show when=move || signing.try_get().unwrap_or(false) fallback=move || view! { "Sign" }>
                        <Spinner/>
                    </Show>
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(on_verify)
                    disabled=Signal::derive(move || verifying.try_get().unwrap_or(false))
                >
                    <Show when=move || verifying.try_get().unwrap_or(false) fallback=move || view! { "Verify" }>
                        <Spinner/>
                    </Show>
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| show_compare.set(true))
                >
                    "Compare"
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(on_export)
                    disabled=Signal::derive(move || exporting.try_get().unwrap_or(false))
                >
                    <Show when=move || exporting.try_get().unwrap_or(false) fallback=move || view! { "Export" }>
                        <Spinner/>
                    </Show>
                </Button>
            </div>

            // Sign result
            {move || sign_result.try_get().flatten().map(|result| {
                match result {
                    Ok(resp) => view! {
                        <div class="mt-3 space-y-1">
                            <Badge variant=BadgeVariant::Success>"Signed"</Badge>
                            <div class="grid gap-1 text-xs mt-2">
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"CPID"</span>
                                    <span class="font-mono">{resp.cpid}</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Signature"</span>
                                    <span class="font-mono truncate max-w-48">{resp.signature}</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Signed at"</span>
                                    <span>{format_datetime(&resp.signed_at)}</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Signed by"</span>
                                    <span>{resp.signed_by}</span>
                                </div>
                            </div>
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="mt-3 text-xs text-destructive">{format!("Sign error: {}", e)}</div>
                    }.into_any(),
                }
            })}

            // Verify result
            {move || verify_result.try_get().flatten().map(|result| {
                match result {
                    Ok(resp) if resp.is_valid => view! {
                        <div class="mt-3 flex items-center gap-2">
                            <Badge variant=BadgeVariant::Success>"Valid Signature"</Badge>
                        </div>
                    }.into_any(),
                    Ok(resp) => view! {
                        <div class="mt-3">
                            <Badge variant=BadgeVariant::Destructive>"Invalid Signature"</Badge>
                            {resp.error.map(|e| view! {
                                <p class="text-xs text-destructive mt-1">{e}</p>
                            })}
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="mt-3 text-xs text-destructive">{format!("Verify error: {}", e)}</div>
                    }.into_any(),
                }
            })}

            // Compare dialog
            {move || {
                if show_compare.try_get().unwrap_or(false) {
                    let cpid = cpid_compare.clone();
                    view! {
                        <PolicyCompareDialog
                            cpid=cpid
                            on_close=Callback::new(move |_| show_compare.set(false))
                        />
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </Card>
    }
}

/// Triggers a browser download of JSON text as a file
fn trigger_json_download(cpid: &str, json_text: &str) {
    use wasm_bindgen::JsCast;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let document = match window.document() {
        Some(d) => d,
        None => return,
    };

    // Create a Blob from the JSON text
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&wasm_bindgen::JsValue::from_str(json_text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = match web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &options) {
        Ok(b) => b,
        Err(_) => return,
    };

    let url = match web_sys::Url::create_object_url_with_blob(&blob) {
        Ok(u) => u,
        Err(_) => return,
    };

    // Create a temporary anchor and click it
    if let Ok(el) = document.create_element("a") {
        if let Some(anchor) = el.dyn_ref::<web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download(&format!("{}.json", cpid));
            anchor.click();
        }
    }

    let _ = web_sys::Url::revoke_object_url(&url);
}

/// Dialog for comparing two policy packs
#[component]
fn PolicyCompareDialog(cpid: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let compare_cpid = RwSignal::new(String::new());
    let comparing = RwSignal::new(false);
    let result = RwSignal::new(None::<Result<PolicyComparisonResponse, String>>);
    let dialog_open = RwSignal::new(true);

    let cpid_for_compare = cpid.clone();

    // Sync dialog_open -> on_close (handles Escape key and backdrop click)
    Effect::new(move || {
        if !dialog_open.try_get().unwrap_or(true) {
            on_close.run(());
        }
    });

    let close = move || {
        dialog_open.set(false);
    };

    let on_compare = move |_| {
        let cpid_1 = cpid_for_compare.clone();
        let cpid_2 = compare_cpid.try_get().unwrap_or_default();
        if cpid_2.trim().is_empty() {
            return;
        }
        comparing.set(true);
        result.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let request = PolicyComparisonRequest { cpid_1, cpid_2 };
            let res = client.compare_policies(&request).await;
            let _ = result.try_set(Some(res.map_err(|e| e.user_message())));
            let _ = comparing.try_set(false);
        });
    };

    let compare_disabled = Signal::derive(move || {
        comparing.try_get().unwrap_or(false)
            || compare_cpid.try_get().unwrap_or_default().trim().is_empty()
    });

    view! {
        <Dialog
            open=dialog_open
            title="Compare Policies".to_string()
            description=format!("Compare \"{}\" with another policy pack.", cpid)
        >
            <div class="space-y-4">
                <Input
                    value=compare_cpid
                    label="Compare with CPID".to_string()
                    placeholder="e.g., policy-pack-002".to_string()
                    required=true
                />
                <div class="flex gap-2">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(on_compare)
                        disabled=compare_disabled
                    >
                        <Show when=move || comparing.try_get().unwrap_or(false) fallback=move || view! { "Compare" }>
                            <Spinner/>
                        </Show>
                    </Button>
                    <Button
                        variant=ButtonVariant::Ghost
                        on_click=Callback::new(move |_| close())
                    >
                        "Cancel"
                    </Button>
                </div>

                // Results
                {move || result.try_get().flatten().map(|res| {
                    match res {
                        Ok(resp) if resp.identical => view! {
                            <div class="mt-2">
                                <Badge variant=BadgeVariant::Success>"Identical"</Badge>
                                <p class="text-xs text-muted-foreground mt-1">"The two policy packs are identical."</p>
                            </div>
                        }.into_any(),
                        Ok(resp) => view! {
                            <div class="mt-2 space-y-2">
                                <Badge variant=BadgeVariant::Warning>"Differences Found"</Badge>
                                <ul class="text-xs list-disc pl-4 space-y-1">
                                    {resp.differences.into_iter().map(|d| view! { <li class="text-muted-foreground">{d}</li> }).collect::<Vec<_>>()}
                                </ul>
                            </div>
                        }.into_any(),
                        Err(e) => view! {
                            <div class="mt-2 text-xs text-destructive">{format!("Error: {}", e)}</div>
                        }.into_any(),
                    }
                })}
            </div>
        </Dialog>
    }
}

// =============================================================================
// Assignments Panel
// =============================================================================

/// Panel showing policy assignments
#[component]
fn PolicyAssignmentsPanel(
    assignments: ReadSignal<LoadingState<Vec<PolicyAssignmentResponse>>>,
    refetch: Refetch,
) -> impl IntoView {
    let columns: Vec<Column<PolicyAssignmentResponse>> = vec![
        Column::custom("ID", |a: &PolicyAssignmentResponse| {
            let id = a.id.clone();
            view! {
                <span class="font-mono text-xs">{id}</span>
            }
        }),
        Column::custom("Policy Pack", |a: &PolicyAssignmentResponse| {
            let ppid = a.policy_pack_id.clone();
            view! {
                <span class="font-mono text-xs">{ppid}</span>
            }
        }),
        Column::custom("Target Type", |a: &PolicyAssignmentResponse| {
            let tt = a.target_type.clone();
            view! {
                <span class="text-sm">{tt}</span>
            }
        }),
        Column::custom("Target ID", |a: &PolicyAssignmentResponse| {
            let tid = a.target_id.clone().unwrap_or_else(|| "-".to_string());
            view! {
                <span class="font-mono text-xs text-muted-foreground">{tid}</span>
            }
        }),
        Column::custom("Priority", |a: &PolicyAssignmentResponse| {
            let p = a
                .priority
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            view! {
                <span class="text-sm">{p}</span>
            }
        }),
        Column::custom("Enforced", |a: &PolicyAssignmentResponse| {
            let enforced = a.enforced;
            view! {
                {if enforced {
                    view! { <Badge variant=BadgeVariant::Success>"Yes"</Badge> }.into_any()
                } else {
                    view! { <Badge variant=BadgeVariant::Secondary>"No"</Badge> }.into_any()
                }}
            }
        }),
    ];

    view! {
        <DataTable
            data=assignments
            columns=columns
            on_retry=Callback::new(move |_| refetch.run(()))
            empty_title="No policy assignments"
            empty_description="Policy assignments bind policy packs to specific targets. No assignments have been configured."
        />
    }
}

// =============================================================================
// Violations Panel
// =============================================================================

/// Panel showing policy violations
#[component]
fn PolicyViolationsPanel(
    violations: ReadSignal<LoadingState<Vec<PolicyViolationResponse>>>,
    refetch: Refetch,
) -> impl IntoView {
    let columns: Vec<Column<PolicyViolationResponse>> = vec![
        Column::custom("ID", |v: &PolicyViolationResponse| {
            let id = v.id.clone();
            view! {
                <span class="font-mono text-xs">{id}</span>
            }
        }),
        Column::custom("Tenant", |v: &PolicyViolationResponse| {
            let tid = v.tenant_id.clone();
            view! {
                <span class="font-mono text-xs">{tid}</span>
            }
        }),
        Column::custom("Resource", |v: &PolicyViolationResponse| {
            let rt = v.resource_type.clone();
            view! {
                <span class="text-sm">{humanize(&rt)}</span>
            }
        }),
        Column::custom("Severity", |v: &PolicyViolationResponse| {
            let severity = v.severity.clone();
            let variant = match severity.as_str() {
                "critical" | "high" => BadgeVariant::Destructive,
                "medium" => BadgeVariant::Warning,
                _ => BadgeVariant::Secondary,
            };
            view! {
                <Badge variant=variant>{severity}</Badge>
            }
        }),
        Column::custom("Message", |v: &PolicyViolationResponse| {
            let msg = v.message.clone();
            let msg_title = msg.clone();
            view! {
                <span class="text-xs text-muted-foreground truncate max-w-48" title=msg_title>{msg}</span>
            }
        }),
        Column::custom("Resolved", |v: &PolicyViolationResponse| {
            let resolved = v.resolved;
            view! {
                {if resolved {
                    view! { <Badge variant=BadgeVariant::Success>"Yes"</Badge> }.into_any()
                } else {
                    view! { <Badge variant=BadgeVariant::Destructive>"No"</Badge> }.into_any()
                }}
            }
        }),
        Column::custom("Created", |v: &PolicyViolationResponse| {
            let created = format_datetime(&v.created_at);
            view! {
                <span class="text-xs text-muted-foreground">{created}</span>
            }
        }),
    ];

    view! {
        <DataTable
            data=violations
            columns=columns
            on_retry=Callback::new(move |_| refetch.run(()))
            empty_title="No policy violations"
            empty_description="Policy violations are recorded when inference requests breach enforcement rules. No violations found."
        />
    }
}
