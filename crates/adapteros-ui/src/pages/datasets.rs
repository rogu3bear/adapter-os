//! Datasets management page
//!
//! Provides UI for managing training datasets - listing, viewing,
//! and deleting datasets used for adapter training.

use crate::api::{ApiClient, DatasetListResponse, DatasetVersionsResponse};
use crate::components::{
    Badge, BadgeVariant, BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, EmptyState, ErrorDisplay, LoadingDisplay, PageHeader,
    RefreshButton, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api, use_api_resource, LoadingState};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::sync::Arc;

/// Datasets list page
#[component]
pub fn Datasets() -> impl IntoView {
    let (datasets, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_datasets(None).await });

    let refetch_trigger = RwSignal::new(0u32);
    let show_upload_dialog = RwSignal::new(false);
    let navigate = use_navigate();

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch.run(());
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    let on_upload = Callback::new(move |_| show_upload_dialog.set(true));
    let on_dataset_uploaded = Callback::new(move |outcome: DatasetUploadOutcome| {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
        navigate(
            &format!("/datasets/{}", outcome.dataset_id),
            Default::default(),
        );
    });

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Datasets"
                subtitle="Manage training datasets for adapter fine-tuning"
            >
                <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_upload_dialog.set(true))
                >
                    "Upload Dataset"
                </Button>
            </PageHeader>

            {move || {
                match datasets.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading datasets..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <DatasetsList
                                datasets=data
                                refetch_trigger=refetch_trigger
                                on_upload=on_upload
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
                            />
                        }.into_any()
                    }
                }
            }}

            <DatasetUploadWizard
                open=show_upload_dialog
                on_complete=on_dataset_uploaded
            />
        </div>
    }
}

/// List of datasets component
#[component]
fn DatasetsList(
    datasets: DatasetListResponse,
    refetch_trigger: RwSignal<u32>,
    on_upload: Callback<()>,
) -> impl IntoView {
    if datasets.datasets.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    title="No datasets"
                    description="Upload a dataset directly or generate one from documents."
                    action_label="Upload Dataset"
                    on_action=Callback::new(move |_| on_upload.run(()))
                    secondary_label="Upload Documents"
                    secondary_href="/documents"
                />
            </Card>
        }
        .into_any();
    }

    let client = use_api();
    let navigate = use_navigate();

    // Delete confirmation dialog state
    let show_delete_confirm = RwSignal::new(false);
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let pending_delete_name = RwSignal::new(String::new());
    let deleting = RwSignal::new(false);
    let delete_error = RwSignal::new(Option::<String>::None);

    // Reset dialog state
    let reset_delete_state = move || {
        pending_delete_id.set(None);
        pending_delete_name.set(String::new());
        delete_error.set(None);
    };

    // Handle cancel/close of delete dialog
    let on_cancel_delete = Callback::new(move |_| {
        show_delete_confirm.set(false);
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
                    match client.delete_dataset(&id).await {
                        Ok(_) => {
                            refetch_trigger.update(|n| *n = n.wrapping_add(1));
                            show_delete_confirm.set(false);
                            reset_delete_state();
                        }
                        Err(e) => {
                            delete_error.set(Some(format!("Failed to delete: {}", e)));
                        }
                    }
                    deleting.set(false);
                });
            }
        })
    };

    view! {
        <Card>
            <div class="mb-4 text-sm text-muted-foreground">
                {format!("{} dataset(s)", datasets.total)}
            </div>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Type"</TableHead>
                        <TableHead>"Format"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {datasets.datasets.into_iter().map(|dataset| {
                        let id_for_nav = dataset.id.clone();
                        let id_for_delete = dataset.id.clone();
                        let name = dataset.name.clone();
                        let name_for_title = name.clone();
                        let name_for_aria = name.clone();
                        let name_for_delete = dataset.name.clone();

                        let status_variant = match dataset.status.as_str() {
                            "ready" | "indexed" => BadgeVariant::Success,
                            "processing" => BadgeVariant::Warning,
                            "failed" | "error" => BadgeVariant::Destructive,
                            _ => BadgeVariant::Secondary,
                        };
                        let validation_status = dataset.validation_status.clone();
                        let validation_errors = dataset.validation_errors.clone();

                        let size_display = dataset
                            .total_size_bytes
                            .map(format_bytes)
                            .unwrap_or_else(|| "—".to_string());

                        let nav = navigate.clone();

                        view! {
                            <TableRow>
                                <TableCell>
                                    <button
                                        class="font-medium text-primary hover:underline text-left truncate"
                                        title=name_for_title
                                        aria-label=format!("View dataset {}", name_for_aria.as_str())
                                        on:click=move |_| {
                                            nav(&format!("/datasets/{}", id_for_nav), Default::default());
                                        }
                                    >
                                        {name}
                                    </button>
                                </TableCell>
                                <TableCell>
                                    {match dataset.dataset_type.as_deref() {
                                        Some("identity") => view! { <Badge variant=BadgeVariant::Secondary>"Identity"</Badge> }.into_any(),
                                        _ => view! { <Badge variant=BadgeVariant::Outline>"Standard"</Badge> }.into_any(),
                                    }}
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">{dataset.format.clone()}</span>
                                </TableCell>
                                <TableCell>
                                    <div class="space-y-1">
                                        <Badge variant=status_variant>{dataset.status.clone()}</Badge>
                                        {validation_status.clone().map(|status| {
                                            let variant = match status.as_str() {
                                                "valid" | "ready" => BadgeVariant::Success,
                                                "invalid" | "failed" => BadgeVariant::Destructive,
                                                "pending" | "processing" => BadgeVariant::Warning,
                                                _ => BadgeVariant::Secondary,
                                            };
                                            view! {
                                                <div class="text-xs text-muted-foreground">
                                                    "Validation: "
                                                    <Badge variant=variant>{status}</Badge>
                                                </div>
                                            }
                                        })}
                                        {validation_errors
                                            .as_ref()
                                            .map(|errs| errs.len())
                                            .filter(|count| *count > 0)
                                            .map(|count| {
                                                view! {
                                                    <div class="text-xs text-destructive">
                                                        {format!("{} validation error(s)", count)}
                                                    </div>
                                                }
                                            })}
                                    </div>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm">{size_display}</span>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">{format_date(&dataset.created_at)}</span>
                                </TableCell>
                                <TableCell class="text-right">
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        aria_label=format!("Delete dataset {}", name_for_delete.clone())
                                        on_click=Callback::new(move |_| {
                                            pending_delete_id.set(Some(id_for_delete.clone()));
                                            pending_delete_name.set(name_for_delete.clone());
                                            show_delete_confirm.set(true);
                                        })
                                    >
                                        <svg
                                            xmlns="http://www.w3.org/2000/svg"
                                            class="h-4 w-4 text-destructive"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                        >
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                        </svg>
                                    </Button>
                                </TableCell>
                            </TableRow>
                        }
                    }).collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>

        <ConfirmationDialog
            open=show_delete_confirm
            title="Delete Dataset"
            description=format!("Are you sure you want to delete this dataset? This action cannot be undone.")
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            cancel_text="Cancel"
            on_confirm=on_confirm_delete.clone()
            on_cancel=on_cancel_delete
            loading=Signal::derive(move || deleting.get())
        />
    }
    .into_any()
}

/// Dataset detail page
#[component]
pub fn DatasetDetail() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();

    let dataset_id = move || params.get().get("id").unwrap_or_default();

    let (dataset, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = dataset_id();
        async move { client.get_dataset(&id).await }
    });

    let (stats, stats_refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = dataset_id();
        async move { client.get_dataset_statistics(&id).await }
    });
    let (versions, versions_refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = dataset_id();
        async move { client.list_dataset_versions(&id).await }
    });

    let refetch_trigger = RwSignal::new(0u32);
    let refetch_stored = StoredValue::new(refetch);
    let stats_refetch_stored = StoredValue::new(stats_refetch);
    let versions_refetch_stored = StoredValue::new(versions_refetch);

    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch_stored.with_value(|f| f.run(()));
        stats_refetch_stored.with_value(|f| f.run(()));
        versions_refetch_stored.with_value(|f| f.run(()));
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    // Delete state
    let client = use_api();
    let deleting = RwSignal::new(false);
    let show_delete_confirm = RwSignal::new(false);
    let delete_error = RwSignal::new(Option::<String>::None);

    let on_cancel_delete = Callback::new(move |_| {
        show_delete_confirm.set(false);
        delete_error.set(None);
    });

    let on_confirm_delete = {
        let client = Arc::clone(&client);
        let nav = navigate.clone();
        Callback::new(move |_| {
            let id = dataset_id();
            deleting.set(true);
            delete_error.set(None);
            let client = Arc::clone(&client);
            let nav = nav.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_dataset(&id).await {
                    Ok(_) => {
                        nav("/datasets", Default::default());
                    }
                    Err(e) => {
                        delete_error.set(Some(format!("Failed to delete: {}", e)));
                        deleting.set(false);
                    }
                }
            });
        })
    };

    view! {
        <div class="space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Datasets", "/datasets"),
                BreadcrumbItem::current(dataset_id()),
            ]/>

            {move || {
                match dataset.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading dataset..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let validation_diagnostics = data.validation_diagnostics.clone();
                        let validation_errors_overview = data.validation_errors.clone();
                        let validation_errors_list = data.validation_errors.clone();
                        let validation_status = data.validation_status.clone();
                        let trust_state = data.trust_state.clone();
                        let current_version_id = data.dataset_version_id.clone();

                        let validation_status_view = validation_status.map(|status| {
                            let variant = validation_badge_variant(&status);
                            view! {
                                <div class="flex justify-between">
                                    <dt class="text-muted-foreground">"Validation"</dt>
                                    <dd>
                                        <Badge variant=variant>{status}</Badge>
                                    </dd>
                                </div>
                            }
                        });

                        let validation_error_count_view = validation_errors_overview.and_then(|errors| {
                            let count = errors.len();
                            if count == 0 {
                                None
                            } else {
                                Some(view! {
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Validation Errors"</dt>
                                        <dd class="text-destructive">{count.to_string()}</dd>
                                    </div>
                                })
                            }
                        });

                        let trust_state_view = trust_state.map(|state| {
                            let variant = trust_state_badge_variant(&state);
                            view! {
                                <div class="flex justify-between">
                                    <dt class="text-muted-foreground">"Trust State"</dt>
                                    <dd>
                                        <Badge variant=variant>{state}</Badge>
                                    </dd>
                                </div>
                            }
                        });

                        let current_version_view = current_version_id.map(|version| {
                            view! {
                                <div class="flex justify-between">
                                    <dt class="text-muted-foreground">"Current Version"</dt>
                                    <dd class="font-mono text-xs truncate max-w-48">{version}</dd>
                                </div>
                            }
                        });

                        let hash_view = data.hash_b3.clone().map(|hash| {
                            view! {
                                <div class="flex justify-between">
                                    <dt class="text-muted-foreground">"Hash (B3)"</dt>
                                    <dd class="font-mono text-xs truncate max-w-48">{hash}</dd>
                                </div>
                            }
                        });

                        let validation_errors_view = validation_errors_list.and_then(|errors| {
                            if errors.is_empty() {
                                None
                            } else {
                                Some(view! {
                                    <Card>
                                        <h3 class="text-lg font-semibold mb-4">"Validation Errors"</h3>
                                        <ul class="space-y-2 text-sm text-destructive">
                                            {errors.into_iter().map(|err| {
                                                view! { <li>{err}</li> }
                                            }).collect_view()}
                                        </ul>
                                    </Card>
                                })
                            }
                        });

                        let validation_diagnostics_view = validation_diagnostics.map(|diagnostics| {
                            view! {
                                <Card>
                                    <h3 class="text-lg font-semibold mb-4">"Validation Diagnostics"</h3>
                                    <div class="space-y-3 text-sm">
                                        {diagnostics.into_iter().map(|diag| view! {
                                            <div class="rounded border border-muted p-3">
                                                <div class="flex items-center justify-between">
                                                    <span class="text-muted-foreground">"Line"</span>
                                                    <span class="font-mono">{diag.line_number.to_string()}</span>
                                                </div>
                                                {diag.raw_snippet.map(|snippet| view! {
                                                    <div class="mt-2 font-mono text-xs text-muted-foreground truncate">{snippet}</div>
                                                })}
                                                {diag.missing_fields.map(|fields| view! {
                                                    <div class="mt-2">
                                                        <span class="text-muted-foreground">"Missing: "</span>
                                                        <span>{fields.join(", ")}</span>
                                                    </div>
                                                })}
                                                {diag.invalid_field_types.map(|fields| view! {
                                                    <div class="mt-2">
                                                        <span class="text-muted-foreground">"Invalid types: "</span>
                                                        <span>
                                                            {fields
                                                                .iter()
                                                                .map(|field| format!("{} ({} -> {})", field.field, field.actual, field.expected))
                                                                .collect::<Vec<_>>()
                                                                .join(", ")}
                                                        </span>
                                                    </div>
                                                })}
                                                {diag.contract_version_expected.map(|version| view! {
                                                    <div class="mt-2 text-muted-foreground">
                                                        "Contract version expected: " {version}
                                                    </div>
                                                })}
                                            </div>
                                        }).collect_view()}
                                    </div>
                                </Card>
                            }
                        });

                        view! {
                            <PageHeader
                                title=data.name.clone()
                                subtitle=data.description.clone().unwrap_or_else(|| "Training dataset".to_string())
                            >
                                <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                                <Button
                                    variant=ButtonVariant::Destructive
                                    on_click=Callback::new(move |_| show_delete_confirm.set(true))
                                >
                                    "Delete"
                                </Button>
                            </PageHeader>

                            <div class="grid gap-6 md:grid-cols-2">
                                // Overview card
                                <Card>
                                    <h3 class="text-lg font-semibold mb-4">"Overview"</h3>
                                    <dl class="space-y-3">
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"ID"</dt>
                                            <dd class="font-mono text-xs">{data.id.clone()}</dd>
                                        </div>
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"Type"</dt>
                                            <dd>
                                                {match data.dataset_type.as_deref() {
                                                    Some("identity") => view! { <Badge variant=BadgeVariant::Secondary>"Identity Set"</Badge> }.into_any(),
                                                    _ => view! { <Badge variant=BadgeVariant::Outline>"Standard"</Badge> }.into_any(),
                                                }}
                                            </dd>
                                        </div>
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"Format"</dt>
                                            <dd>{data.format.clone()}</dd>
                                        </div>
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"Status"</dt>
                                            <dd>
                                                <Badge variant={
                                                    match data.status.as_str() {
                                                        "ready" | "indexed" => BadgeVariant::Success,
                                                        "processing" => BadgeVariant::Warning,
                                                        "failed" | "error" => BadgeVariant::Destructive,
                                                        _ => BadgeVariant::Secondary,
                                                    }
                                                }>{data.status.clone()}</Badge>
                                            </dd>
                                        </div>
                                        {validation_status_view}
                                        {validation_error_count_view}
                                        {trust_state_view}
                                        {current_version_view}
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"File Count"</dt>
                                            <dd>{data.file_count.unwrap_or(0)}</dd>
                                        </div>
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"Total Size"</dt>
                                            <dd>{data.total_size_bytes.map(format_bytes).unwrap_or_else(|| "—".to_string())}</dd>
                                        </div>
                                        <div class="flex justify-between">
                                            <dt class="text-muted-foreground">"Created"</dt>
                                            <dd>{format_date(&data.created_at)}</dd>
                                        </div>
                                        {hash_view}
                                    </dl>
                                </Card>

                                // Statistics card
                                <Card>
                                    <h3 class="text-lg font-semibold mb-4">"Statistics"</h3>
                                    {move || match stats.get() {
                                        LoadingState::Idle | LoadingState::Loading => {
                                            view! { <div class="flex justify-center py-4"><Spinner/></div> }.into_any()
                                        }
                                        LoadingState::Loaded(stats_data) => {
                                            view! {
                                                <dl class="space-y-3">
                                                    <div class="flex justify-between">
                                                        <dt class="text-muted-foreground">"Examples"</dt>
                                                        <dd>{stats_data.num_examples.to_string()}</dd>
                                                    </div>
                                                    <div class="flex justify-between">
                                                        <dt class="text-muted-foreground">"Total Tokens"</dt>
                                                        <dd>{stats_data.total_tokens.to_string()}</dd>
                                                    </div>
                                                    <div class="flex justify-between">
                                                        <dt class="text-muted-foreground">"Avg Input Length"</dt>
                                                        <dd>{format!("{:.1}", stats_data.avg_input_length)}</dd>
                                                    </div>
                                                    <div class="flex justify-between">
                                                        <dt class="text-muted-foreground">"Avg Target Length"</dt>
                                                        <dd>{format!("{:.1}", stats_data.avg_target_length)}</dd>
                                                    </div>
                                                </dl>
                                            }.into_any()
                                        }
                                        LoadingState::Error(_) => {
                                            view! {
                                                <p class="text-sm text-muted-foreground">"Statistics unavailable"</p>
                                            }.into_any()
                                        }
                                    }}
                                </Card>
                            </div>

                            {validation_errors_view}

                            <Card>
                                <h3 class="text-lg font-semibold mb-4">"Versions"</h3>
                                {move || match versions.get() {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! { <div class="flex justify-center py-4"><Spinner/></div> }.into_any()
                                    }
                                    LoadingState::Loaded(DatasetVersionsResponse { versions, .. }) => {
                                        if versions.is_empty() {
                                            view! {
                                                <p class="text-sm text-muted-foreground">"No dataset versions found."</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <Table>
                                                    <TableHeader>
                                                        <TableRow>
                                                            <TableHead>"Version"</TableHead>
                                                            <TableHead>"Label"</TableHead>
                                                            <TableHead>"Trust"</TableHead>
                                                            <TableHead>"Hash"</TableHead>
                                                            <TableHead>"Created"</TableHead>
                                                        </TableRow>
                                                    </TableHeader>
                                                    <TableBody>
                                                        {versions.into_iter().map(|version| {
                                                            let trust_state = version.trust_state.clone().unwrap_or_else(|| "unknown".to_string());
                                                            let trust_variant = trust_state_badge_variant(&trust_state);
                                                            let hash = version
                                                                .hash_b3
                                                                .clone()
                                                                .map(|h| h.chars().take(10).collect::<String>())
                                                                .unwrap_or_else(|| "—".to_string());
                                                            view! {
                                                                <TableRow>
                                                                    <TableCell>
                                                                        <div class="space-y-1">
                                                                            <div class="font-medium">
                                                                                {"v"}{version.version_number.to_string()}
                                                                            </div>
                                                                            <div class="text-xs text-muted-foreground font-mono truncate max-w-xs">
                                                                                {version.dataset_version_id.clone()}
                                                                            </div>
                                                                            {version.repo_slug.clone().map(|slug| view! {
                                                                                <div class="text-xs text-muted-foreground truncate">{slug}</div>
                                                                            })}
                                                                        </div>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <span class="text-sm text-muted-foreground">
                                                                            {version.version_label.clone().unwrap_or_else(|| "—".to_string())}
                                                                        </span>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <Badge variant=trust_variant>{trust_state}</Badge>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <span class="font-mono text-xs text-muted-foreground">{hash}</span>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <span class="text-sm text-muted-foreground">
                                                                            {format_date(&version.created_at)}
                                                                        </span>
                                                                    </TableCell>
                                                                </TableRow>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </TableBody>
                                                </Table>
                                            }.into_any()
                                        }
                                    }
                                    LoadingState::Error(_) => {
                                        view! {
                                            <p class="text-sm text-muted-foreground">"Versions unavailable"</p>
                                        }.into_any()
                                    }
                                }}
                            </Card>

                            {move || {
                                match versions.get() {
                                    LoadingState::Loaded(DatasetVersionsResponse { versions, .. }) => {
                                        let latest_id = versions.first().map(|latest| latest.dataset_version_id.clone());
                                        latest_id.map(|id| {
                                            view! {
                                                <Card>
                                                    <h3 class="text-lg font-semibold mb-4">"Usage"</h3>
                                                    <p class="text-sm text-muted-foreground mb-3">
                                                        "Use a dataset version ID in inference or training to pin the exact data snapshot."
                                                    </p>
                                                    <div class="rounded-md bg-muted p-3 font-mono text-sm break-all">
                                                        {format!("dataset_version_id: \"{}\"", id)}
                                                    </div>
                                                </Card>
                                            }.into_any()
                                        })
                                    }
                                    _ => None,
                                }
                            }}

                            {validation_diagnostics_view}

                            <ConfirmationDialog
                                open=show_delete_confirm
                                title="Delete Dataset"
                                description=format!("Are you sure you want to delete this dataset? This action cannot be undone.")
                                severity=ConfirmationSeverity::Destructive
                                confirm_text="Delete"
                                cancel_text="Cancel"
                                on_confirm=on_confirm_delete.clone()
                                on_cancel=on_cancel_delete
                                loading=Signal::derive(move || deleting.get())
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Format bytes to human-readable string
fn format_bytes(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format date string for display (simplified)
fn format_date(date_str: &str) -> String {
    // Just show the date part for now
    date_str.split('T').next().unwrap_or(date_str).to_string()
}

fn validation_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "valid" | "ready" => BadgeVariant::Success,
        "invalid" | "failed" => BadgeVariant::Destructive,
        "pending" | "processing" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    }
}

fn trust_state_badge_variant(state: &str) -> BadgeVariant {
    match state {
        "allowed" | "trusted" | "approved" => BadgeVariant::Success,
        "needs_approval" | "pending" => BadgeVariant::Warning,
        "blocked" | "rejected" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}
