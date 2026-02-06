//! Datasets management page
//!
//! Provides UI for managing training datasets - listing, viewing,
//! and deleting datasets used for adapter training.

use crate::api::{
    ApiClient, DatasetListResponse, DatasetSafetyCheckResult, DatasetStatisticsResponse,
    DatasetVersionsResponse, ModelWithStatsResponse,
};
use crate::components::{
    Badge, BadgeVariant, BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, Card, Checkbox,
    Combobox, ComboboxOption, ConfirmationDialog, ConfirmationSeverity, CopyableId, EmptyState,
    ErrorDisplay, Input, LoadingDisplay, PageHeader, RefreshButton, Spinner, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow, Toggle,
};
use crate::hooks::{use_api, use_api_resource, use_delete_dialog, LoadingState};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use crate::utils::{format_bytes, format_date};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};
use serde_json::json;
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
        <div class="p-6 space-y-6">
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

    // Delete confirmation dialog state using reusable hook
    let delete_state = use_delete_dialog();

    // Handle cancel/close of delete dialog
    let delete_state_for_cancel = delete_state.clone();
    let on_cancel_delete = Callback::new(move |_| {
        delete_state_for_cancel.cancel();
    });

    // Handle confirmed deletion
    let delete_state_for_confirm = delete_state.clone();
    let on_confirm_delete = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            if let Some(id) = delete_state_for_confirm.get_pending_id() {
                delete_state_for_confirm.start_delete();
                let client = Arc::clone(&client);
                let delete_state = delete_state_for_confirm.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match client.delete_dataset(&id).await {
                        Ok(_) => {
                            refetch_trigger.update(|n| *n = n.wrapping_add(1));
                            delete_state.finish_delete(Ok(()));
                        }
                        Err(e) => {
                            delete_state.finish_delete(Err(format!("Failed to delete: {}", e)));
                        }
                    }
                });
            }
        })
    };

    // Clone for use in the row click handler
    let delete_state_for_rows = delete_state.clone();
    let delete_state_for_loading = delete_state.clone();

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
                        let delete_state = delete_state_for_rows.clone();

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
                                        on_click=Callback::new({
                                            let delete_state = delete_state.clone();
                                            move |_| {
                                                delete_state.confirm(id_for_delete.clone(), name_for_delete.clone());
                                            }
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
            open=delete_state.show
            title="Delete Dataset"
            description=format!("Are you sure you want to delete this dataset? This action cannot be undone.")
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            cancel_text="Cancel"
            on_confirm=on_confirm_delete.clone()
            on_cancel=on_cancel_delete
            loading=Signal::derive(move || delete_state_for_loading.is_deleting())
        />
    }
    .into_any()
}

/// Dataset detail page
#[component]
pub fn DatasetDetail() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let navigate_store = StoredValue::new(navigate);

    let dataset_id = move || params.get().get("id").unwrap_or_default();
    let query = use_query_map();
    let is_draft = Signal::derive(move || {
        let id = dataset_id();
        id == "draft" || id.starts_with("draft-")
    });
    let draft_source = Signal::derive(move || {
        query
            .get()
            .get("source")
            .unwrap_or_else(|| "unknown".to_string())
    });
    let draft_items = Signal::derive(move || {
        query
            .get()
            .get("items")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    });
    let draft_name = Signal::derive(move || {
        query.get().get("name").map(|raw| {
            js_sys::decode_uri_component(&raw)
                .map(|s| s.as_string().unwrap_or_else(|| raw.clone()))
                .unwrap_or(raw)
        })
    });
    let draft_document_ids = Signal::derive(move || {
        let params = query.get();
        let mut ids = Vec::new();
        if let Some(id) = params.get("document_id") {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                ids.push(trimmed.to_string());
            }
        }
        if let Some(raw_ids) = params.get("document_ids") {
            for id in raw_ids.split(',') {
                let trimmed = id.trim();
                if !trimmed.is_empty() {
                    ids.push(trimmed.to_string());
                }
            }
        }
        ids
    });
    let draft_dataset_id = Signal::derive(move || query.get().get("dataset_id"));
    let draft_base_model_id = Signal::derive(move || {
        query.get().get("base_model_id").and_then(|raw| {
            js_sys::decode_uri_component(&raw)
                .ok()
                .and_then(|s| s.as_string())
        })
    });

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
        let nav_store = navigate_store;
        Callback::new(move |_| {
            let id = dataset_id();
            deleting.set(true);
            delete_error.set(None);
            let client = Arc::clone(&client);
            let nav_store = nav_store;
            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_dataset(&id).await {
                    Ok(_) => {
                        nav_store.with_value(|nav| nav("/datasets", Default::default()));
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
                if is_draft.get() {
                    view! {
                        <DatasetDraftView
                            source=draft_source.get()
                            name=draft_name.get()
                            items=draft_items.get()
                            document_ids=draft_document_ids.get()
                            dataset_id=draft_dataset_id.get()
                            base_model_id=draft_base_model_id.get()
                        />
                    }.into_any()
                } else {
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
                                        <dd class="font-mono text-xs truncate max-w-sm">{version}</dd>
                                    </div>
                                }
                            });

                            let hash_view = data.hash_b3.clone().map(|hash| {
                                view! {
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Hash (B3)"</dt>
                                        <dd class="font-mono text-xs truncate max-w-sm">{hash}</dd>
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

                            // Determine if dataset is trainable
                            let is_trainable = {
                                let validation_ok = data.validation_status.as_deref()
                                    .map(|s| s == "valid")
                                    .unwrap_or(false);
                                let trust_ok = data.trust_state.as_deref()
                                    .map(|s| matches!(s, "allowed" | "trusted" | "approved"))
                                    .unwrap_or(true); // Allow if trust_state not set
                                let status_ok = matches!(data.status.as_str(), "ready" | "indexed");
                                validation_ok && trust_ok && status_ok
                            };
                            let dataset_id_for_train = data.id.clone();

                            view! {
                                <PageHeader
                                    title=data.name.clone()
                                    subtitle=data.description.clone().unwrap_or_else(|| "Training dataset".to_string())
                                >
                                    <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                                    {is_trainable.then(|| {
                                        let nav_store = navigate_store;
                                        let id = dataset_id_for_train.clone();
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Primary
                                                on_click=Callback::new(move |_| {
                                                    nav_store.with_value(|nav| {
                                                        nav(&format!("/training?dataset_id={}", id), Default::default());
                                                    });
                                                })
                                            >
                                                "Train Adapter"
                                            </Button>
                                        }
                                    })}
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
                                            <CopyableId id=data.id.clone() label="ID".to_string() truncate=24 />
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
                }
            }}
        </div>
    }
}

/// Dataset draft view (minimal training integration)
#[component]
fn DatasetDraftView(
    source: String,
    name: Option<String>,
    items: usize,
    document_ids: Vec<String>,
    dataset_id: Option<String>,
    base_model_id: Option<String>,
) -> impl IntoView {
    let pii_scrub = RwSignal::new(true);
    let dedupe = RwSignal::new(true);
    let adapter_type = RwSignal::new("identify".to_string());
    let base_model = RwSignal::new(base_model_id.unwrap_or_default());
    let training_status = RwSignal::new(None::<String>);
    let training_error = RwSignal::new(None::<String>);
    let training_job_id = RwSignal::new(None::<String>);
    let training_job_status = RwSignal::new(None::<String>);
    let is_training = RwSignal::new(false);
    let safety_check_result = RwSignal::new(None::<DatasetSafetyCheckResult>);
    let safety_warning_acknowledged = RwSignal::new(false);
    let dataset_id_state = RwSignal::new(dataset_id);
    let document_ids_store = StoredValue::new(document_ids);
    let client = use_api();
    let poll_nonce = RwSignal::new(0u64);

    // Statistics state
    let stats_state = RwSignal::new(LoadingState::<DatasetStatisticsResponse>::Idle);

    // Models state for combobox
    let models_state = RwSignal::new(Vec::<ModelWithStatsResponse>::new());

    // Fetch available models on mount
    {
        let client = Arc::clone(&client);
        Effect::new(move |_| {
            let client = Arc::clone(&client);
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(async move {
                match client.list_models().await {
                    Ok(resp) => {
                        models_state.set(resp.models);
                    }
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("Failed to load models for combobox: {}", e).into(),
                        );
                    }
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = client;
            }
        });
    }

    // Fetch statistics when dataset_id_state changes
    {
        let client = Arc::clone(&client);
        Effect::new(move |_| {
            let dataset_id = dataset_id_state.get();
            if let Some(id) = dataset_id {
                stats_state.set(LoadingState::Loading);
                let client = Arc::clone(&client);
                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(async move {
                    match client.get_dataset_statistics(&id).await {
                        Ok(stats) => {
                            stats_state.set(LoadingState::Loaded(stats));
                        }
                        Err(e) => {
                            stats_state.set(LoadingState::Error(e));
                        }
                    }
                });
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = client;
                }
            } else {
                stats_state.set(LoadingState::Idle);
            }
        });
    }

    // Fetch safety check when dataset_id_state changes
    {
        let client = Arc::clone(&client);
        Effect::new(move |_| {
            let dataset_id = dataset_id_state.get();
            if let Some(id) = dataset_id {
                let client = Arc::clone(&client);
                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(async move {
                    match client.check_dataset_safety(&id).await {
                        Ok(result) => {
                            safety_check_result.set(Some(result));
                        }
                        Err(_) => {
                            // Safety check failed - allow training with unknown state
                            safety_check_result.set(None);
                        }
                    }
                });
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = client;
                }
            } else {
                safety_check_result.set(None);
            }
        });
    }

    // Training configuration signals
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let show_advanced = RwSignal::new(false);
    let validation_split = RwSignal::new("0.1".to_string());
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());

    let source_label = match source.as_str() {
        "file" => "File upload",
        "paste" => "Pasted text",
        "chat" => "Chat selection",
        _ => "Unknown source",
    };

    let name_label = name.unwrap_or_else(|| "Untitled draft".to_string());
    let item_label = if items == 0 {
        "Unknown".to_string()
    } else {
        items.to_string()
    };

    let train_disabled = Signal::derive(move || {
        is_training.get()
            || base_model.get().trim().is_empty()
            || (dataset_id_state.get().is_none()
                && document_ids_store.with_value(|ids| ids.is_empty()))
    });

    // Reason why train button is disabled (for user hint)
    let train_disabled_reason = Signal::derive(move || {
        if is_training.get() {
            Some("Training in progress...".to_string())
        } else if base_model.get().trim().is_empty() {
            Some("Select a base model to enable training".to_string())
        } else if dataset_id_state.get().is_none()
            && document_ids_store.with_value(|ids| ids.is_empty())
        {
            Some("Attach a document or select a dataset first".to_string())
        } else {
            None
        }
    });

    // Poll training job status when a job id is available
    {
        let client = Arc::clone(&client);
        Effect::new(move |_| {
            let job_id = training_job_id.get();
            poll_nonce.update(|v| *v = v.wrapping_add(1));
            let nonce = poll_nonce.get_untracked();

            if let Some(job_id) = job_id {
                training_job_status.set(Some("pending".to_string()));
                training_status.set(Some("Training queued".to_string()));
                let client = Arc::clone(&client);
                let training_status = training_status.clone();
                let training_job_status = training_job_status.clone();
                let training_error = training_error.clone();
                let poll_nonce = poll_nonce.clone();

                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(async move {
                    loop {
                        if poll_nonce.get_untracked() != nonce {
                            break;
                        }
                        match client.get_training_job(&job_id).await {
                            Ok(job) => {
                                let status = job.status.clone();
                                training_job_status.set(Some(status.clone()));
                                training_status.set(Some(format!("Training {}", status)));
                                if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
                                    break;
                                }
                            }
                            Err(e) => {
                                training_error
                                    .set(Some(format!("Failed to refresh training status: {}", e)));
                                break;
                            }
                        }
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                    }
                });
            }
        });
    }

    let on_train = {
        let client = Arc::clone(&client);
        let name_label = name_label.clone();
        Callback::new(move |_| {
            if is_training.get() {
                return;
            }

            training_error.set(None);
            let base_model_val = base_model.get();
            if base_model_val.trim().is_empty() {
                training_error.set(Some("Base model ID is required.".to_string()));
                return;
            }

            let adapter_type_val = adapter_type.get();
            let existing_dataset_id = dataset_id_state.get();
            let document_ids = document_ids_store.with_value(|ids| ids.clone());

            // Capture training config values
            let epochs_val: u32 = epochs.get().parse().unwrap_or(10);
            let learning_rate_val: f64 = learning_rate.get().parse().unwrap_or(0.0001);
            let batch_size_val: u32 = batch_size.get().parse().unwrap_or(4);
            let rank_val: u32 = rank.get().parse().unwrap_or(8);
            let alpha_val: u32 = alpha.get().parse().unwrap_or(16);

            // Capture preprocessing options
            let pii_scrub_val = pii_scrub.get();
            let dedupe_val = dedupe.get();

            is_training.set(true);
            training_status.set(Some("Preparing training...".to_string()));

            #[cfg(target_arch = "wasm32")]
            {
                let client = Arc::clone(&client);
                let dataset_id_state = dataset_id_state.clone();
                let training_status = training_status.clone();
                let training_error = training_error.clone();
                let training_job_id = training_job_id.clone();
                let is_training = is_training.clone();
                let safety_check_result = safety_check_result.clone();
                let safety_warning_acknowledged = safety_warning_acknowledged.clone();

                let name_label = name_label.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let dataset_id = if let Some(id) = existing_dataset_id {
                        id
                    } else if !document_ids.is_empty() {
                        training_status.set(Some("Creating dataset...".to_string()));
                        match client
                            .create_dataset_from_documents(document_ids, Some(name_label))
                            .await
                        {
                            Ok(ds) => {
                                dataset_id_state.set(Some(ds.id.clone()));
                                ds.id
                            }
                            Err(e) => {
                                training_error.set(Some(format!("Dataset error: {}", e)));
                                is_training.set(false);
                                return;
                            }
                        }
                    } else {
                        training_error
                            .set(Some("No documents attached to this draft.".to_string()));
                        is_training.set(false);
                        return;
                    };

                    // Run preprocessing if enabled (PII scrub or deduplication)
                    if pii_scrub_val || dedupe_val {
                        training_status.set(Some("Preprocessing dataset...".to_string()));
                        match client
                            .start_dataset_preprocessing(&dataset_id, pii_scrub_val, dedupe_val)
                            .await
                        {
                            Ok(_preprocess_response) => {
                                // Poll for preprocessing completion (max 5 minutes = 300 polls)
                                const MAX_PREPROCESS_POLLS: usize = 300;
                                for poll_count in 0..MAX_PREPROCESS_POLLS {
                                    gloo_timers::future::TimeoutFuture::new(1000).await;
                                    match client.get_dataset_preprocess_status(&dataset_id).await {
                                        Ok(status) => {
                                            let lines_info = if status.lines_removed > 0 {
                                                format!(
                                                    " ({} lines processed, {} removed)",
                                                    status.lines_processed, status.lines_removed
                                                )
                                            } else {
                                                format!(
                                                    " ({} lines processed)",
                                                    status.lines_processed
                                                )
                                            };
                                            training_status.set(Some(format!(
                                                "Preprocessing: {}{}",
                                                status.status, lines_info
                                            )));
                                            if status.status == "completed" {
                                                break;
                                            } else if status.status == "failed" {
                                                let error_msg =
                                                    status.error_message.unwrap_or_else(|| {
                                                        "Preprocessing failed".to_string()
                                                    });
                                                training_error.set(Some(error_msg));
                                                is_training.set(false);
                                                return;
                                            }
                                        }
                                        Err(e) => {
                                            // If we can't get status, continue polling
                                            leptos::logging::log!(
                                                "Preprocessing status check failed: {}",
                                                e
                                            );
                                        }
                                    }
                                    // Timeout after max polls
                                    if poll_count == MAX_PREPROCESS_POLLS - 1 {
                                        training_error.set(Some(
                                            "Preprocessing timed out after 5 minutes".to_string(),
                                        ));
                                        is_training.set(false);
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                // If preprocessing fails to start, log but proceed
                                // (preprocessing is optional enhancement)
                                leptos::logging::log!(
                                    "Preprocessing failed to start, proceeding: {}",
                                    e
                                );
                            }
                        }
                    }

                    // Safety gate check before training
                    training_status.set(Some("Checking dataset safety...".to_string()));
                    match client.check_dataset_safety(&dataset_id).await {
                        Ok(safety_result) => {
                            safety_check_result.set(Some(safety_result.clone()));
                            match safety_result.trust_state.as_str() {
                                "blocked" => {
                                    let reasons = if safety_result.blocking_reasons.is_empty() {
                                        "Dataset safety check failed".to_string()
                                    } else {
                                        safety_result.blocking_reasons.join("; ")
                                    };
                                    training_error
                                        .set(Some(format!("Training blocked: {}", reasons)));
                                    is_training.set(false);
                                    return;
                                }
                                "needs_approval" => {
                                    training_error.set(Some(
                                        "Dataset requires approval before training. Please contact an administrator.".to_string()
                                    ));
                                    is_training.set(false);
                                    return;
                                }
                                "allowed_with_warning" => {
                                    // Show warning but proceed if acknowledged
                                    if !safety_warning_acknowledged.get_untracked() {
                                        // Set warning acknowledged so next attempt proceeds
                                        safety_warning_acknowledged.set(true);
                                        let warnings = if safety_result.warnings.is_empty() {
                                            "Dataset has safety warnings".to_string()
                                        } else {
                                            safety_result.warnings.join("; ")
                                        };
                                        training_error.set(Some(format!(
                                            "Warning: {}. Click Train again to proceed.",
                                            warnings
                                        )));
                                        is_training.set(false);
                                        return;
                                    }
                                }
                                // "allowed" or "unknown" - proceed
                                _ => {}
                            }
                        }
                        Err(e) => {
                            // Log but don't block on safety check failure
                            leptos::logging::log!("Safety check failed, proceeding: {}", e);
                        }
                    }

                    training_status.set(Some("Starting training...".to_string()));
                    let request = json!({
                        "base_model_id": base_model_val,
                        "dataset_id": dataset_id,
                        "config": {
                            "rank": rank_val,
                            "alpha": alpha_val,
                            "targets": ["q_proj", "v_proj"],
                            "epochs": epochs_val,
                            "learning_rate": learning_rate_val,
                            "batch_size": batch_size_val
                        },
                        "adapter_type": adapter_type_val,
                        "category": "docs",
                        "synthetic_mode": false
                    });

                    match client
                        .post::<_, TrainingJobResponse>("/v1/training/jobs", &request)
                        .await
                    {
                        Ok(job) => {
                            training_status.set(Some("Training queued".to_string()));
                            training_job_id.set(Some(job.id));
                        }
                        Err(e) => {
                            training_error.set(Some(format!("Training error: {}", e)));
                        }
                    }
                    is_training.set(false);
                });
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = (
                    client,
                    adapter_type_val,
                    base_model_val,
                    name_label,
                    dataset_id_state,
                    training_status,
                    training_job_id,
                    training_error,
                );
                training_error.set(Some(
                    "Training is only available in the web UI.".to_string(),
                ));
                is_training.set(false);
            }
        })
    };

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Dataset Draft"
                subtitle="Review draft data before training an adapter."
            >
                <div>
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=train_disabled
                        loading=Signal::derive(move || is_training.get())
                        on_click=on_train
                    >
                        "Train Adapter"
                    </Button>
                    {move || train_disabled_reason.get().map(|reason| view! {
                        <p class="text-xs text-muted-foreground mt-1">{reason}</p>
                    })}
                </div>
            </PageHeader>

            {move || training_error.get().map(|msg| {
                // Determine heading based on error phase (Dataset vs Training)
                let is_dataset_error = msg.starts_with("Dataset");
                let heading = if is_dataset_error {
                    "Dataset creation failed"
                } else {
                    "Training blocked"
                };
                view! {
                    <Card>
                        <div class="flex items-center justify-between">
                            <div>
                                <h3 class="text-lg font-semibold text-destructive">{heading}</h3>
                                <p class="text-sm text-muted-foreground">{msg}</p>
                            </div>
                            <Badge variant=BadgeVariant::Destructive>"Error"</Badge>
                        </div>
                    </Card>
                }
            })}

            {move || training_status.get().map(|status| view! {
                <Card>
                    <div class="flex items-center justify-between gap-4">
                        <div>
                            <h3 class="text-lg font-semibold">{status.clone()}</h3>
                            <p class="text-sm text-muted-foreground">
                                "Track training progress in the Training Jobs view."
                            </p>
                        </div>
                        <Badge variant=BadgeVariant::Secondary>
                            {move || training_job_status.get().unwrap_or_else(|| "queued".to_string())}
                        </Badge>
                    </div>
                    {move || training_job_id.get().map(|job_id| {
                        let href = format!("/training?job_id={}", job_id);
                        view! {
                            <div class="mt-3 flex items-center gap-4">
                                <CopyableId id=job_id label="Training job".to_string() truncate=24 />
                                <a href=href class="text-primary hover:underline text-sm">"View job →"</a>
                            </div>
                        }
                    })}
                </Card>
            })}

            // Safety Gate Card - shows trust state and any warnings
            {move || safety_check_result.get().map(|result| {
                let trust_state = result.trust_state.clone();
                let badge_variant = trust_state_badge_variant(&trust_state);
                let has_warnings = !result.warnings.is_empty();
                let has_blocking_reasons = !result.blocking_reasons.is_empty();
                let is_blocked = trust_state == "blocked";
                let needs_approval = trust_state == "needs_approval";
                let has_warning_state = trust_state == "allowed_with_warning";

                view! {
                    <Card>
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-lg font-semibold">"Safety Gate"</h3>
                            <Badge variant=badge_variant>{trust_state.clone()}</Badge>
                        </div>

                        {is_blocked.then(|| view! {
                            <div class="p-3 rounded-md bg-destructive/10 border border-destructive/20 mb-3">
                                <p class="text-sm text-destructive font-medium">
                                    "Training is blocked for this dataset."
                                </p>
                            </div>
                        })}

                        {needs_approval.then(|| view! {
                            <div class="p-3 rounded-md bg-warning/10 border border-warning/20 mb-3">
                                <p class="text-sm text-warning-foreground font-medium">
                                    "This dataset requires approval before training can proceed."
                                </p>
                            </div>
                        })}

                        {has_warning_state.then(|| view! {
                            <div class="p-3 rounded-md bg-warning/10 border border-warning/20 mb-3">
                                <p class="text-sm text-warning-foreground font-medium">
                                    "Training allowed with warnings. Review before proceeding."
                                </p>
                            </div>
                        })}

                        {has_blocking_reasons.then(|| {
                            let reasons = result.blocking_reasons.clone();
                            view! {
                                <div class="mb-3">
                                    <h4 class="text-sm font-medium text-destructive mb-2">"Blocking Reasons"</h4>
                                    <ul class="space-y-1 text-sm text-muted-foreground">
                                        {reasons.into_iter().map(|reason| view! {
                                            <li class="flex items-start gap-2">
                                                <span class="text-destructive">"•"</span>
                                                <span>{reason}</span>
                                            </li>
                                        }).collect_view()}
                                    </ul>
                                </div>
                            }
                        })}

                        {has_warnings.then(|| {
                            let warnings = result.warnings.clone();
                            view! {
                                <div>
                                    <h4 class="text-sm font-medium text-warning-foreground mb-2">"Warnings"</h4>
                                    <ul class="space-y-1 text-sm text-muted-foreground">
                                        {warnings.into_iter().map(|warning| view! {
                                            <li class="flex items-start gap-2">
                                                <span class="text-warning">"•"</span>
                                                <span>{warning}</span>
                                            </li>
                                        }).collect_view()}
                                    </ul>
                                </div>
                            }
                        })}

                        {(!has_warnings && !has_blocking_reasons && !is_blocked && !needs_approval).then(|| view! {
                            <p class="text-sm text-muted-foreground">
                                "No safety concerns detected."
                            </p>
                        })}
                    </Card>
                }
            })}

            <div class="grid gap-6 md:grid-cols-2">
                <Card>
                    <h3 class="text-lg font-semibold mb-4">"Draft Summary"</h3>
                    <dl class="space-y-3 text-sm">
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Name"</dt>
                            <dd>{name_label}</dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Source"</dt>
                            <dd>
                                <Badge variant=BadgeVariant::Outline>{source_label}</Badge>
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Item count"</dt>
                            <dd>{item_label}</dd>
                        </div>
                        <div class="flex flex-col gap-1">
                            <dt class="text-muted-foreground">"Sources"</dt>
                            <dd class="space-y-1">
                                {move || {
                                    let mut sources = Vec::new();
                                    if let Some(ds_id) = dataset_id_state.get() {
                                        sources.push(format!("Dataset {}", ds_id));
                                    }
                                    let doc_ids = document_ids_store.with_value(|ids| ids.clone());
                                    sources.extend(doc_ids);
                                    if sources.is_empty() {
                                        sources.push("Unknown".to_string());
                                    }
                                    sources
                                        .into_iter()
                                        .map(|item| {
                                            view! { <div class="font-mono text-xs">{item}</div> }
                                        })
                                        .collect::<Vec<_>>()
                                }}
                            </dd>
                        </div>
                    </dl>
                </Card>

                <Card>
                    <h3 class="text-lg font-semibold mb-4">"Training"</h3>
                    <div class="space-y-4 text-sm">
                        <div class="space-y-2">
                            <label class="text-xs text-muted-foreground">"Base model"</label>
                            <Combobox
                                value=base_model
                                options=Signal::derive(move || {
                                    models_state.get()
                                        .into_iter()
                                        .map(|m| {
                                            let desc = match (&m.format, &m.backend) {
                                                (Some(f), Some(b)) => format!("{} / {}", f, b),
                                                (Some(f), None) => f.clone(),
                                                (None, Some(b)) => b.clone(),
                                                (None, None) => String::new(),
                                            };
                                            ComboboxOption {
                                                value: m.id.clone(),
                                                label: m.name.clone(),
                                                description: if desc.is_empty() { None } else { Some(desc) },
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                                placeholder="Select or type a model ID".to_string()
                                allow_free_text=true
                            />
                        </div>
                        <div class="flex items-center justify-between gap-3">
                            <div>
                                <p class="text-xs text-muted-foreground">"Adapter type"</p>
                                <p class="text-xs text-muted-foreground">
                                    "Identify focuses style; Behavior focuses Q/A."
                                </p>
                            </div>
                            <div class="flex items-center rounded-full border border-border bg-muted/30 p-0.5 text-xs">
                                <button
                                    class=move || if adapter_type.get() == "identify" {
                                        "rounded-full px-2 py-1 text-foreground bg-background shadow-sm"
                                    } else {
                                        "rounded-full px-2 py-1 text-muted-foreground"
                                    }
                                    on:click=move |_| adapter_type.set("identify".to_string())
                                >
                                    "Identify"
                                </button>
                                <button
                                    class=move || if adapter_type.get() == "behavior" {
                                        "rounded-full px-2 py-1 text-foreground bg-background shadow-sm"
                                    } else {
                                        "rounded-full px-2 py-1 text-muted-foreground"
                                    }
                                    on:click=move |_| adapter_type.set("behavior".to_string())
                                >
                                    "Behavior"
                                </button>
                            </div>
                        </div>
                        {move || {
                            if base_model.get().trim().is_empty() {
                                Some(view! {
                                    <p class="text-xs text-muted-foreground">
                                        "Add a base model ID to enable training."
                                    </p>
                                })
                            } else if dataset_id_state.get().is_none()
                                && document_ids_store.with_value(|ids| ids.is_empty())
                            {
                                Some(view! {
                                    <p class="text-xs text-muted-foreground">
                                        "Attach documents to enable training."
                                    </p>
                                })
                            } else {
                                None
                            }
                        }}
                    </div>
                </Card>
            </div>

            // Statistics card - only shown when dataset_id is available
            {move || dataset_id_state.get().map(|_| view! {
                <Card>
                    <h3 class="text-lg font-semibold mb-4">"Statistics"</h3>
                    {move || match stats_state.get() {
                        LoadingState::Idle => {
                            view! { <p class="text-sm text-muted-foreground">"No dataset selected"</p> }.into_any()
                        }
                        LoadingState::Loading => {
                            view! { <div class="flex justify-center py-4"><Spinner/></div> }.into_any()
                        }
                        LoadingState::Loaded(stats_data) => {
                            view! {
                                <dl class="space-y-3 text-sm">
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
            })}

            <Card>
                <h3 class="text-lg font-semibold mb-4">"Training Configuration"</h3>
                <div class="space-y-4 text-sm">
                    // Basic config: epochs and learning_rate
                    <div class="grid gap-4 md:grid-cols-2">
                        <div class="space-y-2">
                            <label class="text-xs text-muted-foreground">"Epochs"</label>
                            <Input
                                value=epochs
                                input_type="number".to_string()
                                placeholder="10".to_string()
                            />
                            <p class="text-xs text-muted-foreground">"Number of training epochs"</p>
                        </div>
                        <div class="space-y-2">
                            <label class="text-xs text-muted-foreground">"Learning Rate"</label>
                            <Input
                                value=learning_rate
                                input_type="text".to_string()
                                placeholder="0.0001".to_string()
                            />
                            <p class="text-xs text-muted-foreground">"Learning rate for optimizer"</p>
                        </div>
                    </div>

                    // Advanced toggle
                    <Toggle
                        checked=show_advanced
                        label="Show advanced options".to_string()
                        description="Configure LoRA rank, alpha, batch size, and validation split".to_string()
                    />

                    // Advanced options (conditionally shown)
                    {move || show_advanced.get().then(|| view! {
                        <div class="pt-4 border-t border-border space-y-4">
                            <div class="grid gap-4 md:grid-cols-2">
                                <div class="space-y-2">
                                    <label class="text-xs text-muted-foreground">"LoRA Rank"</label>
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                        placeholder="8".to_string()
                                    />
                                    <p class="text-xs text-muted-foreground">"Low-rank adaptation dimension"</p>
                                </div>
                                <div class="space-y-2">
                                    <label class="text-xs text-muted-foreground">"LoRA Alpha"</label>
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                        placeholder="16".to_string()
                                    />
                                    <p class="text-xs text-muted-foreground">"Scaling factor for LoRA weights"</p>
                                </div>
                            </div>
                            <div class="grid gap-4 md:grid-cols-2">
                                <div class="space-y-2">
                                    <label class="text-xs text-muted-foreground">"Batch Size"</label>
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                        placeholder="4".to_string()
                                    />
                                    <p class="text-xs text-muted-foreground">"Training batch size"</p>
                                </div>
                                <div class="space-y-2">
                                    <label class="text-xs text-muted-foreground">"Validation Split"</label>
                                    <Input
                                        value=validation_split
                                        input_type="text".to_string()
                                        placeholder="0.1".to_string()
                                    />
                                    <p class="text-xs text-muted-foreground">"Fraction held out for validation"</p>
                                </div>
                            </div>
                        </div>
                    })}
                </div>
            </Card>

            <Card>
                <h3 class="text-lg font-semibold mb-4">"Preprocessing"</h3>
                <div class="space-y-3 text-sm">
                    <Checkbox
                        checked=Signal::derive(move || pii_scrub.get())
                        on_change=Callback::new(move |val| pii_scrub.set(val))
                        label="PII scrub".to_string()
                    />
                    <Checkbox
                        checked=Signal::derive(move || dedupe.get())
                        on_change=Callback::new(move |val| dedupe.set(val))
                        label="Dedupe".to_string()
                    />
                    <p class="text-xs text-muted-foreground">
                        "These settings are UI-only in the MVP."
                    </p>
                </div>
            </Card>
        </div>
    }
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
