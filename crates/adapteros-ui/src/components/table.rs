//! Table component with sorting, striping, and enhanced hover states
//!
//! Uses semantic CSS classes from components.css.
//! No Tailwind selector syntax.

use leptos::prelude::*;

/// Sort direction for table columns
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortDirection {
    /// No sort applied
    #[default]
    None,
    /// Ascending order (A-Z, 0-9)
    Ascending,
    /// Descending order (Z-A, 9-0)
    Descending,
}

impl SortDirection {
    /// Cycle to next sort state: None -> Ascending -> Descending -> None
    pub fn next(&self) -> Self {
        match self {
            Self::None => Self::Ascending,
            Self::Ascending => Self::Descending,
            Self::Descending => Self::None,
        }
    }

    fn class(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Ascending => "sorted-asc",
            Self::Descending => "sorted-desc",
        }
    }

    fn aria_sort(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }
}

/// Table wrapper with styling options
#[component]
pub fn Table(
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// Enable striped rows (alternating background)
    #[prop(optional)]
    striped: bool,
    /// Enable hover highlighting on rows (default: true)
    #[prop(optional, default = true)]
    hoverable: bool,
    children: Children,
) -> impl IntoView {
    let striped_class = if striped { "table-striped" } else { "" };
    let hover_class = if hoverable { "table-hoverable" } else { "" };
    let full_class = format!("table {} {} {}", striped_class, hover_class, class);

    view! {
        <div class="table-wrapper">
            <table class=full_class>
                {children()}
            </table>
        </div>
    }
}

/// Table header - child rows get bottom border via CSS
#[component]
pub fn TableHeader(children: Children) -> impl IntoView {
    view! {
        <thead class="table-header">
            {children()}
        </thead>
    }
}

/// Table body - last row has no border via CSS
#[component]
pub fn TableBody(children: Children) -> impl IntoView {
    view! {
        <tbody class="table-body">
            {children()}
        </tbody>
    }
}

/// Table row with hover and selected states
#[component]
pub fn TableRow(
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// Whether this row is selected
    #[prop(optional)]
    selected: bool,
    children: Children,
) -> impl IntoView {
    let full_class = format!("table-row {}", class);
    let data_state = if selected { "selected" } else { "" };

    view! {
        <tr class=full_class data-state=data_state>
            {children()}
        </tr>
    }
}

/// Table head cell (non-sortable)
#[component]
pub fn TableHead(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("table-header-cell {}", class);

    view! {
        <th class=full_class scope="col">
            {children()}
        </th>
    }
}

/// Sortable table header cell with sort indicator
#[component]
pub fn TableHeadSortable(
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// Current sort direction for this column
    sort: SortDirection,
    /// Callback when header is clicked to toggle sort
    on_sort: Callback<()>,
    children: Children,
) -> impl IntoView {
    let full_class = format!(
        "table-header-cell table-header-sortable {} {}",
        sort.class(),
        class
    );

    view! {
        <th class=full_class scope="col" aria-sort=sort.aria_sort() role="columnheader">
            <button class="table-sort-button" on:click=move |_| on_sort.run(()) type="button">
                <span class="table-sort-label">{children()}</span>
                <span class="table-sort-icon" aria-hidden="true">
                    {match sort {
                        SortDirection::None => {
                            view! {
                                <svg
                                    class="sort-icon sort-icon-none"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                >
                                    <path d="M7 15l5 5 5-5M7 9l5-5 5 5"/>
                                </svg>
                            }
                                .into_any()
                        }
                        SortDirection::Ascending => {
                            view! {
                                <svg
                                    class="sort-icon sort-icon-asc"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                >
                                    <path d="M7 14l5-5 5 5"/>
                                </svg>
                            }
                                .into_any()
                        }
                        SortDirection::Descending => {
                            view! {
                                <svg
                                    class="sort-icon sort-icon-desc"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                >
                                    <path d="M7 10l5 5 5-5"/>
                                </svg>
                            }
                                .into_any()
                        }
                    }}
                </span>
            </button>
        </th>
    }
}

/// Table cell
#[component]
pub fn TableCell(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("table-cell {}", class);

    view! {
        <td class=full_class>
            {children()}
        </td>
    }
}
