//! Policies page
//!
//! Policy pack management with list view and detail panel.

use crate::api::client::{ApiClient, PolicyPackResponse};
use crate::components::{
    Button, ButtonVariant, Card, Shell, Spinner, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
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
    let (policies, refetch_policies) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_policies().await
    });

    // Store refetch in a signal for sharing
    let refetch_signal = StoredValue::new(refetch_policies);

    let on_policy_select = move |cpid: String| {
        selected_cpid.set(Some(cpid));
    };

    let on_close_detail = move || {
        selected_cpid.set(None);
    };

    // Dynamic class for left panel width
    let left_panel_class = move || {
        if selected_cpid.get().is_some() {
            "w-1/2 space-y-6 pr-4"
        } else {
            "flex-1 space-y-6 pr-4"
        }
    };

    view! {
        <Shell>
            <div class="flex h-full">
                // Left panel: Policy list
                <div class=left_panel_class>
                    <div class="flex items-center justify-between">
                        <div>
                            <h1 class="text-3xl font-bold tracking-tight">"Policy Packs"</h1>
                            <p class="text-muted-foreground mt-1">"Manage inference policies and enforcement rules"</p>
                        </div>
                        <div class="flex items-center gap-2">
                            <Button
                                variant=ButtonVariant::Outline
                                on_click=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                            >
                                "Refresh"
                            </Button>
                        </div>
                    </div>

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
                                    <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                        <p class="text-destructive">{e.to_string()}</p>
                                        <button
                                            class="mt-2 text-sm text-destructive underline"
                                            on:click=move |_| refetch_signal.with_value(|f| f())
                                        >
                                            "Retry"
                                        </button>
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                </div>

                // Right panel: Policy detail (when selected)
                {move || {
                    selected_cpid.get().map(|cpid| {
                        view! {
                            <div class="w-1/2 border-l pl-4">
                                <PolicyDetail
                                    cpid=cpid
                                    on_close=on_close_detail
                                />
                            </div>
                        }
                    })
                }}
            </div>
        </Shell>
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
                <div class="py-8 text-center">
                    <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="h-8 w-8 text-muted-foreground"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
                        </svg>
                    </div>
                    <p class="text-muted-foreground">"No policy packs found."</p>
                    <p class="text-sm text-muted-foreground mt-1">"Policy packs define enforcement rules for inference."</p>
                </div>
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
fn PolicyDetail(
    cpid: String,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let cpid_for_fetch = cpid.clone();

    // Fetch policy details
    let (policy, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = cpid_for_fetch.clone();
        async move { client.get_policy(&id).await }
    });

    let refetch_signal = StoredValue::new(refetch);

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="text-xl font-semibold">"Policy Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
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
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                                <button
                                    class="mt-2 text-sm text-destructive underline"
                                    on:click=move |_| refetch_signal.with_value(|f| f())
                                >
                                    "Retry"
                                </button>
                            </div>
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
    view! {
        // Metadata
        <Card title="Metadata".to_string()>
            <div class="grid gap-3 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"CPID"</span>
                    <span class="font-mono text-xs">{policy.cpid.clone()}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Hash (BLAKE3)"</span>
                    <span class="font-mono text-xs truncate max-w-[200px]" title=policy.hash_b3.clone()>
                        {policy.hash_b3.clone()}
                    </span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Created"</span>
                    <span>{format_date(&policy.created_at)}</span>
                </div>
            </div>
        </Card>

        // Policy content
        <Card title="Policy Content".to_string() class="mt-4".to_string()>
            <div class="bg-zinc-950 rounded-md p-4 overflow-auto max-h-96">
                <pre class="text-xs text-green-400 font-mono whitespace-pre-wrap">
                    {policy.content.clone()}
                </pre>
            </div>
        </Card>

        // Actions (will be role-gated in future)
        <Card title="Actions".to_string() class="mt-4".to_string()>
            <div class="flex gap-2">
                <Button variant=ButtonVariant::Outline>
                    "Validate"
                </Button>
                <Button variant=ButtonVariant::Primary>
                    "Apply to Stack"
                </Button>
            </div>
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
