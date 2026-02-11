//! Pagination controls component.
//!
//! Reuses the same previous/page/next control pattern across list pages.

use super::button::{Button, ButtonSize, ButtonVariant};
use leptos::prelude::*;

/// Shared pagination controls for list views.
#[component]
pub fn PaginationControls(
    /// Current page (1-indexed).
    current_page: usize,
    /// Total pages (minimum 1).
    total_pages: usize,
    /// Optional total items count for summary text.
    #[prop(optional)]
    total_items: Option<usize>,
    /// Whether to center the controls.
    #[prop(optional, default = false)]
    centered: bool,
    /// Additional container classes.
    #[prop(optional, into)]
    class: String,
    /// Previous page callback.
    on_prev: Callback<()>,
    /// Next page callback.
    on_next: Callback<()>,
) -> impl IntoView {
    if total_pages <= 1 {
        return view! {}.into_any();
    }

    let summary = total_items
        .map(|count| format!("Page {} of {} ({} total)", current_page, total_pages, count))
        .unwrap_or_else(|| format!("Page {} of {}", current_page, total_pages));

    if centered {
        view! {
            <div class=format!("flex items-center justify-center gap-2 {}", class)>
                <Button
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Sm
                    disabled=Signal::derive(move || current_page <= 1)
                    on_click=on_prev
                >
                    "Previous"
                </Button>
                <span class="text-sm text-muted-foreground">{summary.clone()}</span>
                <Button
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Sm
                    disabled=Signal::derive(move || current_page >= total_pages)
                    on_click=on_next
                >
                    "Next"
                </Button>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class=format!("flex items-center justify-between {}", class)>
                <div class="text-sm text-muted-foreground">{summary}</div>
                <div class="flex items-center gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        disabled=Signal::derive(move || current_page <= 1)
                        on_click=on_prev
                    >
                        "Previous"
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        disabled=Signal::derive(move || current_page >= total_pages)
                        on_click=on_next
                    >
                        "Next"
                    </Button>
                </div>
            </div>
        }
        .into_any()
    }
}
