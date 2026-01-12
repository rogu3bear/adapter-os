//! Banner to detect and surface frontend/backend version drift.
use crate::api::{ui_build_version, ApiClient};
use crate::components::Button;
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Shows a reload prompt when the UI build version differs from backend health version.
#[component]
pub fn VersionSkewBanner() -> impl IntoView {
    let ui_version = ui_build_version();
    let (health, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.health().await });

    let retry = StoredValue::new(refetch);

    // Force reload to pull new assets (cache-busting)
    let hard_reload = Callback::new(move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    });

    view! {
        <Show
            when=move || match health.get() {
                LoadingState::Loaded(data) => {
                    let backend = data.version.trim();
                    let ui = ui_version.trim();
                    !backend.is_empty() && !ui.is_empty() && backend != ui
                }
                _ => false,
            }
        >
            <div class="offline-banner">
                <div class="offline-banner-content">
                    <span class="offline-banner-title">"Update available"</span>
                    <span class="offline-banner-message">
                        {move || {
                            if let LoadingState::Loaded(data) = health.get() {
                                format!(
                                    "Frontend {} differs from backend {}. Reload to pick up the latest assets.",
                                    ui_version,
                                    data.version
                                )
                            } else {
                                "Frontend build differs from backend. Reload to update.".to_string()
                            }
                        }}
                    </span>
                </div>
                <div class="offline-banner-actions">
                    <Button
                        variant=crate::components::ButtonVariant::Outline
                        class="offline-banner-action".to_string()
                        on_click=Callback::new(move |_| retry.with_value(|f| f()))
                    >
                        "Recheck"
                    </Button>
                    <Button class="offline-banner-action".to_string() on_click=hard_reload>
                        "Reload"
                    </Button>
                </div>
            </div>
        </Show>
    }
}
