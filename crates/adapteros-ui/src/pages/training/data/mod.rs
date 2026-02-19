//! Training Data Management Tab.
//!
//! Provides visibility into the full data lifecycle:
//! - Documents (raw uploaded files)
//! - Datasets (validated training data)
//! - Preprocessed (CoreML feature cache)
//!
//! ## Layout
//!
//! Three-column layout:
//! - Left: DataSourceNav (source selection + upload)
//! - Center: DataList (items for selected source)
//! - Right: DataDetailPanel (details for selected item)

mod data_list;
mod detail_panel;
mod source_nav;
pub mod state;
mod upload_dialog;

// Re-export upload types for external use
pub use upload_dialog::{SafetyScanStatus, UploadResult};

use crate::api::{report_error_with_toast, ApiClient};
use crate::components::ErrorDisplay;
use crate::hooks::{use_api_resource, use_navigate, LoadingState};
use data_list::{DataList, DataListItem};
use detail_panel::DataDetailPanel;
use leptos::prelude::*;
use source_nav::DataSourceNav;
use state::{DataSource, PreprocessStatus};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use upload_dialog::DocumentUploadDialog;

/// Training Data Management component.
///
/// Displays a three-column layout for managing training data:
/// source navigation, item list, and detail panel.
#[component]
pub fn TrainingData() -> impl IntoView {
    let is_active = Arc::new(AtomicBool::new(true));
    {
        let is_active = Arc::clone(&is_active);
        on_cleanup(move || {
            is_active.store(false, Ordering::Relaxed);
        });
    }

    // State
    let active_source = RwSignal::new(DataSource::Datasets);
    let selected_id = RwSignal::new(None::<String>);

    // Fetch datasets
    let (datasets, refetch_datasets) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_datasets().await
    });

    // Fetch preprocessed cache list
    let (preprocessed_cache, refetch_preprocessed) = use_api_resource(
        move |client: Arc<ApiClient>| async move { client.list_preprocessed_cache().await },
    );

    // Fetch documents
    let (documents, refetch_documents) = use_api_resource(
        move |client: Arc<ApiClient>| async move { client.list_documents(None).await },
    );

    let refetch_datasets_signal = StoredValue::new(refetch_datasets);
    let refetch_documents_signal = StoredValue::new(refetch_documents);
    let refetch_preprocessed_signal = StoredValue::new(refetch_preprocessed);

    // Derive counts for source nav
    let doc_count = Signal::derive(move || match documents.get() {
        LoadingState::Loaded(data) => data.data.len(),
        _ => 0,
    });

    let dataset_count = Signal::derive(move || match datasets.get() {
        LoadingState::Loaded(data) => data.datasets.len(),
        _ => 0,
    });

    let (cache_count, refetch_cache_count) = use_api_resource(
        move |client: Arc<ApiClient>| async move { client.get_preprocessed_cache_count().await },
    );
    let refetch_cache_count_signal = StoredValue::new(refetch_cache_count);

    let cached_count = Signal::derive(move || match cache_count.get() {
        LoadingState::Loaded(data) => data.count as usize,
        _ => 0,
    });

    // Convert data to list items based on active source
    let list_items = Signal::derive(move || match active_source.get() {
        DataSource::Datasets => match datasets.get() {
            LoadingState::Loaded(data) => data
                .datasets
                .iter()
                .map(DataListItem::from_dataset)
                .collect(),
            _ => Vec::new(),
        },
        DataSource::Documents => match documents.get() {
            LoadingState::Loaded(data) => {
                data.data.iter().map(DataListItem::from_document).collect()
            }
            _ => Vec::new(),
        },
        DataSource::Preprocessed => match preprocessed_cache.get() {
            LoadingState::Loaded(data) => data
                .entries
                .iter()
                .map(DataListItem::from_preprocessed)
                .collect(),
            _ => Vec::new(),
        },
    });

    // Loading state based on active source
    let is_loading = Signal::derive(move || match active_source.get() {
        DataSource::Datasets => {
            matches!(datasets.get(), LoadingState::Idle | LoadingState::Loading)
        }
        DataSource::Documents => {
            matches!(documents.get(), LoadingState::Idle | LoadingState::Loading)
        }
        DataSource::Preprocessed => {
            matches!(preprocessed_cache.get(), LoadingState::Idle | LoadingState::Loading)
        }
    });

    // Selected dataset (for detail panel)
    let selected_dataset = Signal::derive(move || {
        let id = selected_id.get()?;
        match datasets.get() {
            LoadingState::Loaded(data) => data.datasets.iter().find(|d| d.id == id).cloned(),
            _ => None,
        }
    });

    // Selected preprocessed entry (for detail panel)
    let selected_preprocessed = Signal::derive(move || {
        let id = selected_id.get()?;
        match preprocessed_cache.get() {
            LoadingState::Loaded(data) => data
                .entries
                .iter()
                .find(|entry| format!("{}::{}", entry.dataset_id, entry.preprocess_id) == id)
                .cloned(),
            _ => None,
        }
    });

    let selected_preprocess_status = Signal::derive(move || {
        if selected_preprocessed.get().is_some() {
            PreprocessStatus::Cached
        } else {
            PreprocessStatus::None
        }
    });

    // Selected document (for detail panel)
    let selected_document = Signal::derive(move || {
        let id = selected_id.get()?;
        match documents.get() {
            LoadingState::Loaded(data) => data.data.iter().find(|d| d.document_id == id).cloned(),
            _ => None,
        }
    });

    // Upload dialog state
    let show_doc_upload = RwSignal::new(false);
    let navigate = use_navigate();

    // Callbacks
    let on_select = Callback::new(move |id: String| {
        selected_id.set(Some(id));
    });

    let on_close_detail = move || {
        selected_id.set(None);
    };

    let on_upload = {
        let navigate = navigate.clone();
        Callback::new(move |source: DataSource| {
            match source {
                DataSource::Documents => {
                    show_doc_upload.set(true);
                }
                DataSource::Datasets => {
                    navigate("/training?open_wizard=1");
                }
                DataSource::Preprocessed => {
                    // Preprocessed cannot be uploaded directly
                }
            }
        })
    };

    let on_doc_upload_success = {
        Callback::new(move |_document_id: String| {
            // Refetch documents after successful upload
            refetch_documents_signal.with_value(|f| f());
            // Switch to documents view
            active_source.set(DataSource::Documents);
        })
    };

    // State for create dataset action
    let creating_dataset = RwSignal::new(false);
    let create_dataset_error = RwSignal::new(None::<String>);

    let on_create_dataset = {
        Callback::new(move |document_id: String| {
            if creating_dataset.get() {
                return;
            }

            creating_dataset.set(true);
            create_dataset_error.set(None);

            #[cfg(target_arch = "wasm32")]
            {
                use crate::api::{api_base_url, ApiClient};
                let is_active = Arc::clone(&is_active);

                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    let client = ApiClient::with_base_url(&api_base_url());

                    match client
                        .create_dataset_from_documents(vec![document_id.clone()], None)
                        .await
                    {
                        Ok(response) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = creating_dataset.try_set(false);
                            // Refetch datasets and switch view
                            refetch_datasets_signal.with_value(|f| f());
                            let _ = active_source.try_set(DataSource::Datasets);
                            let _ = selected_id.try_set(Some(response.id));
                            tracing::info!("Created dataset from document: {}", document_id);
                        }
                        Err(e) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            report_error_with_toast(&e, "Failed to create dataset", Some("/training"), true);
                            let _ = create_dataset_error.try_set(Some(e.user_message()));
                            let _ = creating_dataset.try_set(false);
                            tracing::error!("Failed to create dataset: {}", e);
                        }
                    }
                });
            }
        })
    };

    let on_start_training = {
        let navigate = navigate.clone();
        Callback::new(move |dataset_id: String| {
            navigate(&format!("/training?dataset_id={}&open_wizard=1", dataset_id));
        })
    };

    let on_invalidate_cache = {
        Callback::new(move |dataset_id: String| {
            #[cfg(target_arch = "wasm32")]
            {
                use crate::api::{api_base_url, ApiClient};
                let is_active = Arc::clone(&is_active);

                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    let client = ApiClient::with_base_url(&api_base_url());
                    match client.invalidate_preprocessed_cache(&dataset_id).await {
                        Ok(()) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            refetch_preprocessed_signal.with_value(|f| f());
                            refetch_cache_count_signal.with_value(|f| f());
                            let _ = selected_id.try_set(None);
                            let _ = active_source.try_set(DataSource::Preprocessed);
                            tracing::info!(
                                dataset_id = %dataset_id,
                                "Invalidated preprocessed cache"
                            );
                        }
                        Err(e) => {
                            report_error_with_toast(
                                &e,
                                "Failed to invalidate preprocessed cache",
                                Some("/training"),
                                true,
                            );
                            tracing::error!(
                                dataset_id = %dataset_id,
                                error = %e,
                                "Failed to invalidate preprocessed cache"
                            );
                        }
                    }
                });
            }
        })
    };

    // Derive whether detail panel should be shown
    let has_selection = Signal::derive(move || selected_id.get().is_some());

    view! {
        <div class="training-data">
            // Three-column layout
            <div class="training-data-layout">
                // Left: Source navigation
                <aside class="training-data-nav">
                    <DataSourceNav
                        active=active_source
                        doc_count=doc_count
                        dataset_count=dataset_count
                        cached_count=cached_count
                        on_upload=on_upload
                    />
                </aside>

                // Center: Data list
                <main class="training-data-list">
                    {move || {
                        // Check for errors based on active source
                        let error = match active_source.get() {
                            DataSource::Datasets => match datasets.get() {
                                LoadingState::Error(e) => Some(e),
                                _ => None,
                            },
                            DataSource::Documents => match documents.get() {
                                LoadingState::Error(e) => Some(e),
                                _ => None,
                            },
                            DataSource::Preprocessed => None,
                        };

                        if let Some(e) = error {
                            view! {
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| {
                                        match active_source.get() {
                                            DataSource::Datasets => refetch_datasets_signal.with_value(|f| f()),
                                            DataSource::Documents => refetch_documents_signal.with_value(|f| f()),
                                            DataSource::Preprocessed => {}
                                        }
                                    })
                                />
                            }.into_any()
                        } else {
                            let source_signal: Signal<DataSource> = active_source.into();
                            view! {
                                <DataList
                                    source=source_signal
                                    items=list_items
                                    selected_id=selected_id
                                    on_select=on_select
                                    loading=is_loading
                                />
                            }.into_any()
                        }
                    }}
                </main>

                // Right: Detail panel (shown when item selected)
                <aside
                    class=move || {
                        if has_selection.get() {
                            "training-data-detail training-data-detail-open"
                        } else {
                            "training-data-detail"
                        }
                    }
                    aria-hidden=move || (!has_selection.get()).to_string()
                >
                    {
                        let source_signal: Signal<DataSource> = active_source.into();
                        let item_id_signal: Signal<Option<String>> = selected_id.into();
                        view! {
                            <DataDetailPanel
                                source=source_signal
                                item_id=item_id_signal
                                document=selected_document
                                dataset=selected_dataset
                                preprocessed_entry=selected_preprocessed
                                preprocess_status=selected_preprocess_status
                                loading=is_loading
                                on_close=Callback::new(move |_| on_close_detail())
                                on_create_dataset=on_create_dataset
                                on_start_training=on_start_training
                                on_invalidate_cache=on_invalidate_cache
                            />
                        }
                    }
                </aside>
            </div>

            // Mobile: Show back button when detail panel is open
            <Show when=move || has_selection.get()>
                <div class="training-data-mobile-overlay">
                    <button
                        type="button"
                        class="training-data-mobile-back"
                        on:click=move |_| on_close_detail()
                    >
                        "← Back to list"
                    </button>
                </div>
            </Show>

            // Document upload dialog
            <DocumentUploadDialog
                open=show_doc_upload
                on_success=on_doc_upload_success
            />
        </div>
    }
}
