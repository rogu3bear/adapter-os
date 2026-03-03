//! Stepped wizard for creating training jobs.
//!
//! Goal-first 3-step flow:
//! 1. Build dataset
//! 2. Train adapter
//! 3. Continue to chat

use crate::api::error::format_structured_details;
use crate::api::{
    use_api_client, ApiClient, ApiError, CreateTrainingJobRequest, DatasetListResponse,
    DatasetPreviewResponse, DatasetResponse, DocumentListResponse, ModelListResponse,
    TrainingConfigRequest,
};
use crate::components::{
    AsyncBoundary, Button, ButtonType, ButtonVariant, Card, DialogSize, DocumentUploadDialog,
    FormField, Input, Select, StepFormDialog,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use crate::pages::training::config_presets::TrainingPreset;
use crate::pages::training::dataset_wizard::{DatasetOutcome, DatasetUploadWizard};
use crate::signals::use_notifications;
use crate::utils::status_display_label;
use crate::validation::{
    rules, use_field_error, use_form_state, validate_on_blur, FormState, ValidationRule,
};
use adapteros_api_types::{
    DatasetVersionSelection, TrainingBackendKind, TrainingBackendPolicy,
    TRAINING_DATA_CONTRACT_VERSION,
};
use leptos::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Wizard step enum
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WizardStep {
    /// Step 0: Build or select dataset
    #[default]
    BuildDataset,
    /// Step 1: Name + configure adapter
    Train,
    /// Step 2: Review and start; handoff to chat-ready path
    ContinueToChat,
}

impl WizardStep {
    fn index(&self) -> usize {
        match self {
            WizardStep::BuildDataset => 0,
            WizardStep::Train => 1,
            WizardStep::ContinueToChat => 2,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            WizardStep::BuildDataset => "Build dataset",
            WizardStep::Train => "Train adapter",
            WizardStep::ContinueToChat => "Continue to chat",
        }
    }

    fn next(&self) -> Option<WizardStep> {
        match self {
            WizardStep::BuildDataset => Some(WizardStep::Train),
            WizardStep::Train => Some(WizardStep::ContinueToChat),
            WizardStep::ContinueToChat => None,
        }
    }

    fn prev(&self) -> Option<WizardStep> {
        match self {
            WizardStep::BuildDataset => None,
            WizardStep::Train => Some(WizardStep::BuildDataset),
            WizardStep::ContinueToChat => Some(WizardStep::Train),
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Train,
            2 => Self::ContinueToChat,
            _ => Self::BuildDataset,
        }
    }
}

const STEPS: [WizardStep; 3] = [
    WizardStep::BuildDataset,
    WizardStep::Train,
    WizardStep::ContinueToChat,
];
#[cfg(target_arch = "wasm32")]
const TRAINING_WIZARD_DRAFT_KEY: &str = "adapteros_training_wizard_draft_v1";
const TRAINING_WIZARD_STALE_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AdvancedDatasetDraft {
    dataset_id: String,
    dataset_version_id: String,
    weight: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TrainingWizardDraft {
    updated_at_ms: f64,
    step_index: usize,
    adapter_name: String,
    skill_purpose: String,
    base_model_id: String,
    dataset_id: String,
    dataset_version_id: Option<String>,
    selected_dataset_id: String,
    category: String,
    training_preset: String,
    epochs: String,
    learning_rate: String,
    validation_split: String,
    early_stopping: bool,
    batch_size: String,
    rank: String,
    alpha: String,
    preferred_backend: String,
    backend_policy: String,
    coreml_training_fallback: String,
    show_train_advanced: bool,
    show_knowledge_advanced: bool,
    multi_dataset_enabled: bool,
    advanced_dataset_versions: Vec<AdvancedDatasetDraft>,
    source_repo_id: Option<String>,
    source_branch: Option<String>,
    source_version_id: Option<String>,
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
}

fn format_draft_age(age_ms: f64) -> String {
    let seconds = (age_ms / 1000.0).max(0.0) as u64;
    if seconds < 60 {
        "less than a minute".to_string()
    } else if seconds < 3600 {
        format!("{} minute(s)", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} hour(s)", seconds / 3600)
    } else {
        format!("{} day(s)", seconds / 86_400)
    }
}

#[cfg(target_arch = "wasm32")]
fn load_training_wizard_draft() -> Option<TrainingWizardDraft> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(TRAINING_WIZARD_DRAFT_KEY).ok().flatten())
        .and_then(|raw| serde_json::from_str::<TrainingWizardDraft>(&raw).ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_training_wizard_draft() -> Option<TrainingWizardDraft> {
    None
}

#[cfg(target_arch = "wasm32")]
fn save_training_wizard_draft(draft: &TrainingWizardDraft) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        if let Ok(json) = serde_json::to_string(draft) {
            let _ = storage.set_item(TRAINING_WIZARD_DRAFT_KEY, &json);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_training_wizard_draft(_draft: &TrainingWizardDraft) {}

#[cfg(target_arch = "wasm32")]
fn clear_training_wizard_draft() {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(TRAINING_WIZARD_DRAFT_KEY);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_training_wizard_draft() {}

#[cfg(target_arch = "wasm32")]
async fn wait_for_document_indexed(client: &ApiClient, document_id: &str) -> Result<(), String> {
    const MAX_POLLS: usize = 80;
    const POLL_DELAY_MS: u32 = 1500;

    for _ in 0..MAX_POLLS {
        match client.get_document(document_id).await {
            Ok(document) => match document.status.as_str() {
                "indexed" | "ready" => return Ok(()),
                "failed" | "error" => {
                    return Err(document.error_message.unwrap_or_else(|| {
                        "One of your files could not be prepared. Please try another file."
                            .to_string()
                    }))
                }
                _ => {}
            },
            Err(e) => return Err(e.user_message()),
        }
        gloo_timers::future::TimeoutFuture::new(POLL_DELAY_MS).await;
    }

    Err("The document is still preparing. Please try again in a moment.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn wait_for_document_indexed(_client: &ApiClient, _document_id: &str) -> Result<(), String> {
    Ok(())
}

fn map_training_submit_error(error: &ApiError) -> String {
    match error.code() {
        Some("DATASET_TRUST_BLOCKED")
        | Some("DATASET_TRUST_NEEDS_APPROVAL")
        | Some("VALIDATION_ERROR") => "The selected dataset needs a quick review before training.".to_string(),
        Some("LINEAGE_REQUIRED") => {
            "This training run needs dataset lineage context. Start from adapter version controls or select a dataset version.".to_string()
        }
        Some("DATASET_EMPTY") => {
            "The selected dataset has no trainable examples. Upload or generate data before starting training.".to_string()
        }
        Some("DATA_SPEC_HASH_MISMATCH") => {
            "Dataset metadata changed while preparing the run. Re-select the dataset to refresh version details.".to_string()
        }
        Some("TRAINING_CAPACITY_LIMIT")
        | Some("MEMORY_PRESSURE_CRITICAL")
        | Some("CAPACITY_CHECK_ERROR")
        | Some("BACKPRESSURE")
        | Some("MEMORY_PRESSURE")
        | Some("OUT_OF_MEMORY")
        | Some("SERVICE_UNAVAILABLE") => {
            "Training is busy. Try again in a few minutes.".to_string()
        }
        Some("WORKER_CAPABILITY_MISSING") => {
            "No training worker with GPU backward capability is available. Register an MLX-capable worker and try again."
                .to_string()
        }
        _ => format_structured_details(error),
    }
}

fn parse_backend_kind(value: &str) -> Option<TrainingBackendKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "coreml" => Some(TrainingBackendKind::CoreML),
        "mlx" => Some(TrainingBackendKind::Mlx),
        "metal" => Some(TrainingBackendKind::Metal),
        "cpu" => Some(TrainingBackendKind::Cpu),
        _ => None,
    }
}

fn parse_backend_policy(value: &str) -> Option<TrainingBackendPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "coreml_only" => Some(TrainingBackendPolicy::CoremlOnly),
        "coreml_else_fallback" => Some(TrainingBackendPolicy::CoremlElseFallback),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KnowledgeGateState {
    NeedsDatasetSelection,
    DatasetAcquisitionInProgress,
    DatasetMetadataLoading,
    DatasetMetadataUnavailable,
    DatasetNotReady,
    DatasetTrustNeedsApproval,
    DatasetTrustBlocked,
    DatasetTrustUnknown,
    DatasetValidationIncomplete,
    DatasetVersionUnavailable,
    DatasetEmpty,
    ReadyWithWarning,
    Ready,
}

impl KnowledgeGateState {
    fn is_ready(self) -> bool {
        matches!(self, Self::Ready | Self::ReadyWithWarning)
    }

    fn message(self) -> Option<&'static str> {
        match self {
            Self::NeedsDatasetSelection => Some("Choose or create a dataset first."),
            Self::DatasetAcquisitionInProgress => Some("Dataset preparation is in progress."),
            Self::DatasetMetadataLoading => Some("Dataset details are still loading."),
            Self::DatasetMetadataUnavailable => {
                Some("Dataset details could not be loaded. Re-select or create a dataset.")
            }
            Self::DatasetNotReady => Some("Dataset is still preparing."),
            Self::DatasetTrustNeedsApproval => {
                Some("Dataset trust requires approval before training.")
            }
            Self::DatasetTrustBlocked => Some("Dataset trust is blocked for training."),
            Self::DatasetTrustUnknown => Some("Dataset trust state is unknown."),
            Self::DatasetValidationIncomplete => Some("Dataset validation is not complete."),
            Self::DatasetVersionUnavailable => {
                Some("No dataset version could be resolved. Re-select the dataset.")
            }
            Self::DatasetEmpty => {
                Some("Dataset has zero examples and cannot be used for training.")
            }
            Self::ReadyWithWarning => Some("Dataset is train-eligible with warnings."),
            Self::Ready => None,
        }
    }

    fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::DatasetTrustNeedsApproval | Self::DatasetTrustBlocked | Self::DatasetTrustUnknown
        )
    }

    fn style_classes(self) -> (&'static str, &'static str) {
        if self.is_ready() {
            (
                "border border-status-success/50 bg-status-success/5",
                "text-status-success",
            )
        } else if self.is_destructive() {
            (
                "border border-destructive/50 bg-destructive/10",
                "text-destructive",
            )
        } else {
            (
                "border border-status-warning/50 bg-status-warning/10",
                "text-status-warning",
            )
        }
    }
}

fn normalize_gate_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn evaluate_knowledge_gate(
    dataset_id: &str,
    dataset_version_id: Option<&str>,
    dataset_sample_count: Option<usize>,
    dataset_info: Option<&DatasetResponse>,
    dataset_lookup_loading: bool,
    creating_document_dataset: bool,
    dataset_wizard_open: bool,
) -> KnowledgeGateState {
    if creating_document_dataset || dataset_wizard_open {
        return KnowledgeGateState::DatasetAcquisitionInProgress;
    }

    let selected_dataset_id = dataset_id.trim();
    if selected_dataset_id.is_empty() {
        return KnowledgeGateState::NeedsDatasetSelection;
    }

    if dataset_lookup_loading {
        return KnowledgeGateState::DatasetMetadataLoading;
    }

    let Some(dataset) = dataset_info else {
        return KnowledgeGateState::DatasetMetadataUnavailable;
    };

    if !dataset.id.eq_ignore_ascii_case(selected_dataset_id) {
        return KnowledgeGateState::DatasetMetadataLoading;
    }

    if normalize_gate_token(&dataset.status) != "ready" {
        return KnowledgeGateState::DatasetNotReady;
    }

    let trust_state = dataset
        .trust_state
        .as_deref()
        .map(normalize_gate_token)
        .unwrap_or_else(|| "unknown".to_string());

    let allow_with_warning = match trust_state.as_str() {
        "allowed" => false,
        "allowed_with_warning" => true,
        "needs_approval" => return KnowledgeGateState::DatasetTrustNeedsApproval,
        "blocked" => return KnowledgeGateState::DatasetTrustBlocked,
        _ => return KnowledgeGateState::DatasetTrustUnknown,
    };

    let validation_status = dataset
        .validation_status
        .as_deref()
        .map(normalize_gate_token)
        .unwrap_or_else(|| "unknown".to_string());

    if validation_status != "valid" {
        return KnowledgeGateState::DatasetValidationIncomplete;
    }

    if dataset_version_id.is_none_or(|id| id.trim().is_empty()) {
        return KnowledgeGateState::DatasetVersionUnavailable;
    }

    if dataset_sample_count == Some(0) {
        return KnowledgeGateState::DatasetEmpty;
    }

    if allow_with_warning {
        KnowledgeGateState::ReadyWithWarning
    } else {
        KnowledgeGateState::Ready
    }
}

/// Training job creation wizard
#[component]
pub fn CreateJobWizard(
    open: RwSignal<bool>,
    on_created: impl Fn(String) + Clone + Send + Sync + 'static,
    /// Optional initial dataset ID (e.g., from dataset detail page navigation)
    #[prop(optional)]
    initial_dataset_id: Option<RwSignal<Option<String>>>,
    /// Optional source document ID (e.g., from document-to-training workflow)
    #[prop(optional)]
    source_document_id: Option<RwSignal<Option<String>>>,
    /// Optional initial base model ID (from deep-link query params)
    #[prop(optional)]
    initial_base_model_id: Option<RwSignal<Option<String>>>,
    /// Optional initial preferred backend (from deep-link query params)
    #[prop(optional)]
    initial_preferred_backend: Option<RwSignal<Option<String>>>,
    /// Optional initial backend policy (from deep-link query params)
    #[prop(optional)]
    initial_backend_policy: Option<RwSignal<Option<String>>>,
    /// Optional initial epochs (from deep-link query params)
    #[prop(optional)]
    initial_epochs: Option<RwSignal<Option<String>>>,
    /// Optional initial learning rate (from deep-link query params)
    #[prop(optional)]
    initial_learning_rate: Option<RwSignal<Option<String>>>,
    /// Optional initial batch size (from deep-link query params)
    #[prop(optional)]
    initial_batch_size: Option<RwSignal<Option<String>>>,
    /// Optional initial LoRA rank (from deep-link query params)
    #[prop(optional)]
    initial_rank: Option<RwSignal<Option<String>>>,
    /// Optional initial LoRA alpha (from deep-link query params)
    #[prop(optional)]
    initial_alpha: Option<RwSignal<Option<String>>>,
    /// Optional repository ID when launched from adapter version controls
    #[prop(optional)]
    initial_repo_id: Option<RwSignal<Option<String>>>,
    /// Optional branch when launched from adapter version controls
    #[prop(optional)]
    initial_branch: Option<RwSignal<Option<String>>>,
    /// Optional source version ID when launched from adapter version controls
    #[prop(optional)]
    initial_source_version_id: Option<RwSignal<Option<String>>>,
) -> impl IntoView {
    let is_active = Arc::new(AtomicBool::new(true));
    {
        let is_active = Arc::clone(&is_active);
        on_cleanup(move || {
            is_active.store(false, Ordering::Relaxed);
        });
    }

    // Wizard step state
    let current_step = RwSignal::new(WizardStep::default());

    // Form state - persists across steps
    let adapter_name = RwSignal::new(String::new());
    let skill_purpose = RwSignal::new(String::new());
    let base_model_id = RwSignal::new(String::new());
    let dataset_id = RwSignal::new(String::new());
    let dataset_version_id = RwSignal::new(None::<String>);
    let selected_dataset_id = RwSignal::new(String::new());
    let dataset_message = RwSignal::new(None::<String>);
    let source_repo_id = RwSignal::new(None::<String>);
    let source_branch = RwSignal::new(None::<String>);
    let source_version_id = RwSignal::new(None::<String>);
    let version_feed_context = RwSignal::new(None::<String>);

    // Initialize dataset_id from initial_dataset_id if provided
    if let Some(init_ds) = initial_dataset_id {
        Effect::new(move || {
            if let Some(ds_id) = init_ds.try_get().flatten() {
                let _ = dataset_id.try_set(ds_id.clone());
                let _ = selected_dataset_id.try_set(ds_id.clone());
                let _ = dataset_message.try_set(Some(format!("Using selected dataset: {}", ds_id)));
            }
        });
    }

    // Pre-populate from source document if provided
    if let Some(src_doc) = source_document_id {
        Effect::new(move || {
            if let Some(doc_id) = src_doc.try_get().flatten() {
                let _ = dataset_message.try_set(Some(format!(
                    "Document {} is available. Convert it to a dataset in the Documents section.",
                    doc_id
                )));
            }
        });
    }
    // Initialize base_model_id from deep-link if provided
    if let Some(init_model) = initial_base_model_id {
        Effect::new(move || {
            if let Some(model) = init_model.try_get().flatten() {
                let _ = base_model_id.try_set(model);
            }
        });
    }

    if let Some(init_repo) = initial_repo_id {
        Effect::new(move || {
            if let Some(repo_id) = init_repo.try_get().flatten() {
                let _ = source_repo_id.try_set(Some(repo_id));
            }
        });
    }
    if let Some(init_branch) = initial_branch {
        Effect::new(move || {
            if let Some(branch) = init_branch.try_get().flatten() {
                let _ = source_branch.try_set(Some(branch));
            }
        });
    }
    if let Some(init_version) = initial_source_version_id {
        Effect::new(move || {
            if let Some(version_id) = init_version.try_get().flatten() {
                let _ = source_version_id.try_set(Some(version_id));
            }
        });
    }

    Effect::new(move || {
        let repo = source_repo_id.get();
        let branch = source_branch.get();
        let version_id = source_version_id.get();

        if repo.is_none() && branch.is_none() && version_id.is_none() {
            version_feed_context.set(None);
            return;
        }

        let mut parts = Vec::new();
        if let Some(repo_id) = repo {
            parts.push(format!("repo {}", repo_id));
        }
        if let Some(branch_name) = branch {
            parts.push(format!("branch {}", branch_name));
        }
        if let Some(version) = version_id {
            parts.push(format!("version {}", version));
        }

        version_feed_context.set(Some(format!(
            "Starting from adapter version context: {}.",
            parts.join(", ")
        )));
    });

    let category = RwSignal::new("code".to_string());

    // Training parameters with preset support
    let training_preset = RwSignal::new("qa".to_string());
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let validation_split = RwSignal::new("0.15".to_string());
    let early_stopping = RwSignal::new(true);
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());

    // Dataset sample count for time estimation (updated when dataset is selected)
    let dataset_sample_count = RwSignal::new(None::<usize>);

    // Train step advanced options
    let show_train_advanced = RwSignal::new(false);
    let show_knowledge_advanced = RwSignal::new(false);
    let multi_dataset_enabled = RwSignal::new(false);
    let advanced_dataset_versions = RwSignal::new(Vec::<AdvancedDatasetDraft>::new());
    let preferred_backend = RwSignal::new("auto".to_string());
    let backend_policy = RwSignal::new("auto".to_string());
    let coreml_training_fallback = RwSignal::new("mlx".to_string());
    let document_upload_open = RwSignal::new(false);
    let selected_document_id = RwSignal::new(String::new());
    let creating_document_dataset = RwSignal::new(false);
    let draft_notice = RwSignal::new(None::<String>);
    let draft_is_stale = RwSignal::new(false);

    // Lifted state: survives step transitions so data isn't re-fetched
    // and UI toggles aren't lost when navigating between steps.
    let (models, refetch_models) = use_api_resource(
        |client: std::sync::Arc<ApiClient>| async move { client.list_models().await },
    );
    let (datasets, _refetch_datasets) =
        use_api_resource(|client: std::sync::Arc<ApiClient>| async move {
            client.list_datasets(None).await
        });
    let (documents, refetch_documents) =
        use_api_resource(|client: std::sync::Arc<ApiClient>| async move {
            client.list_documents(None).await
        });
    let use_custom_model = RwSignal::new(false);
    let dataset_info = RwSignal::new(None::<DatasetResponse>);
    let dataset_lookup_loading = RwSignal::new(false);
    let dataset_preview = RwSignal::new(None::<DatasetPreviewResponse>);
    let dataset_preview_loading = RwSignal::new(false);
    let dataset_preview_error = RwSignal::new(None::<String>);

    // Shared API client for closures and handlers
    let client = use_api_client();

    // Fetch dataset details reactively when dataset_id changes
    {
        let _client = client.clone();
        #[cfg(target_arch = "wasm32")]
        let is_active = Arc::clone(&is_active);
        Effect::new(move || {
            let id = dataset_id.get();
            if id.trim().is_empty() {
                dataset_info.set(None);
                dataset_version_id.set(None);
                dataset_lookup_loading.set(false);
                return;
            }
            dataset_lookup_loading.set(true);
            #[cfg(target_arch = "wasm32")]
            {
                let client = _client.clone();
                let is_active = Arc::clone(&is_active);
                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    match client.get_dataset(&id).await {
                        Ok(resp) => {
                            let version =
                                if resp.dataset_version_id.is_some() {
                                    resp.dataset_version_id.clone()
                                } else {
                                    client.list_dataset_versions(&id).await.ok().and_then(
                                        |history| {
                                            history
                                                .versions
                                                .into_iter()
                                                .next()
                                                .map(|v| v.dataset_version_id)
                                        },
                                    )
                                };
                            let _ = dataset_info.try_set(Some(resp));
                            let _ = dataset_version_id.try_set(version);
                            let _ = dataset_lookup_loading.try_set(false);
                        }
                        Err(_) => {
                            let _ = dataset_info.try_set(None);
                            let _ = dataset_version_id.try_set(None);
                            let _ = dataset_lookup_loading.try_set(false);
                        }
                    }
                });
            }
        });
    }

    // Fetch dataset preview reactively when the browse selection changes.
    {
        let _client = client.clone();
        #[cfg(target_arch = "wasm32")]
        let is_active = Arc::clone(&is_active);
        Effect::new(move || {
            let selected_id = selected_dataset_id.get().trim().to_string();
            if selected_id.is_empty() {
                dataset_preview.set(None);
                dataset_preview_loading.set(false);
                dataset_preview_error.set(None);
                return;
            }

            dataset_preview_loading.set(true);
            dataset_preview_error.set(None);
            #[cfg(target_arch = "wasm32")]
            {
                let client = _client.clone();
                let is_active = Arc::clone(&is_active);
                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    match client.preview_dataset(&selected_id, Some(10)).await {
                        Ok(resp) => {
                            if selected_dataset_id.get_untracked().trim() != selected_id {
                                return;
                            }
                            let _ = dataset_preview.try_set(Some(resp));
                            let _ = dataset_preview_loading.try_set(false);
                            let _ = dataset_preview_error.try_set(None);
                        }
                        Err(err) => {
                            if selected_dataset_id.get_untracked().trim() != selected_id {
                                return;
                            }
                            let _ = dataset_preview.try_set(None);
                            let _ = dataset_preview_loading.try_set(false);
                            let _ = dataset_preview_error
                                .try_set(Some(format_structured_details(&err)));
                        }
                    }
                });
            }
        });
    }

    // Fetch dataset statistics reactively for readiness gating (including zero-example datasets).
    {
        let _client = client.clone();
        #[cfg(target_arch = "wasm32")]
        let is_active = Arc::clone(&is_active);
        Effect::new(move || {
            let id = dataset_id.get().trim().to_string();
            if id.is_empty() {
                dataset_sample_count.set(None);
                return;
            }

            dataset_sample_count.set(None);
            #[cfg(target_arch = "wasm32")]
            {
                let client = _client.clone();
                let is_active = Arc::clone(&is_active);
                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    match client.get_dataset_statistics(&id).await {
                        Ok(stats) => {
                            if dataset_id.get_untracked().trim() != id {
                                return;
                            }
                            let count = stats.num_examples.max(0) as usize;
                            let _ = dataset_sample_count.try_set(Some(count));
                        }
                        Err(_) => {
                            if dataset_id.get_untracked().trim() != id {
                                return;
                            }
                            let _ = dataset_sample_count.try_set(None);
                        }
                    }
                });
            }
        });
    }

    // If no base model is preselected, default to the first non-CoreML model.
    Effect::new(move || {
        if !base_model_id.get().trim().is_empty() {
            return;
        }
        if let LoadingState::Loaded(resp) = models.get() {
            let mut candidates: Vec<_> = resp.models.iter().collect();
            candidates.sort_by(|a, b| {
                let a_coreml = a.backend.as_deref() == Some("coreml");
                let b_coreml = b.backend.as_deref() == Some("coreml");
                a_coreml
                    .cmp(&b_coreml)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.id.cmp(&b.id))
            });
            if let Some(model) = candidates.into_iter().next() {
                let _ = base_model_id.try_set(model.id.clone());
            }
        }
    });

    // Initialize training parameters from deep-link props
    if let Some(init_epochs) = initial_epochs {
        Effect::new(move || {
            if let Some(v) = init_epochs.try_get().flatten() {
                let _ = epochs.try_set(v);
            }
        });
    }
    if let Some(init_lr) = initial_learning_rate {
        Effect::new(move || {
            if let Some(v) = init_lr.try_get().flatten() {
                let _ = learning_rate.try_set(v);
            }
        });
    }
    if let Some(init_bs) = initial_batch_size {
        Effect::new(move || {
            if let Some(v) = init_bs.try_get().flatten() {
                let _ = batch_size.try_set(v);
            }
        });
    }
    if let Some(init_rank) = initial_rank {
        Effect::new(move || {
            if let Some(v) = init_rank.try_get().flatten() {
                let _ = rank.try_set(v);
            }
        });
    }
    if let Some(init_alpha) = initial_alpha {
        Effect::new(move || {
            if let Some(v) = init_alpha.try_get().flatten() {
                let _ = alpha.try_set(v);
            }
        });
    }
    if let Some(init_backend) = initial_preferred_backend {
        Effect::new(move || {
            if let Some(v) = init_backend.try_get().flatten() {
                let _ = preferred_backend.try_set(v);
            }
        });
    }
    if let Some(init_policy) = initial_backend_policy {
        Effect::new(move || {
            if let Some(v) = init_policy.try_get().flatten() {
                let _ = backend_policy.try_set(v);
            }
        });
    }

    // Wizard state
    let dataset_wizard_open = RwSignal::new(false);
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let form_state = use_form_state();
    let notifications = use_notifications();

    let on_created_clone = on_created.clone();

    // Dataset callback — unified for dataset upload and document conversion.
    let on_dataset_ready = Callback::new(move |outcome: DatasetOutcome| {
        let sample_count = (outcome.sample_count > 0).then_some(outcome.sample_count);
        let source = if outcome.is_synthetic {
            "Generated dataset"
        } else {
            "Dataset"
        };
        let message = if let Some(count) = sample_count {
            format!(
                "{source} created ({} examples). Checking readiness...",
                count
            )
        } else {
            format!("{source} created. Checking readiness...")
        };
        selected_dataset_id.set(outcome.dataset_id.clone());
        dataset_id.set(outcome.dataset_id);
        dataset_version_id.set(outcome.dataset_version_id.clone());
        dataset_sample_count.set(sample_count);
        dataset_message.set(Some(message));
    });

    // Convert a selected document into a dataset for training.
    let use_document_for_dataset = {
        let client = client.clone();
        let is_active = Arc::clone(&is_active);
        Callback::new(move |document_id: String| {
            let document_id = document_id.trim().to_string();
            if document_id.is_empty() || creating_document_dataset.get() {
                return;
            }

            creating_document_dataset.set(true);
            error.set(None);
            dataset_message.set(Some("Preparing dataset from document…".to_string()));

            let client = client.clone();
            let is_active = Arc::clone(&is_active);
            let on_dataset_ready = on_dataset_ready;
            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                if let Err(message) = wait_for_document_indexed(&client, &document_id).await {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = error.try_set(Some(message));
                    let _ = creating_document_dataset.try_set(false);
                    return;
                }
                match client
                    .create_dataset_from_documents(vec![document_id.clone()], None)
                    .await
                {
                    Ok(dataset) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let sample_count = match client.get_dataset_statistics(&dataset.id).await {
                            Ok(stats) => Some(stats.num_examples.max(0) as usize),
                            Err(_) => None,
                        };
                        on_dataset_ready.run(DatasetOutcome {
                            dataset_id: dataset.id.clone(),
                            dataset_version_id: dataset.dataset_version_id.clone(),
                            sample_count: sample_count.unwrap_or(0),
                            is_synthetic: false,
                            source_hash: dataset
                                .dataset_hash_b3
                                .clone()
                                .or(dataset.hash_b3.clone()),
                            receipt_count: 0,
                        });
                        let _ = creating_document_dataset.try_set(false);
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = error.try_set(Some(format_structured_details(&e)));
                        let _ = creating_document_dataset.try_set(false);
                    }
                }
            });
        })
    };

    let use_existing_dataset = {
        let client = client.clone();
        let is_active = Arc::clone(&is_active);
        Callback::new(move |selected_id: String| {
            let selected_id = selected_id.trim().to_string();
            if selected_id.is_empty() {
                return;
            }
            selected_dataset_id.set(selected_id.clone());
            dataset_id.set(selected_id.clone());
            dataset_message.set(Some(
                "Selected existing dataset. Checking readiness...".to_string(),
            ));
            error.set(None);

            let client = client.clone();
            let is_active = Arc::clone(&is_active);
            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                let sample_count = match client.get_dataset_statistics(&selected_id).await {
                    Ok(stats) => Some(stats.num_examples.max(0) as usize),
                    Err(_) => None,
                };
                let _ = dataset_sample_count.try_set(sample_count);
                let latest_version_id = client
                    .list_dataset_versions(&selected_id)
                    .await
                    .ok()
                    .and_then(|history| {
                        history
                            .versions
                            .into_iter()
                            .next()
                            .map(|v| v.dataset_version_id)
                    });
                let _ = dataset_version_id.try_set(latest_version_id);
            });
        })
    };

    let add_selected_dataset_to_advanced = {
        let client = client.clone();
        let is_active = Arc::clone(&is_active);
        Callback::new(move |selected_id: String| {
            let selected_id = selected_id.trim().to_string();
            if selected_id.is_empty() {
                return;
            }

            let client = client.clone();
            let is_active = Arc::clone(&is_active);
            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }

                let latest_version_id = client
                    .list_dataset_versions(&selected_id)
                    .await
                    .ok()
                    .and_then(|history| history.versions.into_iter().next())
                    .map(|version| version.dataset_version_id);

                let Some(dataset_version_id) = latest_version_id else {
                    let _ = error.try_set(Some(
                        "Could not resolve a dataset version for advanced blend mode.".to_string(),
                    ));
                    return;
                };

                advanced_dataset_versions.update(|entries| {
                    if entries.iter().any(|entry| {
                        entry
                            .dataset_version_id
                            .eq_ignore_ascii_case(&dataset_version_id)
                    }) {
                        return;
                    }
                    entries.push(AdvancedDatasetDraft {
                        dataset_id: selected_id.clone(),
                        dataset_version_id,
                        weight: 1.0,
                    });
                });
            });
        })
    };

    let on_document_upload_success = Callback::new(move |document_id: String| {
        refetch_documents.run(());
        use_document_for_dataset.run(document_id);
    });

    let knowledge_gate_state = Signal::derive(move || {
        evaluate_knowledge_gate(
            &dataset_id.get(),
            dataset_version_id.get().as_deref(),
            dataset_sample_count.get(),
            dataset_info.get().as_ref(),
            dataset_lookup_loading.get(),
            creating_document_dataset.get(),
            dataset_wizard_open.get(),
        )
    });

    let current_step_valid = Signal::derive(move || match current_step.get() {
        WizardStep::BuildDataset => knowledge_gate_state.get().is_ready(),
        _ => true,
    });

    // Step validation
    let validate_name_step = move || -> bool {
        let adapter_name_rules = rules::adapter_name();
        let name = adapter_name.get();
        validate_on_blur("adapter_name", &name, &adapter_name_rules, form_state)
    };

    let validate_knowledge_step = move || -> bool {
        let gate_state = knowledge_gate_state.get();
        let dataset_rules = [ValidationRule::Pattern {
            pattern: r"^\s*\S.*$",
            message: "Choose a dataset or convert a document before continuing",
        }];
        let dataset = dataset_id.get();
        if matches!(gate_state, KnowledgeGateState::NeedsDatasetSelection) {
            let _ = validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state);
        }
        gate_state.is_ready()
    };

    let validate_config_step = {
        move || -> bool {
            let mut valid = true;

            let epochs_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 1000 },
            ];
            let epochs_value = epochs.get();
            if !validate_on_blur("epochs", &epochs_value, &epochs_rules, form_state) {
                valid = false;
            }

            let learning_rate_value = learning_rate.get();
            let learning_rate_rules = rules::learning_rate();
            if !validate_on_blur(
                "learning_rate",
                &learning_rate_value,
                &learning_rate_rules,
                form_state,
            ) {
                valid = false;
            }

            let validation_split_rules = [ValidationRule::Range { min: 0.0, max: 0.5 }];
            let validation_split_value = validation_split.get();
            if !validate_on_blur(
                "validation_split",
                &validation_split_value,
                &validation_split_rules,
                form_state,
            ) {
                valid = false;
            }

            let batch_size_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ];
            let batch_size_value = batch_size.get();
            if !validate_on_blur(
                "batch_size",
                &batch_size_value,
                &batch_size_rules,
                form_state,
            ) {
                valid = false;
            }

            let rank_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ];
            let rank_value = rank.get();
            if !validate_on_blur("rank", &rank_value, &rank_rules, form_state) {
                valid = false;
            }

            let alpha_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 512 },
            ];
            let alpha_value = alpha.get();
            if !validate_on_blur("alpha", &alpha_value, &alpha_rules, form_state) {
                valid = false;
            }

            valid
        }
    };

    let validate_train_step = move || -> bool { validate_name_step() && validate_config_step() };

    // Navigation
    let go_next = move |_: ()| {
        let step = current_step.get();
        let can_proceed = match step {
            WizardStep::BuildDataset => validate_knowledge_step(),
            WizardStep::Train => validate_train_step(),
            WizardStep::ContinueToChat => true,
        };

        if can_proceed {
            if let Some(next) = step.next() {
                current_step.set(next);
            }
        }
    };

    let go_back = move |_: ()| {
        if let Some(prev) = current_step.get().prev() {
            current_step.set(prev);
            form_state.update(|state| state.clear_all());
        }
    };

    // Submit handler
    let submit = {
        let on_created = on_created_clone.clone();
        let notifications = notifications.clone();
        let client = client.clone();
        move |_: ()| {
            submitting.set(true);
            error.set(None);
            let gate_state = knowledge_gate_state.get_untracked();
            if !gate_state.is_ready() {
                error.set(Some(
                    gate_state
                        .message()
                        .unwrap_or("Dataset is not ready for training.")
                        .to_string(),
                ));
                submitting.set(false);
                return;
            }

            let name = adapter_name.get();
            let name_for_toast = name.clone();
            let model = base_model_id.get();
            let ds_id = dataset_id.get();
            let ds_version_id = dataset_version_id.get();
            let epochs_val: u32 = epochs.get().parse().unwrap_or(10);
            let lr_val: f32 = learning_rate.get().parse().unwrap_or(0.0001);
            let val_split: f32 = validation_split.get().parse().unwrap_or(0.0);
            let batch_val: u32 = batch_size.get().parse().unwrap_or(4);
            let rank_val: u32 = rank.get().parse().unwrap_or(8);
            let alpha_val: u32 = alpha.get().parse().unwrap_or(16);
            let backend_val = preferred_backend.get();
            let policy_val = backend_policy.get();
            let fallback_val = coreml_training_fallback.get();
            let early_stopping_enabled = early_stopping.get();
            let description = skill_purpose.get();
            let adapter_category = category.get();
            let repo_context = source_repo_id.get();
            let branch_context = source_branch.get();
            let source_version_context = source_version_id.get();
            let use_multi_dataset = multi_dataset_enabled.get();
            let advanced_datasets = advanced_dataset_versions.get();

            let on_created = on_created.clone();
            let notifications = notifications.clone();
            let client = client.clone();
            let is_active = Arc::clone(&is_active);

            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                let model_id = model.trim().to_string();
                if model_id.is_empty() {
                    let _ = error.try_set(Some(
                        "A training model is still loading. Please try again in a moment."
                            .to_string(),
                    ));
                    let _ = submitting.try_set(false);
                    return;
                }

                let dataset_id = ds_id.trim().to_string();
                if dataset_id.is_empty() {
                    let _ = error.try_set(Some(
                        "Add training examples before starting this adapter.".to_string(),
                    ));
                    let _ = submitting.try_set(false);
                    return;
                }

                let params =
                    match serde_json::from_value::<TrainingConfigRequest>(serde_json::json!({
                        "rank": rank_val,
                        "alpha": alpha_val,
                        "targets": ["q_proj", "v_proj"],
                        "training_contract_version": TRAINING_DATA_CONTRACT_VERSION,
                        "pad_token_id": 0,
                        "ignore_index": -100,
                        "epochs": epochs_val,
                        "learning_rate": lr_val,
                        "batch_size": batch_val,
                        "warmup_steps": serde_json::Value::Null,
                        "max_seq_length": serde_json::Value::Null,
                        "gradient_accumulation_steps": serde_json::Value::Null,
                        "validation_split": (val_split > 0.0).then_some(val_split),
                        "preferred_backend": parse_backend_kind(&backend_val),
                        "backend_policy": parse_backend_policy(&policy_val),
                        "coreml_training_fallback": if backend_val == "coreml"
                            || policy_val == "coreml_else_fallback"
                        {
                            parse_backend_kind(&fallback_val)
                        } else {
                            None
                        },
                        "enable_coreml_export": serde_json::Value::Null,
                        "require_gpu": serde_json::Value::Null,
                        "max_gpu_memory_mb": serde_json::Value::Null,
                        "force_resume": serde_json::Value::Null,
                        "multi_module_training": serde_json::Value::Null,
                        "lora_layer_indices": serde_json::Value::Null,
                        "early_stopping": early_stopping_enabled,
                        "patience": serde_json::Value::Null,
                        "min_delta": serde_json::Value::Null
                    })) {
                        Ok(params) => params,
                        Err(_err) => {
                            let _ = error.try_set(Some(
                                "Unable to prepare the training configuration. Please try again."
                                    .to_string(),
                            ));
                            let _ = submitting.try_set(false);
                            return;
                        }
                    };

                let description_value = {
                    let trimmed = description.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                };

                let mut advanced_version_selections: Vec<DatasetVersionSelection> = Vec::new();
                let mut submit_via_advanced_versions = false;

                if use_multi_dataset && !advanced_datasets.is_empty() {
                    let Some(repo_id) = repo_context.clone() else {
                        let _ = error.try_set(Some(
                            "Advanced multi-dataset mode needs repository context. Open this flow from adapter operator controls."
                                .to_string(),
                        ));
                        let _ = submitting.try_set(false);
                        return;
                    };

                    let mut primary_version = ds_version_id
                        .as_deref()
                        .filter(|id| !id.trim().is_empty())
                        .map(str::to_string);
                    if primary_version.is_none() {
                        primary_version = client
                            .list_dataset_versions(&dataset_id)
                            .await
                            .ok()
                            .and_then(|history| history.versions.into_iter().next())
                            .map(|version| version.dataset_version_id);
                    }

                    let Some(primary_version_id) = primary_version else {
                        let _ = error.try_set(Some(
                            "Could not resolve a version for the primary dataset.".to_string(),
                        ));
                        let _ = submitting.try_set(false);
                        return;
                    };

                    advanced_version_selections.push(DatasetVersionSelection {
                        dataset_version_id: primary_version_id,
                        weight: 1.0,
                    });
                    for entry in advanced_datasets {
                        let version_id = entry.dataset_version_id.trim();
                        if version_id.is_empty()
                            || advanced_version_selections.iter().any(|existing| {
                                existing.dataset_version_id.eq_ignore_ascii_case(version_id)
                            })
                        {
                            continue;
                        }
                        advanced_version_selections.push(DatasetVersionSelection {
                            dataset_version_id: version_id.to_string(),
                            weight: if entry.weight.is_finite() && entry.weight > 0.0 {
                                entry.weight
                            } else {
                                1.0
                            },
                        });
                    }

                    if advanced_version_selections.len() > 1 {
                        let start_payload = serde_json::json!({
                            "adapter_name": name,
                            "config": params.clone(),
                            "template_id": serde_json::Value::Null,
                            "repo_id": repo_id,
                            "target_branch": branch_context,
                            "base_version_id": source_version_context,
                            "dataset_id": dataset_id,
                            "dataset_version_ids": advanced_version_selections,
                            "synthetic_mode": false,
                            "data_lineage_mode": "versioned",
                            "base_model_id": model_id,
                            "collection_id": serde_json::Value::Null,
                            "scope": serde_json::Value::Null,
                            "category": adapter_category.clone(),
                            "description": description_value.clone(),
                            "adapter_type": serde_json::Value::Null,
                        });
                        submit_via_advanced_versions = true;
                        match client
                            .post::<_, adapteros_api_types::TrainingJobResponse>(
                                "/v1/training/start",
                                &start_payload,
                            )
                            .await
                        {
                            Ok(response) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                clear_training_wizard_draft();
                                let _ = submitting.try_set(false);
                                let job_href = format!("/training?job_id={}", response.id);
                                notifications.success_with_action(
                                    "Adapter training started",
                                    &format!(
                                        "\"{}\" is now training. You can watch progress live.",
                                        name_for_toast
                                    ),
                                    "View training",
                                    &job_href,
                                );
                                if is_active.load(Ordering::Relaxed) {
                                    on_created(response.id);
                                }
                            }
                            Err(e) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ = error.try_set(Some(map_training_submit_error(&e)));
                                let _ = submitting.try_set(false);
                            }
                        }
                    }
                }

                if submit_via_advanced_versions {
                    return;
                }

                let request = CreateTrainingJobRequest {
                    workspace_id: String::new(),
                    base_model_id: model_id,
                    dataset_id,
                    dataset_version_id: ds_version_id.filter(|id| !id.trim().is_empty()),
                    adapter_name: Some(name),
                    params,
                    lora_tier: None,
                    template_id: None,
                    repo_id: repo_context,
                    description: description_value,
                    adapter_type: None,
                    category: Some(adapter_category),
                };

                match client.create_training_job(&request).await {
                    Ok(response) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        clear_training_wizard_draft();
                        let _ = submitting.try_set(false);
                        let job_href = format!("/training?job_id={}", response.id);
                        notifications.success_with_action(
                            "Adapter training started",
                            &format!(
                                "\"{}\" is now training. You can watch progress live.",
                                name_for_toast
                            ),
                            "View training",
                            &job_href,
                        );
                        if is_active.load(Ordering::Relaxed) {
                            on_created(response.id);
                        }
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = error.try_set(Some(map_training_submit_error(&e)));
                        let _ = submitting.try_set(false);
                    }
                }
            });
        }
    };

    let reset_form = move || {
        current_step.set(WizardStep::default());
        error.set(None);
        form_state.update(|state| state.clear_all());
        submitting.set(false);
        dataset_wizard_open.set(false);
        // Reset form state
        adapter_name.set(String::new());
        skill_purpose.set(String::new());
        base_model_id.set(String::new());
        dataset_id.set(String::new());
        dataset_version_id.set(None);
        selected_dataset_id.set(String::new());
        dataset_message.set(None);
        dataset_sample_count.set(None);
        source_repo_id.set(None);
        source_branch.set(None);
        source_version_id.set(None);
        version_feed_context.set(None);
        // Reset to QA preset defaults
        training_preset.set("qa".to_string());
        epochs.set("10".to_string());
        learning_rate.set("0.0001".to_string());
        validation_split.set("0.15".to_string());
        early_stopping.set(true);
        batch_size.set("4".to_string());
        rank.set("8".to_string());
        alpha.set("16".to_string());
        preferred_backend.set("auto".to_string());
        backend_policy.set("auto".to_string());
        show_train_advanced.set(false);
        show_knowledge_advanced.set(false);
        multi_dataset_enabled.set(false);
        advanced_dataset_versions.set(Vec::new());
        document_upload_open.set(false);
        selected_document_id.set(String::new());
        creating_document_dataset.set(false);
        draft_notice.set(None);
        draft_is_stale.set(false);
        // Reset lifted step state
        use_custom_model.set(false);
        dataset_info.set(None);
        dataset_lookup_loading.set(false);
        dataset_preview.set(None);
        dataset_preview_loading.set(false);
        dataset_preview_error.set(None);
    };

    // Restore draft on open, reset in-memory state on close.
    let was_open = StoredValue::new(open.get_untracked());
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let prev = was_open.get_value();
        was_open.set_value(is_open);
        if !prev && is_open {
            if let Some(draft) = load_training_wizard_draft() {
                let age_ms = (now_ms() - draft.updated_at_ms).max(0.0);
                current_step.set(WizardStep::from_index(draft.step_index));
                adapter_name.set(draft.adapter_name);
                skill_purpose.set(draft.skill_purpose);
                base_model_id.set(draft.base_model_id);
                dataset_id.set(draft.dataset_id);
                dataset_version_id.set(draft.dataset_version_id);
                selected_dataset_id.set(draft.selected_dataset_id);
                category.set(draft.category);
                training_preset.set(draft.training_preset);
                epochs.set(draft.epochs);
                learning_rate.set(draft.learning_rate);
                validation_split.set(draft.validation_split);
                early_stopping.set(draft.early_stopping);
                batch_size.set(draft.batch_size);
                rank.set(draft.rank);
                alpha.set(draft.alpha);
                preferred_backend.set(draft.preferred_backend);
                backend_policy.set(draft.backend_policy);
                coreml_training_fallback.set(draft.coreml_training_fallback);
                show_train_advanced.set(draft.show_train_advanced);
                show_knowledge_advanced.set(draft.show_knowledge_advanced);
                multi_dataset_enabled.set(draft.multi_dataset_enabled);
                advanced_dataset_versions.set(draft.advanced_dataset_versions);
                source_repo_id.set(draft.source_repo_id);
                source_branch.set(draft.source_branch);
                source_version_id.set(draft.source_version_id);
                draft_is_stale.set(age_ms > TRAINING_WIZARD_STALE_MS);
                draft_notice.set(Some(if age_ms > TRAINING_WIZARD_STALE_MS {
                    format!(
                        "Restored a stale draft from {} ago. Review it before starting training.",
                        format_draft_age(age_ms)
                    )
                } else {
                    format!(
                        "Restored your last draft from {} ago.",
                        format_draft_age(age_ms)
                    )
                }));
            } else {
                draft_notice.set(None);
                draft_is_stale.set(false);
            }
        }
        if prev && !is_open {
            reset_form();
        }
    });

    // Persist draft while wizard is open.
    Effect::new(move || {
        if !open.get() {
            return;
        }
        let draft = TrainingWizardDraft {
            updated_at_ms: now_ms(),
            step_index: current_step.get().index(),
            adapter_name: adapter_name.get(),
            skill_purpose: skill_purpose.get(),
            base_model_id: base_model_id.get(),
            dataset_id: dataset_id.get(),
            dataset_version_id: dataset_version_id.get(),
            selected_dataset_id: selected_dataset_id.get(),
            category: category.get(),
            training_preset: training_preset.get(),
            epochs: epochs.get(),
            learning_rate: learning_rate.get(),
            validation_split: validation_split.get(),
            early_stopping: early_stopping.get(),
            batch_size: batch_size.get(),
            rank: rank.get(),
            alpha: alpha.get(),
            preferred_backend: preferred_backend.get(),
            backend_policy: backend_policy.get(),
            coreml_training_fallback: coreml_training_fallback.get(),
            show_train_advanced: show_train_advanced.get(),
            show_knowledge_advanced: show_knowledge_advanced.get(),
            multi_dataset_enabled: multi_dataset_enabled.get(),
            advanced_dataset_versions: advanced_dataset_versions.get(),
            source_repo_id: source_repo_id.get(),
            source_branch: source_branch.get(),
            source_version_id: source_version_id.get(),
        };
        save_training_wizard_draft(&draft);
    });

    let step_labels = STEPS
        .iter()
        .map(|step| step.label().to_string())
        .collect::<Vec<_>>();

    view! {
        <StepFormDialog
            open=open
            title="Create Adapter".to_string()
            current_step=Signal::derive(move || current_step.try_get().unwrap_or_default().index())
            total_steps=STEPS.len()
            step_labels=step_labels
            step_valid=current_step_valid
            loading=Signal::derive(move || submitting.try_get().unwrap_or(false))
            on_next=Callback::new(go_next)
            on_back=Callback::new(go_back)
            on_submit=Callback::new(submit.clone())
            submit_label="Start training".to_string()
            size=DialogSize::Lg
            scrollable=true
        >
            // Draft restore notice
            {move || draft_notice.try_get().flatten().map(|notice| {
                let stale = draft_is_stale.get();
                view! {
                    <div class=if stale {
                        "mb-4 rounded-lg border border-status-warning/50 bg-status-warning/10 p-3"
                    } else {
                        "mb-4 rounded-lg border border-border/70 bg-muted/30 p-3"
                    }>
                        <p class=if stale {
                            "text-sm text-status-warning"
                        } else {
                            "text-sm text-muted-foreground"
                        }>
                            {notice}
                        </p>
                        <div class="mt-2">
                            <Button
                                button_type=ButtonType::Button
                                variant=ButtonVariant::Secondary
                                size=crate::components::ButtonSize::Sm
                                on_click=Callback::new(move |_| {
                                    clear_training_wizard_draft();
                                    reset_form();
                                })
                            >
                                "Start fresh"
                            </Button>
                        </div>
                    </div>
                }
            })}

            // Error message
            {move || error.try_get().flatten().map(|e| view! {
                <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                    <p class="text-sm text-destructive">{e}</p>
                </div>
            })}

            // Step content
            <div class="wizard-step-content min-w-0">
                {move || match current_step.try_get().unwrap_or_default() {
                    WizardStep::BuildDataset => view! {
                        <DatasetStepContent
                            dataset_id=dataset_id
                            dataset_version_id=dataset_version_id
                            selected_dataset_id=selected_dataset_id
                            dataset_message=dataset_message
                            feed_context_message=version_feed_context
                            dataset_sample_count=dataset_sample_count
                            dataset_info=dataset_info
                            dataset_preview=dataset_preview
                            dataset_preview_loading=dataset_preview_loading
                            dataset_preview_error=dataset_preview_error
                            knowledge_gate_state=knowledge_gate_state
                            show_knowledge_advanced=show_knowledge_advanced
                            datasets=datasets
                            document_upload_open=document_upload_open
                            selected_document_id=selected_document_id
                            creating_document_dataset=creating_document_dataset
                            documents=documents
                            on_use_dataset=use_existing_dataset
                            on_use_document=use_document_for_dataset
                            on_add_advanced_dataset=add_selected_dataset_to_advanced
                            dataset_wizard_open=dataset_wizard_open
                            dataset_lookup_loading=dataset_lookup_loading
                            multi_dataset_enabled=multi_dataset_enabled
                            advanced_dataset_versions=advanced_dataset_versions
                            multi_dataset_supported=Signal::derive(move || source_repo_id.get().is_some())
                            form_state=form_state
                        />
                    }.into_any(),
                    WizardStep::Train => view! {
                        <div class="space-y-6">
                            <NameStepContent
                                adapter_name=adapter_name
                                skill_purpose=skill_purpose
                                form_state=form_state
                            />
                            <TrainStepContent
                                base_model_id=base_model_id
                                category=category
                                training_preset=training_preset
                                epochs=epochs
                                learning_rate=learning_rate
                                validation_split=validation_split
                                early_stopping=early_stopping
                                batch_size=batch_size
                                rank=rank
                                alpha=alpha
                                show_advanced=show_train_advanced
                                preferred_backend=preferred_backend
                                backend_policy=backend_policy
                                coreml_fallback=coreml_training_fallback
                                form_state=form_state
                                sample_count=dataset_sample_count.try_get().flatten()
                                models=models
                                refetch_models=refetch_models
                                use_custom_model=use_custom_model
                            />
                        </div>
                    }.into_any(),
                    WizardStep::ContinueToChat => view! {
                        <ReviewStepContent
                            adapter_name=adapter_name.try_get().unwrap_or_default()
                            base_model_id=base_model_id.try_get().unwrap_or_default()
                            dataset_id=dataset_id.try_get().unwrap_or_default()
                            dataset_version_id=dataset_version_id.try_get().flatten()
                            category=category.try_get().unwrap_or_default()
                            preset=training_preset.try_get().unwrap_or_default()
                            epochs=epochs.try_get().unwrap_or_default()
                            learning_rate=learning_rate.try_get().unwrap_or_default()
                            validation_split=validation_split.try_get().unwrap_or_default()
                            early_stopping=early_stopping.try_get().unwrap_or(true)
                            batch_size=batch_size.try_get().unwrap_or_default()
                            rank=rank.try_get().unwrap_or_default()
                            alpha=alpha.try_get().unwrap_or_default()
                            backend=preferred_backend.try_get().unwrap_or_default()
                            feed_context=version_feed_context.try_get().flatten()
                        />
                    }.into_any(),
                }}
            </div>

            // Embedded wizards (modals inside modal)
            <DocumentUploadDialog
                open=document_upload_open
                on_success=on_document_upload_success
            />
            <DatasetUploadWizard
                open=dataset_wizard_open
                on_complete=on_dataset_ready
            />
        </StepFormDialog>
    }
}

/// Step 1: Adapter name and purpose.
#[component]
fn NameStepContent(
    adapter_name: RwSignal<String>,
    skill_purpose: RwSignal<String>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let adapter_name_error = use_field_error(form_state, "adapter_name");

    view! {
        <div class="space-y-6">
            <div class="rounded-lg border border-border/60 bg-card/40 p-4 space-y-1">
                <p class="text-sm font-medium">"Name your adapter"</p>
                <p class="text-xs text-muted-foreground">
                    "Use a short name and, optionally, what you want it to help with."
                </p>
            </div>

            <FormField
                label="Adapter name"
                name="adapter_name"
                required=true
                help="A short name such as \"billing-help\" or \"code-review\""
                error=adapter_name_error
            >
                <Input
                    value=adapter_name
                    placeholder="billing-help".to_string()
                    on_blur=Callback::new(move |_| {
                        let adapter_name_rules = rules::adapter_name();
                        let value = adapter_name.get();
                        let _ = validate_on_blur("adapter_name", &value, &adapter_name_rules, form_state);
                    })
                />
            </FormField>

            <FormField
                label="What should it help with?"
                name="skill_purpose"
                required=false
                help="Optional plain-language goal"
            >
                <Input
                    value=skill_purpose
                    placeholder="Answers customer questions about billing and account access.".to_string()
                />
            </FormField>
        </div>
    }
}

/// Step 0: Choose knowledge source for the adapter.
#[component]
fn DatasetStepContent(
    dataset_id: RwSignal<String>,
    dataset_version_id: RwSignal<Option<String>>,
    selected_dataset_id: RwSignal<String>,
    dataset_message: RwSignal<Option<String>>,
    feed_context_message: RwSignal<Option<String>>,
    dataset_sample_count: RwSignal<Option<usize>>,
    dataset_info: RwSignal<Option<DatasetResponse>>,
    dataset_preview: RwSignal<Option<DatasetPreviewResponse>>,
    dataset_preview_loading: RwSignal<bool>,
    dataset_preview_error: RwSignal<Option<String>>,
    knowledge_gate_state: Signal<KnowledgeGateState>,
    show_knowledge_advanced: RwSignal<bool>,
    datasets: ReadSignal<LoadingState<DatasetListResponse>>,
    document_upload_open: RwSignal<bool>,
    selected_document_id: RwSignal<String>,
    creating_document_dataset: RwSignal<bool>,
    documents: ReadSignal<LoadingState<DocumentListResponse>>,
    on_use_dataset: Callback<String>,
    on_use_document: Callback<String>,
    on_add_advanced_dataset: Callback<String>,
    dataset_wizard_open: RwSignal<bool>,
    dataset_lookup_loading: RwSignal<bool>,
    multi_dataset_enabled: RwSignal<bool>,
    advanced_dataset_versions: RwSignal<Vec<AdvancedDatasetDraft>>,
    multi_dataset_supported: Signal<bool>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let _ = dataset_lookup_loading;
    let has_dataset = Signal::derive(move || !dataset_id.get().trim().is_empty());
    let dataset_error = use_field_error(form_state, "dataset_id");

    // Initialize selected dataset once list loads.
    Effect::new(move || {
        if !selected_dataset_id.get().trim().is_empty() {
            return;
        }
        let LoadingState::Loaded(resp) = datasets.get() else {
            return;
        };
        if let Some(dataset) = resp
            .datasets
            .iter()
            .find(|ds| ds.status.eq_ignore_ascii_case("ready"))
            .or_else(|| resp.datasets.first())
        {
            selected_dataset_id.set(dataset.id.clone());
        }
    });

    // Initialize selected document once list loads.
    Effect::new(move || {
        if !selected_document_id.get().trim().is_empty() {
            return;
        }
        let LoadingState::Loaded(resp) = documents.get() else {
            return;
        };
        if let Some(doc) = resp
            .data
            .iter()
            .find(|doc| doc.status.eq_ignore_ascii_case("indexed"))
            .or_else(|| resp.data.first())
        {
            selected_document_id.set(doc.document_id.clone());
        }
    });

    view! {
        <div class="space-y-6">
            <div class="text-center py-2">
                <h3 class="heading-4 mb-1">"Build your dataset"</h3>
                <p class="text-sm text-muted-foreground">
                    "Choose a ready dataset or convert uploaded files into one."
                </p>
            </div>

            {move || feed_context_message.get().map(|context| view! {
                <div class="rounded-lg border border-border/70 bg-muted/30 p-3">
                    <p class="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                        "Version feed context"
                    </p>
                    <p class="text-sm mt-1">{context}</p>
                </div>
            })}

            {move || {
                let gate = knowledge_gate_state.get();
                gate.message().map(|message| {
                    let (container_class, message_class) = gate.style_classes();
                    view! {
                        <div class=format!("rounded-lg p-3 {}", container_class)>
                            <p class=format!("text-sm font-medium {}", message_class)>
                                {message}
                            </p>
                        </div>
                    }
                })
            }}

            // Current dataset status
            {move || {
                if has_dataset.get() {
                    let msg = dataset_message
                        .get()
                        .unwrap_or_else(|| "Dataset selected. Checking readiness...".to_string());
                    let gate = knowledge_gate_state.get();
                    let (container_class, _) = gate.style_classes();
                    let details = dataset_info.get();
                    view! {
                        <div class=format!("rounded-lg p-4 {}", container_class)>
                            <div class="flex items-start gap-3">
                                <div class="rounded-full bg-background/70 p-2 shrink-0">
                                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                    </svg>
                                </div>
                                <div class="min-w-0 flex-1">
                                    <p class="text-sm font-medium">{msg}</p>
                                    {move || details.clone().map(|info| {
                                        let display_name = info.display_name.clone().unwrap_or_else(|| info.name.clone());
                                        view! {
                                            <p class="text-xs text-muted-foreground mt-1">
                                                {format!("Dataset: {}", display_name)}
                                            </p>
                                            <p class="text-xs text-muted-foreground mt-1" title=info.status.clone()>
                                                {format!("Status: {}", status_display_label(&info.status))}
                                            </p>
                                            {info.trust_state.clone().map(|trust| view! {
                                                <p class="text-xs text-muted-foreground mt-1" title=trust.clone()>
                                                    {format!("Trust: {}", status_display_label(&trust))}
                                                </p>
                                            })}
                                            {info.validation_status.clone().map(|validation| view! {
                                                <p class="text-xs text-muted-foreground mt-1" title=validation.clone()>
                                                    {format!("Validation: {}", status_display_label(&validation))}
                                                </p>
                                            })}
                                        }
                                    })}
                                    {move || dataset_version_id.get().map(|version| {
                                        view! {
                                            <p class="text-xs text-muted-foreground mt-1">
                                                {format!("Version: {}", version)}
                                            </p>
                                        }
                                    })}
                                    {move || dataset_sample_count.get().map(|count| {
                                        view! {
                                            <p class="text-xs text-muted-foreground mt-1">
                                                {format!("{} examples", count)}
                                            </p>
                                        }
                                    })}
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    let msg = dataset_message.get();
                    view! {
                        {msg.map(|text| view! {
                            <div class="rounded-lg border border-border/70 bg-muted/30 p-3">
                                <p class="text-sm text-muted-foreground">{text}</p>
                            </div>
                        })}
                    }.into_any()
                }
            }}

            <Card>
                <div class="p-5 space-y-3">
                    <h4 class="font-medium">"Datasets"</h4>
                    <p class="text-sm text-muted-foreground">
                        "Use an existing structured dataset or upload JSONL/CSV training data."
                    </p>
                    <AsyncBoundary
                        state=datasets
                        on_retry=Callback::new(move |_| {})
                        loading_message="Loading datasets...".to_string()
                        render=move |resp: DatasetListResponse| {
                            let options = resp
                                .datasets
                                .iter()
                                .map(|dataset| {
                                    let name = dataset
                                        .display_name
                                        .clone()
                                        .unwrap_or_else(|| dataset.name.clone());
                                    let status = status_display_label(&dataset.status);
                                    (
                                        dataset.id.clone(),
                                        format!("{name} ({status})"),
                                    )
                                })
                                .collect::<Vec<_>>();

                            if options.is_empty() {
                                view! {
                                    <div class="space-y-3">
                                        <p class="text-sm text-muted-foreground">
                                            "No datasets found yet."
                                        </p>
                                        <Button
                                            button_type=ButtonType::Button
                                            variant=ButtonVariant::Secondary
                                            on_click=Callback::new(move |_| dataset_wizard_open.set(true))
                                        >
                                            "Upload structured dataset (JSONL/CSV)"
                                        </Button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-3">
                                        <Select value=selected_dataset_id options=options />
                                        <div class="flex flex-wrap gap-2">
                                            <Button
                                                button_type=ButtonType::Button
                                                variant=ButtonVariant::Secondary
                                                disabled=Signal::derive(move || selected_dataset_id.get().trim().is_empty())
                                                on_click=Callback::new(move |_| on_use_dataset.run(selected_dataset_id.get()))
                                            >
                                                "Use selected dataset"
                                            </Button>
                                            <Button
                                                button_type=ButtonType::Button
                                                variant=ButtonVariant::Secondary
                                                on_click=Callback::new(move |_| dataset_wizard_open.set(true))
                                            >
                                                "Upload structured dataset (JSONL/CSV)"
                                            </Button>
                                        </div>
                                        <div class="rounded-lg border border-border/70 bg-muted/20 p-3">
                                            <div class="flex items-center justify-between gap-2">
                                                <p class="text-sm font-medium">"Browse selected dataset"</p>
                                                <p class="text-xs text-muted-foreground">
                                                    "Showing first 10 examples"
                                                </p>
                                            </div>
                                            {move || {
                                                if dataset_preview_loading.get() {
                                                    return view! {
                                                        <p class="mt-2 text-sm text-muted-foreground">
                                                            "Loading preview..."
                                                        </p>
                                                    }.into_any();
                                                }
                                                if let Some(err) = dataset_preview_error.get() {
                                                    return view! {
                                                        <p class="mt-2 text-sm text-destructive">{err}</p>
                                                    }.into_any();
                                                }
                                                match dataset_preview.get() {
                                                    Some(preview) if !preview.examples.is_empty() => {
                                                        view! {
                                                            <div class="mt-3 space-y-2">
                                                                <p class="text-xs text-muted-foreground">
                                                                    {format!(
                                                                        "{} preview examples",
                                                                        preview.examples.len()
                                                                    )}
                                                                </p>
                                                                {preview
                                                                    .examples
                                                                    .into_iter()
                                                                    .enumerate()
                                                                    .map(|(idx, example)| {
                                                                        let rendered = serde_json::to_string_pretty(&example)
                                                                            .unwrap_or_else(|_| example.to_string());
                                                                        view! {
                                                                            <div class="rounded-md border border-border/70 bg-background/70 p-3">
                                                                                <p class="text-[11px] uppercase tracking-wide text-muted-foreground">
                                                                                    {format!("Example {}", idx + 1)}
                                                                                </p>
                                                                                <pre class="mt-2 max-h-44 overflow-auto whitespace-pre-wrap break-words rounded bg-muted/30 p-2 text-xs">{rendered}</pre>
                                                                            </div>
                                                                        }
                                                                    })
                                                                    .collect_view()}
                                                            </div>
                                                        }.into_any()
                                                    }
                                                    Some(_) => {
                                                        view! {
                                                            <p class="mt-2 text-sm text-muted-foreground">
                                                                "No preview examples are available for this dataset."
                                                            </p>
                                                        }.into_any()
                                                    }
                                                    None => {
                                                        view! {
                                                            <p class="mt-2 text-sm text-muted-foreground">
                                                                "Choose a dataset above to browse its contents."
                                                            </p>
                                                        }.into_any()
                                                    }
                                                }
                                            }}
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }
                    />
                </div>
            </Card>

            <Card>
                <div class="p-5 space-y-3">
                    <h4 class="font-medium">"Add your files (recommended)"</h4>
                    <p class="text-sm text-muted-foreground">
                        "Upload a file or select one that is already indexed, then convert it into a dataset."
                    </p>
                    <Button
                        button_type=ButtonType::Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| document_upload_open.set(true))
                    >
                        "Upload new document"
                    </Button>
                    <AsyncBoundary
                        state=documents
                        on_retry=Callback::new(move |_| {})
                        loading_message="Loading uploaded files...".to_string()
                        render=move |resp: DocumentListResponse| {
                            let options = resp
                                .data
                                .iter()
                                .map(|doc| {
                                    let status = status_display_label(&doc.status);
                                    (
                                        doc.document_id.clone(),
                                        format!("{} ({}, {})", doc.name, status, format_bytes(doc.size_bytes)),
                                    )
                                })
                                .collect::<Vec<_>>();

                            if options.is_empty() {
                                view! {
                                    <p class="text-sm text-muted-foreground">
                                        "No documents available yet. Upload one above."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-3">
                                        <Select value=selected_document_id options=options />
                                        <Button
                                            button_type=ButtonType::Button
                                            variant=ButtonVariant::Secondary
                                            disabled=Signal::derive(move || {
                                                creating_document_dataset.get()
                                                    || selected_document_id.get().trim().is_empty()
                                            })
                                            on_click=Callback::new(move |_| on_use_document.run(selected_document_id.get()))
                                        >
                                            {move || if creating_document_dataset.get() { "Preparing..." } else { "Convert selected document" }}
                                        </Button>
                                    </div>
                                }.into_any()
                            }
                        }
                    />
                </div>
            </Card>

            <div class="border-t pt-4">
                <button
                    type="button"
                    class="btn btn-link btn-sm flex items-center gap-2 px-0 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| show_knowledge_advanced.update(|v| *v = !*v)
                >
                    <svg
                        class=move || if show_knowledge_advanced.get() { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced options"
                </button>

                <Show when=move || show_knowledge_advanced.get()>
                    <div class="mt-4 space-y-4 pl-6">
                        <FormField
                            label="Dataset ID (manual fallback)"
                            name="dataset_id"
                            help="Use a known dataset ID directly"
                            error=dataset_error
                        >
                            <Input
                                value=dataset_id
                                placeholder="ds-abc123".to_string()
                                on_blur=Callback::new(move |_| {
                                    let dataset_rules = [ValidationRule::Pattern {
                                        pattern: r"^\s*\S.*$",
                                        message: "Choose a dataset or convert a document before continuing",
                                    }];
                                    let dataset = dataset_id.get();
                                    let _ = validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state);
                                })
                            />
                        </FormField>

                        <div class="rounded-lg border border-border/70 bg-muted/20 p-4 space-y-3">
                            <label class="flex items-start gap-2">
                                <input
                                    type="checkbox"
                                    prop:checked=move || multi_dataset_enabled.get()
                                    prop:disabled=move || !multi_dataset_supported.get()
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        if let Some(input) = ev
                                            .target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                        {
                                            multi_dataset_enabled.set(input.checked());
                                        }
                                    }
                                />
                                <span class="text-sm">
                                    <span class="font-medium">"Blend multiple datasets"</span>
                                    <span class="block text-xs text-muted-foreground mt-1">
                                        "Advanced operator path. Default flow stays single-dataset."
                                    </span>
                                </span>
                            </label>

                            {move || (!multi_dataset_supported.get()).then(|| view! {
                                <p class="text-xs text-muted-foreground">
                                    "Open this wizard from adapter operator controls to enable multi-dataset training."
                                </p>
                            })}

                            <Show when=move || multi_dataset_enabled.get() && multi_dataset_supported.get()>
                                <div class="space-y-3">
                                    <p class="text-xs text-muted-foreground">
                                        "Primary dataset keeps weight 1.0. Add extra datasets with optional weights."
                                    </p>
                                    <Button
                                        button_type=ButtonType::Button
                                        variant=ButtonVariant::Secondary
                                        disabled=Signal::derive(move || selected_dataset_id.get().trim().is_empty())
                                        on_click=Callback::new(move |_| {
                                            on_add_advanced_dataset.run(selected_dataset_id.get());
                                        })
                                    >
                                        "Add selected dataset to blend"
                                    </Button>

                                    {move || {
                                        let rows = advanced_dataset_versions.get();
                                        if rows.is_empty() {
                                            view! {
                                                <p class="text-xs text-muted-foreground">
                                                    "No extra datasets in the blend yet."
                                                </p>
                                            }
                                                .into_any()
                                        } else {
                                            view! {
                                                <div class="space-y-2">
                                                    {rows
                                                        .into_iter()
                                                        .enumerate()
                                                        .map(|(idx, row)| {
                                                            let dataset_label = if row.dataset_id.trim().is_empty() {
                                                                row.dataset_version_id.clone()
                                                            } else {
                                                                format!(
                                                                    "{} ({})",
                                                                    row.dataset_id, row.dataset_version_id
                                                                )
                                                            };
                                                            view! {
                                                                <div class="rounded-md border border-border/70 bg-background/60 p-3 space-y-2">
                                                                    <div class="flex items-start justify-between gap-2">
                                                                        <p class="text-sm font-medium break-all">
                                                                            {dataset_label}
                                                                        </p>
                                                                        <button
                                                                            type="button"
                                                                            class="btn btn-link btn-xs px-0 text-destructive hover:underline"
                                                                            on:click=move |_| {
                                                                                advanced_dataset_versions.update(|entries| {
                                                                                    if idx < entries.len() {
                                                                                        entries.remove(idx);
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            "Remove"
                                                                        </button>
                                                                    </div>
                                                                    <div class="flex items-center gap-2">
                                                                        <label class="text-xs text-muted-foreground">
                                                                            "Weight"
                                                                        </label>
                                                                        <input
                                                                            type="number"
                                                                            min="0.01"
                                                                            step="0.01"
                                                                            class="w-24 rounded-md border border-border/70 bg-background px-2 py-1 text-sm"
                                                                            prop:value=move || {
                                                                                advanced_dataset_versions
                                                                                    .get()
                                                                                    .get(idx)
                                                                                    .map(|entry| format!("{:.2}", entry.weight))
                                                                                    .unwrap_or_else(|| "1.00".to_string())
                                                                            }
                                                                            on:input=move |ev| {
                                                                                use wasm_bindgen::JsCast;
                                                                                if let Some(input) = ev
                                                                                    .target()
                                                                                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                                                {
                                                                                    let parsed = input
                                                                                        .value()
                                                                                        .parse::<f32>()
                                                                                        .ok()
                                                                                        .filter(|v| v.is_finite() && *v > 0.0)
                                                                                        .unwrap_or(1.0);
                                                                                    advanced_dataset_versions.update(|entries| {
                                                                                        if let Some(entry) = entries.get_mut(idx) {
                                                                                            entry.weight = parsed;
                                                                                        }
                                                                                    });
                                                                                }
                                                                            }
                                                                        />
                                                                    </div>
                                                                </div>
                                                            }
                                                        })
                                                        .collect_view()}
                                                </div>
                                            }
                                                .into_any()
                                        }
                                    }}
                                </div>
                            </Show>
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Step 2: Train setup (defaults first, advanced optional).
#[component]
fn TrainStepContent(
    base_model_id: RwSignal<String>,
    category: RwSignal<String>,
    form_state: RwSignal<FormState>,
    training_preset: RwSignal<String>,
    epochs: RwSignal<String>,
    learning_rate: RwSignal<String>,
    validation_split: RwSignal<String>,
    early_stopping: RwSignal<bool>,
    batch_size: RwSignal<String>,
    rank: RwSignal<String>,
    alpha: RwSignal<String>,
    show_advanced: RwSignal<bool>,
    preferred_backend: RwSignal<String>,
    backend_policy: RwSignal<String>,
    coreml_fallback: RwSignal<String>,
    sample_count: Option<usize>,
    models: ReadSignal<LoadingState<ModelListResponse>>,
    refetch_models: Refetch,
    use_custom_model: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <ModelStepContent
                base_model_id=base_model_id
                category=category
                form_state=form_state
                models=models
                refetch_models=refetch_models
                use_custom_model=use_custom_model
            />

            <div class="rounded-lg border border-border/60 bg-card/40 p-4 space-y-1">
                <p class="text-sm font-medium">"Default training plan"</p>
                <p class="text-xs text-muted-foreground">
                    "We will use balanced defaults. Open advanced options if you want to tune details."
                </p>
                {sample_count.map(|count| view! {
                    <p class="text-xs text-muted-foreground">{format!("Using {} examples", count)}</p>
                })}
            </div>

            <ConfigStepContent
                training_preset=training_preset
                epochs=epochs
                learning_rate=learning_rate
                validation_split=validation_split
                early_stopping=early_stopping
                batch_size=batch_size
                rank=rank
                alpha=alpha
                show_advanced=show_advanced
                preferred_backend=preferred_backend
                backend_policy=backend_policy
                coreml_fallback=coreml_fallback
                form_state=form_state
                sample_count=sample_count
            />
        </div>
    }
}

/// Train step: base model and adapter type.
#[component]
fn ModelStepContent(
    base_model_id: RwSignal<String>,
    category: RwSignal<String>,
    form_state: RwSignal<FormState>,
    /// Models resource fetched by the parent — survives step transitions.
    models: ReadSignal<LoadingState<ModelListResponse>>,
    /// Refetch handle for the models resource.
    refetch_models: Refetch,
    /// "Enter model ID manually" toggle — survives step transitions.
    use_custom_model: RwSignal<bool>,
) -> impl IntoView {
    let base_model_error = use_field_error(form_state, "base_model_id");

    view! {
        <div class="space-y-6">
            <FormField
                label="Starting model"
                name="base_model_id"
                required=true
                help="Choose the model this adapter should learn from"
                error=base_model_error
            >
                {move || {
                    if use_custom_model.try_get().unwrap_or(false) {
                        view! {
                            <div class="space-y-2">
                                <Input
                                    value=base_model_id
                                    placeholder="model-id".to_string()
                                    on_blur=Callback::new(move |_| {
                                        let model_rules = [ValidationRule::Pattern {
                                            pattern: r"^\s*\S.*$",
                                            message: "Base model is required",
                                        }];
                                        let value = base_model_id.get();
                                        let _ = validate_on_blur("base_model_id", &value, &model_rules, form_state);
                                    })
                                />
                                <button
                                    class="btn btn-link btn-xs px-0 text-primary hover:underline"
                                    type="button"
                                    on:click=move |_| use_custom_model.set(false)
                                >
                                    "Choose from available models"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <AsyncBoundary
                                state=models
                                on_retry=Callback::new(move |_| refetch_models.run(()))
                                loading_message="Loading models...".to_string()
                                render=move |resp| {
                                    let mut model_rows = resp.models.clone();
                                    model_rows.sort_by(|a, b| {
                                        let a_coreml = a.backend.as_deref() == Some("coreml");
                                        let b_coreml = b.backend.as_deref() == Some("coreml");
                                        a_coreml
                                            .cmp(&b_coreml)
                                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                                            .then_with(|| a.id.cmp(&b.id))
                                    });

                                    // Track which model IDs use CoreML backend
                                    let coreml_ids: Vec<String> = model_rows.iter()
                                        .filter(|m| m.backend.as_deref() == Some("coreml"))
                                        .map(|m| m.id.clone())
                                        .collect();
                                    let options: Vec<(String, String)> = model_rows.into_iter().map(|m| {
                                        let is_coreml = m.backend.as_deref() == Some("coreml");
                                        let mut label = if m.name.trim() != m.id {
                                            format!("{} ({})", m.name, m.id)
                                        } else {
                                            m.name.clone()
                                        };
                                        if let Some(q) = m.quantization.as_deref() {
                                            if !q.trim().is_empty() {
                                                label.push_str(&format!(" • {}", q));
                                            }
                                        }
                                        if is_coreml {
                                            label.push_str(" • CoreML, no adapter support");
                                        } else if let Some(backend) = m.backend.as_deref() {
                                            if !backend.trim().is_empty() {
                                                label.push_str(&format!(" • {}", backend.to_uppercase()));
                                            }
                                        }
                                        (m.id.clone(), label)
                                    }).collect();
                                    view! {
                                        <div class="space-y-2">
                                            <Select
                                                value=base_model_id
                                                options=options
                                                on_change=Callback::new(move |selected: String| {
                                                    let model_rules = [ValidationRule::Pattern {
                                                        pattern: r"^\s*\S.*$",
                                                        message: "Base model is required",
                                                    }];
                                                    let _ = validate_on_blur("base_model_id", &selected, &model_rules, form_state);
                                                })
                                            />
                                            {move || {
                                                let selected = base_model_id.get();
                                                coreml_ids.contains(&selected).then(|| view! {
                                                    <div class="rounded-md border border-status-warning/40 bg-status-warning/10 p-3">
                                                        <p class="text-xs text-status-warning">
                                                            "This model cannot train new skills in this environment. Choose an MLX model instead."
                                                        </p>
                                                    </div>
                                                })
                                            }}
                                            <button
                                                class="btn btn-link btn-xs px-0 text-primary hover:underline"
                                                type="button"
                                                on:click=move |_| use_custom_model.set(true)
                                            >
                                                "Enter model ID manually"
                                            </button>
                                        </div>
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </FormField>

            <FormField
                label="Adapter type"
                name="category"
                help="Used for organization and filtering"
            >
                <Select
                    value=category
                    options=vec![
                        ("code".to_string(), "Code tasks".to_string()),
                        ("framework".to_string(), "Framework guidance".to_string()),
                        ("codebase".to_string(), "Codebase helper".to_string()),
                        ("docs".to_string(), "Documentation".to_string()),
                        ("domain".to_string(), "Domain expertise".to_string()),
                    ]
                />
            </FormField>
        </div>
    }
}

/// Step 3: Training configuration with presets
#[component]
fn ConfigStepContent(
    training_preset: RwSignal<String>,
    epochs: RwSignal<String>,
    learning_rate: RwSignal<String>,
    validation_split: RwSignal<String>,
    early_stopping: RwSignal<bool>,
    batch_size: RwSignal<String>,
    rank: RwSignal<String>,
    alpha: RwSignal<String>,
    show_advanced: RwSignal<bool>,
    preferred_backend: RwSignal<String>,
    backend_policy: RwSignal<String>,
    coreml_fallback: RwSignal<String>,
    form_state: RwSignal<FormState>,
    sample_count: Option<usize>,
) -> impl IntoView {
    let preset_label = TrainingPreset::parse_str(&training_preset.get())
        .label()
        .to_string();
    let epochs_error = use_field_error(form_state, "epochs");
    let learning_rate_error = use_field_error(form_state, "learning_rate");
    let validation_split_error = use_field_error(form_state, "validation_split");
    let batch_size_error = use_field_error(form_state, "batch_size");
    let rank_error = use_field_error(form_state, "rank");
    let alpha_error = use_field_error(form_state, "alpha");

    view! {
        <div class="space-y-6">
            <Card>
                <div class="p-4 space-y-1">
                    <p class="text-sm font-medium">"Balanced default plan"</p>
                    <p class="text-xs text-muted-foreground">
                        "Training uses sensible defaults for most adapters."
                    </p>
                    <p class="text-xs text-muted-foreground">
                        {format!("Preset: {}", preset_label)}
                    </p>
                    {sample_count.map(|count| view! {
                        <p class="text-xs text-muted-foreground">{format!("Estimated input size: {} examples", count)}</p>
                    })}
                </div>
            </Card>

            // Advanced options section
            <div class="border-t pt-4">
                <button
                    type="button"
                    class="btn btn-link btn-sm flex items-center gap-2 px-0 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| show_advanced.update(|v| *v = !*v)
                >
                    <svg
                        class=move || if show_advanced.try_get().unwrap_or(false) { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced options"
                </button>

                {move || show_advanced.try_get().unwrap_or(false).then(|| view! {
                    <div class="mt-4 space-y-6 pl-6">
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Training parameters"</h4>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <FormField
                                    label="Epochs"
                                    name="epochs"
                                    required=true
                                    help="Number of passes over your training examples"
                                    error=epochs_error
                                >
                                    <Input
                                        value=epochs
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let epochs_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 1000 },
                                            ];
                                            let value = epochs.get();
                                            let _ = validate_on_blur("epochs", &value, &epochs_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Learning rate"
                                    name="learning_rate"
                                    required=true
                                    help="Step size for parameter updates"
                                    error=learning_rate_error
                                >
                                    <Input
                                        value=learning_rate
                                        on_blur=Callback::new(move |_| {
                                            let learning_rate_rules = rules::learning_rate();
                                            let value = learning_rate.get();
                                            let _ = validate_on_blur(
                                                "learning_rate",
                                                &value,
                                                &learning_rate_rules,
                                                form_state,
                                            );
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Validation split"
                                    name="validation_split"
                                    required=true
                                    help="Portion of data held out for validation"
                                    error=validation_split_error
                                >
                                    <Input
                                        value=validation_split
                                        on_blur=Callback::new(move |_| {
                                            let validation_split_rules =
                                                [ValidationRule::Range { min: 0.0, max: 0.5 }];
                                            let value = validation_split.get();
                                            let _ = validate_on_blur(
                                                "validation_split",
                                                &value,
                                                &validation_split_rules,
                                                form_state,
                                            );
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Early stopping"
                                    name="early_stopping"
                                    required=false
                                    help="Stop automatically when progress plateaus"
                                >
                                    <label class="flex items-center gap-2 text-sm">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || early_stopping.get()
                                            on:change=move |ev| {
                                                use wasm_bindgen::JsCast;
                                                if let Some(input) = ev
                                                    .target()
                                                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                {
                                                    early_stopping.set(input.checked());
                                                }
                                            }
                                        />
                                        <span>"Enable early stopping"</span>
                                    </label>
                                </FormField>
                            </div>
                        </div>

                        // Batch size and LoRA config
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Adapter settings"</h4>
                            <div class="grid gap-4 sm:grid-cols-3">
                                <FormField
                                    label="Batch Size"
                                    name="batch_size"
                                    required=true
                                    help="Examples per step (1-256)"
                                    error=batch_size_error
                                >
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let batch_size_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = batch_size.get();
                                            let _ = validate_on_blur("batch_size", &value, &batch_size_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter dimension (4, 8, 16 typical)"
                                    error=rank_error
                                >
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let rank_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = rank.get();
                                            let _ = validate_on_blur("rank", &value, &rank_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=alpha_error
                                >
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let alpha_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 512 },
                                            ];
                                            let value = alpha.get();
                                            let _ = validate_on_blur("alpha", &value, &alpha_rules, form_state);
                                        })
                                    />
                                </FormField>
                            </div>
                        </div>

                        // Backend selection
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Runtime Selection"</h4>
                            <p class="text-xs text-muted-foreground mb-3">
                                "By default, the system automatically picks the best available runtime."
                            </p>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <FormField label="Preferred Runtime" name="preferred_backend">
                                    <Select
                                        value=preferred_backend
                                        options=vec![
                                            ("auto".to_string(), "Auto (recommended)".to_string()),
                                            ("mlx".to_string(), "MLX".to_string()),
                                            ("coreml".to_string(), "CoreML".to_string()),
                                            ("metal".to_string(), "Metal".to_string()),
                                        ]
                                    />
                                </FormField>
                                <FormField label="Runtime Policy" name="backend_policy">
                                    <Select
                                        value=backend_policy
                                        options=vec![
                                            ("auto".to_string(), "Auto".to_string()),
                                            ("coreml_only".to_string(), "CoreML Only".to_string()),
                                            ("coreml_else_fallback".to_string(), "CoreML with Fallback".to_string()),
                                        ]
                                    />
                                </FormField>
                            </div>
                            {move || (preferred_backend.try_get().unwrap_or_default() == "coreml" || backend_policy.try_get().unwrap_or_default() == "coreml_else_fallback").then(|| view! {
                                <div class="mt-4">
                                    <FormField label="Fallback Runtime" name="coreml_fallback">
                                        <Select
                                            value=coreml_fallback
                                            options=vec![
                                                ("mlx".to_string(), "MLX".to_string()),
                                                ("metal".to_string(), "Metal".to_string()),
                                            ]
                                        />
                                    </FormField>
                                </div>
                            })}
                        </div>
                    </div>
                })}
            </div>
        </div>
    }
}

/// Step 4: Review and submit
#[component]
fn ReviewStepContent(
    adapter_name: String,
    base_model_id: String,
    dataset_id: String,
    dataset_version_id: Option<String>,
    category: String,
    preset: String,
    epochs: String,
    learning_rate: String,
    validation_split: String,
    early_stopping: bool,
    batch_size: String,
    rank: String,
    alpha: String,
    backend: String,
    feed_context: Option<String>,
) -> impl IntoView {
    let preset_label = TrainingPreset::parse_str(&preset).label();
    let val_split_display = validation_split
        .parse::<f32>()
        .map(|v| format!("{:.0}%", v * 100.0))
        .unwrap_or_else(|_| validation_split.clone());

    view! {
        <div class="space-y-6">
            <div class="text-center py-2">
                <h3 class="heading-4">"Ready to start training"</h3>
                <p class="text-sm text-muted-foreground">
                    "Review this plan, start training, then continue to chat as soon as the build is ready."
                </p>
            </div>

            <div class="rounded-lg border border-border/70 bg-muted/30 p-3">
                <p class="text-sm text-muted-foreground">
                    "After this starts, keep the Build details open. A direct continue-to-chat action appears when training completes."
                </p>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Adapter name" value=adapter_name/>
                <ReviewRow label="Starting model" value=base_model_id/>
                <ReviewRow label="Dataset ID" value=if dataset_id.is_empty() { "None selected".to_string() } else { dataset_id }/>
                {dataset_version_id.map(|version| view! {
                    <ReviewRow label="Dataset version" value=version/>
                })}
                <ReviewRow label="Adapter type" value=category/>
                {feed_context.map(|context| view! {
                    <ReviewRow label="Version context" value=context/>
                })}
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Training plan" value=preset_label.to_string()/>
                <ReviewRow label="Epochs" value=epochs/>
                <ReviewRow label="Learning Rate" value=learning_rate/>
                <ReviewRow label="Validation split" value=val_split_display/>
                <ReviewRow label="Early stopping" value=if early_stopping { "Enabled".to_string() } else { "Disabled".to_string() }/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Batch Size" value=batch_size/>
                <ReviewRow label="Adapter Capacity" value=rank/>
                <ReviewRow label="Adapter Strength" value=alpha/>
                <ReviewRow label="Compute Runtime" value=if backend == "auto" { "Automatic (recommended)".to_string() } else { backend }/>
            </div>
        </div>
    }
}

#[component]
fn ReviewRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between gap-4 py-3 px-4">
            <span class="text-sm text-muted-foreground shrink-0">{label}</span>
            <span class="text-sm font-medium min-w-0 truncate">{value}</span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_dataset() -> DatasetResponse {
        DatasetResponse {
            schema_version: "1".to_string(),
            id: "ds-1".to_string(),
            dataset_version_id: Some("dsv-1".to_string()),
            name: "dataset".to_string(),
            description: None,
            format: "jsonl".to_string(),
            hash_b3: None,
            dataset_hash_b3: None,
            storage_path: None,
            status: "ready".to_string(),
            workspace_id: None,
            validation_status: Some("valid".to_string()),
            validation_errors: None,
            validation_diagnostics: None,
            trust_state: Some("allowed".to_string()),
            file_count: None,
            total_size_bytes: None,
            dataset_type: None,
            created_by: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: None,
            display_name: None,
        }
    }

    #[test]
    fn evaluate_gate_does_not_mark_empty_when_sample_count_unknown() {
        let dataset = ready_dataset();
        let state = evaluate_knowledge_gate(
            &dataset.id,
            dataset.dataset_version_id.as_deref(),
            None,
            Some(&dataset),
            false,
            false,
            false,
        );
        assert_eq!(state, KnowledgeGateState::Ready);
    }

    #[test]
    fn evaluate_gate_marks_empty_when_sample_count_is_zero() {
        let dataset = ready_dataset();
        let state = evaluate_knowledge_gate(
            &dataset.id,
            dataset.dataset_version_id.as_deref(),
            Some(0),
            Some(&dataset),
            false,
            false,
            false,
        );
        assert_eq!(state, KnowledgeGateState::DatasetEmpty);
    }
}
