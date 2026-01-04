//! Table component
//!
//! Uses semantic CSS classes from components.css.
//! No Tailwind selector syntax.

use leptos::prelude::*;

/// Table wrapper with overflow handling
#[component]
pub fn Table(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("table {}", class);

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
pub fn TableRow(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("table-row {}", class);

    view! {
        <tr class=full_class>
            {children()}
        </tr>
    }
}

/// Table head cell
#[component]
pub fn TableHead(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("table-header-cell {}", class);

    view! {
        <th class=full_class>
            {children()}
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
