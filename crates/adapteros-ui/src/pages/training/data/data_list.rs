//! Data list component for the center panel.
//!
//! Displays a list of items based on the active data source (documents, datasets, or preprocessed).

use super::state::{DataSort, DataSource, DatasetStatus, DocumentStatus};
use crate::api::{DatasetResponse, DocumentResponse, PreprocessedCacheEntry};
use crate::components::{Badge, BadgeVariant, Spinner};
use crate::utils::{format_bytes, format_date};
use leptos::prelude::*;

/// Data item representation for unified list display.
#[derive(Clone, Debug)]
pub struct DataListItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub status_variant: BadgeVariant,
    pub size_bytes: i64,
    pub format: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl DataListItem {
    /// Convert a DatasetResponse to a DataListItem.
    pub fn from_dataset(dataset: &DatasetResponse) -> Self {
        let status_variant = match dataset.status.parse::<DatasetStatus>().unwrap_or_default() {
            DatasetStatus::Valid => BadgeVariant::Success,
            DatasetStatus::Invalid => BadgeVariant::Destructive,
            DatasetStatus::Pending => BadgeVariant::Warning,
        };

        Self {
            id: dataset.id.clone(),
            name: dataset.name.clone(),
            description: dataset.description.clone(),
            status: dataset.status.clone(),
            status_variant,
            size_bytes: dataset.total_size_bytes.unwrap_or(0),
            format: Some(dataset.format.clone()),
            created_at: dataset.created_at.clone(),
            updated_at: dataset.updated_at.clone().unwrap_or_default(),
        }
    }

    /// Convert a DocumentResponse to a DataListItem.
    pub fn from_document(doc: &DocumentResponse) -> Self {
        let status_variant = match doc.status.parse::<DocumentStatus>().unwrap_or_default() {
            DocumentStatus::Indexed => BadgeVariant::Success,
            DocumentStatus::Failed => BadgeVariant::Destructive,
            DocumentStatus::Processing => BadgeVariant::Warning,
            DocumentStatus::Raw => BadgeVariant::Default,
        };

        Self {
            id: doc.document_id.clone(),
            name: doc.name.clone(),
            description: None,
            status: doc.status.clone(),
            status_variant,
            size_bytes: doc.size_bytes,
            format: Some(doc.mime_type.clone()),
            created_at: doc.created_at.clone(),
            updated_at: doc.updated_at.clone().unwrap_or_default(),
        }
    }

    /// Convert a PreprocessedCacheEntry to a DataListItem.
    pub fn from_preprocessed(entry: &PreprocessedCacheEntry) -> Self {
        Self {
            id: format!("{}::{}", entry.dataset_id, entry.preprocess_id),
            name: entry
                .dataset_name
                .clone()
                .unwrap_or_else(|| entry.dataset_id.clone()),
            description: Some(format!(
                "Cache {} · {} examples",
                entry.preprocess_id, entry.example_count
            )),
            status: "cached".to_string(),
            status_variant: BadgeVariant::Success,
            size_bytes: 0,
            format: Some(entry.backend.clone()),
            created_at: entry.produced_at.clone().unwrap_or_default(),
            updated_at: entry.produced_at.clone().unwrap_or_default(),
        }
    }
}

/// Center panel list component.
#[component]
pub fn DataList(
    /// Active data source
    #[prop(into)]
    source: Signal<DataSource>,
    /// Items to display
    #[prop(into)]
    items: Signal<Vec<DataListItem>>,
    /// Currently selected item ID
    selected_id: RwSignal<Option<String>>,
    /// Callback when an item is selected
    on_select: Callback<String>,
    /// Whether the list is loading
    #[prop(into, default = Signal::derive(|| false))]
    loading: Signal<bool>,
    /// Current sort order
    #[prop(optional)]
    sort: Option<RwSignal<DataSort>>,
) -> impl IntoView {
    let sort_signal = sort.unwrap_or_else(|| RwSignal::new(DataSort::default()));
    let search_query = RwSignal::new(String::new());
    let status_filter = RwSignal::new(None::<String>);

    // Filter items based on search and status
    let filtered_items = Signal::derive(move || {
        let query = search_query.get().to_lowercase();
        let status = status_filter.get();
        let all_items = items.get();

        all_items
            .into_iter()
            .filter(|item| {
                // Search filter
                let matches_search = query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query))
                        .unwrap_or(false);

                // Status filter
                let matches_status = status
                    .as_ref()
                    .map(|s| item.status.to_lowercase() == s.to_lowercase())
                    .unwrap_or(true);

                matches_search && matches_status
            })
            .collect::<Vec<_>>()
    });

    // Get unique statuses for filter dropdown
    let available_statuses = Signal::derive(move || {
        let all_items = items.get();
        let mut statuses: Vec<String> = all_items
            .iter()
            .map(|item| item.status.clone())
            .collect();
        statuses.sort();
        statuses.dedup();
        statuses
    });

    // Keyboard navigation
    let focused_index = RwSignal::new(None::<usize>);

    let handle_keydown = move |ev: web_sys::KeyboardEvent| {
        let key = ev.key();
        let items_list = filtered_items.get();

        if items_list.is_empty() {
            return;
        }

        match key.as_str() {
            "j" | "ArrowDown" => {
                ev.prevent_default();
                let new_index = match focused_index.get() {
                    None => 0,
                    Some(i) => (i + 1).min(items_list.len() - 1),
                };
                focused_index.set(Some(new_index));
                // Update selection to match focus
                if let Some(item) = items_list.get(new_index) {
                    selected_id.set(Some(item.id.clone()));
                }
            }
            "k" | "ArrowUp" => {
                ev.prevent_default();
                let new_index = match focused_index.get() {
                    None => items_list.len().saturating_sub(1),
                    Some(i) => i.saturating_sub(1),
                };
                focused_index.set(Some(new_index));
                // Update selection to match focus
                if let Some(item) = items_list.get(new_index) {
                    selected_id.set(Some(item.id.clone()));
                }
            }
            "Enter" => {
                if let Some(idx) = focused_index.get() {
                    if let Some(item) = items_list.get(idx) {
                        on_select.run(item.id.clone());
                    }
                }
            }
            "Escape" => {
                focused_index.set(None);
                selected_id.set(None);
            }
            "/" => {
                // Focus search input
                ev.prevent_default();
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    if let Some(window) = web_sys::window() {
                        if let Some(doc) = window.document() {
                            if let Ok(Some(el)) = doc.query_selector(".data-list-search-input") {
                                if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                                    let _ = input.focus();
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    };

    view! {
        <div
            class="data-list"
            tabindex="0"
            on:keydown=handle_keydown
        >
            // Search bar
            <div class="data-list-search">
                <input
                    type="search"
                    class="data-list-search-input"
                    placeholder="Search by name..."
                    aria-label="Search datasets"
                    prop:value=move || search_query.get()
                    on:input=move |ev| search_query.set(event_target_value(&ev))
                />
                {move || {
                    let statuses = available_statuses.get();
                    if statuses.len() > 1 {
                        view! {
                            <select
                                class="data-list-status-filter"
                                aria-label="Filter by status"
                                on:change=move |ev| {
                                    let value = event_target_value(&ev);
                                    if value.is_empty() {
                                        status_filter.set(None);
                                    } else {
                                        status_filter.set(Some(value));
                                    }
                                }
                            >
                                <option value="" selected=move || status_filter.get().is_none()>
                                    "All statuses"
                                </option>
                                {statuses.into_iter().map(|status| {
                                    let s_value = status.clone();
                                    let s_selected = status.clone();
                                    let s_display = status.clone();
                                    view! {
                                        <option
                                            value=s_value
                                            selected=move || status_filter.get().as_ref() == Some(&s_selected)
                                        >
                                            {s_display}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>

            // Header with count and sort controls
            <div class="data-list-header">
                <div class="data-list-count">
                    {move || {
                        let filtered_count = filtered_items.get().len();
                        let total_count = items.get().len();
                        let source_label = source.get().label();
                        if filtered_count != total_count {
                            format!("{} of {} {}", filtered_count, total_count, source_label)
                        } else {
                            format!("{} {}", total_count, source_label)
                        }
                    }}
                </div>
                <div class="data-list-sort">
                    <select
                        class="data-list-sort-select"
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            let sort = match value.as_str() {
                                "name_asc" => DataSort::NameAsc,
                                "name_desc" => DataSort::NameDesc,
                                "date_asc" => DataSort::DateAsc,
                                "date_desc" => DataSort::DateDesc,
                                "size_asc" => DataSort::SizeAsc,
                                "size_desc" => DataSort::SizeDesc,
                                _ => DataSort::DateDesc,
                            };
                            sort_signal.set(sort);
                        }
                    >
                        <option value="date_desc" selected=move || sort_signal.get() == DataSort::DateDesc>
                            "Newest first"
                        </option>
                        <option value="date_asc" selected=move || sort_signal.get() == DataSort::DateAsc>
                            "Oldest first"
                        </option>
                        <option value="name_asc" selected=move || sort_signal.get() == DataSort::NameAsc>
                            "Name (A-Z)"
                        </option>
                        <option value="name_desc" selected=move || sort_signal.get() == DataSort::NameDesc>
                            "Name (Z-A)"
                        </option>
                        <option value="size_desc" selected=move || sort_signal.get() == DataSort::SizeDesc>
                            "Largest first"
                        </option>
                        <option value="size_asc" selected=move || sort_signal.get() == DataSort::SizeAsc>
                            "Smallest first"
                        </option>
                    </select>
                </div>
            </div>

            // List content
            <div class="data-list-content">
                {move || {
                    if loading.get() {
                        view! {
                            <div class="data-list-loading">
                                <Spinner />
                            </div>
                        }.into_any()
                    } else {
                        let all_items = items.get();
                        let items_vec = filtered_items.get();

                        if all_items.is_empty() {
                            view! {
                                <DataListEmpty source=source.get() />
                            }.into_any()
                        } else if items_vec.is_empty() {
                            // Show "no results" when filtered to empty
                            view! {
                                <div class="data-list-no-results">
                                    <p>"No items match your search."</p>
                                    <button
                                        type="button"
                                        class="data-list-clear-filters"
                                        on:click=move |_| {
                                            search_query.set(String::new());
                                            status_filter.set(None);
                                        }
                                    >
                                        "Clear filters"
                                    </button>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="data-list-items" role="listbox">
                                    {items_vec.into_iter().enumerate().map(|(idx, item)| {
                                        let item_idx = idx;
                                        let item_id_for_click = item.id.clone();
                                        let item_id_for_class = item.id.clone();
                                        let item_id_for_aria = item.id.clone();
                                        let item_name = item.name.clone();
                                        let item_status = item.status.clone();
                                        let item_desc = item.description.clone();
                                        let item_format = item.format.clone();
                                        let item_size = format_bytes(item.size_bytes);
                                        let item_date = if item.updated_at.is_empty() {
                                            format_date(&item.created_at)
                                        } else {
                                            format_date(&item.updated_at)
                                        };

                                        view! {
                                            <button
                                                type="button"
                                                role="option"
                                                aria-selected=move || {
                                                    selected_id.get().as_ref() == Some(&item_id_for_aria)
                                                }.to_string()
                                                class=move || {
                                                    let is_selected = selected_id.get().as_ref() == Some(&item_id_for_class);
                                                    let is_focused = focused_index.get() == Some(item_idx);
                                                    match (is_selected, is_focused) {
                                                        (true, true) => "data-list-item data-list-item-selected data-list-item-focused",
                                                        (true, false) => "data-list-item data-list-item-selected",
                                                        (false, true) => "data-list-item data-list-item-focused",
                                                        (false, false) => "data-list-item",
                                                    }
                                                }
                                                on:click=move |_| {
                                                    focused_index.set(Some(item_idx));
                                                    on_select.run(item_id_for_click.clone());
                                                }
                                            >
                                                <div class="data-list-item-header">
                                                    <span class="data-list-item-name">{item_name.clone()}</span>
                                                    <Badge variant=item.status_variant>
                                                        {item_status.clone()}
                                                    </Badge>
                                                </div>
                                                {item_desc.as_ref().map(|desc| {
                                                    let d = desc.clone();
                                                    view! {
                                                        <p class="data-list-item-desc">{d}</p>
                                                    }
                                                })}
                                                <div class="data-list-item-meta">
                                                    {item_format.as_ref().map(|fmt| {
                                                        let f = fmt.clone();
                                                        view! {
                                                            <span class="data-list-item-format">{f.to_uppercase()}</span>
                                                        }
                                                    })}
                                                    <span class="data-list-item-size">{item_size.clone()}</span>
                                                    <span class="data-list-item-date">{item_date.clone()}</span>
                                                </div>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </div>
        </div>
    }
}

/// Empty state component for the data list.
#[component]
fn DataListEmpty(source: DataSource) -> impl IntoView {
    let (title, description) = match source {
        DataSource::Documents => (
            "No documents",
            "Upload documents to create training datasets from PDFs, markdown, and other files.",
        ),
        DataSource::Datasets => (
            "No datasets",
            "Upload or create datasets to train adapters. Datasets are JSONL files with instruction-response pairs.",
        ),
        DataSource::Preprocessed => (
            "No preprocessed data",
            "Run CoreML preprocessing on datasets to cache features for faster training.",
        ),
    };

    view! {
        <div class="data-list-empty">
            <div class="data-list-empty-icon">{source.icon()}</div>
            <h3 class="data-list-empty-title">{title}</h3>
            <p class="data-list-empty-desc">{description}</p>
        </div>
    }
}
