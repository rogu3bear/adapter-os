//! Detail panel for viewing and managing data items.
//!
//! Right panel that shows detailed information about the selected item.

use super::state::{DataSource, DatasetStatus, DocumentStatus, PreprocessStatus};
use crate::api::{DatasetResponse, DocumentResponse, PreprocessedCacheEntry};
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, Spinner};
use crate::utils::{format_bytes, format_datetime};
use leptos::prelude::*;

/// Dataset version summary (used in detail panel).
#[derive(Clone, Debug, Default)]
pub struct DatasetVersionSummary {
    pub dataset_version_id: String,
    pub version_number: i64,
    pub version_label: Option<String>,
    pub created_at: String,
}

/// Right panel showing details of the selected item.
#[component]
pub fn DataDetailPanel(
    /// Active data source type
    #[prop(into)]
    source: Signal<DataSource>,
    /// Selected item ID
    #[prop(into)]
    item_id: Signal<Option<String>>,
    /// Document data (when source is Documents)
    #[prop(optional)]
    document: Option<Signal<Option<DocumentResponse>>>,
    /// Dataset data (when source is Datasets)
    #[prop(optional)]
    dataset: Option<Signal<Option<DatasetResponse>>>,
    /// Dataset versions (when source is Datasets)
    #[prop(optional)]
    versions: Option<Signal<Vec<DatasetVersionSummary>>>,
    /// Preprocess status (when source is Preprocessed)
    #[prop(optional)]
    preprocess_status: Option<Signal<PreprocessStatus>>,
    /// Preprocessed cache entry (when source is Preprocessed)
    #[prop(optional)]
    preprocessed_entry: Option<Signal<Option<PreprocessedCacheEntry>>>,
    /// Whether data is loading
    #[prop(into, default = Signal::derive(|| false))]
    loading: Signal<bool>,
    /// Callback when close is requested
    on_close: Callback<()>,
    /// Callback for "Create Dataset" action (Documents only)
    #[prop(optional)]
    on_create_dataset: Option<Callback<String>>,
    /// Callback for "Start Training" action (Datasets only)
    #[prop(optional)]
    on_start_training: Option<Callback<String>>,
    /// Callback for "Invalidate Cache" action (Preprocessed only)
    #[prop(optional)]
    on_invalidate_cache: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <div class="data-detail-panel">
            {move || {
                let id = item_id.get();
                let src = source.get();

                if loading.get() {
                    return view! {
                        <div class="data-detail-loading">
                            <Spinner />
                        </div>
                    }.into_any();
                }

                match id {
                    None => view! {
                        <DataDetailEmpty source=src />
                    }.into_any(),
                    Some(id) => {
                        match src {
                            DataSource::Documents => {
                                let doc = document.and_then(|d: Signal<Option<DocumentResponse>>| d.get());
                                view! {
                                    <DocumentDetail
                                        document_id=id.clone()
                                        document=doc
                                        on_close=on_close
                                        on_create_dataset=on_create_dataset
                                    />
                                }.into_any()
                            }
                            DataSource::Datasets => {
                                let ds = dataset.and_then(|d| d.get());
                                let vers = versions.map(|v| v.get()).unwrap_or_default();
                                view! {
                                    <DatasetDetail
                                        dataset=ds
                                        versions=vers
                                        on_close=on_close
                                        on_start_training=on_start_training
                                    />
                                }.into_any()
                            },
                            DataSource::Preprocessed => {
                                let status = preprocess_status.map(|s| s.get()).unwrap_or_default();
                                let entry = preprocessed_entry.and_then(|e| e.get());
                                let dataset_id = entry
                                    .as_ref()
                                    .map(|item| item.dataset_id.clone())
                                    .unwrap_or_else(|| id.clone());
                                view! {
                                    <PreprocessDetail
                                        dataset_id=dataset_id
                                        entry=entry
                                        status=status
                                        on_close=on_close
                                        on_invalidate_cache=on_invalidate_cache
                                    />
                                }.into_any()
                            },
                        }
                    }
                }
            }}
        </div>
    }
}

/// Empty state when no item is selected.
#[component]
fn DataDetailEmpty(source: DataSource) -> impl IntoView {
    let hint = match source {
        DataSource::Documents => "Select a document to view details and processing status.",
        DataSource::Datasets => "Select a dataset to view metadata, preview rows, and versions.",
        DataSource::Preprocessed => "Select a preprocessed dataset to view cache status.",
    };

    view! {
        <div class="data-detail-empty">
            <div class="data-detail-empty-icon">{source.icon()}</div>
            <p class="data-detail-empty-hint">{hint}</p>
        </div>
    }
}

/// Document detail view.
#[component]
fn DocumentDetail(
    document_id: String,
    document: Option<DocumentResponse>,
    on_close: Callback<()>,
    on_create_dataset: Option<Callback<String>>,
) -> impl IntoView {
    let doc_id_for_action = document_id.clone();

    let title = document
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Document Details".to_string());

    view! {
        <div class="data-detail-content">
            <div class="data-detail-header">
                <h2 class="data-detail-title">{title}</h2>
                <button
                    type="button"
                    class="data-detail-close"
                    on:click=move |_| on_close.run(())
                    aria-label="Close detail panel"
                >
                    "×"
                </button>
            </div>

            {document.map(|doc| {
                let status_variant = match doc.status.parse::<DocumentStatus>().unwrap_or_default() {
                    DocumentStatus::Indexed => BadgeVariant::Success,
                    DocumentStatus::Failed => BadgeVariant::Destructive,
                    DocumentStatus::Processing => BadgeVariant::Warning,
                    DocumentStatus::Raw => BadgeVariant::Default,
                };
                let doc_id = doc.document_id.clone();
                let doc_status = doc.status.clone();
                let doc_mime = doc.mime_type.clone();
                let doc_size = format_bytes(doc.size_bytes);
                let doc_hash = doc.hash_b3.clone();
                let doc_created = format_datetime(&doc.created_at);
                let doc_chunks = doc.chunk_count;
                let doc_error = doc.error_message.clone();
                let doc_retries = doc.retry_count;
                let doc_max_retries = doc.max_retries;
                let is_indexed = matches!(doc.status.as_str(), "indexed");

                view! {
                    <>
                        <Card title="Metadata">
                            <dl class="data-detail-metadata">
                                <div class="data-detail-metadata-item">
                                    <dt>"Document ID"</dt>
                                    <dd class="font-mono text-sm">{doc_id}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Status"</dt>
                                    <dd>
                                        <Badge variant=status_variant>{doc_status}</Badge>
                                    </dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Type"</dt>
                                    <dd>{doc_mime}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Size"</dt>
                                    <dd>{doc_size}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Hash (BLAKE3)"</dt>
                                    <dd class="font-mono text-sm">{doc_hash}</dd>
                                </div>
                                {doc_chunks.map(|count: i32| {
                                    view! {
                                        <div class="data-detail-metadata-item">
                                            <dt>"Chunks"</dt>
                                            <dd>{count.to_string()}</dd>
                                        </div>
                                    }
                                })}
                                <div class="data-detail-metadata-item">
                                    <dt>"Created"</dt>
                                    <dd>{doc_created}</dd>
                                </div>
                            </dl>
                        </Card>

                        {doc_error.map(|err| {
                            view! {
                                <Card title="Processing Error">
                                    <div class="data-detail-error">
                                        <p class="data-detail-error-message">{err}</p>
                                        <p class="data-detail-error-retries">
                                            "Retries: " {doc_retries.to_string()} " / " {doc_max_retries.to_string()}
                                        </p>
                                    </div>
                                </Card>
                            }
                        })}

                        <Card title="Actions">
                            <div class="data-detail-actions">
                                {if is_indexed {
                                    on_create_dataset.map(|callback| {
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Primary
                                                on_click=Callback::new(move |_| callback.run(doc_id_for_action.clone()))
                                            >
                                                "Create Dataset"
                                            </Button>
                                        }
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        </Card>
                    </>
                }
            })}
        </div>
    }
}

/// Dataset detail view.
#[component]
fn DatasetDetail(
    dataset: Option<DatasetResponse>,
    versions: Vec<DatasetVersionSummary>,
    on_close: Callback<()>,
    on_start_training: Option<Callback<String>>,
) -> impl IntoView {
    let title = dataset
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Dataset Details".to_string());

    view! {
        <div class="data-detail-content">
            <div class="data-detail-header">
                <h2 class="data-detail-title">{title}</h2>
                <button
                    type="button"
                    class="data-detail-close"
                    on:click=move |_| on_close.run(())
                    aria-label="Close detail panel"
                >
                    "×"
                </button>
            </div>

            {dataset.map(|ds| {
                let status_variant = match ds.status.parse::<DatasetStatus>().unwrap_or_default() {
                    DatasetStatus::Valid => BadgeVariant::Success,
                    DatasetStatus::Invalid => BadgeVariant::Destructive,
                    DatasetStatus::Pending => BadgeVariant::Warning,
                };
                let ds_id = ds.id.clone();
                let ds_id_for_action = ds.id.clone();
                let ds_status = ds.status.clone();
                let ds_format = ds.format.clone();
                let ds_size = format_bytes(ds.total_size_bytes.unwrap_or(0));
                let ds_files = ds.file_count.unwrap_or(0).to_string();
                let ds_desc = ds.description.clone();
                let ds_created = format_datetime(&ds.created_at);
                let validation_errors = ds.validation_errors.clone();

                view! {
                    <>
                        <Card title="Metadata">
                            <dl class="data-detail-metadata">
                                <div class="data-detail-metadata-item">
                                    <dt>"Dataset ID"</dt>
                                    <dd class="font-mono text-sm">{ds_id}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Status"</dt>
                                    <dd>
                                        <Badge variant=status_variant>{ds_status}</Badge>
                                    </dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Format"</dt>
                                    <dd>{ds_format}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Size"</dt>
                                    <dd>{ds_size}</dd>
                                </div>
                                <div class="data-detail-metadata-item">
                                    <dt>"Files"</dt>
                                    <dd>{ds_files}</dd>
                                </div>
                                {ds_desc.map(|desc| {
                                    view! {
                                        <div class="data-detail-metadata-item">
                                            <dt>"Description"</dt>
                                            <dd>{desc}</dd>
                                        </div>
                                    }
                                })}
                                <div class="data-detail-metadata-item">
                                    <dt>"Created"</dt>
                                    <dd>{ds_created}</dd>
                                </div>
                            </dl>
                        </Card>

                        {validation_errors.and_then(|errors| {
                            if errors.is_empty() {
                                None
                            } else {
                                Some(view! {
                                    <Card title="Validation Errors">
                                        <ul class="data-detail-errors">
                                            {errors.into_iter().map(|err| {
                                                view! { <li>{err}</li> }
                                            }).collect_view()}
                                        </ul>
                                    </Card>
                                })
                            }
                        })}

                        {if !versions.is_empty() {
                            Some(view! {
                                <Card title="Versions">
                                    <div class="data-detail-versions">
                                        {versions.into_iter().map(|v| {
                                            let version_id = v.dataset_version_id.clone();
                                            let version_num = v.version_number.to_string();
                                            let version_label = v.version_label.clone();
                                            let version_date = format_datetime(&v.created_at);
                                            view! {
                                                <div class="data-detail-version-item" data-version-id=version_id>
                                                    <span class="data-detail-version-number">
                                                        "v" {version_num}
                                                    </span>
                                                    {version_label.map(|label| {
                                                        view! {
                                                            <span class="data-detail-version-label">{label}</span>
                                                        }
                                                    })}
                                                    <span class="data-detail-version-date">{version_date}</span>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                </Card>
                            })
                        } else {
                            None
                        }}

                        <Card title="Actions">
                            <div class="data-detail-actions">
                                {on_start_training.map(|callback| {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Primary
                                            on_click=Callback::new(move |_| callback.run(ds_id_for_action.clone()))
                                        >
                                            "Start Training"
                                        </Button>
                                    }
                                })}
                            </div>
                        </Card>
                    </>
                }
            })}
        </div>
    }
}

/// Preprocess cache detail view.
#[component]
fn PreprocessDetail(
    dataset_id: String,
    entry: Option<PreprocessedCacheEntry>,
    status: PreprocessStatus,
    on_close: Callback<()>,
    on_invalidate_cache: Option<Callback<String>>,
) -> impl IntoView {
    let ds_id = dataset_id.clone();
    let ds_id_for_action = dataset_id.clone();

    let title = entry
        .as_ref()
        .and_then(|e| e.dataset_name.clone())
        .unwrap_or_else(|| dataset_id.clone());

    let status_variant = match status {
        PreprocessStatus::Cached => BadgeVariant::Success,
        PreprocessStatus::Stale => BadgeVariant::Warning,
        PreprocessStatus::None => BadgeVariant::Default,
    };
    let status_label = status.label().to_string();

    view! {
        <div class="data-detail-content">
            <div class="data-detail-header">
                <h2 class="data-detail-title">{title}</h2>
                <button
                    type="button"
                    class="data-detail-close"
                    on:click=move |_| on_close.run(())
                    aria-label="Close detail panel"
                >
                    "×"
                </button>
            </div>

            <Card title="Cache Status">
                <dl class="data-detail-metadata">
                    <div class="data-detail-metadata-item">
                        <dt>"Dataset ID"</dt>
                        <dd class="font-mono text-sm">{ds_id}</dd>
                    </div>
                    {entry.as_ref().map(|e| {
                        let produced_at = e.produced_at.clone().unwrap_or_default();
                        view! {
                            <div class="data-detail-metadata-item">
                                <dt>"Preprocess ID"</dt>
                                <dd class="font-mono text-sm">{e.preprocess_id.clone()}</dd>
                            </div>
                            <div class="data-detail-metadata-item">
                                <dt>"Backend"</dt>
                                <dd>{e.backend.clone()}</dd>
                            </div>
                            <div class="data-detail-metadata-item">
                                <dt>"Examples"</dt>
                                <dd>{e.example_count}</dd>
                            </div>
                            <div class="data-detail-metadata-item">
                                <dt>"Produced"</dt>
                                <dd>{produced_at}</dd>
                            </div>
                        }
                    })}
                    <div class="data-detail-metadata-item">
                        <dt>"Status"</dt>
                        <dd>
                            <Badge variant=status_variant>{status_label}</Badge>
                        </dd>
                    </div>
                </dl>
            </Card>

            {if matches!(status, PreprocessStatus::Cached | PreprocessStatus::Stale) {
                Some(view! {
                    <Card title="Actions">
                        <div class="data-detail-actions">
                            {on_invalidate_cache.map(|callback| {
                                view! {
                                    <Button
                                        variant=ButtonVariant::Destructive
                                        on_click=Callback::new(move |_| callback.run(ds_id_for_action.clone()))
                                    >
                                        "Invalidate Cache"
                                    </Button>
                                }
                            })}
                        </div>
                    </Card>
                })
            } else {
                None
            }}
        </div>
    }
}
