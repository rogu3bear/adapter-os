//! Offline banner to indicate backend connectivity issues.

use crate::api::ApiClient;
use crate::components::Button;
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Banner displayed when the backend is unreachable.
#[component]
pub fn OfflineBanner() -> impl IntoView {
    let (health, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.health().await });

    let retry = StoredValue::new(refetch);

    view! {
        <Show when=move || matches!(health.get(), LoadingState::Error(_))>
            <div class="offline-banner">
                <div class="offline-banner-content">
                    <span class="offline-banner-title">"Backend Offline"</span>
                    <span class="offline-banner-message">
                        "Unable to reach the adapterOS API. Some data may be stale."
                    </span>
                </div>
                <Button
                    class="offline-banner-action".to_string()
                    on_click=Callback::new(move |_| retry.with_value(|f| f()))
                >
                    "Retry"
                </Button>
            </div>
        </Show>
    }
}
