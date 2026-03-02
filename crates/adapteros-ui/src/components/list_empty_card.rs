//! Shared empty-card wrapper for list pages.
//!
//! Keeps empty-state card structure consistent across route screens.

use crate::components::async_state::{EmptyState, EmptyStateVariant};
use crate::components::Card;
use leptos::prelude::*;

/// Standard list empty state rendered inside a card.
#[component]
pub fn ListEmptyCard(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] variant: EmptyStateVariant,
    #[prop(optional, into)] action_label: Option<String>,
    #[prop(optional)] on_action: Option<Callback<()>>,
) -> impl IntoView {
    let action = action_label.zip(on_action);

    match (description, action) {
        (Some(description), Some((action_label, on_action))) => view! {
            <Card>
                <EmptyState
                    title=title
                    description=description
                    variant=variant
                    action_label=action_label
                    on_action=on_action
                />
            </Card>
        }
        .into_any(),
        (Some(description), None) => view! {
            <Card>
                <EmptyState
                    title=title
                    description=description
                    variant=variant
                />
            </Card>
        }
        .into_any(),
        (None, Some((action_label, on_action))) => view! {
            <Card>
                <EmptyState
                    title=title
                    variant=variant
                    action_label=action_label
                    on_action=on_action
                />
            </Card>
        }
        .into_any(),
        (None, None) => view! {
            <Card>
                <EmptyState
                    title=title
                    variant=variant
                />
            </Card>
        }
        .into_any(),
    }
}
