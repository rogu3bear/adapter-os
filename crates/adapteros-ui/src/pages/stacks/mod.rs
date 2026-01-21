//! Adapter Stacks management page
//!
//! Provides UI for managing adapter stacks - compositions of adapters
//! that can be activated together for inference.

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

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch.run(());
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Adapter Stacks"
                subtitle="Manage adapter compositions for inference"
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
