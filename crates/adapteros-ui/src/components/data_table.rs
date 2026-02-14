//! DataTable component with built-in async state handling
//!
//! High-level table that wraps LoadingState and renders loading, empty, or data automatically.
//!
//! When the caller already has a `Vec<T>` (no loading state needed), use [`loaded_signal`]
//! to wrap it into the signal type DataTable expects:
//!
//! ```rust,ignore
//! let items = Signal::derive(move || filtered_items.get());
//! view! { <DataTable data=loaded_signal(items) columns=columns /> }
//! ```

use crate::components::{
    Card, EmptyState, EmptyStateVariant, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::LoadingState;
use leptos::prelude::*;
use std::sync::Arc;

/// Column definition for DataTable
#[derive(Clone)]
pub struct Column<T: Clone + 'static> {
    /// Column header label
    pub header: String,
    /// Function to render cell content (Arc for cloneability)
    pub cell: Arc<dyn Fn(&T) -> AnyView + Send + Sync>,
    /// Optional CSS class for the column
    pub class: Option<String>,
}

impl<T: Clone + 'static> Column<T> {
    /// Create a simple text column
    pub fn text<F>(header: impl Into<String>, extractor: F) -> Self
    where
        F: Fn(&T) -> String + Send + Sync + Clone + 'static,
    {
        let extractor = extractor.clone();
        Self {
            header: header.into(),
            cell: Arc::new(move |item| {
                let text = extractor(item);
                view! { <span>{text}</span> }.into_any()
            }),
            class: None,
        }
    }

    /// Create a column with custom rendering
    pub fn custom<V, F>(header: impl Into<String>, render: F) -> Self
    where
        V: IntoView + 'static,
        F: Fn(&T) -> V + Send + Sync + Clone + 'static,
    {
        let render = render.clone();
        Self {
            header: header.into(),
            cell: Arc::new(move |item| render(item).into_any()),
            class: None,
        }
    }

    /// Add a CSS class to this column
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }
}

/// Wrap a `Signal<Vec<T>>` into the `ReadSignal<LoadingState<Vec<T>>>` that [`DataTable`] expects.
///
/// Use this when the parent already owns the data and handles loading/error states itself,
/// eliminating the boilerplate of creating a `LoadingState::Loaded` wrapper signal.
///
/// # Example
/// ```rust,ignore
/// let items = Signal::derive(move || my_vec_signal.get());
/// view! { <DataTable data=loaded_signal(items) columns=columns /> }
/// ```
pub fn loaded_signal<T>(source: Signal<Vec<T>>) -> ReadSignal<LoadingState<Vec<T>>>
where
    T: Clone + Send + Sync + 'static,
{
    let (read, write) = signal(LoadingState::Loaded(source.get_untracked()));
    Effect::new(move || {
        write.set(LoadingState::Loaded(source.get()));
    });
    read
}

/// High-level data table with async state handling
///
/// Wraps `LoadingState` and automatically renders:
/// - Loading spinner during fetch
/// - Empty state when data is empty
/// - Error display with retry on failure
/// - Full table when data is available
///
/// # Example
/// ```rust,ignore
/// let (users, refetch) = use_api_resource(|c| async move { c.list_users().await });
///
/// let columns = vec![
///     Column::text("Name", |u: &User| u.name.clone()),
///     Column::text("Email", |u: &User| u.email.clone()),
///     Column::custom("Role", |u: &User| view! {
///         <Badge>{u.role.clone()}</Badge>
///     }),
/// ];
///
/// view! {
///     <DataTable
///         data=users
///         columns=columns
///         empty_title="No users found"
///         on_retry=refetch
///     />
/// }
/// ```
#[component]
pub fn DataTable<T>(
    /// The loading state signal containing Vec<T>
    data: ReadSignal<LoadingState<Vec<T>>>,
    /// Column definitions
    columns: Vec<Column<T>>,
    /// Optional retry callback for error state
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
    /// Optional loading message (reserved for future use)
    #[prop(optional, into)]
    _loading_message: Option<String>,
    /// Empty state title
    #[prop(optional, into)]
    empty_title: Option<String>,
    /// Empty state description  
    #[prop(optional, into)]
    empty_description: Option<String>,
    /// Optional row click callback
    #[prop(optional)]
    on_row_click: Option<Callback<T>>,
    /// Optional per-row CSS class function.
    /// Called for each row; the returned string is appended to the row's class attribute.
    #[prop(optional)]
    row_class: Option<Arc<dyn Fn(&T) -> String + Send + Sync>>,
    /// Whether to wrap in a Card
    #[prop(optional, default = true)]
    card: bool,
    /// Additional table CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
{
    let columns = StoredValue::new(columns);
    let row_class = StoredValue::new(row_class);
    let empty_title = empty_title.unwrap_or_else(|| "No data".to_string());
    let empty_description = empty_description.clone();

    let table_view = move |items: Vec<T>| {
        let cols = columns.get_value();

        if items.is_empty() {
            return match empty_description.clone() {
                Some(desc) => view! {
                    <EmptyState
                        title=empty_title.clone()
                        description=desc
                        variant=EmptyStateVariant::Empty
                    />
                }
                .into_any(),
                None => view! {
                    <EmptyState
                        title=empty_title.clone()
                        variant=EmptyStateVariant::Empty
                    />
                }
                .into_any(),
            };
        }

        let class_inner = class.clone();

        view! {
            <Table class=class_inner>
                <TableHeader>
                    <TableRow>
                        {cols.iter().map(|col| {
                            let header = col.header.clone();
                            let col_class = col.class.clone().unwrap_or_default();
                            view! {
                                <TableHead class=col_class>
                                    {header}
                                </TableHead>
                            }
                        }).collect::<Vec<_>>()}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {items.into_iter().map(|item| {
                        let cols = columns.get_value();
                        let item_for_click = item.clone();
                        let on_click = on_row_click;
                        let has_click = on_click.is_some();

                        let mut tr_class = if has_click {
                            "table-row cursor-pointer".to_string()
                        } else {
                            "table-row".to_string()
                        };
                        if let Some(ref rc) = row_class.get_value() {
                            let extra = rc(&item);
                            if !extra.is_empty() {
                                tr_class.push(' ');
                                tr_class.push_str(&extra);
                            }
                        }

                        view! {
                            <tr
                                class=tr_class
                                on:click=move |_| {
                                    if let Some(ref cb) = on_click {
                                        cb.run(item_for_click.clone());
                                    }
                                }
                            >
                                {cols.iter().map(|col| {
                                    let content = (col.cell)(&item);
                                    let col_class = col.class.clone().unwrap_or_default();
                                    view! {
                                        <TableCell class=col_class>
                                            {content}
                                        </TableCell>
                                    }
                                }).collect::<Vec<_>>()}
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </TableBody>
            </Table>
        }
        .into_any()
    };

    // Handle loading state directly instead of using AsyncBoundary
    // (AsyncBoundary's children closure has complex type requirements)
    let inner = view! {
        {move || {
            match data.try_get().unwrap_or(LoadingState::Loading) {
                LoadingState::Idle | LoadingState::Loading => {
                    view! {
                        <div class="flex items-center justify-center py-8">
                            <crate::components::Spinner />
                        </div>
                    }.into_any()
                }
                LoadingState::Loaded(items) => {
                    table_view(items)
                }
                LoadingState::Error(e) => {
                    match on_retry {
                        Some(retry) => view! {
                            <crate::components::ErrorDisplay error=e on_retry=retry />
                        }.into_any(),
                        None => view! {
                            <crate::components::ErrorDisplay error=e />
                        }.into_any(),
                    }
                }
            }
        }}
    };

    if card {
        view! {
            <Card>
                {inner}
            </Card>
        }
        .into_any()
    } else {
        inner.into_any()
    }
}
