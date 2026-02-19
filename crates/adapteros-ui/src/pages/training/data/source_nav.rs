//! Data source navigation sidebar.
//!
//! Left sidebar for selecting data source type (Documents, Datasets, Preprocessed).

use super::state::DataSource;
use crate::components::Button;
use leptos::prelude::*;

/// Left sidebar navigation for data sources.
#[component]
pub fn DataSourceNav(
    /// Currently active data source
    active: RwSignal<DataSource>,
    /// Count of documents
    #[prop(into)]
    doc_count: Signal<usize>,
    /// Count of datasets
    #[prop(into)]
    dataset_count: Signal<usize>,
    /// Count of preprocessed/cached datasets
    #[prop(into)]
    cached_count: Signal<usize>,
    /// Callback when upload is requested
    #[prop(optional)]
    on_upload: Option<Callback<DataSource>>,
) -> impl IntoView {
    let sources = [
        (DataSource::Documents, doc_count),
        (DataSource::Datasets, dataset_count),
        (DataSource::Preprocessed, cached_count),
    ];

    view! {
        <nav class="data-source-nav" aria-label="Data source navigation">
            <div class="data-source-nav-header">
                <h3 class="data-source-nav-title">"Sources"</h3>
            </div>

            <div class="data-source-nav-list">
                {sources
                    .into_iter()
                    .map(|(source, count)| {
                        let source_for_click = source;
                        let source_for_class = source;
                        let source_for_aria = source;

                        view! {
                            <button
                                type="button"
                                role="tab"
                                aria-selected=move || (active.get() == source_for_aria).to_string()
                                class=move || {
                                    if active.get() == source_for_class {
                                        "data-source-item data-source-item-active"
                                    } else {
                                        "data-source-item"
                                    }
                                }
                                on:click=move |_| active.set(source_for_click)
                            >
                                <span class="data-source-icon">{source.icon()}</span>
                                <span class="data-source-label">{source.label()}</span>
                                <span class="data-source-count">{move || count.get()}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>

            {move || {
                let current = active.get();
                // Only show upload for Documents and Datasets
                if matches!(current, DataSource::Documents | DataSource::Datasets) {
                    if let Some(on_upload) = on_upload {
                        let upload_label = match current {
                            DataSource::Documents => "Upload Document",
                            DataSource::Datasets => "Upload Dataset",
                            DataSource::Preprocessed => "",
                        };
                        Some(view! {
                            <div class="data-source-nav-actions">
                                <Button
                                    on_click=Callback::new(move |_| on_upload.run(current))
                                >
                                    {upload_label}
                                </Button>
                            </div>
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }}
        </nav>
    }
}
