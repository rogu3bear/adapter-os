//! Documents management page
//!
//! Provides document list, detail, and management functionality.

use crate::api::client::{
    ChunkListResponse, DatasetResponse as ApiDatasetResponse, DocumentListParams, DocumentResponse,
};
use crate::api::{report_error_with_toast, ApiClient};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonLink, ButtonSize, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, CopyableId, DocumentUploadDialog, EmptyState, EmptyStateVariant,
    ErrorDisplay, IconExternalLink, InlineProgress, LoadingDisplay, PageBreadcrumbItem,
    PageScaffold, PageScaffoldActions, PageScaffoldPrimaryAction, ProgressStage, ProgressStages,
    RefreshButton, Select, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{
    use_api, use_api_resource, use_conditional_polling, use_delete_dialog, use_system_status,
    LoadingState,
};
use crate::signals::{try_use_route_context, SelectedEntity};
use crate::utils::{
    chat_path_with_adapter, format_bytes, format_datetime, format_relative_time,
    status_display_label, status_display_with_raw,
};
use adapteros_api_types::{
    CreateTrainingJobRequest, ModelLoadStatus, TrainingConfigRequest, TrainingListParams,
    TRAINING_DATA_CONTRACT_VERSION,
};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde_json::Value;
use std::sync::Arc;

use adapteros_api_types::StatusIndicator as ApiStatusIndicator;

/// Get badge variant based on document status
fn status_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "indexed" | "ready" => BadgeVariant::Success,
        "processing" | "uploaded" | "chunked" | "embedded" => BadgeVariant::Warning,
        "failed" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

/// Compute progress stage state from document status.
///
/// Returns (current_stage, completed_stages, error_stages).
fn document_processing_state(status: &str) -> (Option<String>, Vec<String>, Vec<String>) {
    let stage_order = [
        "uploaded",
        "processing",
        "chunked",
        "embedded",
        "indexed",
        "ready",
    ];

    if status == "failed" {
        return (
            None,
            vec!["uploaded".to_string()],
            vec!["processing".to_string()],
        );
    }

    let position = match status {
        "uploaded" => 0,
        "processing" => 1,
        "chunked" => 2,
        "embedded" => 3,
        "indexed" | "ready" => stage_order.len(),
        _ => 0,
    };

    let completed: Vec<String> = stage_order[..position.min(stage_order.len())]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let current = if position < stage_order.len() {
        Some(stage_order[position].to_string())
    } else {
        None
    };

    (current, completed, vec![])
}

fn training_route_for_document(doc_id: &str) -> String {
    format!("/training?source=document&document_id={}", doc_id)
}

const TALK_STAGE_SOURCE_STORED: &str = "source_stored";
const TALK_STAGE_PARSED: &str = "parsed";
const TALK_STAGE_DATASET_READY: &str = "dataset_ready";
const TALK_STAGE_TRAINING_RUNNING: &str = "training_running";
const TALK_STAGE_ADAPTER_READY: &str = "adapter_ready";
const TALK_STAGE_CHAT_READY: &str = "chat_ready";

const TALK_FLOW_POLL_MS: u32 = 2500;
const TALK_FLOW_MAX_POLLS: usize = 120;
const TALK_FLOW_MAX_DATASET_SCAN: usize = 24;
const TALK_FLOW_PREVIEW_LIMIT: usize = 3;

#[derive(Clone, Debug)]
struct DocumentTalkFlowState {
    document_status: String,
    dataset_id: Option<String>,
    training_job_id: Option<String>,
    training_status: Option<String>,
    adapter_id: Option<String>,
    waiting_reason: Option<String>,
    detail_message: Option<String>,
    chat_ready: bool,
}

impl DocumentTalkFlowState {
    fn idle(document_status: &str) -> Self {
        Self {
            document_status: document_status.to_string(),
            dataset_id: None,
            training_job_id: None,
            training_status: None,
            adapter_id: None,
            waiting_reason: None,
            detail_message: None,
            chat_ready: false,
        }
    }
}

fn is_document_parsed(status: &str) -> bool {
    matches!(status, "indexed" | "ready")
}

fn is_training_active(status: &str) -> bool {
    matches!(status, "running" | "pending" | "queued")
}

fn is_training_terminal_error(status: &str) -> bool {
    matches!(status, "failed" | "cancelled")
}

fn talk_flow_wait_reason_for_document(status: &str) -> &'static str {
    match status {
        "uploaded" => "Document uploaded. Waiting for parsing to start.",
        "processing" | "chunked" | "embedded" => "Parsing in progress.",
        "indexed" | "ready" => "Parsing complete.",
        "failed" => "Parsing failed. Retry this document first.",
        _ => "Waiting for document parsing.",
    }
}

fn talk_flow_wait_reason_for_training(status: &str) -> &'static str {
    match status {
        "pending" | "queued" => "Training is queued.",
        "running" => "Training is running.",
        "completed" => "Training finished. Finalizing adapter.",
        "failed" => "Training failed.",
        "cancelled" => "Training was cancelled.",
        _ => "Checking training status.",
    }
}

fn talk_flow_stages() -> Vec<ProgressStage> {
    vec![
        ProgressStage::new(TALK_STAGE_SOURCE_STORED, "Source stored"),
        ProgressStage::new(TALK_STAGE_PARSED, "Parsed"),
        ProgressStage::new(TALK_STAGE_DATASET_READY, "Dataset ready"),
        ProgressStage::new(TALK_STAGE_TRAINING_RUNNING, "Training running"),
        ProgressStage::new(TALK_STAGE_ADAPTER_READY, "Adapter ready"),
        ProgressStage::new(TALK_STAGE_CHAT_READY, "Chat ready"),
    ]
}

fn talk_flow_stage_state(
    flow: &DocumentTalkFlowState,
) -> (Option<String>, Vec<String>, Vec<String>) {
    let mut completed = vec![TALK_STAGE_SOURCE_STORED.to_string()];
    let mut errors = vec![];
    let mut current: Option<String> = Some(TALK_STAGE_PARSED.to_string());

    if flow.document_status == "failed" {
        errors.push(TALK_STAGE_PARSED.to_string());
        current = None;
    } else if is_document_parsed(&flow.document_status) {
        completed.push(TALK_STAGE_PARSED.to_string());
        current = Some(TALK_STAGE_DATASET_READY.to_string());
    }

    if flow.dataset_id.is_some() {
        completed.push(TALK_STAGE_DATASET_READY.to_string());
        current = Some(TALK_STAGE_TRAINING_RUNNING.to_string());
    }

    if let Some(status) = flow.training_status.as_deref() {
        if is_training_active(status) || status == "completed" {
            completed.push(TALK_STAGE_TRAINING_RUNNING.to_string());
        } else if is_training_terminal_error(status) {
            errors.push(TALK_STAGE_TRAINING_RUNNING.to_string());
            current = None;
        }
    } else if flow.training_job_id.is_some() {
        current = Some(TALK_STAGE_TRAINING_RUNNING.to_string());
    }

    if flow.adapter_id.is_some() {
        completed.push(TALK_STAGE_ADAPTER_READY.to_string());
        current = Some(TALK_STAGE_CHAT_READY.to_string());
    }

    if flow.chat_ready {
        completed.push(TALK_STAGE_CHAT_READY.to_string());
        current = None;
    }

    (current, completed, errors)
}

fn preview_contains_document(preview_row: &Value, document_id: &str) -> bool {
    let Some(metadata) = preview_row
        .get("metadata")
        .and_then(|value| value.as_object())
    else {
        return false;
    };

    let source_id = metadata
        .get("source_document_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let fallback_id = metadata
        .get("document_id")
        .and_then(Value::as_str)
        .unwrap_or_default();

    source_id == document_id || fallback_id == document_id
}

async fn find_lineage_dataset_for_document(
    client: &ApiClient,
    document_id: &str,
) -> Result<Option<ApiDatasetResponse>, crate::api::ApiError> {
    let mut datasets = client.list_datasets(None).await?.datasets;
    datasets.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    for dataset in datasets.into_iter().take(TALK_FLOW_MAX_DATASET_SCAN) {
        let preview = match client
            .preview_dataset(&dataset.id, Some(TALK_FLOW_PREVIEW_LIMIT))
            .await
        {
            Ok(value) => value,
            Err(_) => continue,
        };

        if preview
            .examples
            .iter()
            .any(|row| preview_contains_document(row, document_id))
        {
            return Ok(Some(dataset));
        }
    }

    Ok(None)
}

async fn select_base_model_id(client: &ApiClient) -> Result<Option<String>, crate::api::ApiError> {
    let models = client.list_models_status().await?;

    if let Some(ready) = models
        .models
        .iter()
        .find(|m| m.status == ModelLoadStatus::Ready || m.is_loaded)
    {
        return Ok(Some(ready.model_id.clone()));
    }

    Ok(models.models.first().map(|model| model.model_id.clone()))
}

fn build_document_training_request(
    dataset: &ApiDatasetResponse,
    base_model_id: String,
    document_name: &str,
) -> CreateTrainingJobRequest {
    let params = TrainingConfigRequest {
        rank: 8,
        alpha: 16,
        targets: vec!["q_proj".to_string(), "v_proj".to_string()],
        training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
        pad_token_id: 0,
        ignore_index: -100,
        epochs: 10,
        learning_rate: 0.0001,
        batch_size: 4,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        validation_split: Some(0.15),
        preferred_backend: None,
        backend_policy: None,
        coreml_training_fallback: None,
        #[cfg(not(target_arch = "wasm32"))]
        coreml_placement: None,
        enable_coreml_export: None,
        require_gpu: None,
        max_gpu_memory_mb: None,
        #[cfg(not(target_arch = "wasm32"))]
        base_model_path: None,
        #[cfg(not(target_arch = "wasm32"))]
        preprocessing: None,
        force_resume: None,
        multi_module_training: None,
        lora_layer_indices: None,
        early_stopping: Some(true),
        patience: None,
        min_delta: None,
    };

    CreateTrainingJobRequest {
        workspace_id: String::new(),
        base_model_id,
        dataset_id: dataset.id.clone(),
        dataset_version_id: dataset.dataset_version_id.clone(),
        adapter_name: None,
        params,
        lora_tier: None,
        template_id: None,
        repo_id: None,
        description: Some(format!("Created from document '{}'.", document_name)),
        adapter_type: None,
        category: Some("docs".to_string()),
    }
}

async fn run_document_talk_flow(
    client: Arc<ApiClient>,
    document: DocumentResponse,
    flow_state: WriteSignal<DocumentTalkFlowState>,
    refetch_trigger: WriteSignal<u32>,
) -> Result<String, String> {
    let document_id = document.document_id.clone();
    let mut current_document = document;
    let mut flow = DocumentTalkFlowState::idle(&current_document.status);
    flow.waiting_reason = Some("Checking document status.".to_string());
    flow.detail_message = Some("Starting document-to-chat handoff.".to_string());
    flow_state.set(flow.clone());

    let mut parse_polls = 0usize;
    while !is_document_parsed(&current_document.status) {
        flow.document_status = current_document.status.clone();
        flow.waiting_reason =
            Some(talk_flow_wait_reason_for_document(&current_document.status).to_string());
        flow_state.set(flow.clone());

        if current_document.status == "failed" {
            let message = current_document
                .error_message
                .clone()
                .unwrap_or_else(|| "Parsing failed. Retry this document first.".to_string());
            return Err(message);
        }

        if parse_polls >= TALK_FLOW_MAX_POLLS {
            return Err("This document is still parsing. Try again in a moment.".to_string());
        }

        TimeoutFuture::new(TALK_FLOW_POLL_MS).await;
        current_document = client
            .get_document(&document_id)
            .await
            .map_err(|e| format!("Unable to refresh document status: {}", e.user_message()))?;
        let _ = refetch_trigger.try_update(|value| *value += 1);
        parse_polls += 1;
    }

    flow.document_status = current_document.status.clone();
    flow.waiting_reason = Some("Checking existing dataset lineage.".to_string());
    flow.detail_message = Some("Looking for an existing dataset from this document.".to_string());
    flow_state.set(flow.clone());

    let dataset = if let Some(existing) = find_lineage_dataset_for_document(&client, &document_id)
        .await
        .map_err(|e| format!("Unable to check existing datasets: {}", e.user_message()))?
    {
        flow.dataset_id = Some(existing.id.clone());
        flow.waiting_reason = Some("Reusing an existing dataset for this document.".to_string());
        flow.detail_message = Some("Lineage match found in existing datasets.".to_string());
        flow_state.set(flow.clone());
        existing
    } else {
        flow.waiting_reason = Some("Creating dataset from this document.".to_string());
        flow.detail_message = Some("No lineage match found, creating a new dataset.".to_string());
        flow_state.set(flow.clone());

        let created = client
            .create_dataset_from_documents(vec![document_id.clone()], None)
            .await
            .map_err(|e| format!("Unable to create dataset: {}", e.user_message()))?;
        flow.dataset_id = Some(created.id.clone());
        flow.waiting_reason = Some("Dataset is ready.".to_string());
        flow.detail_message = Some("Dataset created from current document.".to_string());
        flow_state.set(flow.clone());
        created
    };

    flow.waiting_reason = Some("Checking for an existing adapter.".to_string());
    flow.detail_message = Some("Looking for adapters already linked to this dataset.".to_string());
    flow_state.set(flow.clone());

    if let Ok(lineage) = client.get_dataset_adapters(&dataset.id).await {
        if let Some(adapter_id) = lineage
            .adapters
            .into_iter()
            .find_map(|entry| (!entry.adapter_id.trim().is_empty()).then_some(entry.adapter_id))
        {
            flow.adapter_id = Some(adapter_id.clone());
            flow.waiting_reason = Some("Adapter found. Opening chat.".to_string());
            flow.detail_message = Some("Using existing adapter lineage.".to_string());
            flow.chat_ready = true;
            flow_state.set(flow);
            return Ok(chat_path_with_adapter(&adapter_id));
        }
    }

    flow.waiting_reason = Some("Checking existing training jobs.".to_string());
    flow.detail_message = Some("Searching for a reusable build for this dataset.".to_string());
    flow_state.set(flow.clone());

    let mut jobs = client
        .list_training_jobs(Some(&TrainingListParams {
            status: None,
            page: Some(1),
            page_size: Some(100),
            adapter_name: None,
            template_id: None,
            dataset_id: Some(dataset.id.clone()),
        }))
        .await
        .map_err(|e| format!("Unable to check training jobs: {}", e.user_message()))?
        .jobs;
    jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    let mut completed_with_adapter = None;
    let mut active_job = None;
    let mut completed_without_adapter = None;

    for job in jobs {
        let has_adapter = job
            .adapter_id
            .as_ref()
            .is_some_and(|id| !id.trim().is_empty());

        if completed_with_adapter.is_none() && job.status == "completed" && has_adapter {
            completed_with_adapter = Some(job.clone());
        }
        if active_job.is_none() && is_training_active(&job.status) {
            active_job = Some(job.clone());
        }
        if completed_without_adapter.is_none() && job.status == "completed" && !has_adapter {
            completed_without_adapter = Some(job.clone());
        }
    }

    if let Some(existing) = completed_with_adapter {
        if let Some(adapter_id) = existing.adapter_id {
            flow.training_job_id = Some(existing.id);
            flow.training_status = Some("completed".to_string());
            flow.adapter_id = Some(adapter_id.clone());
            flow.waiting_reason = Some("Adapter ready. Opening chat.".to_string());
            flow.detail_message = Some("Reusing completed training lineage.".to_string());
            flow.chat_ready = true;
            flow_state.set(flow);
            return Ok(chat_path_with_adapter(&adapter_id));
        }
    }

    let mut job = if let Some(existing) = active_job.or(completed_without_adapter) {
        flow.training_job_id = Some(existing.id.clone());
        flow.training_status = Some(existing.status.clone());
        flow.waiting_reason =
            Some(talk_flow_wait_reason_for_training(&existing.status).to_string());
        flow.detail_message = Some("Reusing an existing training job.".to_string());
        flow_state.set(flow.clone());
        existing
    } else {
        flow.waiting_reason = Some("Starting training with sensible defaults.".to_string());
        flow.detail_message = Some("No existing training job found for this dataset.".to_string());
        flow_state.set(flow.clone());

        let base_model_id = select_base_model_id(&client)
            .await
            .map_err(|e| format!("Unable to resolve a training model: {}", e.user_message()))?
            .ok_or_else(|| {
                "No training model is available yet. Load a model, then try again.".to_string()
            })?;

        let request =
            build_document_training_request(&dataset, base_model_id, &current_document.name);
        let created = client
            .create_training_job(&request)
            .await
            .map_err(|e| format!("Unable to start training: {}", e.user_message()))?;
        flow.training_job_id = Some(created.id.clone());
        flow.training_status = Some(created.status.clone());
        flow.detail_message = Some("Training job created for this document lineage.".to_string());
        flow.waiting_reason = Some(talk_flow_wait_reason_for_training(&created.status).to_string());
        flow_state.set(flow.clone());
        created
    };

    let mut training_polls = 0usize;
    loop {
        flow.training_job_id = Some(job.id.clone());
        flow.training_status = Some(job.status.clone());
        flow.waiting_reason = Some(talk_flow_wait_reason_for_training(&job.status).to_string());
        flow_state.set(flow.clone());

        if let Some(adapter_id) = job.adapter_id.clone().filter(|id| !id.trim().is_empty()) {
            flow.adapter_id = Some(adapter_id.clone());
            flow.waiting_reason = Some("Adapter ready. Opening chat.".to_string());
            flow.detail_message = Some("Training lineage is ready for chat handoff.".to_string());
            flow.chat_ready = true;
            flow_state.set(flow);
            return Ok(chat_path_with_adapter(&adapter_id));
        }

        if is_training_terminal_error(&job.status) {
            let message = job.error_message.unwrap_or_else(|| {
                "Training stopped before an adapter was ready. Open training job details."
                    .to_string()
            });
            return Err(message);
        }

        if training_polls >= TALK_FLOW_MAX_POLLS {
            return Err(
                "Training is still running. Open training job details to keep watching progress."
                    .to_string(),
            );
        }

        TimeoutFuture::new(TALK_FLOW_POLL_MS).await;
        job = client
            .get_training_job(&job.id)
            .await
            .map_err(|e| format!("Unable to refresh training status: {}", e.user_message()))?;
        training_polls += 1;
    }
}

#[derive(Clone, Debug, Default)]
struct DocumentStatusCounts {
    indexed: u64,
    processing: u64,
    failed: u64,
}

/// Documents list page
#[component]
pub fn Documents() -> impl IntoView {
    let _client = use_api();

    // Filter state - use RwSignal<String> for Select component
    let status_filter = RwSignal::new(String::new());
    let (current_page, set_current_page) = signal(1u32);
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);
    let show_upload_dialog = RwSignal::new(false);
    let navigate = use_navigate();
    let navigate_upload = navigate.clone();

    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    let (status_counts, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let _trigger = refetch_trigger.try_get().unwrap_or_default();
        async move {
            let base_params = |status: Option<String>| DocumentListParams {
                status,
                page: Some(1),
                limit: Some(1),
            };

            let indexed = client
                .list_documents(Some(&base_params(Some("indexed".to_string()))))
                .await?
                .total;
            let processing = client
                .list_documents(Some(&base_params(Some("processing".to_string()))))
                .await?
                .total;
            let failed = client
                .list_documents(Some(&base_params(Some("failed".to_string()))))
                .await?
                .total;

            Ok(DocumentStatusCounts {
                indexed,
                processing,
                failed,
            })
        }
    });

    let (documents, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let status_val = status_filter.get();
        let status = if status_val.is_empty() {
            None
        } else {
            Some(status_val)
        };
        let page = current_page.get();
        let _trigger = refetch_trigger.get();
        async move {
            let params = DocumentListParams {
                status,
                page: Some(page),
                limit: Some(20),
            };
            client.list_documents(Some(&params)).await
        }
    });

    // Refetch and reset page on filter change
    Effect::new(move || {
        let _ = status_filter.get();
        let _ = set_current_page.try_set(1);
        let _ = set_refetch_trigger.try_update(|t| *t += 1);
    });

    view! {
        <PageScaffold
            title="Documents"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Training", "/documents"),
                PageBreadcrumbItem::current("Documents"),
            ]
            full_width=true
        >
            <PageScaffoldPrimaryAction slot>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_upload_dialog.set(true))
                >
                    "Upload Document"
                </Button>
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Select
                    value=status_filter
                    options=vec![
                        ("".to_string(), "All Statuses".to_string()),
                        ("indexed".to_string(), "Ready/Indexed".to_string()),
                        ("processing".to_string(), "Processing".to_string()),
                        ("failed".to_string(), "Failed".to_string()),
                    ]
                    class="w-40".to_string()
                />
                <Button
                    variant=ButtonVariant::Ghost
                    size=ButtonSize::Sm
                    on_click=Callback::new({
                        let navigate = navigate.clone();
                        move |_| navigate("/training", Default::default())
                    })
                >
                    "Go to Training"
                </Button>
                <RefreshButton
                    on_click=Callback::new(move |_| refetch())
                />
            </PageScaffoldActions>

            {move || {
                match documents.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading documents..."/>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let total_pages = data.pages;
                        let current = current_page.try_get().unwrap_or(1);
                        view! {
                            // Pipeline summary (click to filter)
                            <div class="rounded-lg border bg-card p-3">
                                <div class="flex flex-wrap items-center gap-2">
                                    {move || {
                                        let active = status_filter.try_get().unwrap_or_default();
                                        let button = |label: &'static str,
                                                      count: Option<u64>,
                                                      value: &'static str,
                                                      badge_variant: BadgeVariant| {
                                            let is_active = !value.is_empty() && active == value;
                                            view! {
                                                <Button
                                                    variant=if is_active { ButtonVariant::Secondary } else { ButtonVariant::Ghost }
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new(
                                                        move |_| status_filter.set(value.to_string())
                                                    )
                                                >
                                                    <span class="flex items-center gap-2">
                                                        <span class="text-sm">{label}</span>
                                                        <Badge variant=badge_variant>
                                                            {count.map(|c| c.to_string()).unwrap_or_else(|| "…".to_string())}
                                                        </Badge>
                                                    </span>
                                                </Button>
                                            }
                                        };

                                        match status_counts.try_get().unwrap_or(LoadingState::Idle) {
                                            LoadingState::Loaded(counts) => view! {
                                                {button("Ready/Indexed", Some(counts.indexed), "indexed", BadgeVariant::Success)}
                                                {button("Processing", Some(counts.processing), "processing", BadgeVariant::Warning)}
                                                {button("Failed", Some(counts.failed), "failed", BadgeVariant::Destructive)}
                                            }.into_any(),
                                            _ => view! {
                                                {button("Ready/Indexed", None, "indexed", BadgeVariant::Success)}
                                                {button("Processing", None, "processing", BadgeVariant::Warning)}
                                                {button("Failed", None, "failed", BadgeVariant::Destructive)}
                                            }.into_any(),
                                        }
                                    }}
                                </div>
                            </div>

                            <DocumentsList
                                documents=data.data.clone()
                                on_upload=Callback::new(move |_| show_upload_dialog.set(true))
                                on_refetch=Callback::new(move |_| set_refetch_trigger.update(|t| *t += 1))
                            />

                            // Pagination
                            {if total_pages > 1 {
                                view! {
                                    <div class="flex items-center justify-center gap-2 mt-6">
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Sm
                                            disabled=Signal::derive(move || current_page.try_get().unwrap_or(1) <= 1)
                                            on_click=Callback::new(move |_| set_current_page.update(|p| *p = p.saturating_sub(1).max(1)))
                                        >
                                            "Previous"
                                        </Button>
                                        <span class="text-sm text-muted-foreground">
                                            {format!("Page {} of {}", current, total_pages)}
                                        </span>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Sm
                                            disabled=Signal::derive(move || current_page.try_get().unwrap_or(1) >= total_pages)
                                            on_click=Callback::new(move |_| set_current_page.update(|p| *p = (*p + 1).min(total_pages)))
                                        >
                                            "Next"
                                        </Button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch())
                            />
                        }.into_any()
                    }
                }
            }}

            <DocumentUploadDialog
                open=show_upload_dialog
                on_success=Callback::new(move |doc_id| {
                    refetch();
                    navigate_upload(&format!("/documents/{}", doc_id), Default::default());
                })
            />
        </PageScaffold>
    }
}

#[component]
fn DocumentsList(
    documents: Vec<DocumentResponse>,
    on_upload: Callback<()>,
    on_refetch: Callback<()>,
) -> impl IntoView {
    if documents.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No documents found"
                    description="Upload documents to begin indexing for RAG."
                    action_label="Upload Document"
                    on_action=Callback::new(move |_| on_upload.run(()))
                />
            </Card>
        }
        .into_any();
    }

    let client = use_api();
    let delete_state = use_delete_dialog();
    let reprocessing_id = RwSignal::new(Option::<String>::None);

    let (system_status, _) = use_system_status();
    let system_not_ready = Memo::new(move |_| {
        !matches!(
            system_status.get(),
            LoadingState::Loaded(ref s) if matches!(s.readiness.overall, ApiStatusIndicator::Ready)
        )
    });

    let delete_state_for_cancel = delete_state;
    let on_cancel_delete = Callback::new(move |_| {
        delete_state_for_cancel.cancel();
    });

    let delete_state_for_confirm = delete_state;
    let on_confirm_delete = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            if let Some(id) = delete_state_for_confirm.get_pending_id() {
                delete_state_for_confirm.start_delete();
                let client = Arc::clone(&client);
                let delete_state = delete_state_for_confirm;
                wasm_bindgen_futures::spawn_local(async move {
                    match client.delete_document(&id).await {
                        Ok(_) => {
                            // delete_state uses its own internal signals; these methods are safe
                            delete_state.finish_delete(Ok(()));
                            on_refetch.run(());
                        }
                        Err(e) => {
                            delete_state.finish_delete(Err(format!("Delete failed: {}", e)));
                        }
                    }
                });
            }
        })
    };

    let delete_state_for_rows = delete_state;
    let delete_state_for_loading = delete_state;

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Chunks"</TableHead>
                        <TableHead>"Type"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {documents
                        .into_iter()
                        .map(|doc| {
                            let id = doc.document_id.clone();
                            let id_link = id.clone();
                            let id_reprocess = id.clone();
                            let id_delete = id.clone();
                            let name = doc.name.clone();
                            let name_for_delete = name.clone();
                            let status = doc.status.clone();
                            let status_label = status_display_label(&status);
                            let status_variant = status_badge_variant(&status);
                            let size = format_bytes(doc.size_bytes);
                            let chunks = doc.chunk_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                            let mime = doc.mime_type.clone();
                            let created = format_relative_time(&doc.created_at);
                            let error = doc.error_message.clone();
                            let delete_state = delete_state_for_rows;
                            let client = Arc::clone(&client);
                            let is_terminal_ready = matches!(status.as_str(), "indexed" | "ready");
                            let is_failed = status == "failed";
                            let is_in_flight = !is_terminal_ready && !is_failed;

                            view! {
                                <TableRow>
                                    <TableCell>
                                        <a
                                            href=format!("/documents/{}", id_link)
                                            class="font-medium hover:underline"
                                        >
                                            {name}
                                        </a>
                                    </TableCell>
                                    <TableCell>
                                        <div class="space-y-1">
                                            <span title=status.clone()>
                                                <Badge variant=status_variant>
                                                    {status_label}
                                                </Badge>
                                            </span>
                                            {error
                                                .clone()
                                                .filter(|err| !err.is_empty())
                                                .map(|err| {
                                                    let err_title = err.clone();
                                                    view! {
                                                        <div class="text-xs text-destructive line-clamp-1" title=err_title>
                                                            {err}
                                                        </div>
                                                    }
                                                })}
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground">{size}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground">{chunks}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-xs text-muted-foreground font-mono">{mime}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{created}</span>
                                    </TableCell>
                                    <TableCell class="text-right">
                                        <div class="flex items-center justify-end gap-1.5">
                                            {is_in_flight.then(|| {
                                                view! {
                                                    <InlineProgress
                                                        label=Signal::derive(|| "Processing".to_string())
                                                    />
                                                }
                                            })}
                                            {is_terminal_ready.then(|| {
                                                let doc_id_for_train = id.clone();
                                                view! {
                                                    <ButtonLink
                                                        href=format!(
                                                            "/documents/{}#train-adapter-cta",
                                                            doc_id_for_train
                                                        )
                                                        variant=ButtonVariant::Ghost
                                                        size=ButtonSize::Sm
                                                        aria_label="Train using this document"
                                                    >
                                                        "Train"
                                                    </ButtonLink>
                                                }
                                            })}
                                            {(!is_in_flight).then(|| {
                                                let aria = if is_failed {
                                                    "Retry document"
                                                } else {
                                                    "Reprocess document"
                                                };
                                                let label = if is_failed { "Retry" } else { "Reprocess" };
                                                view! {
                                                    <Button
                                                        variant=ButtonVariant::Ghost
                                                        size=ButtonSize::Sm
                                                        aria_label=aria
                                                        disabled=Signal::derive({
                                                            let id = id_reprocess.clone();
                                                            move || {
                                                                reprocessing_id.try_get().flatten().as_deref() == Some(id.as_str())
                                                                    || system_not_ready.get()
                                                            }
                                                        })
                                                        on_click=Callback::new({
                                                            let client = Arc::clone(&client);
                                                            let id = id_reprocess.clone();
                                                            move |_| {
                                                                let client = Arc::clone(&client);
                                                                let id = id.clone();
                                                                reprocessing_id.set(Some(id.clone()));
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    if is_failed {
                                                                        if let Err(e) = client.retry_document(&id).await {
                                                                            report_error_with_toast(&e, "Failed to retry document", Some("/documents"), true);
                                                                        }
                                                                    } else if let Err(e) = client.process_document(&id).await {
                                                                        report_error_with_toast(&e, "Failed to reprocess document", Some("/documents"), true);
                                                                    }
                                                                    let _ = reprocessing_id.try_set(None);
                                                                    on_refetch.run(());
                                                                });
                                                            }
                                                        })
                                                    >
                                                        <svg
                                                            xmlns="http://www.w3.org/2000/svg"
                                                            class="h-4 w-4"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                        >
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                                                        </svg>
                                                        <span class="ml-1">{label}</span>
                                                    </Button>
                                                }
                                            })}
                                            {is_failed.then(|| {
                                                let error_href = "/runs".to_string();
                                                view! {
                                                    <a
                                                        href=error_href
                                                        class="inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-accent text-muted-foreground"
                                                        title="Open incidents/errors"
                                                        aria-label="Open incidents/errors"
                                                    >
                                                        <IconExternalLink class="h-4 w-4".to_string() aria_label="".to_string() />
                                                    </a>
                                                }
                                            })}
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                size=ButtonSize::Sm
                                                aria_label="Delete document"
                                                on_click=Callback::new({
                                                    let delete_state = delete_state;
                                                    move |_| {
                                                        delete_state.confirm(id_delete.clone(), name_for_delete.clone());
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
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                                </svg>
                                            </Button>
                                        </div>
                                    </TableCell>
                                </TableRow>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
        <ConfirmationDialog
            open=delete_state.show
            title="Delete Document"
            description="Are you sure you want to delete this document and all associated chunks? This action cannot be undone."
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            cancel_text="Cancel"
            on_confirm=on_confirm_delete
            on_cancel=on_cancel_delete
            loading=Signal::derive(move || delete_state_for_loading.deleting.try_get().unwrap_or(false))
        />
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn training_route_points_to_production_flow() {
        let route = training_route_for_document("doc_123");
        assert_eq!(route, "/training?source=document&document_id=doc_123");
        let forbidden = format!("/testkit/{}_{}", "create_training_job", "stub");
        assert!(!route.contains(&forbidden));
    }
}

/// Document detail page
#[component]
pub fn DocumentDetail() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();

    // Get document ID from URL
    let document_id = Memo::new(move |_| {
        params
            .try_get()
            .unwrap_or_default()
            .get("id")
            .unwrap_or_default()
    });

    // Refetch trigger
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);
    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    // Fetch document details
    let (document, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = document_id.try_get().unwrap_or_default();
        let _trigger = refetch_trigger.try_get().unwrap_or_default();
        async move { client.get_document(&id).await }
    });

    // Poll while the document is mid-pipeline so the "stages" UI advances during demos.
    let should_poll = Signal::derive(
        move || matches!(document.try_get().unwrap_or(LoadingState::Idle), LoadingState::Loaded(ref doc) if !matches!(doc.status.as_str(), "indexed" | "ready" | "failed")),
    );
    let _ = use_conditional_polling(2000, should_poll, move || async move {
        set_refetch_trigger.update(|t| *t += 1);
    });

    // Fetch document chunks
    let (chunks, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = document_id.try_get().unwrap_or_default();
        let _trigger = refetch_trigger.try_get().unwrap_or_default();
        async move { client.get_document_chunks(&id).await }
    });

    // Action states
    let (deleting, set_deleting) = signal(false);
    let (processing, set_processing) = signal(false);
    let (action_error, set_action_error) = signal(Option::<String>::None);

    // Publish document selection to RouteContext for contextual actions in Command Palette
    {
        Effect::new(move || {
            if let Some(route_ctx) = try_use_route_context() {
                if let Some(LoadingState::Loaded(doc)) = document.try_get() {
                    route_ctx.set_selected(SelectedEntity::with_status(
                        "document",
                        doc.document_id.clone(),
                        doc.name.clone(),
                        doc.status.clone(),
                    ));
                }
            }
        });
    }

    view! {
        <PageScaffold
            title="Document Details"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Training", "/documents"),
                PageBreadcrumbItem::new("Documents", "/documents"),
                PageBreadcrumbItem::current(document_id.try_get().unwrap_or_default()),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                <RefreshButton
                    on_click=Callback::new(move |_| refetch())
                />
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| {
                        // UI-only: synthesis from an already-uploaded document requires
                        // either re-upload or a dedicated backend endpoint. We route the user
                        // into the training flow with the document preselected.
                        let doc_id = document_id.try_get().unwrap_or_default();
                        navigate(&training_route_for_document(&doc_id), Default::default());
                    })
                >
                    "Create synthesized dataset"
                </Button>
            </PageScaffoldActions>

            // Action error message
            {move || action_error.try_get().flatten().map(|err| view! {
                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                    <p class="text-destructive">{err}</p>
                </div>
            })}

            {move || {
                match document.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading document..."/>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let chunks_data = match chunks.try_get().unwrap_or(LoadingState::Idle) {
                            LoadingState::Loaded(c) => Some(c),
                            _ => None,
                        };
                        view! {
                            <DocumentDetailContent
                                document=data
                                chunks=chunks_data
                                deleting=deleting
                                set_deleting=set_deleting
                                processing=processing
                                set_processing=set_processing
                                set_action_error=set_action_error
                                refetch_trigger=set_refetch_trigger
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) if e.is_not_found() => {
                        view! {
                            <div class="flex min-h-[40vh] flex-col items-center justify-center px-4">
                                <Card class="p-8 max-w-md w-full text-center">
                                    <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                    <h2 class="heading-3 mb-2">"Document not found"</h2>
                                    <p class="text-muted-foreground mb-6">
                                        "This document may have been deleted or doesn't exist."
                                    </p>
                                    <ButtonLink
                                        href="/documents"
                                        variant=ButtonVariant::Primary
                                        size=ButtonSize::Md
                                    >
                                        "View all documents"
                                    </ButtonLink>
                                </Card>
                            </div>
                        }
                            .into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch())
                            />
                        }
                            .into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}

#[component]
fn DocumentDetailContent(
    document: DocumentResponse,
    chunks: Option<ChunkListResponse>,
    deleting: ReadSignal<bool>,
    set_deleting: WriteSignal<bool>,
    processing: ReadSignal<bool>,
    set_processing: WriteSignal<bool>,
    set_action_error: WriteSignal<Option<String>>,
    refetch_trigger: WriteSignal<u32>,
) -> AnyView {
    let client = use_api();

    let (system_status, _) = use_system_status();
    let system_not_ready = Memo::new(move |_| {
        !matches!(
            system_status.get(),
            LoadingState::Loaded(ref s) if matches!(s.readiness.overall, ApiStatusIndicator::Ready)
        )
    });

    let navigate = use_navigate();
    let status_variant = status_badge_variant(&document.status);
    let status_label = status_display_label(&document.status);
    let doc_id = document.document_id.clone();
    let doc_id_for_delete = doc_id.clone();
    let doc_id_for_process = doc_id.clone();
    let doc_id_for_retry = doc_id.clone();

    // Delete confirmation dialog state
    let show_delete_dialog = RwSignal::new(false);
    let doc_name_for_confirm = document.name.clone();

    // Open delete confirmation dialog
    let open_delete_dialog = move |_| {
        show_delete_dialog.set(true);
    };

    // Handle cancel/close of delete dialog
    let on_cancel_delete = Callback::new(move |_| {
        // Reset any error state when dialog is dismissed
        set_action_error.set(None);
    });

    // Delete action (called from confirmation dialog)
    let delete_action = {
        let doc_id_for_delete = doc_id_for_delete.clone();
        let navigate = navigate.clone();
        let client = Arc::clone(&client);
        Callback::new(move |_: ()| {
            let client = Arc::clone(&client);
            let id = doc_id_for_delete.clone();
            let navigate = navigate.clone();
            set_deleting.set(true);
            set_action_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_document(&id).await {
                    Ok(_) => {
                        let _ = set_deleting.try_set(false);
                        let _ = show_delete_dialog.try_set(false);
                        navigate("/documents", Default::default());
                    }
                    Err(e) => {
                        let _ = set_action_error.try_set(Some(format!("Delete failed: {}", e)));
                        let _ = set_deleting.try_set(false);
                    }
                }
            });
        })
    };

    // Process action (for reprocessing)
    let process_action = {
        let client = Arc::clone(&client);
        move |_| {
            let client = Arc::clone(&client);
            let id = doc_id_for_process.clone();
            set_processing.set(true);
            set_action_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match client.process_document(&id).await {
                    Ok(_) => {
                        let _ = set_processing.try_set(false);
                        let _ = refetch_trigger.try_update(|t| *t += 1);
                    }
                    Err(e) => {
                        let _ = set_action_error.try_set(Some(format!("Process failed: {}", e)));
                        let _ = set_processing.try_set(false);
                    }
                }
            });
        }
    };

    // Retry action (for failed documents)
    let retry_action = {
        let client = Arc::clone(&client);
        move |_| {
            let client = Arc::clone(&client);
            let id = doc_id_for_retry.clone();
            set_processing.set(true);
            set_action_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match client.retry_document(&id).await {
                    Ok(_) => {
                        let _ = set_processing.try_set(false);
                        let _ = refetch_trigger.try_update(|t| *t += 1);
                    }
                    Err(e) => {
                        let _ = set_action_error.try_set(Some(format!("Retry failed: {}", e)));
                        let _ = set_processing.try_set(false);
                    }
                }
            });
        }
    };

    let talk_flow_state = RwSignal::new(DocumentTalkFlowState::idle(&document.status));
    let talk_in_progress = RwSignal::new(false);

    let talk_current_stage =
        Signal::derive(move || talk_flow_stage_state(&talk_flow_state.get()).0);
    let talk_completed_stages =
        Signal::derive(move || talk_flow_stage_state(&talk_flow_state.get()).1);
    let talk_error_stages = Signal::derive(move || talk_flow_stage_state(&talk_flow_state.get()).2);

    let talk_to_this_action = {
        let client = Arc::clone(&client);
        let navigate = navigate.clone();
        let document_for_flow = document.clone();
        Callback::new(move |_| {
            if talk_in_progress.get_untracked() {
                return;
            }

            talk_in_progress.set(true);
            set_action_error.set(None);
            let _ = talk_flow_state.try_set(DocumentTalkFlowState::idle(&document_for_flow.status));

            let client = Arc::clone(&client);
            let navigate = navigate.clone();
            let document = document_for_flow.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match run_document_talk_flow(
                    client,
                    document,
                    talk_flow_state.write_only(),
                    refetch_trigger,
                )
                .await
                {
                    Ok(path) => {
                        navigate(&path, Default::default());
                    }
                    Err(message) => {
                        let _ = set_action_error.try_set(Some(message));
                    }
                }
                let _ = talk_in_progress.try_set(false);
            });
        })
    };

    let is_failed = document.status == "failed";
    let is_indexed = matches!(document.status.as_str(), "indexed" | "ready");
    let status_for_stages = document.status.clone();
    let _issue_error_message = document.error_message.clone();
    let _issue_error_code = document.error_code.clone();
    let status_for_eligibility = document.status.clone();
    let eligible_chunks = {
        let from_doc = document.chunk_count.unwrap_or(0);
        let from_chunks = chunks.as_ref().map(|c| c.total_chunks).unwrap_or(0);
        from_chunks.max(from_doc)
    };
    let _is_eligible_for_training = is_indexed && eligible_chunks > 0;
    let _not_eligible_reason = match status_for_eligibility.as_str() {
        "failed" => "Document failed processing.",
        "processing" | "uploaded" | "chunked" | "embedded" => "Document is still processing.",
        "indexed" | "ready" => "No chunks available yet.",
        other => {
            // Keep the reason anchored to the backend status string.
            // This avoids inventing pipeline states not guaranteed by the API.
            return view! { <span>{format!("Status: {}", status_display_with_raw(other))}</span> }
                .into_any();
        }
    };

    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{document.name.clone()}</p>
                    </div>
                    <div>
                        <CopyableId
                            id=document.document_id.clone()
                            label="Document ID".to_string()
                            truncate=28
                        />
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Hash (BLAKE3)"</p>
                        <p class="font-mono text-sm truncate">{document.hash_b3.clone()}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Tenant"</p>
                        <p class="font-mono text-sm">{document.tenant_id.clone()}</p>
                    </div>
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center gap-2">
                        <span title=document.status.clone()>
                            <Badge variant=status_variant>
                                {status_label}
                            </Badge>
                        </span>
                        {document.deduplicated.then(|| view! {
                            <Badge variant=BadgeVariant::Secondary>
                                "Deduplicated"
                            </Badge>
                        })}
                    </div>

                    // Error info for failed documents
                    {document.error_message.clone().map(|err| view! {
                        <div class="rounded-lg border border-destructive bg-destructive/10 p-3 mt-3">
                            <p class="text-sm font-medium text-destructive">"Error"</p>
                            <p class="text-sm text-destructive/80 mt-1">{err}</p>
                            {document.error_code.clone().map(|code| view! {
                                <p class="text-xs text-destructive/60 mt-1 font-mono">"Code: "{code}</p>
                            })}
                        </div>
                    })}

                    // Retry info
                    {(document.retry_count > 0).then(|| view! {
                        <div class="text-sm text-muted-foreground">
                            "Retries: "{document.retry_count}" / "{document.max_retries}
                        </div>
                    })}

                    <div id="train-adapter-cta" class="pt-2 space-y-3">
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Sm
                            disabled=Signal::derive(move || {
                                talk_in_progress.get()
                                    || processing.try_get().unwrap_or(false)
                                    || deleting.try_get().unwrap_or(false)
                                    || system_not_ready.get()
                            })
                            on_click=talk_to_this_action
                        >
                            {move || if talk_in_progress.get() { "Working..." } else { "Talk to this" }}
                        </Button>

                        <p class="text-sm text-muted-foreground">
                            {move || {
                                talk_flow_state
                                    .try_get()
                                    .and_then(|state| state.waiting_reason)
                                    .unwrap_or_else(|| "Drop a document, then use this to land in chat with the right adapter.".to_string())
                            }}
                        </p>

                        <ProgressStages
                            stages=talk_flow_stages()
                            current_stage=talk_current_stage
                            completed_stages=talk_completed_stages
                            error_stages=talk_error_stages
                        />

                        <details class="rounded-md border p-3">
                            <summary class="cursor-pointer text-xs font-medium text-muted-foreground">
                                "Advanced controls"
                            </summary>
                            <div class="mt-3 space-y-2">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    size=ButtonSize::Sm
                                    disabled=Signal::derive(move || !is_indexed)
                                    on_click=Callback::new({
                                        let doc_id_for_train = doc_id.clone();
                                        let navigate = navigate.clone();
                                        move |_| {
                                            let route = training_route_for_document(&doc_id_for_train);
                                            navigate(&route, Default::default());
                                        }
                                    })
                                >
                                    "Train adapter manually"
                                </Button>
                                {(!is_indexed).then(|| view! {
                                    <p class="text-xs text-muted-foreground">
                                        "Manual training opens once parsing is complete."
                                    </p>
                                })}
                            </div>
                        </details>

                        <details class="rounded-md border p-3">
                            <summary class="cursor-pointer text-xs font-medium text-muted-foreground">
                                "Operator details"
                            </summary>
                            <div class="mt-3 space-y-1 text-xs text-muted-foreground font-mono">
                                <div>
                                    "document_status: "
                                    {move || talk_flow_state.get().document_status}
                                </div>
                                {move || talk_flow_state.get().dataset_id.map(|id| view! {
                                    <div>"dataset_id: "{id}</div>
                                })}
                                {move || talk_flow_state.get().training_job_id.map(|id| view! {
                                    <div>"training_job_id: "{id}</div>
                                })}
                                {move || talk_flow_state.get().training_status.map(|status| view! {
                                    <div>"training_status: "{status}</div>
                                })}
                                {move || talk_flow_state.get().adapter_id.map(|id| view! {
                                    <div>"adapter_id: "{id}</div>
                                })}
                                {move || talk_flow_state.get().detail_message.map(|detail| view! {
                                    <div class="font-sans text-muted-foreground">{detail}</div>
                                })}
                            </div>
                        </details>
                    </div>

                    // Recovery actions
                    <div class="pt-2 border-t">
                        <p class="text-xs font-medium text-muted-foreground mt-3">"Recovery actions"</p>
                        <div class="flex flex-wrap gap-2 mt-2">
                            {is_failed.then(|| {
                                view! {
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        size=ButtonSize::Sm
                                        disabled=Signal::derive(move || processing.try_get().unwrap_or(false) || system_not_ready.get())
                                        on_click=Callback::new(retry_action)
                                    >
                                        {move || if processing.try_get().unwrap_or(false) { "Retrying..." } else { "Retry" }}
                                    </Button>
                                }
                            })}
                            <Button
                                variant=ButtonVariant::Secondary
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || processing.try_get().unwrap_or(false) || system_not_ready.get())
                                on_click=Callback::new(process_action)
                            >
                                {move || if processing.try_get().unwrap_or(false) { "Processing..." } else { "Reprocess" }}
                            </Button>
                            <Button
                                variant=ButtonVariant::Destructive
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || deleting.try_get().unwrap_or(false))
                                on_click=Callback::new(open_delete_dialog)
                            >
                                {move || if deleting.try_get().unwrap_or(false) { "Deleting..." } else { "Delete" }}
                            </Button>
                            {is_failed.then(|| {
                                view! {
                                    <a href="/runs" class="text-sm text-primary hover:underline self-center">
                                        "View execution records"
                                    </a>
                                }
                            })}
                        </div>
                    </div>
                </div>
            </Card>

            // Delete confirmation dialog
            <ConfirmationDialog
                open=show_delete_dialog
                title="Delete Document"
                description=format!(
                    "This will permanently delete the document '{}' and all associated chunks. This action cannot be undone.",
                    doc_name_for_confirm
                )
                severity=ConfirmationSeverity::Destructive
                confirm_text="Delete"
                typed_confirmation=doc_name_for_confirm.clone()
                on_confirm=delete_action
                on_cancel=on_cancel_delete
                loading=Signal::derive(move || deleting.try_get().unwrap_or(false))
            />
        </div>

        // Processing stages (shown when not yet indexed)
        {(!is_indexed).then(|| {
            let stages = vec![
                ProgressStage::new("uploaded", "Uploaded"),
                ProgressStage::new("processing", "Processing"),
                ProgressStage::new("chunked", "Chunked"),
                ProgressStage::new("embedded", "Embedded"),
                ProgressStage::new("indexed", "Indexed"),
                ProgressStage::new("ready", "Ready"),
            ];
            let (current, completed, errors) = document_processing_state(&status_for_stages);
            let current_signal = Signal::derive({
                let current = current.clone();
                move || current.clone()
            });
            let completed_signal = Signal::derive({
                let completed = completed.clone();
                move || completed.clone()
            });
            let error_signal = Signal::derive({
                let errors = errors.clone();
                move || errors.clone()
            });
            view! {
                <Card title="Processing Progress".to_string() class="mt-6".to_string()>
                    <ProgressStages
                        stages=stages
                        current_stage=current_signal
                        completed_stages=completed_signal
                        error_stages=error_signal
                    />
                </Card>
            }
        })}


        // Document details and timestamps
        <Card title="Document Details".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Size"</p>
                    <p class="font-medium">{format_bytes(document.size_bytes)}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"MIME Type"</p>
                    <p class="font-mono text-sm">{document.mime_type.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Chunks"</p>
                    <p class="font-medium">{document.chunk_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Created"</p>
                    <p class="font-medium">{format_datetime(&document.created_at)}</p>
                </div>
            </div>
        </Card>

        // Chunks preview (if available)
        {chunks.map(|chunk_data| {
            if chunk_data.chunks.is_empty() {
                view! {
                    <Card title="Document Chunks".to_string() class="mt-6".to_string()>
                        <p class="text-muted-foreground">"No chunks available"</p>
                    </Card>
                }.into_any()
            } else {
                let total = chunk_data.total_chunks;
                let preview_chunks = chunk_data.chunks.into_iter().take(5).collect::<Vec<_>>();
                view! {
                    <Card title=format!("Document Chunks ({} total)", total) class="mt-6".to_string()>
                        <div class="space-y-4">
                            {preview_chunks.into_iter().map(|chunk| {
                                view! {
                                    <div class="rounded-lg border p-3">
                                        <div class="flex items-center justify-between mb-2">
                                            <span class="text-sm font-medium">"Chunk "{chunk.chunk_index + 1}</span>
                                            <span class="text-xs text-muted-foreground font-mono">{chunk.chunk_id.clone()}</span>
                                        </div>
                                        <p class="text-sm text-muted-foreground line-clamp-3">{chunk.text.clone()}</p>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                            {(total > 5).then(|| view! {
                                <p class="text-sm text-muted-foreground text-center">
                                    "Showing 5 of "{total}" chunks"
                                </p>
                            })}
                        </div>
                    </Card>
                }.into_any()
            }
        })}
    }
    .into_any()
}
