//! Reviews page
//!
//! Human-in-the-loop review queue management.

use crate::components::Card;
use leptos::prelude::*;

/// Reviews queue page
#[component]
pub fn Reviews() -> impl IntoView {
    view! {
        <div class="p-6 space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Reviews Queue"</h1>
                    <p class="text-muted-foreground mt-1">"Human-in-the-loop review management"</p>
                </div>
            </div>

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
            <div class="py-8 text-center">
                <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-8 w-8 text-muted-foreground"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                    </svg>
                </div>
                <p class="text-muted-foreground">"No pending reviews"</p>
                <p class="text-sm text-muted-foreground mt-1">"Items requiring human review will appear here."</p>
            </div>
        </Card>
    }
}
