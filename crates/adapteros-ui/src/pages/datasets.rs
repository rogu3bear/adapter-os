//! Datasets management pages.
//!
//! Provides a list view (`/datasets`) and detail/manage view (`/datasets/:id`).

use crate::api::types::DatasetPreprocessStatusResponse;
use crate::api::{
    use_api_client, ApiClient, ApiError, CanonicalRow, DatasetPreviewResponse,
    DatasetVersionTrustOverrideRequest, ValidateDatasetRequest,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonType, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, Dialog, DialogSize, EmptyState, EmptyStateVariant, ErrorDisplay, Input,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, PageScaffoldPrimaryAction,
    RefreshButton, Select, SkeletonTable, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, use_conditional_polling, LoadingState, Refetch};
use crate::utils::{format_bytes, format_datetime, status_display_label, status_display_with_raw};
use adapteros_api_types::training::DatasetFileResponse;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use std::sync::Arc;

fn token(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('-', "_")
}

fn status_badge_variant(status: &str) -> BadgeVariant {
    match token(status).as_str() {
        "ready" | "valid" | "allowed" => BadgeVariant::Success,
        "processing" | "running" | "pending" | "validating" | "needs_approval" => {
            BadgeVariant::Warning
        }
        "invalid" | "blocked" | "failed" | "error" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

fn action_error_message(action: &str, error: &ApiError) -> String {
    if matches!(error, ApiError::Forbidden(_)) || error.code() == Some("FORBIDDEN") {
        format!(
            "You do not have permission to {}. Ask an administrator for dataset-management access.",
            action
        )
    } else {
        format!("Unable to {}: {}", action, error.user_message())
    }
}

fn preprocess_is_running(status: &DatasetPreprocessStatusResponse) -> bool {
    matches!(token(&status.status).as_str(), "pending" | "running")
}

#[component]
pub fn Datasets() -> impl IntoView {
    let navigate = use_navigate();
    let search = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let trust_filter = RwSignal::new(String::new());
    let validation_filter = RwSignal::new(String::new());

    let (datasets, refetch_datasets) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_datasets(None).await });

    view! {
        <PageScaffold
            title="Datasets"
            subtitle="Browse datasets, inspect trust and validation state, and launch training."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Build", "/training"),
                PageBreadcrumbItem::current("Datasets"),
            ]
            full_width=true
        >
            <PageScaffoldPrimaryAction slot>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| navigate("/training?open_wizard=1", Default::default()))
                >
                    "Create dataset"
                </Button>
            </PageScaffoldPrimaryAction>

            <PageScaffoldActions slot>
                <Input value=search placeholder="Search by name or ID".to_string() />
                <Select
                    value=status_filter
                    options=vec![
                        ("".to_string(), "All statuses".to_string()),
                        ("ready".to_string(), "Ready".to_string()),
                        ("processing".to_string(), "Processing".to_string()),
                        ("pending".to_string(), "Pending".to_string()),
                        ("failed".to_string(), "Failed".to_string()),
                    ]
                    class="w-40".to_string()
                />
                <Select
                    value=trust_filter
                    options=vec![
                        ("".to_string(), "All trust".to_string()),
                        ("allowed".to_string(), "Allowed".to_string()),
                        (
                            "allowed_with_warning".to_string(),
                            "Allowed With Warning".to_string(),
                        ),
                        ("needs_approval".to_string(), "Needs Approval".to_string()),
                        ("blocked".to_string(), "Blocked".to_string()),
                        ("unknown".to_string(), "Unknown".to_string()),
                    ]
                    class="w-48".to_string()
                />
                <Select
                    value=validation_filter
                    options=vec![
                        ("".to_string(), "All validation".to_string()),
                        ("valid".to_string(), "Valid".to_string()),
                        ("validating".to_string(), "Validating".to_string()),
                        ("pending".to_string(), "Pending".to_string()),
                        ("invalid".to_string(), "Invalid".to_string()),
                    ]
                    class="w-44".to_string()
                />
                <RefreshButton on_click=Callback::new(move |_| refetch_datasets.run(())) />
            </PageScaffoldActions>

            {move || {
                match datasets.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonTable rows=5 columns=6 /> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch_datasets.run(()))
                            />
                        }
                        .into_any()
                    }
                    LoadingState::Loaded(resp) => {
                        let query = search.get().trim().to_ascii_lowercase();
                        let status = token(&status_filter.get());
                        let trust = token(&trust_filter.get());
                        let validation = token(&validation_filter.get());

                        let rows = resp
                            .datasets
                            .into_iter()
                            .filter(|dataset| {
                                if !query.is_empty() {
                                    let display_name = dataset
                                        .display_name
                                        .clone()
                                        .unwrap_or_else(|| dataset.name.clone())
                                        .to_ascii_lowercase();
                                    let id = dataset.id.to_ascii_lowercase();
                                    if !display_name.contains(&query) && !id.contains(&query) {
                                        return false;
                                    }
                                }

                                if !status.is_empty() && token(&dataset.status) != status {
                                    return false;
                                }

                                if !trust.is_empty() {
                                    let trust_state = dataset
                                        .trust_state
                                        .as_deref()
                                        .map(token)
                                        .unwrap_or_else(|| "unknown".to_string());
                                    if trust_state != trust {
                                        return false;
                                    }
                                }

                                if !validation.is_empty() {
                                    let validation_state = dataset
                                        .validation_status
                                        .as_deref()
                                        .map(token)
                                        .unwrap_or_else(|| "unknown".to_string());
                                    if validation_state != validation {
                                        return false;
                                    }
                                }

                                true
                            })
                            .collect::<Vec<_>>();

                        if rows.is_empty() {
                            view! {
                                <EmptyState
                                    variant=EmptyStateVariant::Empty
                                    title="No datasets found"
                                    description="Try different filters or create a new dataset from the training wizard."
                                />
                            }
                            .into_any()
                        } else {
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Name"</TableHead>
                                            <TableHead>"Dataset ID"</TableHead>
                                            <TableHead>"Status"</TableHead>
                                            <TableHead>"Trust"</TableHead>
                                            <TableHead>"Validation"</TableHead>
                                            <TableHead>"Created"</TableHead>
                                            <TableHead>"Actions"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {rows
                                            .into_iter()
                                            .map(|dataset| {
                                                let display_name = dataset
                                                    .display_name
                                                    .clone()
                                                    .unwrap_or_else(|| dataset.name.clone());
                                                let status_label = status_display_label(&dataset.status);
                                                let trust_state = dataset
                                                    .trust_state
                                                    .clone()
                                                    .unwrap_or_else(|| "unknown".to_string());
                                                let validation_state = dataset
                                                    .validation_status
                                                    .clone()
                                                    .unwrap_or_else(|| "unknown".to_string());
                                                let trust_state_label = status_display_label(&trust_state);
                                                let validation_state_label =
                                                    status_display_label(&validation_state);
                                                let dataset_id = dataset.id.clone();
                                                let detail_href = format!("/datasets/{}", dataset.id);
                                                let train_href = format!(
                                                    "/training?open_wizard=1&dataset_id={}",
                                                    dataset.id
                                                );

                                                view! {
                                                    <TableRow>
                                                        <TableCell>{display_name}</TableCell>
                                                        <TableCell>
                                                            <code class="text-xs">{dataset_id.clone()}</code>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span title=dataset.status.clone()>
                                                                <Badge variant=status_badge_variant(&dataset.status)>
                                                                    {status_label}
                                                                </Badge>
                                                            </span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span title=trust_state.clone()>
                                                                <Badge variant=status_badge_variant(&trust_state)>
                                                                    {trust_state_label}
                                                                </Badge>
                                                            </span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span title=validation_state.clone()>
                                                                <Badge variant=status_badge_variant(&validation_state)>
                                                                    {validation_state_label}
                                                                </Badge>
                                                            </span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span class="text-xs text-muted-foreground">
                                                                {format_datetime(&dataset.created_at)}
                                                            </span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <div class="flex items-center gap-2">
                                                                <a href=detail_href.clone()>
                                                                    <Button
                                                                        button_type=ButtonType::Button
                                                                        variant=ButtonVariant::Secondary
                                                                    >
                                                                        "View"
                                                                    </Button>
                                                                </a>
                                                                <a href=train_href>
                                                                    <Button
                                                                        button_type=ButtonType::Button
                                                                        variant=ButtonVariant::Ghost
                                                                    >
                                                                        "Train"
                                                                    </Button>
                                                                </a>
                                                            </div>
                                                        </TableCell>
                                                    </TableRow>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }
                            .into_any()
                        }
                    }
                }
            }}
        </PageScaffold>
    }
}

#[component]
pub fn DatasetDetail() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let client = use_api_client();

    let dataset_id = Signal::derive(move || params.get().get("id").unwrap_or_default());

    let (dataset, refetch_dataset) = use_api_resource(move |client: Arc<ApiClient>| {
        let dataset_id = dataset_id.get();
        async move { client.get_dataset(&dataset_id).await }
    });

    let (versions, refetch_versions) = use_api_resource(move |client: Arc<ApiClient>| {
        let dataset_id = dataset_id.get();
        async move { client.list_dataset_versions(&dataset_id).await }
    });

    let (files, refetch_files) = use_api_resource(move |client: Arc<ApiClient>| {
        let dataset_id = dataset_id.get();
        async move { client.list_dataset_files(&dataset_id).await }
    });

    let (_preview, refetch_preview) = use_api_resource(move |client: Arc<ApiClient>| {
        let dataset_id = dataset_id.get();
        async move { client.preview_dataset(&dataset_id, Some(5)).await }
    });

    let (adapters, refetch_adapters) = use_api_resource(move |client: Arc<ApiClient>| {
        let dataset_id = dataset_id.get();
        async move { client.get_dataset_adapters(&dataset_id).await }
    });

    let selected_version_id = RwSignal::new(String::new());
    let compare_version_id = RwSignal::new(String::new());
    let editing_row_id = RwSignal::new(None::<String>);
    let edit_prompt = RwSignal::new(String::new());
    let edit_response = RwSignal::new(String::new());
    let edit_weight = RwSignal::new(String::new());
    let edit_split = RwSignal::new(String::new());
    let selected_source_file_id = RwSignal::new(None::<String>);
    let selected_source_page = RwSignal::new(None::<i32>);
    let selected_source_row_id = RwSignal::new(None::<String>);
    let pending_row_edits =
        RwSignal::new(HashMap::<String, crate::api::types::DatasetRowEditRequest>::new());

    let (version_rows, refetch_version_rows) = use_api_resource(move |client: Arc<ApiClient>| {
        let version_id = selected_version_id.get();
        async move {
            if version_id.trim().is_empty() {
                Ok(Vec::new())
            } else {
                client.list_dataset_rows(&version_id, None, None).await
            }
        }
    });

    let (version_detail, refetch_version_detail) =
        use_api_resource(move |client: Arc<ApiClient>| {
            let dataset_id = dataset_id.get();
            let revision = selected_version_id.get();
            let compare_to = compare_version_id.get();
            async move {
                let revision = if revision.trim().is_empty() {
                    "latest".to_string()
                } else {
                    revision
                };
                let compare = if compare_to.trim().is_empty() {
                    None
                } else {
                    Some(compare_to.clone())
                };
                client
                    .get_dataset_version_detail(&dataset_id, &revision, compare.as_deref(), false)
                    .await
            }
        });

    let validating = RwSignal::new(false);
    let preprocessing = RwSignal::new(false);
    let applying_override = RwSignal::new(false);
    let saving_row_edits = RwSignal::new(false);
    let regenerating_evaluation = RwSignal::new(false);
    let deleting = RwSignal::new(false);
    let delete_dialog_open = RwSignal::new(false);
    let name_editing = RwSignal::new(false);
    let name_draft = RwSignal::new(String::new());
    let renaming = RwSignal::new(false);

    let override_version_id = RwSignal::new(String::new());
    let override_state = RwSignal::new("allowed".to_string());
    let override_reason = RwSignal::new(String::new());

    let preprocess_status = RwSignal::new(None::<DatasetPreprocessStatusResponse>);
    let action_error = RwSignal::new(None::<String>);
    let action_success = RwSignal::new(None::<String>);

    Effect::new(move || {
        if let LoadingState::Loaded(resp) = versions.get() {
            if let Some(version) = resp.versions.first() {
                if override_version_id.get().trim().is_empty() {
                    override_version_id.set(version.dataset_version_id.clone());
                }
                if selected_version_id.get().trim().is_empty() {
                    selected_version_id.set(version.dataset_version_id.clone());
                }
            }
        }
    });

    let should_poll_preprocess = Signal::derive(move || {
        preprocess_status
            .get()
            .as_ref()
            .map(preprocess_is_running)
            .unwrap_or(false)
    });

    let _cancel_preprocess_poll = use_conditional_polling(1500, should_poll_preprocess, {
        let client = client.clone();
        move || {
            let client = client.clone();
            let dataset_id = dataset_id.get();
            async move {
                match client.get_dataset_preprocess_status(&dataset_id).await {
                    Ok(status) => {
                        preprocess_status.set(Some(status.clone()));
                        if !preprocess_is_running(&status) {
                            preprocessing.set(false);
                            refetch_dataset.run(());
                            if token(&status.status) == "completed" {
                                action_success.set(Some("Preprocessing completed.".to_string()));
                            } else {
                                let reason = status
                                    .error_message
                                    .clone()
                                    .unwrap_or_else(|| "Preprocessing failed.".to_string());
                                action_error.set(Some(reason));
                            }
                        }
                    }
                    Err(e) => {
                        preprocessing.set(false);
                        action_error
                            .set(Some(action_error_message("check preprocessing status", &e)));
                    }
                }
            }
        }
    });

    let validate_action = Callback::new({
        let client = client.clone();
        move |_| {
            if validating.get() {
                return;
            }
            validating.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            wasm_bindgen_futures::spawn_local(async move {
                let request = ValidateDatasetRequest {
                    check_format: Some(true),
                };
                match client.validate_dataset(&dataset_id, &request).await {
                    Ok(resp) => {
                        action_success.set(Some(format!(
                            "Validation finished: {}.",
                            status_display_label(&resp.validation_status.to_string())
                        )));
                        refetch_dataset.run(());
                        refetch_preview.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message("validate this dataset", &e)));
                    }
                }
                validating.set(false);
            });
        }
    });

    let preprocess_action = Callback::new({
        let client = client.clone();
        move |_| {
            if preprocessing.get() {
                return;
            }
            preprocessing.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .start_dataset_preprocessing(&dataset_id, true, true)
                    .await
                {
                    Ok(started) => {
                        action_success.set(Some(format!(
                            "Preprocessing started ({})",
                            status_display_label(&started.status)
                        )));
                        match client.get_dataset_preprocess_status(&dataset_id).await {
                            Ok(status) => {
                                preprocess_status.set(Some(status.clone()));
                                if !preprocess_is_running(&status) {
                                    preprocessing.set(false);
                                }
                            }
                            Err(e) => {
                                preprocessing.set(false);
                                action_error.set(Some(action_error_message(
                                    "fetch preprocessing status",
                                    &e,
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        preprocessing.set(false);
                        action_error.set(Some(action_error_message("start preprocessing", &e)));
                    }
                }
            });
        }
    });

    let apply_override_action = Callback::new({
        let client = client.clone();
        move |_| {
            if applying_override.get() {
                return;
            }

            let version_id = override_version_id.get().trim().to_string();
            let reason = override_reason.get().trim().to_string();
            if version_id.is_empty() {
                action_error.set(Some(
                    "Select a dataset version before applying trust override.".to_string(),
                ));
                return;
            }
            if reason.is_empty() {
                action_error.set(Some("Trust override reason is required.".to_string()));
                return;
            }

            applying_override.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            let state = override_state.get();
            wasm_bindgen_futures::spawn_local(async move {
                let request = DatasetVersionTrustOverrideRequest {
                    override_state: state,
                    reason: Some(reason),
                };

                match client
                    .apply_dataset_version_trust_override(&dataset_id, &version_id, &request)
                    .await
                {
                    Ok(resp) => {
                        action_success.set(Some(format!(
                            "Trust override applied. Effective trust state: {}.",
                            status_display_label(&resp.effective_trust_state)
                        )));
                        refetch_versions.run(());
                        refetch_dataset.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message("apply trust override", &e)));
                    }
                }
                applying_override.set(false);
            });
        }
    });

    let start_row_edit = Callback::new(move |row: CanonicalRow| {
        editing_row_id.set(Some(row.row_id));
        edit_prompt.set(row.prompt);
        edit_response.set(row.response);
        edit_weight.set(format!("{}", row.weight));
        edit_split.set(row.split);
    });

    let cancel_row_edit = Callback::new(move |_| {
        editing_row_id.set(None);
        edit_prompt.set(String::new());
        edit_response.set(String::new());
        edit_weight.set(String::new());
        edit_split.set(String::new());
    });

    let save_row_edit_draft = Callback::new({
        move |row: CanonicalRow| {
            let row_id = row.row_id.clone();
            let mut weight = row.weight;
            if let Ok(parsed) = edit_weight.get().trim().parse::<f32>() {
                weight = parsed;
            }
            let split = if edit_split.get().trim().is_empty() {
                row.split.clone()
            } else {
                edit_split.get().trim().to_ascii_lowercase()
            };
            let prompt = edit_prompt.get();
            let response = edit_response.get();

            let unchanged = prompt == row.prompt
                && response == row.response
                && (weight - row.weight).abs() <= f32::EPSILON
                && split == row.split;

            pending_row_edits.update(|drafts| {
                if unchanged {
                    drafts.remove(&row_id);
                } else {
                    drafts.insert(
                        row_id.clone(),
                        crate::api::types::DatasetRowEditRequest {
                            row_id: row_id.clone(),
                            prompt: Some(prompt.clone()),
                            response: Some(response.clone()),
                            weight: Some(weight),
                            split: Some(split.clone()),
                        },
                    );
                }
            });

            editing_row_id.set(None);
            edit_prompt.set(String::new());
            edit_response.set(String::new());
            edit_weight.set(String::new());
            edit_split.set(String::new());
        }
    });

    let open_row_source = Callback::new(move |row: CanonicalRow| {
        selected_source_row_id.set(Some(row.row_id.clone()));
        selected_source_page.set(row_source_page_start(&row));

        if let LoadingState::Loaded(file_rows) = files.get() {
            if let Some(file) = resolve_source_file_for_row(&row, &file_rows) {
                selected_source_file_id.set(Some(file.file_id));
            }
        }
    });

    let save_edits_as_new_version = Callback::new({
        let client = client.clone();
        move |_| {
            if saving_row_edits.get() {
                return;
            }

            let drafts = pending_row_edits.get();
            if drafts.is_empty() {
                action_error.set(Some("No row edits to save.".to_string()));
                return;
            }
            let base_version = selected_version_id.get();
            if base_version.trim().is_empty() {
                action_error.set(Some(
                    "Select a base dataset version before saving edits.".to_string(),
                ));
                return;
            }

            saving_row_edits.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            let row_edits = drafts.values().cloned().collect::<Vec<_>>();
            wasm_bindgen_futures::spawn_local(async move {
                let request = crate::api::types::CreateDatasetVersionRequest {
                    version_label: Some("Edited rows".to_string()),
                    manifest_path: None,
                    manifest_json: None,
                    base_dataset_version_id: Some(base_version.clone()),
                    row_edits: Some(row_edits),
                };

                match client.create_dataset_version(&dataset_id, &request).await {
                    Ok(resp) => {
                        selected_version_id.set(resp.dataset_version_id.clone());
                        compare_version_id.set(String::new());
                        pending_row_edits.set(HashMap::new());
                        action_success.set(Some(format!(
                            "Created dataset version v{} from row edits.",
                            resp.version_number
                        )));
                        refetch_versions.run(());
                        refetch_version_rows.run(());
                        refetch_version_detail.run(());
                        refetch_preview.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message(
                            "save row edits as a new version",
                            &e,
                        )));
                    }
                }

                saving_row_edits.set(false);
            });
        }
    });

    let regenerate_evaluation = Callback::new({
        let client = client.clone();
        move |_| {
            if regenerating_evaluation.get() {
                return;
            }
            let revision = selected_version_id.get();
            if revision.trim().is_empty() {
                action_error.set(Some(
                    "Select a version before regenerating evaluation.".to_string(),
                ));
                return;
            }

            regenerating_evaluation.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            let compare_to = compare_version_id.get();
            wasm_bindgen_futures::spawn_local(async move {
                let compare = if compare_to.trim().is_empty() {
                    None
                } else {
                    Some(compare_to.as_str())
                };
                match client
                    .get_dataset_version_detail(&dataset_id, &revision, compare, true)
                    .await
                {
                    Ok(_) => {
                        action_success.set(Some("Dataset evaluation regenerated.".to_string()));
                        refetch_version_detail.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message(
                            "regenerate dataset evaluation",
                            &e,
                        )));
                    }
                }
                regenerating_evaluation.set(false);
            });
        }
    });

    let start_rename = Callback::new({
        move |_| {
            if let LoadingState::Loaded(data) = dataset.get() {
                let display_name = data
                    .display_name
                    .clone()
                    .unwrap_or_else(|| data.name.clone());
                name_draft.set(display_name);
                name_editing.set(true);
            }
        }
    });

    let cancel_rename = Callback::new(move |_| {
        name_editing.set(false);
        name_draft.set(String::new());
    });

    let save_rename = Callback::new({
        let client = client.clone();
        move |_| {
            if renaming.get() {
                return;
            }
            let new_name = name_draft.get().trim().to_string();
            if new_name.is_empty() {
                action_error.set(Some("Name cannot be empty.".to_string()));
                return;
            }
            renaming.set(true);
            action_error.set(None);
            action_success.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .update_dataset(&dataset_id, Some(new_name.as_str()), None)
                    .await
                {
                    Ok(_) => {
                        name_editing.set(false);
                        name_draft.set(String::new());
                        action_success.set(Some("Dataset renamed.".to_string()));
                        refetch_dataset.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message("rename this dataset", &e)));
                    }
                }
                renaming.set(false);
            });
        }
    });

    let delete_action = Callback::new({
        let client = client.clone();
        let navigate_after_delete = navigate.clone();
        move |_| {
            if deleting.get() {
                return;
            }
            deleting.set(true);
            action_error.set(None);

            let client = client.clone();
            let dataset_id = dataset_id.get();
            let navigate = navigate_after_delete.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_dataset(&dataset_id).await {
                    Ok(_) => {
                        delete_dialog_open.set(false);
                        navigate("/datasets", Default::default());
                    }
                    Err(e) => {
                        action_error.set(Some(action_error_message("delete this dataset", &e)));
                    }
                }
                deleting.set(false);
            });
        }
    });

    let versions_for_override = Signal::derive(move || {
        if let LoadingState::Loaded(resp) = versions.get() {
            resp.versions
                .iter()
                .map(|v| {
                    (
                        v.dataset_version_id.clone(),
                        format!("v{} ({})", v.version_number, v.dataset_version_id.clone()),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    });

    let versions_for_compare = Signal::derive(move || {
        let mut options = vec![("".to_string(), "No compare".to_string())];
        if let LoadingState::Loaded(resp) = versions.get() {
            for v in resp.versions {
                options.push((
                    v.dataset_version_id.clone(),
                    format!("v{} ({})", v.version_number, v.dataset_version_id),
                ));
            }
        }
        options
    });

    let train_href =
        Signal::derive(move || format!("/training?open_wizard=1&dataset_id={}", dataset_id.get()));
    let files_table_client = client.clone();
    let source_panel_client = client.clone();

    view! {
        <PageScaffold
            title="Dataset Detail"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Build", "/training"),
                PageBreadcrumbItem::new("Datasets", "/datasets"),
                PageBreadcrumbItem::current(dataset_id.get()),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                <Button
                    button_type=ButtonType::Button
                    variant=ButtonVariant::Ghost
                    on_click=Callback::new(move |_| navigate("/datasets", Default::default()))
                >
                    "Back to datasets"
                </Button>
                <RefreshButton
                    on_click=Callback::new(move |_| {
                        refetch_dataset.run(());
                        refetch_versions.run(());
                        refetch_files.run(());
                        refetch_preview.run(());
                        refetch_version_rows.run(());
                        refetch_version_detail.run(());
                    })
                />
            </PageScaffoldActions>

            {move || action_error.get().map(|message| view! {
                <div class="mb-4 rounded-lg border border-destructive/50 bg-destructive/10 p-3">
                    <p class="text-sm text-destructive">{message}</p>
                </div>
            })}

            {move || action_success.get().map(|message| view! {
                <div class="mb-4 rounded-lg border border-status-success/50 bg-status-success/5 p-3">
                    <p class="text-sm text-status-success">{message}</p>
                </div>
            })}

            <div class="grid gap-4 lg:grid-cols-3">
                <div class="space-y-4 lg:col-span-2">
                    <Card title="Summary".to_string()>
                        {move || match dataset.get() {
                            LoadingState::Idle | LoadingState::Loading => {
                                view! { <p class="text-sm text-muted-foreground">"Loading dataset summary..."</p> }.into_any()
                            }
                            LoadingState::Error(e) => {
                                view! { <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch_dataset.run(())) /> }.into_any()
                            }
                            LoadingState::Loaded(data) => {
                                let display_name = data
                                    .display_name
                                    .clone()
                                    .unwrap_or_else(|| data.name.clone());
                                view! {
                                    <div class="grid gap-3 text-sm md:grid-cols-2">
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Name"</p>
                                            {move || if name_editing.get() {
                                                view! {
                                                    <div class="flex items-center gap-2">
                                                        <Input
                                                            value=name_draft
                                                            placeholder="Dataset name".to_string()
                                                            class="flex-1".to_string()
                                                        />
                                                        <Button
                                                            button_type=ButtonType::Button
                                                            variant=ButtonVariant::Primary
                                                            disabled=Signal::derive(move || renaming.get())
                                                            loading=Signal::derive(move || renaming.get())
                                                            on_click=save_rename
                                                        >
                                                            "Save"
                                                        </Button>
                                                        <Button
                                                            button_type=ButtonType::Button
                                                            variant=ButtonVariant::Ghost
                                                            disabled=Signal::derive(move || renaming.get())
                                                            on_click=cancel_rename
                                                        >
                                                            "Cancel"
                                                        </Button>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="flex items-center gap-2">
                                                        <p class="font-medium flex-1">{display_name.clone()}</p>
                                                        <Button
                                                            button_type=ButtonType::Button
                                                            variant=ButtonVariant::Ghost
                                                            on_click=start_rename
                                                        >
                                                            "Rename"
                                                        </Button>
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Dataset ID"</p>
                                            <p class="font-mono text-xs">{data.id.clone()}</p>
                                        </div>
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Format"</p>
                                            <p>{data.format.clone()}</p>
                                        </div>
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Dataset Hash"</p>
                                            <p class="font-mono text-xs">
                                                {data
                                                    .dataset_hash_b3
                                                    .clone()
                                                    .or(data.hash_b3.clone())
                                                    .unwrap_or_else(|| "-".to_string())}
                                            </p>
                                        </div>
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Created"</p>
                                            <p>{format_datetime(&data.created_at)}</p>
                                        </div>
                                        <div>
                                            <p class="text-xs text-muted-foreground">"Updated"</p>
                                            <p>
                                                {data
                                                    .updated_at
                                                    .clone()
                                                    .map(|v| format_datetime(&v))
                                                    .unwrap_or_else(|| "-".to_string())}
                                            </p>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }}
                    </Card>

                    <Card
                        title="Versions".to_string()
                        description="Dataset versions from uploads. Add files via the training wizard.".to_string()
                    >
                        {move || {
                            let add_version_href = format!(
                                "/training?open_wizard=1&dataset_id={}",
                                dataset_id.get()
                            );
                            match versions.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! { <p class="text-sm text-muted-foreground">"Loading versions..."</p> }.into_any()
                                }
                                LoadingState::Error(e) => {
                                    view! { <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch_versions.run(())) /> }.into_any()
                                }
                                LoadingState::Loaded(resp) => {
                                    if resp.versions.is_empty() {
                                        view! {
                                            <EmptyState
                                                variant=EmptyStateVariant::Empty
                                                title="No versions yet"
                                                description="Upload files via the training wizard to create a version."
                                                secondary_label="Add version"
                                                secondary_href=add_version_href
                                            />
                                        }
                                        .into_any()
                                    } else {
                                        view! {
                                            <div class="space-y-3">
                                                <a href=add_version_href>
                                                    <Button button_type=ButtonType::Button variant=ButtonVariant::Secondary>
                                                        "Add version"
                                                    </Button>
                                                </a>
                                                <div class="grid gap-2 md:grid-cols-2">
                                                    <div class="space-y-1">
                                                        <p class="text-xs text-muted-foreground">"Working version"</p>
                                                        <Select
                                                            value=selected_version_id
                                                            options=versions_for_override.get()
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <p class="text-xs text-muted-foreground">"Compare with"</p>
                                                        <Select
                                                            value=compare_version_id
                                                            options=versions_for_compare.get()
                                                        />
                                                    </div>
                                                </div>
                                                <Table>
                                                    <TableHeader>
                                                        <TableRow>
                                                            <TableHead>"Version"</TableHead>
                                                            <TableHead>"Version ID"</TableHead>
                                                            <TableHead>"Trust"</TableHead>
                                                            <TableHead>"Created"</TableHead>
                                                        </TableRow>
                                                    </TableHeader>
                                                    <TableBody>
                                                        {resp
                                                            .versions
                                                            .into_iter()
                                                            .map(|version| {
                                                                let is_selected = selected_version_id.get()
                                                                    == version.dataset_version_id;
                                                                let version_id = version.dataset_version_id.clone();
                                                                let version_id_for_click = version_id.clone();
                                                                let trust = version
                                                                    .trust_state
                                                                    .clone()
                                                                    .unwrap_or_else(|| "unknown".to_string());
                                                                let trust_label = status_display_label(&trust);
                                                                view! {
                                                                    <TableRow class=if is_selected { "bg-primary/5" } else { "" }>
                                                                        <TableCell>{format!("v{}", version.version_number)}</TableCell>
                                                                        <TableCell>
                                                                            <button
                                                                                class="font-mono text-xs text-left hover:underline"
                                                                                on:click=move |_| selected_version_id.set(version_id_for_click.clone())
                                                                            >
                                                                                {version_id}
                                                                            </button>
                                                                        </TableCell>
                                                                        <TableCell>
                                                                            <span title=trust.clone()>
                                                                                <Badge variant=status_badge_variant(&trust)>
                                                                                    {trust_label}
                                                                                </Badge>
                                                                            </span>
                                                                        </TableCell>
                                                                        <TableCell>
                                                                            <span class="text-xs text-muted-foreground">
                                                                                {format_datetime(&version.created_at)}
                                                                            </span>
                                                                        </TableCell>
                                                                    </TableRow>
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </TableBody>
                                                </Table>
                                            </div>
                                        }
                                        .into_any()
                                    }
                                }
                            }
                        }}
                    </Card>

                    <Card title="Files".to_string()>
                        <DatasetFilesTable
                            dataset_id=dataset_id.get()
                            client=files_table_client.clone()
                            files=files
                            refetch=refetch_files
                        />
                    </Card>

                    <Card
                        title="Adapters using this dataset".to_string()
                        description="Adapters trained on this dataset (from training lineage).".to_string()
                    >
                        {move || match adapters.get() {
                            LoadingState::Idle | LoadingState::Loading => {
                                view! { <p class="text-sm text-muted-foreground">"Loading adapters..."</p> }.into_any()
                            }
                            LoadingState::Error(e) => {
                                view! {
                                    <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch_adapters.run(())) />
                                }
                                    .into_any()
                            }
                            LoadingState::Loaded(resp) => {
                                if resp.adapters.is_empty() {
                                    view! {
                                        <EmptyState
                                            variant=EmptyStateVariant::Empty
                                            title="No adapters trained"
                                            description="Adapters will appear here after training jobs use this dataset."
                                        />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="space-y-2">
                                            {resp
                                                .adapters
                                                .into_iter()
                                                .map(|a| {
                                                    let href = format!("/adapters/{}", a.adapter_id);
                                                    let adapter_id = a.adapter_id.clone();
                                                    let version_hint = a
                                                        .dataset_version_id
                                                        .as_ref()
                                                        .map(|vid| format!(" ({})", vid))
                                                        .unwrap_or_default();
                                                    view! {
                                                        <div class="flex items-center gap-2">
                                                            <a href=href class="text-sm font-medium text-primary hover:underline">
                                                                {adapter_id}
                                                            </a>
                                                            {if !version_hint.is_empty() {
                                                                view! {
                                                                    <span class="text-xs text-muted-foreground font-mono">{version_hint}</span>
                                                                }.into_any()
                                                            } else {
                                                                view! {}.into_any()
                                                            }}
                                                        </div>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </div>
                                    }
                                    .into_any()
                                }
                            }
                        }}
                    </Card>

                    <Card
                        title="Rows".to_string()
                        description="Editable dataset rows with provenance and source navigation.".to_string()
                    >
                        <div class="space-y-3">
                            <div class="flex flex-wrap items-center gap-2">
                                <span class="text-xs text-muted-foreground">
                                    {move || {
                                        let selected = selected_version_id.get();
                                        if selected.is_empty() {
                                            "Select a version to inspect rows.".to_string()
                                        } else {
                                            format!("Working version: {}", selected)
                                        }
                                    }}
                                </span>
                                <Badge variant=BadgeVariant::Secondary>
                                    {move || format!("Pending edits: {}", pending_row_edits.get().len())}
                                </Badge>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Primary
                                    disabled=Signal::derive(move || {
                                        saving_row_edits.get() || pending_row_edits.get().is_empty()
                                    })
                                    loading=Signal::derive(move || saving_row_edits.get())
                                    on_click=save_edits_as_new_version
                                >
                                    "Save edits as new version"
                                </Button>
                            </div>

                            <div class="grid gap-4 lg:grid-cols-3">
                                <div class="lg:col-span-2">
                                    {move || match version_rows.get() {
                                        LoadingState::Idle | LoadingState::Loading => {
                                            view! { <p class="text-sm text-muted-foreground">"Loading rows..."</p> }.into_any()
                                        }
                                        LoadingState::Error(e) => {
                                            view! { <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch_version_rows.run(())) /> }.into_any()
                                        }
                                        LoadingState::Loaded(rows) => {
                                            if rows.is_empty() {
                                                view! {
                                                    <p class="text-sm text-muted-foreground">
                                                        "No canonical rows available for this version."
                                                    </p>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="overflow-x-auto">
                                                        <Table>
                                                            <TableHeader>
                                                                <TableRow>
                                                                    <TableHead>"Prompt"</TableHead>
                                                                    <TableHead>"Response"</TableHead>
                                                                    <TableHead>"Weight"</TableHead>
                                                                    <TableHead>"Split"</TableHead>
                                                                    <TableHead>"Provenance"</TableHead>
                                                                    <TableHead>"Source span"</TableHead>
                                                                    <TableHead>"Actions"</TableHead>
                                                                </TableRow>
                                                            </TableHeader>
                                                            <TableBody>
                                                                {rows
                                                                    .into_iter()
                                                                    .map(|row| {
                                                                        let row_for_edit_save = row.clone();
                                                                        let row_for_edit_start = row.clone();
                                                                        let row_for_source_span = row.clone();
                                                                        let row_for_source_action = row.clone();
                                                                        let is_draft = pending_row_edits
                                                                            .get()
                                                                            .contains_key(&row.row_id);
                                                                        let is_editing = editing_row_id
                                                                            .get()
                                                                            .as_ref()
                                                                            .map(|id| id == &row.row_id)
                                                                            .unwrap_or(false);
                                                                        let source_span = row_source_span_label(&row);
                                                                        let provenance_state = if row
                                                                            .metadata
                                                                            .get("provenance_invalidated")
                                                                            .and_then(|v| v.as_bool())
                                                                            .unwrap_or(false) {
                                                                            "invalidated"
                                                                        } else {
                                                                            "linked"
                                                                        };

                                                                        view! {
                                                                            <TableRow class=if is_draft { "bg-warning/5" } else { "" }>
                                                                                <TableCell class="max-w-[18rem] align-top">
                                                                                    {if is_editing {
                                                                                        view! {
                                                                                            <textarea
                                                                                                class="w-full rounded-md border border-border/70 bg-background p-2 text-xs"
                                                                                                rows="4"
                                                                                                prop:value=move || edit_prompt.get()
                                                                                                on:input=move |ev| edit_prompt.set(event_target_value(&ev))
                                                                                            />
                                                                                        }.into_any()
                                                                                    } else {
                                                                                        view! { <span class="text-xs">{shorten_text(&row.prompt, 180)}</span> }.into_any()
                                                                                    }}
                                                                                </TableCell>
                                                                                <TableCell class="max-w-[18rem] align-top">
                                                                                    {if is_editing {
                                                                                        view! {
                                                                                            <textarea
                                                                                                class="w-full rounded-md border border-border/70 bg-background p-2 text-xs"
                                                                                                rows="4"
                                                                                                prop:value=move || edit_response.get()
                                                                                                on:input=move |ev| edit_response.set(event_target_value(&ev))
                                                                                            />
                                                                                        }.into_any()
                                                                                    } else {
                                                                                        view! { <span class="text-xs">{shorten_text(&row.response, 180)}</span> }.into_any()
                                                                                    }}
                                                                                </TableCell>
                                                                                <TableCell class="align-top">
                                                                                    {if is_editing {
                                                                                        view! {
                                                                                            <Input
                                                                                                value=edit_weight
                                                                                                placeholder="1.0".to_string()
                                                                                                class="w-24".to_string()
                                                                                            />
                                                                                        }.into_any()
                                                                                    } else {
                                                                                        view! { <span class="text-xs">{format!("{:.3}", row.weight)}</span> }.into_any()
                                                                                    }}
                                                                                </TableCell>
                                                                                <TableCell class="align-top">
                                                                                    {if is_editing {
                                                                                        view! {
                                                                                            <Input
                                                                                                value=edit_split
                                                                                                placeholder="train".to_string()
                                                                                                class="w-24".to_string()
                                                                                            />
                                                                                        }.into_any()
                                                                                    } else {
                                                                                        view! { <span class="text-xs">{row.split.clone()}</span> }.into_any()
                                                                                    }}
                                                                                </TableCell>
                                                                                <TableCell class="align-top">
                                                                                    <Badge
                                                                                        variant=if provenance_state == "invalidated" {
                                                                                            BadgeVariant::Warning
                                                                                        } else {
                                                                                            BadgeVariant::Success
                                                                                        }
                                                                                    >
                                                                                        {provenance_state}
                                                                                    </Badge>
                                                                                </TableCell>
                                                                                <TableCell class="align-top">
                                                                                    <button
                                                                                        class="text-xs text-muted-foreground text-left hover:underline"
                                                                                        on:click=move |_| open_row_source.run(row_for_source_span.clone())
                                                                                    >
                                                                                        {source_span}
                                                                                    </button>
                                                                                </TableCell>
                                                                                <TableCell class="align-top">
                                                                                    <div class="flex items-center gap-2">
                                                                                        {if is_editing {
                                                                                            view! {
                                                                                                <Button
                                                                                                    button_type=ButtonType::Button
                                                                                                    variant=ButtonVariant::Primary
                                                                                                    on_click=Callback::new(move |_| save_row_edit_draft.run(row_for_edit_save.clone()))
                                                                                                >
                                                                                                    "Save Row"
                                                                                                </Button>
                                                                                                <Button
                                                                                                    button_type=ButtonType::Button
                                                                                                    variant=ButtonVariant::Ghost
                                                                                                    on_click=cancel_row_edit
                                                                                                >
                                                                                                    "Cancel"
                                                                                                </Button>
                                                                                            }.into_any()
                                                                                        } else {
                                                                                            view! {
                                                                                                <Button
                                                                                                    button_type=ButtonType::Button
                                                                                                    variant=ButtonVariant::Ghost
                                                                                                    on_click=Callback::new(move |_| start_row_edit.run(row_for_edit_start.clone()))
                                                                                                >
                                                                                                    "Edit"
                                                                                                </Button>
                                                                                                <Button
                                                                                                    button_type=ButtonType::Button
                                                                                                    variant=ButtonVariant::Ghost
                                                                                                    on_click=Callback::new(move |_| open_row_source.run(row_for_source_action.clone()))
                                                                                                >
                                                                                                    "Open Source"
                                                                                                </Button>
                                                                                            }.into_any()
                                                                                        }}
                                                                                    </div>
                                                                                </TableCell>
                                                                            </TableRow>
                                                                        }
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </TableBody>
                                                        </Table>
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                    }}
                                </div>

                                <div class="rounded-md border border-border/70 bg-muted/10 p-3 space-y-2">
                                    <p class="text-sm font-medium">"Source panel"</p>
                                    {move || {
                                        let selected_file_id = selected_source_file_id.get();
                                        let selected_page = selected_source_page.get();
                                        let selected_row = selected_source_row_id.get();
                                        let rows = match files.get() {
                                            LoadingState::Loaded(rows) => rows,
                                            _ => Vec::new(),
                                        };
                                        if let Some(file_id) = selected_file_id {
                                            if let Some(file) = rows.into_iter().find(|f| f.file_id == file_id) {
                                                let mut source_url = source_panel_client
                                                    .dataset_file_content_url(&dataset_id.get(), &file.file_id, true);
                                                if let Some(page) = selected_page {
                                                    source_url = format!("{}#page={}", source_url, page.max(1));
                                                }
                                                return if is_pdf_file(&file) {
                                                    view! {
                                                        <div class="space-y-2">
                                                            <p class="text-xs text-muted-foreground font-mono">{file.file_name.clone()}</p>
                                                            {selected_row.map(|row_id| view! {
                                                                <p class="text-xs text-muted-foreground">"From row: "{row_id}</p>
                                                            })}
                                                            <iframe
                                                                src=source_url
                                                                class="w-full h-[420px] rounded-md border border-border/70 bg-background"
                                                            ></iframe>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="space-y-2">
                                                            <p class="text-xs text-muted-foreground font-mono">{file.file_name}</p>
                                                            <p class="text-xs text-muted-foreground">
                                                                "Selected source is not a PDF. Use the file viewer for text/binary inspection."
                                                            </p>
                                                        </div>
                                                    }.into_any()
                                                };
                                            }
                                        }
                                        view! {
                                            <p class="text-xs text-muted-foreground">
                                                "Pick a row and open its source span to jump here."
                                            </p>
                                        }.into_any()
                                    }}
                                </div>
                            </div>

                            <div class="rounded-md border border-border/70 bg-muted/10 p-3 space-y-2">
                                <p class="text-sm font-medium">"Version compare"</p>
                                {move || match version_detail.get() {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! { <p class="text-xs text-muted-foreground">"Loading compare..."</p> }.into_any()
                                    }
                                    LoadingState::Error(_) => {
                                        view! { <p class="text-xs text-muted-foreground">"Compare unavailable."</p> }.into_any()
                                    }
                                    LoadingState::Loaded(detail) => {
                                        if let Some(compare) = detail.compare {
                                            if compare.changed_rows.is_empty() {
                                                view! { <p class="text-xs text-muted-foreground">"No changed rows between selected versions."</p> }.into_any()
                                            } else {
                                                view! {
                                                    <div class="space-y-2">
                                                        <p class="text-xs text-muted-foreground">
                                                            {format!(
                                                                "{} changed rows (showing up to {}).",
                                                                compare.total_changed_rows,
                                                                compare.changed_rows.len()
                                                            )}
                                                        </p>
                                                        <Table>
                                                            <TableHeader>
                                                                <TableRow>
                                                                    <TableHead>"Row"</TableHead>
                                                                    <TableHead>"Change"</TableHead>
                                                                    <TableHead>"Fields"</TableHead>
                                                                    <TableHead>"Provenance impact"</TableHead>
                                                                </TableRow>
                                                            </TableHeader>
                                                            <TableBody>
                                                                {compare
                                                                    .changed_rows
                                                                    .into_iter()
                                                                    .map(|change| view! {
                                                                        <TableRow>
                                                                            <TableCell>
                                                                                <span class="font-mono text-xs">{change.row_id}</span>
                                                                            </TableCell>
                                                                            <TableCell>{change.change_type}</TableCell>
                                                                            <TableCell>{if change.changed_fields.is_empty() {
                                                                                "-".to_string()
                                                                            } else {
                                                                                change.changed_fields.join(", ")
                                                                            }}</TableCell>
                                                                            <TableCell>{change.provenance_impact}</TableCell>
                                                                        </TableRow>
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </TableBody>
                                                        </Table>
                                                    </div>
                                                }.into_any()
                                            }
                                        } else {
                                            view! { <p class="text-xs text-muted-foreground">"Select a compare version to view changed rows and fields."</p> }.into_any()
                                        }
                                    }
                                }}
                            </div>
                        </div>
                    </Card>
                </div>

                <div class="space-y-4">
                    <Card title="Manage dataset".to_string()>
                        <div class="space-y-4">
                            <Button
                                button_type=ButtonType::Button
                                variant=ButtonVariant::Secondary
                                disabled=Signal::derive(move || validating.get())
                                loading=Signal::derive(move || validating.get())
                                on_click=validate_action
                            >
                                "Validate Dataset"
                            </Button>

                            <Button
                                button_type=ButtonType::Button
                                variant=ButtonVariant::Secondary
                                disabled=Signal::derive(move || preprocessing.get())
                                loading=Signal::derive(move || preprocessing.get())
                                on_click=preprocess_action
                            >
                                "Preprocess"
                            </Button>

                            {move || preprocess_status.get().map(|status| view! {
                                <div class="rounded-md border border-border/70 bg-muted/20 p-3 text-xs space-y-1">
                                    <p>
                                        <span class="text-muted-foreground">"Status: "</span>
                                        <span title=status.status.clone()>
                                            {status_display_label(&status.status)}
                                        </span>
                                    </p>
                                    <p>
                                        <span class="text-muted-foreground">"Processed: "</span>
                                        {status.lines_processed.to_string()}
                                    </p>
                                    <p>
                                        <span class="text-muted-foreground">"Removed: "</span>
                                        {status.lines_removed.to_string()}
                                    </p>
                                    {status.error_message.clone().map(|err| view! {
                                        <p class="text-destructive">{err}</p>
                                    })}
                                </div>
                            })}

                            <div class="border-t pt-3 space-y-2">
                                <p class="text-sm font-medium">"Trust override"</p>
                                <Select
                                    value=override_version_id
                                    options=versions_for_override.get()
                                />
                                <Select
                                    value=override_state
                                    options=vec![
                                        ("allowed".to_string(), "Allowed".to_string()),
                                        (
                                            "allowed_with_warning".to_string(),
                                            "Allowed With Warning".to_string(),
                                        ),
                                        ("needs_approval".to_string(), "Needs Approval".to_string()),
                                        ("blocked".to_string(), "Blocked".to_string()),
                                    ]
                                />
                                <Input
                                    value=override_reason
                                    placeholder="Reason (required)".to_string()
                                />
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Secondary
                                    disabled=Signal::derive(move || {
                                        applying_override.get()
                                            || override_reason.get().trim().is_empty()
                                            || override_version_id.get().trim().is_empty()
                                    })
                                    loading=Signal::derive(move || applying_override.get())
                                    on_click=apply_override_action
                                >
                                    "Apply Trust Override"
                                </Button>
                            </div>

                            <div class="border-t pt-3 space-y-2">
                                <a href=move || train_href.get()>
                                    <Button button_type=ButtonType::Button variant=ButtonVariant::Primary>
                                        "Train from this dataset"
                                    </Button>
                                </a>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Destructive
                                    disabled=Signal::derive(move || deleting.get())
                                    on_click=Callback::new(move |_| delete_dialog_open.set(true))
                                >
                                    "Delete Dataset"
                                </Button>
                            </div>
                        </div>
                    </Card>

                    <Card title="Safety and trust".to_string()>
                        {move || match dataset.get() {
                            LoadingState::Loaded(data) => {
                                let trust = data
                                    .trust_state
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string());
                                let validation = data
                                    .validation_status
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string());
                                view! {
                                    <div class="space-y-2 text-sm">
                                        <p>
                                            <span class="text-muted-foreground">"Trust: "</span>
                                            <span title=trust.clone()>{status_display_with_raw(&trust)}</span>
                                        </p>
                                        <p>
                                            <span class="text-muted-foreground">"Validation: "</span>
                                            <span title=validation.clone()>{status_display_with_raw(&validation)}</span>
                                        </p>
                                        <p>
                                            <span class="text-muted-foreground">"Dataset status: "</span>
                                            <span title=data.status.clone()>{status_display_with_raw(&data.status)}</span>
                                        </p>
                                    </div>
                                }.into_any()
                            }
                            LoadingState::Idle | LoadingState::Loading => {
                                view! { <p class="text-sm text-muted-foreground">"Loading trust details..."</p> }.into_any()
                            }
                            LoadingState::Error(_) => {
                                view! { <p class="text-sm text-muted-foreground">"Trust details unavailable."</p> }.into_any()
                            }
                        }}
                    </Card>

                    <Card title="Evaluation".to_string()>
                        <div class="space-y-3">
                            <div class="flex items-center justify-between gap-2">
                                <p class="text-xs text-muted-foreground">
                                    "Coverage, duplication, leakage risk, and schema anomalies for the selected version."
                                </p>
                                <Button
                                    button_type=ButtonType::Button
                                    variant=ButtonVariant::Ghost
                                    disabled=Signal::derive(move || regenerating_evaluation.get())
                                    loading=Signal::derive(move || regenerating_evaluation.get())
                                    on_click=regenerate_evaluation
                                >
                                    "Regenerate"
                                </Button>
                            </div>
                            {move || match version_detail.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! { <p class="text-xs text-muted-foreground">"Loading evaluation..."</p> }.into_any()
                                }
                                LoadingState::Error(_) => {
                                    view! { <p class="text-xs text-muted-foreground">"Evaluation unavailable."</p> }.into_any()
                                }
                                LoadingState::Loaded(detail) => {
                                    if let Some(evaluation) = detail.evaluation {
                                        let generated_at = evaluation.generated_at.clone();
                                        let generator_version = evaluation.generator_version.clone();
                                        let artifact_id = evaluation.artifact_id.clone();
                                        let coverage = evaluation.coverage_stats;
                                        let duplication = evaluation.duplication_stats;
                                        let leakage = evaluation.leakage_risk;
                                        let anomalies = evaluation.schema_anomalies;
                                        let citations = evaluation.citations;
                                        let examples = evaluation.example_rows;
                                        view! {
                                            <div class="space-y-3 text-sm">
                                                <div class="rounded-md border border-border/70 bg-muted/10 p-3 space-y-1">
                                                    <p class="text-xs text-muted-foreground">
                                                        {format!("Generated {}", format_datetime(&generated_at))}
                                                    </p>
                                                    <p class="text-xs text-muted-foreground">
                                                        {format!("Generator {}", generator_version)}
                                                    </p>
                                                    <p class="text-xs text-muted-foreground font-mono break-all">
                                                        {format!("Artifact {}", artifact_id)}
                                                    </p>
                                                </div>

                                                <div class="grid grid-cols-2 gap-2 text-xs">
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"Rows"</p>
                                                        <p class="font-medium">{coverage.total_rows}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"With response"</p>
                                                        <p class="font-medium">{coverage.rows_with_response}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"With source span"</p>
                                                        <p class="font-medium">{coverage.rows_with_source_span}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"With provenance metadata"</p>
                                                        <p class="font-medium">{coverage.rows_with_provenance_metadata}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"Duplicate pairs"</p>
                                                        <p class="font-medium">{duplication.duplicate_prompt_response_pairs}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2">
                                                        <p class="text-muted-foreground">"Duplicate prompts"</p>
                                                        <p class="font-medium">{duplication.duplicate_prompt_only}</p>
                                                    </div>
                                                    <div class="rounded-md border border-border/60 p-2 col-span-2">
                                                        <p class="text-muted-foreground">"Leakage risk"</p>
                                                        <p class="font-medium">
                                                            {format!(
                                                                "{:.2}% ({} rows)",
                                                                leakage.risk_score * 100.0,
                                                                leakage.risky_row_count
                                                            )}
                                                        </p>
                                                    </div>
                                                </div>

                                                <div class="space-y-1">
                                                    <p class="text-xs font-medium">"Schema anomalies"</p>
                                                    {if anomalies.is_empty() {
                                                        view! {
                                                            <p class="text-xs text-muted-foreground">"None detected."</p>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="space-y-1">
                                                                {anomalies
                                                                    .into_iter()
                                                                    .take(5)
                                                                    .map(|anomaly| view! {
                                                                        <div class="rounded-md border border-border/60 p-2 text-xs">
                                                                            <p class="font-medium">{anomaly.issue}</p>
                                                                            <p class="text-muted-foreground font-mono">{anomaly.row_id}</p>
                                                                        </div>
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        }.into_any()
                                                    }}
                                                </div>

                                                <div class="space-y-1">
                                                    <p class="text-xs font-medium">"Example rows"</p>
                                                    {if examples.is_empty() {
                                                        view! { <p class="text-xs text-muted-foreground">"No examples available."</p> }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="space-y-1">
                                                                {examples
                                                                    .into_iter()
                                                                    .take(3)
                                                                    .map(|row| view! {
                                                                        <div class="rounded-md border border-border/60 p-2 text-xs">
                                                                            <p class="font-mono text-muted-foreground">{row.row_id}</p>
                                                                            <p class="mt-1"><span class="text-muted-foreground">"Prompt: "</span>{shorten_text(&row.prompt, 120)}</p>
                                                                            <p class="mt-1"><span class="text-muted-foreground">"Response: "</span>{shorten_text(&row.response, 120)}</p>
                                                                        </div>
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        }.into_any()
                                                    }}
                                                </div>

                                                <div class="space-y-1">
                                                    <p class="text-xs font-medium">"Citations"</p>
                                                    {if citations.is_empty() {
                                                        view! { <p class="text-xs text-muted-foreground">"No citations available."</p> }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="space-y-1">
                                                                {citations
                                                                    .into_iter()
                                                                    .take(5)
                                                                    .map(|citation| view! {
                                                                        <div class="rounded-md border border-border/60 p-2 text-xs">
                                                                            <p class="font-medium">{citation.issue}</p>
                                                                            <p class="font-mono text-muted-foreground">{citation.row_id}</p>
                                                                        </div>
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        }.into_any()
                                                    }}
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <p class="text-xs text-muted-foreground">"No evaluation artifact yet."</p> }.into_any()
                                    }
                                }
                            }}
                        </div>
                    </Card>
                </div>
            </div>

            <ConfirmationDialog
                open=delete_dialog_open
                title="Delete dataset".to_string()
                description="This permanently removes the dataset and related files.".to_string()
                severity=ConfirmationSeverity::Destructive
                confirm_text="Delete".to_string()
                typed_confirmation=dataset_id.get()
                on_confirm=delete_action
                on_cancel=Callback::new(move |_| delete_dialog_open.set(false))
                loading=Signal::derive(move || deleting.get())
            />
        </PageScaffold>
    }
}

fn is_text_mime(mime: Option<&str>) -> bool {
    let m = mime.unwrap_or("").trim().to_ascii_lowercase();
    m.starts_with("text/")
        || m == "application/json"
        || m == "application/jsonl"
        || m == "application/x-ndjson"
}

fn is_pdf_file(file: &DatasetFileResponse) -> bool {
    if let Some(mime) = file.mime_type.as_deref() {
        if mime.eq_ignore_ascii_case("application/pdf") {
            return true;
        }
    }
    file.file_name.to_ascii_lowercase().trim().ends_with(".pdf")
}

fn metadata_string(row: &CanonicalRow, key: &str) -> Option<String> {
    row.metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn metadata_i32(row: &CanonicalRow, key: &str) -> Option<i32> {
    row.metadata.get(key).and_then(|raw| match raw {
        serde_json::Value::Number(num) => num.as_i64().and_then(|n| i32::try_from(n).ok()),
        serde_json::Value::String(text) => text.trim().parse::<i32>().ok(),
        _ => None,
    })
}

fn row_source_file_hint(row: &CanonicalRow) -> Option<String> {
    metadata_string(row, "source_file")
        .or_else(|| metadata_string(row, "source_document_name"))
        .or_else(|| metadata_string(row, "file_path"))
}

fn row_source_page_start(row: &CanonicalRow) -> Option<i32> {
    metadata_i32(row, "source_page_number").or_else(|| metadata_i32(row, "page_start"))
}

fn row_source_span_label(row: &CanonicalRow) -> String {
    let page_start = row_source_page_start(row);
    let page_end = metadata_i32(row, "source_page_end").or(page_start);
    let char_start =
        metadata_i32(row, "source_start_offset").or_else(|| metadata_i32(row, "char_start"));
    let char_end = metadata_i32(row, "source_end_offset").or_else(|| metadata_i32(row, "char_end"));

    let mut parts = Vec::new();
    if let Some(file) = row_source_file_hint(row) {
        parts.push(file);
    }
    if let Some(page) = page_start {
        if page_end.unwrap_or(page) > page {
            parts.push(format!("p{}-{}", page, page_end.unwrap_or(page)));
        } else {
            parts.push(format!("p{}", page));
        }
    }
    if let Some(start) = char_start {
        if let Some(end) = char_end {
            parts.push(format!("chars {}-{}", start, end));
        } else {
            parts.push(format!("char {}", start));
        }
    }

    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(" · ")
    }
}

fn shorten_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = String::new();
    for ch in value.chars().take(max_chars) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[derive(Clone, Debug)]
struct FileContentDisplay {
    raw: String,
    readable: String,
    has_readable_transform: bool,
}

fn has_file_extension(file_name: &str, extensions: &[&str]) -> bool {
    let normalized = file_name.to_ascii_lowercase();
    extensions.iter().any(|ext| normalized.ends_with(ext))
}

fn is_json_mime(mime: Option<&str>, file_name: &str) -> bool {
    let normalized = mime.unwrap_or("").trim().to_ascii_lowercase();
    normalized == "application/json"
        || normalized.ends_with("+json")
        || has_file_extension(file_name, &[".json"])
}

fn is_jsonl_mime(mime: Option<&str>, file_name: &str) -> bool {
    let normalized = mime.unwrap_or("").trim().to_ascii_lowercase();
    normalized == "application/jsonl"
        || normalized == "application/x-ndjson"
        || normalized == "application/jsonlines"
        || has_file_extension(file_name, &[".jsonl", ".ndjson"])
}

fn format_json_content(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

fn format_jsonl_content(raw: &str) -> Option<String> {
    let mut chunks = Vec::new();
    let mut row_number = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        row_number += 1;
        let parsed: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let pretty = serde_json::to_string_pretty(&parsed).ok()?;
        chunks.push(format!("Row {}:\n{}", row_number, pretty));
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join("\n\n"))
    }
}

fn build_file_content_display(
    raw: &str,
    mime: Option<&str>,
    file_name: &str,
) -> FileContentDisplay {
    if is_jsonl_mime(mime, file_name) {
        if let Some(readable) = format_jsonl_content(raw) {
            return FileContentDisplay {
                raw: raw.to_string(),
                readable,
                has_readable_transform: true,
            };
        }
    }

    if is_json_mime(mime, file_name) {
        if let Some(readable) = format_json_content(raw) {
            return FileContentDisplay {
                raw: raw.to_string(),
                readable,
                has_readable_transform: true,
            };
        }
    }

    FileContentDisplay {
        raw: raw.to_string(),
        readable: raw.to_string(),
        has_readable_transform: false,
    }
}

fn resolve_source_file_for_row(
    row: &CanonicalRow,
    files: &[DatasetFileResponse],
) -> Option<DatasetFileResponse> {
    let hint = row_source_file_hint(row)?;
    let needle = hint.to_ascii_lowercase();

    files
        .iter()
        .find(|file| {
            let name = file.file_name.to_ascii_lowercase();
            name == needle
                || name.ends_with(&needle)
                || needle.ends_with(&name)
                || name.contains(&needle)
                || needle.contains(&name)
        })
        .cloned()
}

#[component]
fn DatasetFilesTable(
    dataset_id: String,
    client: std::sync::Arc<ApiClient>,
    files: ReadSignal<LoadingState<Vec<DatasetFileResponse>>>,
    refetch: Refetch,
) -> impl IntoView {
    let view_dialog_open = RwSignal::new(false);
    let view_file = RwSignal::new(None::<DatasetFileResponse>);
    let view_content = RwSignal::new(None::<Result<FileContentDisplay, ApiError>>);
    let view_loading = RwSignal::new(false);
    let view_human_readable = RwSignal::new(true);

    let open_view = Callback::new({
        let client = client.clone();
        let dataset_id = dataset_id.clone();
        move |file: DatasetFileResponse| {
            view_file.set(Some(file.clone()));
            view_dialog_open.set(true);
            view_content.set(None);
            view_loading.set(true);
            view_human_readable.set(true);

            let client = client.clone();
            let dataset_id = dataset_id.clone();
            let file_id = file.file_id.clone();
            let file_mime = file.mime_type.clone();
            let file_name = file.file_name.clone();
            let content_signal = view_content;
            let loading_signal = view_loading;
            wasm_bindgen_futures::spawn_local(async move {
                let result = client
                    .get_dataset_file_content(&dataset_id, &file_id)
                    .await
                    .map(|raw| build_file_content_display(&raw, file_mime.as_deref(), &file_name));
                content_signal.set(Some(result));
                loading_signal.set(false);
            });
        }
    });

    view! {
        {move || match files.get() {
            LoadingState::Idle | LoadingState::Loading => {
                view! { <p class="text-sm text-muted-foreground">"Loading files..."</p> }.into_any()
            }
            LoadingState::Error(e) => {
                view! { <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch.run(())) /> }
                    .into_any()
            }
            LoadingState::Loaded(rows) => {
                if rows.is_empty() {
                    view! { <p class="text-sm text-muted-foreground">"No files found."</p> }
                        .into_any()
                } else {
                    view! {
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>"File"</TableHead>
                                    <TableHead>"Type"</TableHead>
                                    <TableHead>"Size"</TableHead>
                                    <TableHead>"Created"</TableHead>
                                    <TableHead>"Actions"</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows
                                    .into_iter()
                                    .map(|file| {
                                        let file_clone = file.clone();
                                        let can_view = is_text_mime(file.mime_type.as_deref());
                                        view! {
                                            <TableRow>
                                                <TableCell>{file.file_name}</TableCell>
                                                <TableCell>
                                                    {file.mime_type.unwrap_or_else(|| "-".to_string())}
                                                </TableCell>
                                                <TableCell>{format_bytes(file.size_bytes)}</TableCell>
                                                <TableCell>
                                                    <span class="text-xs text-muted-foreground">
                                                        {format_datetime(&file.created_at)}
                                                    </span>
                                                </TableCell>
                                                <TableCell>
                                                    {if can_view {
                                                        view! {
                                                            <Button
                                                                button_type=ButtonType::Button
                                                                variant=ButtonVariant::Ghost
                                                                on_click=Callback::new(move |_| open_view.run(file_clone.clone()))
                                                            >
                                                                "View"
                                                            </Button>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="text-xs text-muted-foreground" title="Binary file – download to view">
                                                                "Download"
                                                            </span>
                                                        }.into_any()
                                                    }}
                                                </TableCell>
                                            </TableRow>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </TableBody>
                        </Table>
                    }
                    .into_any()
                }
            }
        }}

        <Dialog
            open=view_dialog_open
            title="File content".to_string()
            size=DialogSize::Xl
            scrollable=true
        >
            {move || {
                let filename = view_file.get().map(|f| f.file_name.clone()).unwrap_or_default();
                if view_loading.get() {
                    return view! { <p class="text-sm text-muted-foreground">"Loading..."</p> }.into_any();
                }
                match view_content.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"Loading..."</p> }.into_any(),
                    Some(Err(e)) => view! {
                        <p class="text-sm text-destructive">
                            {format!("Failed to load content: {}", e.user_message())}
                        </p>
                        <p class="text-xs text-muted-foreground mt-2">
                            "Binary files cannot be displayed. Use the download link from the dataset API."
                        </p>
                    }.into_any(),
                    Some(Ok(content)) => view! {
                        <div class="space-y-2">
                            {if !filename.is_empty() {
                                view! {
                                    <p class="text-xs text-muted-foreground font-mono">{filename}</p>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                            {if content.has_readable_transform {
                                view! {
                                    <div class="flex items-center justify-between gap-2">
                                        <p class="text-xs text-muted-foreground">
                                            {move || {
                                                if view_human_readable.get() {
                                                    "Human-readable view".to_string()
                                                } else {
                                                    "Raw file view".to_string()
                                                }
                                            }}
                                        </p>
                                        <div class="inline-flex items-center rounded-md border border-border/70 bg-muted/10 p-0.5">
                                            <button
                                                type="button"
                                                class=move || {
                                                    if view_human_readable.get() {
                                                        "rounded px-2 py-1 text-xs bg-background text-foreground".to_string()
                                                    } else {
                                                        "rounded px-2 py-1 text-xs text-muted-foreground hover:text-foreground".to_string()
                                                    }
                                                }
                                                aria-pressed=move || view_human_readable.get().to_string()
                                                on:click=move |_| view_human_readable.set(true)
                                            >
                                                "Readable"
                                            </button>
                                            <button
                                                type="button"
                                                class=move || {
                                                    if view_human_readable.get() {
                                                        "rounded px-2 py-1 text-xs text-muted-foreground hover:text-foreground".to_string()
                                                    } else {
                                                        "rounded px-2 py-1 text-xs bg-background text-foreground".to_string()
                                                    }
                                                }
                                                aria-pressed=move || (!view_human_readable.get()).to_string()
                                                on:click=move |_| view_human_readable.set(false)
                                            >
                                                "Raw"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                            <pre class="text-xs overflow-x-auto whitespace-pre-wrap break-words max-h-[70vh] bg-muted/20 p-3 rounded-md">
                                {move || if view_human_readable.get() { content.readable.clone() } else { content.raw.clone() }}
                            </pre>
                        </div>
                    }.into_any(),
                }
            }}
        </Dialog>
    }
}

#[component]
#[allow(dead_code)]
fn DatasetPreviewPanel(
    preview: ReadSignal<LoadingState<DatasetPreviewResponse>>,
    refetch: Refetch,
) -> impl IntoView {
    view! {
        {move || match preview.get() {
            LoadingState::Idle | LoadingState::Loading => {
                view! { <p class="text-sm text-muted-foreground">"Loading preview..."</p> }.into_any()
            }
            LoadingState::Error(e) => {
                view! { <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch.run(())) /> }
                    .into_any()
            }
            LoadingState::Loaded(data) => {
                if data.examples.is_empty() {
                    view! {
                        <EmptyState
                            variant=EmptyStateVariant::Empty
                            title="No preview examples"
                            description="Preview endpoint returned no examples for this dataset."
                        />
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="space-y-2">
                            <p class="text-xs text-muted-foreground">
                                {format!("Showing {} of {} examples", data.examples.len(), data.total_examples)}
                            </p>
                            {data
                                .examples
                                .into_iter()
                                .enumerate()
                                .map(|(idx, example)| {
                                    let pretty = serde_json::to_string_pretty(&example)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    view! {
                                        <div class="rounded-md border border-border/70 bg-muted/20 p-3">
                                            <p class="text-xs text-muted-foreground mb-2">{format!("Example {}", idx + 1)}</p>
                                            <pre class="text-xs overflow-x-auto whitespace-pre-wrap">{pretty}</pre>
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </div>
                    }
                    .into_any()
                }
            }
        }}
    }
}
