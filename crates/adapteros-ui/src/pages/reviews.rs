//! Reviews page
//!
//! Human-in-the-loop review queue management.

use crate::components::{Card, EmptyState, EmptyStateVariant, PageHeader};
use leptos::prelude::*;

/// Reviews queue page
#[component]
pub fn Reviews() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <PageHeader
                title="Reviews Queue"
                subtitle="Human-in-the-loop review management"
            />

            <ReviewsQueue />
        </div>
    }
}

/// Reviews queue component
#[component]
fn ReviewsQueue() -> impl IntoView {
    // TODO: Wire to /v1/reviews/paused API
    view! {
        <Card>
            <EmptyState
                variant=EmptyStateVariant::Empty
                title="No pending reviews"
                description="Items requiring human review will appear here."
            />
        </Card>
    }
}
