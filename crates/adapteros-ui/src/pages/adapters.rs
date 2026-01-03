//! Adapters page

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Card, Spinner, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::AdapterResponse;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Adapters list page
#[component]
pub fn Adapters() -> impl IntoView {
    let (adapters, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_adapters().await });

    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold tracking-tight">"Adapters"</h1>
                <button
                    class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                    on:click=move |_| refetch()
                >
                    "Refresh"
                </button>
            </div>

            {move || {
                match adapters.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <AdaptersList adapters=data/> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

#[component]
fn AdaptersList(adapters: Vec<AdapterResponse>) -> impl IntoView {
    if adapters.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <p class="text-muted-foreground">"No adapters found"</p>
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
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {adapters
                        .into_iter()
                        .map(|adapter| {
                            let id = adapter.id.clone();
                            let id_link = id.clone();
                            let id_view = id.clone();
                            let name = adapter.name.clone();
                            view! {
                                <TableRow>
                                    <TableCell>
                                        <a
                                            href=format!("/adapters/{}", id_link)
                                            class="font-medium hover:underline"
                                        >
                                            {name}
                                        </a>
                                    </TableCell>
                                    <TableCell>
                                        <Badge variant=BadgeVariant::Success>
                                            "Available"
                                        </Badge>
                                    </TableCell>
                                    <TableCell>
                                        <a
                                            href=format!("/adapters/{}", id_view)
                                            class="text-sm text-primary hover:underline"
                                        >
                                            "View"
                                        </a>
                                    </TableCell>
                                </TableRow>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Adapter detail page
#[component]
pub fn AdapterDetail() -> impl IntoView {
    let params = use_params_map();

    // Get adapter ID from URL
    let adapter_id = Memo::new(move |_| params.get().get("id").unwrap_or_default());

    // Fetch adapter details
    let (adapter, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = adapter_id.get();
        async move { client.get_adapter(&id).await }
    });

    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <a href="/adapters" class="text-muted-foreground hover:text-foreground">
                        "← Adapters"
                    </a>
                    <h1 class="text-3xl font-bold tracking-tight">"Adapter Details"</h1>
                </div>
                <button
                    class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                    on:click=move |_| refetch()
                >
                    "Refresh"
                </button>
            </div>

            {move || {
                match adapter.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <AdapterDetailContent adapter=data/> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

#[component]
fn AdapterDetailContent(adapter: AdapterResponse) -> impl IntoView {
    let lifecycle_variant = match adapter.lifecycle_state.as_str() {
        "active" => BadgeVariant::Success,
        "deprecated" => BadgeVariant::Warning,
        "retired" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    };

    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{adapter.name.clone()}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Adapter ID"</p>
                        <p class="font-mono text-sm">{adapter.adapter_id.clone()}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Hash (BLAKE3)"</p>
                        <p class="font-mono text-sm truncate">{adapter.hash_b3.clone()}</p>
                    </div>
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="flex items-center gap-2 mb-3">
                    <Badge variant=lifecycle_variant>
                        {adapter.lifecycle_state.clone()}
                    </Badge>
                    {adapter.runtime_state.clone().map(|state| view! {
                        <Badge variant=BadgeVariant::Secondary>
                            {state}
                        </Badge>
                    })}
                </div>
                <div class="space-y-2 text-sm">
                    <div>
                        <span class="text-muted-foreground">"Tier: "</span>
                        <span class="font-medium">{adapter.tier.clone()}</span>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Category: "</span>
                        <span class="font-medium">{adapter.category.clone().unwrap_or_else(|| "N/A".to_string())}</span>
                    </div>
                </div>
            </Card>
        </div>

        // Languages
        <Card title="Languages".to_string() class="mt-6".to_string()>
            <div class="flex flex-wrap gap-2">
                {if adapter.languages.is_empty() {
                    view! { <span class="text-muted-foreground">"No languages specified"</span> }.into_any()
                } else {
                    view! {
                        {adapter.languages.clone().into_iter().map(|lang| view! {
                            <Badge variant=BadgeVariant::Secondary>{lang}</Badge>
                        }).collect::<Vec<_>>()}
                    }.into_any()
                }}
            </div>
        </Card>

        // Metadata
        <Card title="Metadata".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Rank"</p>
                    <p class="font-medium">{adapter.rank}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Version"</p>
                    <p class="font-medium">{adapter.version.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Created At"</p>
                    <p class="font-medium">{adapter.created_at.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Updated At"</p>
                    <p class="font-medium">{adapter.updated_at.clone().unwrap_or_else(|| "N/A".to_string())}</p>
                </div>
            </div>
        </Card>

        // Framework (if available)
        {adapter.framework.clone().map(|fw| view! {
            <Card title="Framework".to_string() class="mt-6".to_string()>
                <div class="grid gap-4 md:grid-cols-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Framework"</p>
                        <p class="font-medium">{fw}</p>
                    </div>
                    {adapter.framework_id.clone().map(|fid| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Framework ID"</p>
                            <p class="font-mono text-sm">{fid}</p>
                        </div>
                    })}
                    {adapter.framework_version.clone().map(|fv| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Framework Version"</p>
                            <p class="font-medium">{fv}</p>
                        </div>
                    })}
                </div>
            </Card>
        })}

        // Stats (if available)
        {adapter.stats.clone().map(|stats| view! {
            <Card title="Statistics".to_string() class="mt-6".to_string()>
                <div class="grid gap-4 md:grid-cols-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Total Activations"</p>
                        <p class="text-2xl font-bold">{stats.total_activations}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selected Count"</p>
                        <p class="text-2xl font-bold">{stats.selected_count}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selection Rate"</p>
                        <p class="text-2xl font-bold">{format!("{:.1}%", stats.selection_rate * 100.0)}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Avg Gate Value"</p>
                        <p class="text-2xl font-bold">{format!("{:.3}", stats.avg_gate_value)}</p>
                    </div>
                </div>
            </Card>
        })}
    }
}
