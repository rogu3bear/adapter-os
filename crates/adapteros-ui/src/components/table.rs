//! Table component

use leptos::prelude::*;

/// Table wrapper
#[component]
pub fn Table(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("w-full caption-bottom text-sm {}", class);

    view! {
        <div class="relative w-full overflow-auto">
            <table class=full_class>
                {children()}
            </table>
        </div>
    }
}

/// Table header
#[component]
pub fn TableHeader(children: Children) -> impl IntoView {
    view! {
        <thead class="[&_tr]:border-b">
            {children()}
        </thead>
    }
}

/// Table body
#[component]
pub fn TableBody(children: Children) -> impl IntoView {
    view! {
        <tbody class="[&_tr:last-child]:border-0">
            {children()}
        </tbody>
    }
}

/// Table row
#[component]
pub fn TableRow(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!(
        "border-b transition-colors hover:bg-muted/50 data-[state=selected]:bg-muted {}",
        class
    );

    view! {
        <tr class=full_class>
            {children()}
        </tr>
    }
}

/// Table head cell
#[component]
pub fn TableHead(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!(
        "h-12 px-4 text-left align-middle font-medium text-muted-foreground [&:has([role=checkbox])]:pr-0 {}",
        class
    );

    view! {
        <th class=full_class>
            {children()}
        </th>
    }
}

/// Table cell
#[component]
pub fn TableCell(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("p-4 align-middle [&:has([role=checkbox])]:pr-0 {}", class);

    view! {
        <td class=full_class>
            {children()}
        </td>
    }
}
