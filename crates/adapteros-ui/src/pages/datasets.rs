//! Datasets management page
//!
//! Provides UI for managing training datasets - listing, viewing,
//! and deleting datasets used for adapter training.

use crate::api::{
    ApiClient, DatasetListResponse, DatasetPreviewResponse, DatasetSafetyCheckResult,
    DatasetStatisticsResponse, DatasetVersionsResponse,
};
use crate::components::{
    Badge, BadgeVariant, BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, Card, Checkbox,
    Combobox, ComboboxOption, ConfirmationDialog, ConfirmationSeverity, CopyableId, EmptyState,
    ErrorDisplay, Input, LoadingDisplay, PageBreadcrumbItem, PageHeader, PageScaffold,
    PageScaffoldActions, RefreshButton, Select, Spinner, TabNav, TabPanel, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow, Toggle,
};
use crate::hooks::{
    use_api, use_api_resource, use_delete_dialog, DeleteDialogState, LoadingState, Refetch,
};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use crate::utils::{format_bytes, format_date};
#[cfg(target_arch = "wasm32")]
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};
#[cfg(target_arch = "wasm32")]
use serde_json::json;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// =============================================================================
// Trainability / Readiness Helpers
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
struct DatasetGating {
    is_trainable: bool,
    readiness_label: &'static str,
    readiness_variant: BadgeVariant,
    inline_reason: String,
    status_ok: bool,
    validation_ok: bool,
    trust_ok: bool,
}

fn is_status_ok(status: &str) -> bool {
    matches!(status, "ready" | "indexed")
}

fn is_validation_ok(validation_status: Option<&str>) -> bool {
    validation_status == Some("valid")
}

fn is_trust_ok(trust_state: Option<&str>) -> bool {
    match trust_state {
        None => true,
        Some(s) => matches!(s, "allowed" | "trusted" | "approved"),
    }
}

fn dataset_gating(ds: &crate::api::DatasetResponse) -> DatasetGating {
    let status_ok = is_status_ok(ds.status.as_str());
    let validation_ok = is_validation_ok(ds.validation_status.as_deref());
    let trust_ok = is_trust_ok(ds.trust_state.as_deref());
    let is_trainable = status_ok && validation_ok && trust_ok;

    let (readiness_label, readiness_variant, inline_reason) = if is_trainable {
        (
            "Trainable",
            BadgeVariant::Success,
            "Ready to train".to_string(),
        )
    } else if !status_ok {
        let variant = match ds.status.as_str() {
            "processing" => BadgeVariant::Warning,
            "failed" | "error" => BadgeVariant::Destructive,
            _ => BadgeVariant::Secondary,
        };
        (
            "Not ready",
            variant,
            "Needs: status ready/indexed".to_string(),
        )
    } else if !validation_ok {
        let label = "Needs validation";
        let variant = match ds.validation_status.as_deref() {
            Some("pending") | Some("processing") => BadgeVariant::Warning,
            Some("invalid") | Some("failed") => BadgeVariant::Destructive,
            Some(_) => BadgeVariant::Secondary,
            None => BadgeVariant::Secondary,
        };
        let reason = match ds.validation_status.as_deref() {
            Some(s) => format!("Needs: validation ({})", s),
            None => "Needs: validation".to_string(),
        };
        (label, variant, reason)
    } else if !trust_ok {
        let variant = match ds.trust_state.as_deref() {
            Some("blocked") => BadgeVariant::Destructive,
            Some("needs_approval") | Some("allowed_with_warning") => BadgeVariant::Warning,
            _ => BadgeVariant::Secondary,
        };
        let reason = match ds.trust_state.as_deref() {
            Some(s) => format!("Needs: trust ({})", s),
            None => "Needs: trust".to_string(),
        };
        ("Needs trust", variant, reason)
    } else {
        (
            "Not trainable",
            BadgeVariant::Secondary,
            "Blocked".to_string(),
        )
    };

    DatasetGating {
        is_trainable,
        readiness_label,
        readiness_variant,
        inline_reason,
        status_ok,
        validation_ok,
        trust_ok,
    }
}

fn dataset_type_label(ds: &crate::api::DatasetResponse) -> &'static str {
    match ds.dataset_type.as_deref() {
        Some("identity") => "Identity",
        _ => "Standard",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatasetViewMode {
    Table,
    Cards,
}

impl DatasetViewMode {
    fn from_query(q: Option<&str>) -> Self {
        match q {
            Some("cards") => Self::Cards,
            _ => Self::Table,
        }
    }

    fn as_query_value(&self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Cards => "cards",
        }
    }
}

fn build_query_string(map: &HashMap<String, String>) -> String {
    if map.is_empty() {
        return String::new();
    }
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    let mut parts = Vec::new();
    for k in keys {
        if let Some(v) = map.get(&k) {
            if v.is_empty() {
                continue;
            }
            let key = js_sys::encode_uri_component(&k);
            let val = js_sys::encode_uri_component(v);
            parts.push(format!("{}={}", key, val));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

#[cfg(target_arch = "wasm32")]
fn scroll_to_element_id(id: &str) {
    let id = id.to_string();
    leptos::task::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id(&id) {
                    // Keep this conservative: `ScrollIntoViewOptions` isn't available in all
                    // `web-sys` feature sets in this repo. Basic scroll keeps the UX acceptable.
                    el.scroll_into_view();
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn scroll_to_element_id(_id: &str) {}

fn set_query_param(map: &mut HashMap<String, String>, key: &str, value: &str) {
    if value.trim().is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value.to_string());
    }
}

/// Datasets list page
#[component]
pub fn Datasets() -> impl IntoView {
    let (datasets, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_datasets(None).await });

    let show_upload_dialog = RwSignal::new(false);
    let query = use_query_map();
    let navigate = use_navigate();
    let view_mode = RwSignal::new(DatasetViewMode::Table);

    // Keep view mode in sync with query param (e.g. ?view=cards)
    Effect::new(move |_| {
        let q = query.get();
        let mode = DatasetViewMode::from_query(q.get("view").as_deref());
        if view_mode.get_untracked() != mode {
            view_mode.set(mode);
        }
    });

    let on_upload = Callback::new(move |_| show_upload_dialog.set(true));
    let navigate_for_view = navigate.clone();

    let on_set_view_mode = Callback::new({
        let navigate = navigate.clone();
        move |mode: DatasetViewMode| {
            view_mode.set(mode);
            let mut params: HashMap<String, String> = query
                .get()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            match mode {
                DatasetViewMode::Table => {
                    params.remove("view");
                }
                DatasetViewMode::Cards => {
                    params.insert("view".to_string(), mode.as_query_value().to_string());
                }
            }
            let qs = build_query_string(&params);
            navigate(&format!("/datasets{}", qs), Default::default());
        }
    });

    let on_dataset_uploaded = Callback::new(move |outcome: DatasetUploadOutcome| {
        refetch.run(());
        navigate(
            &format!("/datasets/{}", outcome.dataset_id),
            Default::default(),
        );
    });

    view! {
        <PageScaffold
            title="Datasets"
            subtitle="Manage training datasets for adapter fine-tuning"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Data", "/datasets"),
                PageBreadcrumbItem::current("Datasets"),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Ghost
                    on_click=Callback::new({
                        let navigate = navigate_for_view.clone();
                        move |_| navigate("/adapters", Default::default())
                    })
                >
                    "View Adapters"
                </Button>
                <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_upload_dialog.set(true))
                >
                    "Upload Dataset"
                </Button>
            </PageScaffoldActions>

            {move || {
                match datasets.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading datasets..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <DatasetsList
                                datasets=data
                                refetch=refetch
                                on_upload=on_upload
                                view_mode=view_mode
                                on_set_view_mode=on_set_view_mode
                            />
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

            <DatasetUploadWizard
                open=show_upload_dialog
                on_complete=on_dataset_uploaded
            />
        </PageScaffold>
    }
}

/// List of datasets component
#[component]
fn DatasetsList(
    datasets: DatasetListResponse,
    refetch: Refetch,
    on_upload: Callback<()>,
    view_mode: RwSignal<DatasetViewMode>,
    on_set_view_mode: Callback<DatasetViewMode>,
) -> impl IntoView {
    let all_datasets = StoredValue::new(datasets.datasets);
    let navigate = use_navigate();
    let navigate_store = StoredValue::new(navigate);
    let query = use_query_map();

    // Filters (client-side)
    let search = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let validation_filter = RwSignal::new(String::new());
    let trust_filter = RwSignal::new(String::new());
    let type_filter = RwSignal::new(String::new());
    let trainable_only = RwSignal::new(false);
    let sort = RwSignal::new("created_desc".to_string());
    let filters_initialized = RwSignal::new(false);
    let last_emitted_query = RwSignal::new(String::new());

    // Initialize filters from query params once (so links are shareable).
    Effect::new(move |_| {
        if filters_initialized.get() {
            return;
        }
        let q = query.get();
        if let Some(v) = q.get("q") {
            search.set(v.clone());
        }
        if let Some(v) = q.get("status") {
            status_filter.set(v.clone());
        }
        if let Some(v) = q.get("validation") {
            validation_filter.set(v.clone());
        }
        if let Some(v) = q.get("trust") {
            trust_filter.set(v.clone());
        }
        if let Some(v) = q.get("type") {
            type_filter.set(v.clone());
        }
        if let Some(v) = q.get("sort") {
            sort.set(v.clone());
        }
        if q.get("trainable").as_deref() == Some("1") {
            trainable_only.set(true);
        }
        filters_initialized.set(true);
    });

    // Persist filters to the URL query string (replace, not push).
    Effect::new(move |_| {
        if !filters_initialized.get() {
            return;
        }

        // Track dependencies
        let qv = search.get();
        let sv = status_filter.get();
        let vv = validation_filter.get();
        let tv = trust_filter.get();
        let ty = type_filter.get();
        let so = sort.get();
        let tr = trainable_only.get();

        let mut params: HashMap<String, String> = query
            .get()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        set_query_param(&mut params, "q", qv.trim());
        set_query_param(&mut params, "status", sv.trim());
        set_query_param(&mut params, "validation", vv.trim());
        set_query_param(&mut params, "trust", tv.trim());
        set_query_param(&mut params, "type", ty.trim());
        set_query_param(&mut params, "sort", so.trim());
        set_query_param(&mut params, "trainable", if tr { "1" } else { "" });

        let qs = build_query_string(&params);
        // Avoid churn by only navigating when the computed query actually changes.
        if last_emitted_query.get_untracked() == qs {
            return;
        }
        last_emitted_query.set(qs.clone());
        navigate_store.with_value(|nav| {
            nav(
                &format!("/datasets{}", qs),
                leptos_router::NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        });
    });

    // Delete confirmation dialog state using reusable hook
    let client = use_api();
    let delete_state = use_delete_dialog();
    let delete_state_for_cancel = delete_state.clone();
    let on_cancel_delete = Callback::new(move |_| delete_state_for_cancel.cancel());
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
                            refetch.run(());
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
    let delete_state_for_loading = delete_state.clone();

    // Readiness counts (computed on full list, not filtered list)
    let trainable_count = Signal::derive(move || {
        all_datasets
            .get_value()
            .iter()
            .filter(|d| dataset_gating(d).is_trainable)
            .count()
    });
    let needs_validation_count = Signal::derive(move || {
        all_datasets
            .get_value()
            .iter()
            .filter(|d| {
                let g = dataset_gating(d);
                g.status_ok && !g.validation_ok
            })
            .count()
    });
    let needs_trust_count = Signal::derive(move || {
        all_datasets
            .get_value()
            .iter()
            .filter(|d| {
                let g = dataset_gating(d);
                g.status_ok && g.validation_ok && !g.trust_ok
            })
            .count()
    });
    let processing_failed_count = Signal::derive(move || {
        all_datasets
            .get_value()
            .iter()
            .filter(|d| matches!(d.status.as_str(), "processing" | "failed" | "error"))
            .count()
    });

    // Quick filter actions (readiness strip)
    let on_quick_trainable = Callback::new(move |_| {
        trainable_only.set(true);
        status_filter.set(String::new());
        validation_filter.set(String::new());
        trust_filter.set(String::new());
        type_filter.set(String::new());
    });
    let on_quick_needs_validation = Callback::new(move |_| {
        trainable_only.set(false);
        status_filter.set("ready_indexed".to_string());
        validation_filter.set("not_valid".to_string());
        trust_filter.set(String::new());
        type_filter.set(String::new());
    });
    let on_quick_needs_trust = Callback::new(move |_| {
        trainable_only.set(false);
        status_filter.set("ready_indexed".to_string());
        validation_filter.set("valid".to_string());
        trust_filter.set("not_ok".to_string());
        type_filter.set(String::new());
    });
    let on_quick_processing_failed = Callback::new(move |_| {
        trainable_only.set(false);
        status_filter.set("processing_failed".to_string());
        validation_filter.set(String::new());
        trust_filter.set(String::new());
        type_filter.set(String::new());
    });

    let filtered_sorted = Signal::derive(move || {
        let mut items = all_datasets.get_value();

        let q = search.get().trim().to_lowercase();
        let status = status_filter.get();
        let validation = validation_filter.get();
        let trust = trust_filter.get();
        let dtype = type_filter.get();
        let only_trainable = trainable_only.get();
        let sort = sort.get();

        items.retain(|d| {
            if !q.is_empty() {
                let hay = format!("{} {}", d.name.to_lowercase(), d.id.to_lowercase());
                if !hay.contains(&q) {
                    return false;
                }
            }

            // Type
            if !dtype.is_empty() {
                match dtype.as_str() {
                    "identity" => {
                        if d.dataset_type.as_deref() != Some("identity") {
                            return false;
                        }
                    }
                    "standard" => {
                        if d.dataset_type.as_deref() == Some("identity") {
                            return false;
                        }
                    }
                    _ => {}
                }
            }

            // Status
            if !status.is_empty() {
                match status.as_str() {
                    "ready" => {
                        if d.status != "ready" {
                            return false;
                        }
                    }
                    "indexed" => {
                        if d.status != "indexed" {
                            return false;
                        }
                    }
                    "processing" => {
                        if d.status != "processing" {
                            return false;
                        }
                    }
                    "failed" => {
                        if !(d.status == "failed" || d.status == "error") {
                            return false;
                        }
                    }
                    "ready_indexed" => {
                        if !is_status_ok(d.status.as_str()) {
                            return false;
                        }
                    }
                    "processing_failed" => {
                        if !matches!(d.status.as_str(), "processing" | "failed" | "error") {
                            return false;
                        }
                    }
                    "other" => {
                        if matches!(
                            d.status.as_str(),
                            "ready" | "indexed" | "processing" | "failed" | "error"
                        ) {
                            return false;
                        }
                    }
                    _ => {}
                }
            }

            // Validation
            if !validation.is_empty() {
                match validation.as_str() {
                    "valid" => {
                        if d.validation_status.as_deref() != Some("valid") {
                            return false;
                        }
                    }
                    "invalid" => {
                        if !matches!(
                            d.validation_status.as_deref(),
                            Some("invalid") | Some("failed")
                        ) {
                            return false;
                        }
                    }
                    "pending" => {
                        if !matches!(
                            d.validation_status.as_deref(),
                            Some("pending") | Some("processing")
                        ) {
                            return false;
                        }
                    }
                    "unknown" => {
                        if d.validation_status.is_some() {
                            return false;
                        }
                    }
                    "not_valid" => {
                        if d.validation_status.as_deref() == Some("valid") {
                            return false;
                        }
                    }
                    _ => {}
                }
            }

            // Trust
            if !trust.is_empty() {
                match trust.as_str() {
                    "ok" => {
                        if !is_trust_ok(d.trust_state.as_deref()) {
                            return false;
                        }
                    }
                    "needs_approval" => {
                        if d.trust_state.as_deref() != Some("needs_approval") {
                            return false;
                        }
                    }
                    "blocked" => {
                        if d.trust_state.as_deref() != Some("blocked") {
                            return false;
                        }
                    }
                    "unknown" => {
                        if d.trust_state.is_some() {
                            return false;
                        }
                    }
                    "not_ok" => {
                        if is_trust_ok(d.trust_state.as_deref()) {
                            return false;
                        }
                    }
                    _ => {}
                }
            }

            if only_trainable && !dataset_gating(d).is_trainable {
                return false;
            }

            true
        });

        // Sort
        match sort.as_str() {
            "created_asc" => items.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            "name_asc" => items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            _ => items.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        }

        items
    });

    let total_filtered = Signal::derive(move || filtered_sorted.get().len());
    let any_filters_active = Signal::derive(move || {
        !search.get().trim().is_empty()
            || !status_filter.get().trim().is_empty()
            || !validation_filter.get().trim().is_empty()
            || !trust_filter.get().trim().is_empty()
            || !type_filter.get().trim().is_empty()
            || sort.get().trim() != "created_desc"
            || trainable_only.get()
    });

    let dataset_count_total = datasets.total;
    let show_empty = Signal::derive(move || all_datasets.get_value().is_empty());
    let delete_state_for_views = delete_state.clone();

    view! {
        <Show
            when=move || show_empty.get()
            fallback=move || {
                let delete_state = delete_state_for_views.clone();
                view! {
                    <div class="space-y-4">
                        // Readiness strip
                        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                            <button
                                class="text-left"
                                on:click={
                                    let cb = on_quick_trainable;
                                    move |_| cb.run(())
                                }
                                title="Show trainable datasets"
                            >
                                <Card>
                                    <div class="p-4 space-y-1 hover:bg-muted/40 transition-colors rounded-md">
                                        <div class="text-xs text-muted-foreground">"Trainable"</div>
                                        <div class="flex items-end justify-between">
                                            <div class="heading-3">{move || trainable_count.get().to_string()}</div>
                                            <Badge variant=BadgeVariant::Success>"Ready"</Badge>
                                        </div>
                                        <div class="text-xs text-muted-foreground">
                                            "Ready for training"
                                        </div>
                                    </div>
                                </Card>
                            </button>
                            <button
                                class="text-left"
                                on:click={
                                    let cb = on_quick_needs_validation;
                                    move |_| cb.run(())
                                }
                                title="Show datasets that need validation"
                            >
                                <Card>
                                    <div class="p-4 space-y-1 hover:bg-muted/40 transition-colors rounded-md">
                                        <div class="text-xs text-muted-foreground">"Needs validation"</div>
                                        <div class="flex items-end justify-between">
                                            <div class="heading-3">{move || needs_validation_count.get().to_string()}</div>
                                            <Badge variant=BadgeVariant::Warning>"Check"</Badge>
                                        </div>
                                        <div class="text-xs text-muted-foreground">
                                            "Fix format or re-validate"
                                        </div>
                                    </div>
                                </Card>
                            </button>
                            <button
                                class="text-left"
                                on:click={
                                    let cb = on_quick_needs_trust;
                                    move |_| cb.run(())
                                }
                                title="Show datasets blocked by trust/approval"
                            >
                                <Card>
                                    <div class="p-4 space-y-1 hover:bg-muted/40 transition-colors rounded-md">
                                        <div class="text-xs text-muted-foreground">"Needs trust"</div>
                                        <div class="flex items-end justify-between">
                                            <div class="heading-3">{move || needs_trust_count.get().to_string()}</div>
                                            <Badge variant=BadgeVariant::Warning>"Gate"</Badge>
                                        </div>
                                        <div class="text-xs text-muted-foreground">
                                            "Approval required"
                                        </div>
                                    </div>
                                </Card>
                            </button>
                            <button
                                class="text-left"
                                on:click={
                                    let cb = on_quick_processing_failed;
                                    move |_| cb.run(())
                                }
                                title="Show datasets still processing or failed"
                            >
                                <Card>
                                    <div class="p-4 space-y-1 hover:bg-muted/40 transition-colors rounded-md">
                                        <div class="text-xs text-muted-foreground">"Processing / Failed"</div>
                                        <div class="flex items-end justify-between">
                                            <div class="heading-3">{move || processing_failed_count.get().to_string()}</div>
                                            <Badge variant=BadgeVariant::Secondary>"Status"</Badge>
                                        </div>
                                        <div class="text-xs text-muted-foreground">
                                            "Wait or investigate"
                                        </div>
                                    </div>
                                </Card>
                            </button>
                        </div>

                        // Filter bar + view toggle
                        <Card>
                            <div class="p-4 space-y-3">
                                <div class="flex items-center justify-between gap-4 flex-wrap">
                                    <div class="text-sm text-muted-foreground">
                                        {move || {
                                            let filtered = total_filtered.get();
                                            if filtered as i64 == dataset_count_total {
                                                format!("{} dataset(s)", dataset_count_total)
                                            } else {
                                                format!("{} of {} dataset(s)", filtered, dataset_count_total)
                                            }
                                        }}
                                    </div>
                                    <div class="inline-flex rounded-md border border-border overflow-hidden">
                                        <button
                                            type="button"
                                            class=move || {
                                                if view_mode.get() == DatasetViewMode::Table {
                                                    "px-3 py-1.5 text-xs bg-muted"
                                                } else {
                                                    "px-3 py-1.5 text-xs hover:bg-muted/60"
                                                }
                                            }
                                            on:click={
                                                let cb = on_set_view_mode;
                                                move |_| cb.run(DatasetViewMode::Table)
                                            }
                                        >
                                            "Table"
                                        </button>
                                        <button
                                            type="button"
                                            class=move || {
                                                if view_mode.get() == DatasetViewMode::Cards {
                                                    "px-3 py-1.5 text-xs bg-muted"
                                                } else {
                                                    "px-3 py-1.5 text-xs hover:bg-muted/60"
                                                }
                                            }
                                            on:click={
                                                let cb = on_set_view_mode;
                                                move |_| cb.run(DatasetViewMode::Cards)
                                            }
                                        >
                                            "Cards"
                                        </button>
                                    </div>
                                </div>

                                <div class="flex flex-wrap items-center gap-3">
                                    <Input
                                        value=search
                                        placeholder="Search by name or ID..."
                                        class="w-64"
                                    />

                                    <Select
                                        value=status_filter
                                        options=vec![
                                            ("".to_string(), "All Status".to_string()),
                                            ("ready_indexed".to_string(), "Ready/Indexed".to_string()),
                                            ("ready".to_string(), "Ready".to_string()),
                                            ("indexed".to_string(), "Indexed".to_string()),
                                            ("processing".to_string(), "Processing".to_string()),
                                            ("failed".to_string(), "Failed/Error".to_string()),
                                            ("other".to_string(), "Other".to_string()),
                                        ]
                                        class="w-40".to_string()
                                    />
                                    <span class="text-xs text-muted-foreground">
                                        "Other = anything not ready/indexed/processing/failed/error"
                                    </span>
                                    <Select
                                        value=validation_filter
                                        options=vec![
                                            ("".to_string(), "All Validation".to_string()),
                                            ("valid".to_string(), "Valid".to_string()),
                                            ("not_valid".to_string(), "Needs Validation".to_string()),
                                            ("invalid".to_string(), "Invalid/Failed".to_string()),
                                            ("pending".to_string(), "Pending/Processing".to_string()),
                                            ("unknown".to_string(), "Unknown".to_string()),
                                        ]
                                        class="w-44".to_string()
                                    />
                                    <Select
                                        value=trust_filter
                                        options=vec![
                                            ("".to_string(), "All Trust".to_string()),
                                            ("ok".to_string(), "Allowed".to_string()),
                                            ("not_ok".to_string(), "Needs Trust".to_string()),
                                            ("needs_approval".to_string(), "Needs Approval".to_string()),
                                            ("blocked".to_string(), "Blocked".to_string()),
                                            ("unknown".to_string(), "Unknown".to_string()),
                                        ]
                                        class="w-40".to_string()
                                    />
                                    <Select
                                        value=type_filter
                                        options=vec![
                                            ("".to_string(), "All Types".to_string()),
                                            ("standard".to_string(), "Standard".to_string()),
                                            ("identity".to_string(), "Identity".to_string()),
                                        ]
                                        class="w-36".to_string()
                                    />
                                    <Select
                                        value=sort
                                        options=vec![
                                            ("created_desc".to_string(), "Created (newest)".to_string()),
                                            ("created_asc".to_string(), "Created (oldest)".to_string()),
                                            ("name_asc".to_string(), "Name (A-Z)".to_string()),
                                        ]
                                        class="w-44".to_string()
                                    />
                                    <Toggle
                                        checked=trainable_only
                                        label="Trainable only".to_string()
                                        class="w-auto"
                                    />
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        disabled=Signal::derive(move || !any_filters_active.get())
                                        on_click=Callback::new(move |_| {
                                            search.set(String::new());
                                            status_filter.set(String::new());
                                            validation_filter.set(String::new());
                                            trust_filter.set(String::new());
                                            type_filter.set(String::new());
                                            sort.set("created_desc".to_string());
                                            trainable_only.set(false);
                                        })
                                    >
                                        "Clear filters"
                                    </Button>
                                </div>
                            </div>
                        </Card>

                        // Results (table or cards)
                        {move || {
                            let items = filtered_sorted.get();
                            if items.is_empty() {
                                view! {
                                    <Card>
                                        <EmptyState
                                            title="No datasets match your filters"
                                            description="Try clearing filters or searching by ID."
                                        />
                                    </Card>
                                }.into_any()
                            } else if view_mode.get() == DatasetViewMode::Cards {
                                view! { <DatasetsCardGrid datasets=items delete_state=delete_state.clone()/> }.into_any()
                            } else {
                                view! { <DatasetsTable datasets=items delete_state=delete_state.clone()/> }.into_any()
                            }
                        }}
                    </div>
                }.into_any()
            }
        >
            <Card>
                <div class="py-10 px-6 text-center space-y-3">
                    <div class="heading-3">"No datasets yet"</div>
                    <p class="text-sm text-muted-foreground max-w-xl mx-auto">
                        "Start from documents (recommended) or upload a dataset file directly."
                    </p>
                    <div class="flex items-center justify-center gap-2 pt-2">
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new({
                                let nav_store = navigate_store;
                                move |_| nav_store.with_value(|nav| nav("/documents", Default::default()))
                            })
                        >
                            "Upload Documents"
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new({
                                let on_upload = on_upload;
                                move |_| on_upload.run(())
                            })
                        >
                            "Upload Dataset"
                        </Button>
                    </div>
                </div>
            </Card>
        </Show>

        <ConfirmationDialog
            open=delete_state.show
            title="Delete Dataset"
            description=format!("Are you sure you want to delete this dataset? This action cannot be undone.")
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            cancel_text="Cancel"
            on_confirm=on_confirm_delete
            on_cancel=on_cancel_delete
            loading=Signal::derive(move || delete_state_for_loading.is_deleting())
        />
    }
    .into_any()
}

#[component]
fn DatasetsTable(
    datasets: Vec<crate::api::DatasetResponse>,
    delete_state: DeleteDialogState,
) -> impl IntoView {
    let navigate = use_navigate();

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Readiness"</TableHead>
                        <TableHead>"Type"</TableHead>
                        <TableHead>"Format"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {datasets.into_iter().map(|dataset| {
                        let gating = dataset_gating(&dataset);
                        let readiness_label = gating.readiness_label;
                        let readiness_variant = gating.readiness_variant;
                        let is_trainable = gating.is_trainable;
                        let inline_reason = gating.inline_reason.clone();
                        let id_for_nav = dataset.id.clone();
                        let id_for_train = dataset.id.clone();
                        let id_for_delete = dataset.id.clone();
                        let name = dataset.name.clone();
                        let name_for_delete = name.clone();
                        let name_for_title = name.clone();
                        let name_for_view_aria = name.clone();
                        let name_for_train_aria = name.clone();
                        let inline_reason_for_readiness = inline_reason.clone();
                        let inline_reason_for_actions = inline_reason.clone();

                        let size_display = dataset
                            .total_size_bytes
                            .map(format_bytes)
                            .unwrap_or_else(|| "—".to_string());

                        let nav = navigate.clone();
                        let nav_for_train = navigate.clone();
                        let delete_state = delete_state.clone();

                        let train_tooltip: String = if is_trainable {
                            "Train an adapter from this dataset".to_string()
                        } else {
                            inline_reason.clone()
                        };

                        view! {
                            <TableRow>
                                <TableCell>
                                    <button
                                        class="font-medium text-primary hover:underline text-left truncate"
                                        title=name_for_title
                                        aria-label=format!("View dataset {}", name_for_view_aria.as_str())
                                        on:click=move |_| {
                                            nav(&format!("/datasets/{}", id_for_nav), Default::default());
                                        }
                                    >
                                        {name.clone()}
                                    </button>
                                    <div class="text-xs text-muted-foreground font-mono truncate max-w-xs">
                                        {adapteros_id::short_id(&dataset.id)}
                                    </div>
                                </TableCell>
                                <TableCell>
                                    <div class="space-y-1">
                                        <Badge variant=readiness_variant>
                                            {readiness_label}
                                        </Badge>
                                        <div class="text-xs text-muted-foreground">
                                            {inline_reason_for_readiness.clone()}
                                        </div>
                                        {dataset.validation_errors.as_ref()
                                            .map(|errs| errs.len())
                                            .filter(|count| *count > 0)
                                            .map(|count| view! {
                                                <div class="text-xs text-destructive">
                                                    {format!("{} validation error(s)", count)}
                                                </div>
                                            })}
                                    </div>
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
                                    <span class="text-sm">{size_display}</span>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">{format_date(&dataset.created_at)}</span>
                                </TableCell>
                                <TableCell class="text-right">
                                    <div class="inline-flex items-center gap-2 justify-end">
                                        <span class="inline-flex items-center gap-1" title=train_tooltip>
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                aria_label=format!("Train adapter from {}", name_for_train_aria.clone())
                                                disabled=!is_trainable
                                                on_click=Callback::new({
                                                    let nav = nav_for_train.clone();
                                                    let id = id_for_train.clone();
                                                    move |_| {
                                                        nav(&format!("/training?open_wizard=1&dataset_id={}", id), Default::default());
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
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                >
                                                    <polygon points="5 3 19 12 5 21 5 3"/>
                                                </svg>
                                            </Button>
                                            <span class="text-xs text-muted-foreground hidden md:inline">
                                                {if is_trainable { "Train".to_string() } else { inline_reason_for_actions.clone() }}
                                            </span>
                                        </span>
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            aria_label=format!("Delete dataset {}", name_for_delete.clone())
                                            on_click=Callback::new(move |_| {
                                                delete_state.confirm(id_for_delete.clone(), name_for_delete.clone());
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
                                    </div>
                                </TableCell>
                            </TableRow>
                        }
                    }).collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
}

#[component]
fn DatasetsCardGrid(
    datasets: Vec<crate::api::DatasetResponse>,
    delete_state: DeleteDialogState,
) -> impl IntoView {
    let navigate = use_navigate();

    view! {
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {datasets.into_iter().map(|dataset| {
                let gating = dataset_gating(&dataset);
                let readiness_label = gating.readiness_label;
                let readiness_variant = gating.readiness_variant;
                let is_trainable = gating.is_trainable;
                let inline_reason = gating.inline_reason.clone();

                let name = dataset.name.clone();
                let created_at = dataset.created_at.clone();
                let format = dataset.format.clone();
                let status = dataset.status.clone();
                let dtype = dataset_type_label(&dataset).to_string();
                let validation_status = dataset.validation_status.clone();
                let trust_state = dataset.trust_state.clone();

                let status_variant = match status.as_str() {
                    "ready" | "indexed" => BadgeVariant::Success,
                    "processing" => BadgeVariant::Warning,
                    "failed" | "error" => BadgeVariant::Destructive,
                    _ => BadgeVariant::Secondary,
                };
                let validation_variant = validation_status.as_deref().map(validation_badge_variant);
                let trust_variant = trust_state.as_deref().map(trust_state_badge_variant);

                let id_for_nav = dataset.id.clone();
                let id_for_train = dataset.id.clone();
                let id_for_delete = dataset.id.clone();
                let name_for_delete = name.clone();

                let nav_to_detail = navigate.clone();
                let nav_to_issues = navigate.clone();
                let nav_for_train = navigate.clone();
                let delete_state = delete_state.clone();

                let size_display = dataset
                    .total_size_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string());

                view! {
                    <Card>
                        <div class="p-4 space-y-3">
                            <div class="flex items-start justify-between gap-3">
                                <div class="min-w-0">
                                    <button
                                        class="font-medium text-primary hover:underline text-left truncate w-full"
                                        on:click=move |_| nav_to_detail(&format!("/datasets/{}", id_for_nav), Default::default())
                                        title=name.clone()
                                    >
                                        {name.clone()}
                                    </button>
                                    <div class="text-xs text-muted-foreground">
                                        {format_date(&created_at)} " · " {size_display}
                                    </div>
                                </div>
                                <Badge variant=readiness_variant>{readiness_label}</Badge>
                            </div>

                            <div class="flex flex-wrap items-center gap-1.5">
                                <Badge variant=if dtype == "Identity" { BadgeVariant::Secondary } else { BadgeVariant::Outline }>
                                    {dtype.clone()}
                                </Badge>
                                <Badge variant=status_variant>{status.clone()}</Badge>
                                {validation_status.clone().map(|s| {
                                    let v = validation_variant.unwrap_or(BadgeVariant::Secondary);
                                    view! { <Badge variant=v>{format!("validation: {}", s)}</Badge> }
                                })}
                                {trust_state.clone().map(|s| {
                                    let v = trust_variant.unwrap_or(BadgeVariant::Secondary);
                                    view! { <Badge variant=v>{format!("trust: {}", s)}</Badge> }
                                })}
                                <Badge variant=BadgeVariant::Outline>{format.clone()}</Badge>
                            </div>

                            <div class="text-xs text-muted-foreground">
                                {inline_reason.clone()}
                            </div>

                            <div class="flex items-center justify-between gap-2 pt-1">
                                {if is_trainable {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Primary
                                            on_click=Callback::new(move |_| {
                                                nav_for_train(&format!("/training?open_wizard=1&dataset_id={}", id_for_train), Default::default());
                                            })
                                        >
                                            "Train Adapter"
                                        </Button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=Callback::new(move |_| {
                                                nav_to_issues(&format!("/datasets/{}?tab=issues#validation-errors", id_for_train), Default::default());
                                            })
                                        >
                                            "View Issues"
                                        </Button>
                                    }.into_any()
                                }}
                                <Button
                                    variant=ButtonVariant::Ghost
                                    aria_label=format!("Delete dataset {}", name_for_delete.clone())
                                    on_click=Callback::new(move |_| {
                                        delete_state.confirm(id_for_delete.clone(), name_for_delete.clone());
                                    })
                                >
                                    "Delete"
                                </Button>
                            </div>
                        </div>
                    </Card>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DatasetDetailTab {
    Preview,
    Issues,
    Versions,
    Details,
}

impl DatasetDetailTab {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "preview" => Some(Self::Preview),
            "issues" => Some(Self::Issues),
            "versions" => Some(Self::Versions),
            "details" => Some(Self::Details),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Issues => "issues",
            Self::Versions => "versions",
            Self::Details => "details",
        }
    }
}

impl fmt::Display for DatasetDetailTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
                .unwrap_or_else(|_| raw)
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

    // Detail tab state (progressive disclosure)
    let active_tab = RwSignal::new(DatasetDetailTab::Preview);
    let tab_initialized = RwSignal::new(false);

    // Initialize tab from query param (?tab=issues), once.
    Effect::new(move |_| {
        if tab_initialized.get() {
            return;
        }
        if let Some(raw) = query.get().get("tab") {
            if let Some(tab) = DatasetDetailTab::from_str(raw.as_str()) {
                active_tab.set(tab);
                tab_initialized.set(true);
            }
        }
    });

    // Keep ?tab= in sync when user clicks tabs (replace, not push).
    Effect::new(move |_| {
        if !tab_initialized.get() {
            return;
        }
        let tab = active_tab.get();
        let mut params: HashMap<String, String> = query
            .get()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        params.insert("tab".to_string(), tab.as_str().to_string());
        let qs = build_query_string(&params);
        navigate_store.with_value(|nav| {
            nav(
                &format!("/datasets/{}{}", dataset_id(), qs),
                leptos_router::NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        });
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

    // Preview state (minimal sample rows for sanity check)
    let preview_limit = RwSignal::new("10".to_string());
    let pretty_json = RwSignal::new(true);
    let (preview, preview_refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = dataset_id();
        let limit_raw = preview_limit.get();
        async move {
            let limit = limit_raw.parse::<usize>().ok();
            client.preview_dataset(&id, limit).await
        }
    });

    // If no explicit tab is requested, pick a default based on trainability.
    Effect::new(move |_| {
        if tab_initialized.get() {
            return;
        }
        if let LoadingState::Loaded(data) = dataset.get() {
            let g = dataset_gating(&data);
            if g.status_ok && (!g.validation_ok || !g.trust_ok) {
                active_tab.set(DatasetDetailTab::Issues);
            } else {
                active_tab.set(DatasetDetailTab::Preview);
            }
            tab_initialized.set(true);
        }
    });

    let refetch_trigger = RwSignal::new(0u32);
    let refetch_stored = StoredValue::new(refetch);
    let stats_refetch_stored = StoredValue::new(stats_refetch);
    let versions_refetch_stored = StoredValue::new(versions_refetch);
    let preview_refetch_stored = StoredValue::new(preview_refetch);

    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch_stored.with_value(|f| f.run(()));
        stats_refetch_stored.with_value(|f| f.run(()));
        versions_refetch_stored.with_value(|f| f.run(()));
        preview_refetch_stored.with_value(|f| f.run(()));
    });

    // Refetch preview when the limit changes
    Effect::new(move |_| {
        let _ = preview_limit.get();
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
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
                            let gating = dataset_gating(&data);
                            let dataset_version_id = data.dataset_version_id.clone();
                            let dataset_version_id_display =
                                dataset_version_id.clone().unwrap_or_else(|| "—".to_string());
                            let dataset_version_id_store = StoredValue::new(dataset_version_id.clone());
                            let trust_state = data.trust_state.clone();
                            let dataset_id_for_train = data.id.clone();

                            view! {
                                <div class="space-y-4">
                                    <PageHeader
                                        title=data.name.clone()
                                        subtitle=data.description.clone().unwrap_or_else(|| "Training dataset".to_string())
                                    >
                                        <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                                        {gating.is_trainable.then(|| {
                                            let nav_store = navigate_store;
                                            let id = dataset_id_for_train.clone();
                                            view! {
                                            <Button
                                                variant=ButtonVariant::Primary
                                                on_click=Callback::new(move |_| {
                                                    nav_store.with_value(|nav| {
                                                        nav(
                                                            &format!(
                                                                "/training?open_wizard=1&dataset_id={}",
                                                                id
                                                            ),
                                                            Default::default(),
                                                        );
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

                                    // Readiness banner
                                    <Card>
                                        <div class="p-4 flex items-start justify-between gap-4 flex-wrap">
                                            <div class="space-y-2 min-w-0">
                                                {move || {
                                                    // Use the real trust_state string for copy.
                                                    let trust = trust_state
                                                        .clone()
                                                        .unwrap_or_else(|| "unknown".to_string());
                                                    let trust_label = if gating.trust_ok {
                                                        "OK".to_string()
                                                    } else if trust == "blocked" {
                                                        "Blocked".to_string()
                                                    } else if trust == "needs_approval" {
                                                        "Needs approval".to_string()
                                                    } else if trust == "allowed_with_warning" {
                                                        "Review warning".to_string()
                                                    } else {
                                                        "Needs trust".to_string()
                                                    };
                                                    view! {
                                                        <div class="flex items-center gap-2">
                                                            <Badge variant=gating.readiness_variant>
                                                                {if gating.is_trainable { "Trainable" } else { "Not trainable" }}
                                                            </Badge>
                                                            <span class="text-sm text-muted-foreground">
                                                                {gating.inline_reason.clone()}
                                                            </span>
                                                        </div>
                                                        <div class="grid gap-1 text-sm mt-2">
                                                            <div class="flex items-center justify-between gap-3">
                                                                <span class="text-muted-foreground">"Status gate"</span>
                                                                <Badge variant=if gating.status_ok { BadgeVariant::Success } else { BadgeVariant::Secondary }>
                                                                    {if gating.status_ok { "OK" } else { "Needs ready/indexed" }}
                                                                </Badge>
                                                            </div>
                                                            <div class="flex items-center justify-between gap-3">
                                                                <span class="text-muted-foreground">"Validation gate"</span>
                                                                <Badge variant=if gating.validation_ok { BadgeVariant::Success } else { BadgeVariant::Warning }>
                                                                    {if gating.validation_ok { "OK" } else { "Needs valid" }}
                                                                </Badge>
                                                            </div>
                                                            <div class="flex items-center justify-between gap-3">
                                                                <span class="text-muted-foreground">"Trust gate"</span>
                                                                <Badge variant=if gating.trust_ok { BadgeVariant::Success } else { BadgeVariant::Warning }>
                                                                    {trust_label}
                                                                </Badge>
                                                            </div>
                                                        </div>
                                                    }
                                                }}
                                            </div>

                                            {(!gating.is_trainable).then(|| {
                                                view! {
                                                    <div class="flex items-center gap-2">
                                                        <Button
                                                            variant=ButtonVariant::Outline
                                                            on_click=Callback::new(move |_| {
                                                                active_tab.set(DatasetDetailTab::Issues);
                                                                scroll_to_element_id("validation-errors");
                                                            })
                                                        >
                                                            "See Validation Errors"
                                                        </Button>
                                                        <Button
                                                            variant=ButtonVariant::Outline
                                                            on_click=Callback::new(move |_| {
                                                                active_tab.set(DatasetDetailTab::Details);
                                                                scroll_to_element_id("trust-state");
                                                            })
                                                        >
                                                            "See Trust State"
                                                        </Button>
                                                    </div>
                                                }
                                            })}
                                        </div>
                                    </Card>

                                    // Tabs
                                    <div class="border-b border-border">
                                        <TabNav
                                            tabs=vec![
                                                (DatasetDetailTab::Preview, "Preview"),
                                                (DatasetDetailTab::Issues, "Issues"),
                                                (DatasetDetailTab::Versions, "Versions"),
                                                (DatasetDetailTab::Details, "Details"),
                                            ]
                                            active=active_tab
                                            aria_label="Dataset detail tabs".to_string()
                                        />
                                    </div>

                                    <TabPanel tab=DatasetDetailTab::Preview active=active_tab tab_id="preview".to_string() class="pt-4 space-y-4">
                                        <Card>
                                            <div class="p-4 space-y-3">
                                                <div class="flex items-center justify-between gap-3 flex-wrap">
                                                    <div>
                                                        <h3 class="heading-4">"Preview"</h3>
                                                        <p class="text-sm text-muted-foreground">
                                                            "First N examples (read-only) for a quick sanity check."
                                                        </p>
                                                    </div>
                                                    <div class="flex items-center gap-3">
                                                        <Select
                                                            value=preview_limit
                                                            options=vec![
                                                                ("10".to_string(), "10".to_string()),
                                                                ("25".to_string(), "25".to_string()),
                                                                ("50".to_string(), "50".to_string()),
                                                            ]
                                                            class="w-24".to_string()
                                                        />
                                                        <Toggle
                                                            checked=pretty_json
                                                            label="Pretty JSON".to_string()
                                                            class="w-auto"
                                                        />
                                                    </div>
                                                </div>

                                                {move || match preview.get() {
                                                    LoadingState::Idle | LoadingState::Loading => {
                                                        view! { <div class="flex justify-center py-6"><Spinner/></div> }.into_any()
                                                    }
                                                    LoadingState::Loaded(DatasetPreviewResponse { examples, total_examples, .. }) => {
                                                        if examples.is_empty() {
                                                            view! {
                                                                <EmptyState
                                                                    title="No preview available"
                                                                    description="This dataset has no readable examples, or you don't have access."
                                                                />
                                                            }.into_any()
                                                        } else {
                                                            let pretty = pretty_json.get();
                                                            view! {
                                                                <div class="space-y-2">
                                                                    <div class="text-xs text-muted-foreground">
                                                                        {format!(
                                                                            "Returned {} example(s) (server reported total_examples={})",
                                                                            examples.len(),
                                                                            total_examples
                                                                        )}
                                                                    </div>
                                                                    <div class="space-y-3">
                                                                        {examples.into_iter().enumerate().map(|(idx, ex)| {
                                                                            let rendered = if pretty {
                                                                                serde_json::to_string_pretty(&ex).unwrap_or_else(|_| ex.to_string())
                                                                            } else {
                                                                                serde_json::to_string(&ex).unwrap_or_else(|_| ex.to_string())
                                                                            };
                                                                            view! {
                                                                                <div class="rounded border border-muted bg-muted/30 p-3">
                                                                                    <div class="text-xs text-muted-foreground mb-2">
                                                                                        {format!("Example {}", idx + 1)}
                                                                                    </div>
                                                                                    <pre class="font-mono text-xs whitespace-pre-wrap break-words">{rendered}</pre>
                                                                                </div>
                                                                            }
                                                                        }).collect_view()}
                                                                    </div>
                                                                </div>
                                                            }.into_any()
                                                        }
                                                    }
                                                    LoadingState::Error(e) => {
                                                        view! {
                                                            <ErrorDisplay
                                                                error=e
                                                                on_retry=Callback::new(move |_| trigger_refresh())
                                                            />
                                                        }.into_any()
                                                    }
                                                }}
                                            </div>
                                        </Card>
                                    </TabPanel>

                                    <TabPanel tab=DatasetDetailTab::Issues active=active_tab tab_id="issues".to_string() class="pt-4 space-y-4">
                                        {move || {
                                            let errors = data.validation_errors.clone().unwrap_or_default();
                                            let diags = data.validation_diagnostics.clone().unwrap_or_default();

                                            if errors.is_empty() && diags.is_empty() {
                                                view! {
                                                    <Card>
                                                        <EmptyState
                                                            title="No issues detected"
                                                            description="This dataset has no validation errors or diagnostics."
                                                        />
                                                    </Card>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="space-y-4">
                                                        {(!errors.is_empty()).then(|| view! {
                                                            <Card>
                                                                <div class="p-4">
                                                                    <h3 class="heading-4 mb-2" id="validation-errors">"Validation Errors"</h3>
                                                                    <ul class="space-y-2 text-sm text-destructive">
                                                                        {errors.into_iter().map(|err| view! { <li>{err}</li> }).collect_view()}
                                                                    </ul>
                                                                </div>
                                                            </Card>
                                                        })}

                                                        {(!diags.is_empty()).then(|| view! {
                                                            <Card>
                                                                <div class="p-4">
                                                                    <h3 class="heading-4 mb-2">"Validation Diagnostics"</h3>
                                                                    <div class="space-y-3 text-sm">
                                                                        {diags.into_iter().map(|diag| view! {
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
                                                                </div>
                                                            </Card>
                                                        })}
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                    </TabPanel>

                                    <TabPanel tab=DatasetDetailTab::Versions active=active_tab tab_id="versions".to_string() class="pt-4 space-y-4">
                                        <Card>
                                            <div class="p-4 space-y-3">
                                                <h3 class="heading-4">"Versions"</h3>
                                                {dataset_version_id_store.get_value().map(|id| view! {
                                                    <CopyableId id=id label="Current dataset_version_id".to_string() truncate=28 />
                                                })}
                                                {move || match versions.get() {
                                                    LoadingState::Idle | LoadingState::Loading => {
                                                        view! { <div class="flex justify-center py-4"><Spinner/></div> }.into_any()
                                                    }
                                                    LoadingState::Loaded(DatasetVersionsResponse { versions, .. }) => {
                                                        if versions.is_empty() {
                                                            view! { <p class="text-sm text-muted-foreground">"No dataset versions found."</p> }.into_any()
                                                        } else {
                                                            let current = dataset_version_id_store.get_value();
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
                                                                            let is_current = current.as_ref().map(|c| c == &version.dataset_version_id).unwrap_or(false);
                                                                            let row_class = if is_current { "bg-muted/50".to_string() } else { String::new() };
                                                                            view! {
                                                                                <TableRow class=row_class>
                                                                                    <TableCell>
                                                                                        <div class="space-y-1">
                                                                                            <div class="flex items-center gap-2">
                                                                                                <div class="font-medium">
                                                                                                    {"v"}{version.version_number.to_string()}
                                                                                                </div>
                                                                                                {is_current.then(|| view! { <Badge variant=BadgeVariant::Success>"Current"</Badge> })}
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
                                                        view! { <p class="text-sm text-muted-foreground">"Versions unavailable"</p> }.into_any()
                                                    }
                                                }}
                                            </div>
                                        </Card>

                                        {move || {
                                            match versions.get() {
                                                LoadingState::Loaded(DatasetVersionsResponse { versions, .. }) => {
                                                    let preferred = dataset_version_id_store.get_value()
                                                        .or_else(|| versions.first().map(|v| v.dataset_version_id.clone()));
                                                    preferred.map(|id| view! {
                                                        <Card>
                                                            <div class="p-4 space-y-2">
                                                                <h3 class="heading-4">"Usage"</h3>
                                                                <p class="text-sm text-muted-foreground">
                                                                    "Use a dataset version ID in inference or training to pin the exact data snapshot."
                                                                </p>
                                                                <div class="rounded-md bg-muted p-3 font-mono text-sm break-all">
                                                                    {format!("dataset_version_id: \"{}\"", id)}
                                                                </div>
                                                            </div>
                                                        </Card>
                                                    }.into_any())
                                                }
                                                _ => None,
                                            }
                                        }}
                                    </TabPanel>

                                    <TabPanel tab=DatasetDetailTab::Details active=active_tab tab_id="details".to_string() class="pt-4 space-y-4">
                                        <div class="grid gap-6 md:grid-cols-2">
                                            <Card>
                                                <div class="p-4">
                                                    <h3 class="heading-4 mb-4">"Overview"</h3>
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
                                                        <div class="flex justify-between">
                                                            <dt class="text-muted-foreground">"Validation"</dt>
                                                            <dd>
                                                                {data.validation_status.clone().map(|s| {
                                                                    let v = validation_badge_variant(&s);
                                                                    view! { <Badge variant=v>{s}</Badge> }
                                                                })}
                                                            </dd>
                                                        </div>
                                                        <div class="flex justify-between" id="trust-state">
                                                            <dt class="text-muted-foreground">"Trust State"</dt>
                                                            <dd>
                                                                {data.trust_state.clone().map(|s| {
                                                                    let v = trust_state_badge_variant(&s);
                                                                    view! { <Badge variant=v>{s}</Badge> }
                                                                })}
                                                            </dd>
                                                        </div>
                                                        <div class="flex justify-between">
                                                            <dt class="text-muted-foreground">"Current Version"</dt>
                                                            <dd class="font-mono text-xs truncate max-w-sm">
                                                                {dataset_version_id_display.clone()}
                                                            </dd>
                                                        </div>
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
                                                        {data.hash_b3.clone().map(|hash| view! {
                                                            <div class="flex justify-between">
                                                                <dt class="text-muted-foreground">"Hash (B3)"</dt>
                                                                <dd class="font-mono text-xs truncate max-w-sm">{hash}</dd>
                                                            </div>
                                                        })}
                                                    </dl>
                                                </div>
                                            </Card>

                                            <Card>
                                                <div class="p-4">
                                                    <h3 class="heading-4 mb-4">"Statistics"</h3>
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
                                                            view! { <p class="text-sm text-muted-foreground">"Statistics unavailable"</p> }.into_any()
                                                        }
                                                    }}
                                                </div>
                                            </Card>
                                        </div>
                                    </TabPanel>

                                    <ConfirmationDialog
                                        open=show_delete_confirm
                                        title="Delete Dataset"
                                        description=format!("Are you sure you want to delete this dataset? This action cannot be undone.")
                                        severity=ConfirmationSeverity::Destructive
                                        confirm_text="Delete"
                                        cancel_text="Cancel"
                                        on_confirm=on_confirm_delete
                                        on_cancel=on_cancel_delete
                                        loading=Signal::derive(move || deleting.get())
                                    />
                                </div>
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
    #[cfg(target_arch = "wasm32")]
    let safety_warning_acknowledged = RwSignal::new(false);
    let dataset_id_state = RwSignal::new(dataset_id);
    let document_ids_store = StoredValue::new(document_ids);
    let client = use_api();
    let poll_nonce = RwSignal::new(0u64);

    // Statistics state
    let stats_state = RwSignal::new(LoadingState::<DatasetStatisticsResponse>::Idle);

    // Fetch available models for combobox
    let (models_resource, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_models().await });
    let models_state = Signal::derive(move || match models_resource.get() {
        LoadingState::Loaded(resp) => resp.models,
        _ => vec![],
    });

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
                    let _ = (client, id);
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
                    let _ = (client, id);
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

            if let Some(job_id) = job_id {
                training_job_status.set(Some("pending".to_string()));
                training_status.set(Some("Training queued".to_string()));

                #[cfg(target_arch = "wasm32")]
                {
                    let nonce = poll_nonce.get_untracked();
                    let client = Arc::clone(&client);
                    let training_status = training_status;
                    let training_job_status = training_job_status;
                    let training_error = training_error;
                    let poll_nonce = poll_nonce;
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
                                    if matches!(
                                        status.as_str(),
                                        "completed" | "failed" | "cancelled"
                                    ) {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    training_error.set(Some(format!(
                                        "Failed to refresh training status: {}",
                                        e
                                    )));
                                    break;
                                }
                            }
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                        }
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (&client, job_id);
                }
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

            is_training.set(true);
            training_status.set(Some("Preparing training...".to_string()));

            #[cfg(target_arch = "wasm32")]
            {
                let adapter_type_val = adapter_type.get();
                let existing_dataset_id = dataset_id_state.get();
                let document_ids = document_ids_store.with_value(|ids| ids.clone());
                let epochs_val: u32 = epochs.get().parse().unwrap_or(10);
                let learning_rate_val: f64 = learning_rate.get().parse().unwrap_or(0.0001);
                let batch_size_val: u32 = batch_size.get().parse().unwrap_or(4);
                let rank_val: u32 = rank.get().parse().unwrap_or(8);
                let alpha_val: u32 = alpha.get().parse().unwrap_or(16);
                let pii_scrub_val = pii_scrub.get();
                let dedupe_val = dedupe.get();

                let client = Arc::clone(&client);
                let name_label = name_label.clone();
                let dataset_id_state = dataset_id_state;
                let training_status = training_status;
                let training_error = training_error;
                let training_job_id = training_job_id;
                let is_training = is_training;
                let safety_check_result = safety_check_result;
                let safety_warning_acknowledged = safety_warning_acknowledged;

                wasm_bindgen_futures::spawn_local(async move {
                    let dataset_id = if let Some(id) = existing_dataset_id {
                        id
                    } else if !document_ids.is_empty() {
                        training_status.set(Some("Creating dataset...".to_string()));
                        match client
                            .create_dataset_from_documents(document_ids, Some(name_label.clone()))
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
                // Reference captured variables to silence unused warnings without moving
                let _ = (
                    &client,
                    &base_model_val,
                    &name_label,
                    &dataset_id_state,
                    &training_status,
                    &training_job_id,
                    &training_error,
                );
                training_error.set(Some(
                    "Training is only available in the web UI.".to_string(),
                ));
                is_training.set(false);
            }
        })
    };

    view! {
        <PageScaffold
            title="Dataset Draft"
            subtitle="Review draft data before training an adapter."
        >
            <PageScaffoldActions slot>
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
            </PageScaffoldActions>

            // Draft readiness banner
            <Card>
                <div class="p-4 flex items-start justify-between gap-4 flex-wrap">
                    <div class="space-y-2">
                        <div class="flex items-center gap-2">
                            <Badge variant=BadgeVariant::Secondary>"Draft"</Badge>
                            <span class="text-sm text-muted-foreground">
                                "Complete the gates below to enable training."
                            </span>
                        </div>
                            <div class="grid gap-1 text-sm">
                                <div class="flex items-center justify-between gap-3">
                                    <span class="text-muted-foreground">"Base model gate"</span>
                                    {move || {
                                        let empty = base_model.get().trim().is_empty();
                                        let variant = if empty { BadgeVariant::Warning } else { BadgeVariant::Success };
                                        view! {
                                            <Badge variant=variant>
                                                {if empty { "Select a model" } else { "OK" }}
                                            </Badge>
                                        }
                                    }}
                                </div>
                                <div class="flex items-center justify-between gap-3">
                                    <span class="text-muted-foreground">"Data gate"</span>
                                    {move || {
                                        let has_dataset = dataset_id_state.get().is_some();
                                        let has_docs = document_ids_store.with_value(|ids| !ids.is_empty());
                                        let variant = if has_dataset || has_docs { BadgeVariant::Success } else { BadgeVariant::Warning };
                                        let label = if has_dataset {
                                            "Dataset attached"
                                        } else if has_docs {
                                            "Documents attached"
                                        } else {
                                            "Attach data"
                                        };
                                        view! { <Badge variant=variant>{label}</Badge> }
                                    }}
                                </div>
                                <div class="flex items-center justify-between gap-3">
                                    <span class="text-muted-foreground">"Safety gate"</span>
                                    {move || {
                                        let trust = safety_check_result
                                            .get()
                                            .map(|r| r.trust_state)
                                            .unwrap_or_else(|| "unknown".to_string());
                                        let variant = match trust.as_str() {
                                            "blocked" => BadgeVariant::Destructive,
                                            "needs_approval" | "allowed_with_warning" => BadgeVariant::Warning,
                                            "unknown" => BadgeVariant::Secondary,
                                            _ => BadgeVariant::Success,
                                        };
                                        view! { <Badge variant=variant>{trust}</Badge> }
                                    }}
                                </div>
                            </div>
                    </div>
                </div>
            </Card>

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
                                <h3 class="heading-4 text-destructive">{heading}</h3>
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
                            <h3 class="heading-4">{status.clone()}</h3>
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
                            <h3 class="heading-4">"Safety Gate"</h3>
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
                    <h3 class="heading-4 mb-4">"Draft Summary"</h3>
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
                    <h3 class="heading-4 mb-4">"Training"</h3>
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
                    <h3 class="heading-4 mb-4">"Statistics"</h3>
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
                <h3 class="heading-4 mb-4">"Training Configuration"</h3>
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
                <h3 class="heading-4 mb-4">"Preprocessing"</h3>
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
        </PageScaffold>
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ds(
        status: &str,
        validation: Option<&str>,
        trust: Option<&str>,
    ) -> crate::api::DatasetResponse {
        crate::api::DatasetResponse {
            schema_version: String::new(),
            id: "ds_123".to_string(),
            dataset_version_id: None,
            name: "Test".to_string(),
            description: None,
            format: "jsonl".to_string(),
            hash_b3: None,
            dataset_hash_b3: None,
            storage_path: None,
            status: status.to_string(),
            workspace_id: None,
            validation_status: validation.map(|s| s.to_string()),
            validation_errors: None,
            validation_diagnostics: None,
            trust_state: trust.map(|s| s.to_string()),
            file_count: None,
            total_size_bytes: None,
            dataset_type: None,
            created_by: None,
            created_at: "2026-02-06T00:00:00Z".to_string(),
            updated_at: None,
        }
    }

    #[test]
    fn gating_trainable_when_all_gates_ok() {
        let ds = make_ds("ready", Some("valid"), None);
        let g = dataset_gating(&ds);
        assert!(g.is_trainable);
        assert!(g.status_ok);
        assert!(g.validation_ok);
        assert!(g.trust_ok);
    }

    #[test]
    fn gating_blocks_on_status() {
        let ds = make_ds("processing", Some("valid"), Some("allowed"));
        let g = dataset_gating(&ds);
        assert!(!g.is_trainable);
        assert!(!g.status_ok);
    }

    #[test]
    fn gating_blocks_on_validation() {
        let ds = make_ds("indexed", Some("invalid"), Some("allowed"));
        let g = dataset_gating(&ds);
        assert!(!g.is_trainable);
        assert!(g.status_ok);
        assert!(!g.validation_ok);
    }

    #[test]
    fn gating_blocks_on_trust() {
        let ds = make_ds("indexed", Some("valid"), Some("blocked"));
        let g = dataset_gating(&ds);
        assert!(!g.is_trainable);
        assert!(g.status_ok);
        assert!(g.validation_ok);
        assert!(!g.trust_ok);
    }
}
