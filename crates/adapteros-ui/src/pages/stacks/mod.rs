//! Adapter Stacks management page
//!
//! Provides UI for managing adapter stacks - compositions of adapters
//! that can be activated together for inference.
//!
//! Automatically refreshes when the global `RefetchTopic::Stacks` signal
//! is triggered (e.g., after training job completion creates a new stack).

mod detail;
mod dialogs;
mod list;

pub mod helpers;

// Re-export main page components
pub use detail::StackDetail;

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, ErrorDisplay, LoadingDisplay, PageHeader, RefreshButton,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::refetch::{use_refetch_signal, RefetchTopic};
use dialogs::CreateStackDialog;
use leptos::prelude::*;
use list::StacksList;
use std::sync::Arc;

/// Stacks list page
#[component]
pub fn Stacks() -> impl IntoView {
    let (stacks, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_stacks().await });

    let show_create_dialog = RwSignal::new(false);
    let refetch_trigger = RwSignal::new(0u32);

    // Subscribe to global stacks refetch topic (triggered on training completion)
    let stacks_refetch_counter = use_refetch_signal(RefetchTopic::Stacks);

    // Call refetch when trigger changes OR global signal fires
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch.run(());
    });

    // Respond to global refetch signal from training completion
    Effect::new(move || {
        let counter = stacks_refetch_counter.get();
        // Skip initial effect run (counter starts at 0)
        if counter > 0 {
            refetch.run(());
        }
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <div class="p-6 space-y-6">
            <PageHeader
                title="Runtime Stacks"
                subtitle="Compose adapter stacks for inference"
            >
                <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                >
                    "Create Stack"
                </Button>
            </PageHeader>

            {move || {
                match stacks.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading stacks..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <StacksList stacks=data refetch_trigger=refetch_trigger/> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
                            />
                        }.into_any()
                    }
                }
            }}

            <CreateStackDialog
                open=show_create_dialog
                refetch_trigger=refetch_trigger
            />
        </div>
    }
}
