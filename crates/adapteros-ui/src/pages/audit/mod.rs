//! Audit page
//!
//! Immutable audit log viewer with event timeline and chain verification summary.

mod components;
mod tabs;

use crate::api::{ApiClient, AuditLogsQuery};
use crate::components::{
    Button, ButtonVariant, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner,
};
use crate::hooks::use_api_resource;
use crate::hooks::LoadingState;
use crate::signals::notifications::try_emit_global_error_with_details;
use leptos::prelude::*;
use std::sync::Arc;

use components::{ChainStatusSummary, FilterSection};
use tabs::TimelineTab;

// ============================================================================
// Audit page - main component
// ============================================================================

/// Audit log viewer page with chain verification summary
#[component]
pub fn Audit() -> impl IntoView {
    // Filter state
    let action_filter = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let resource_filter = RwSignal::new(String::new());

    // Build query from filters
    let query = Memo::new(move |_| AuditLogsQuery {
        action: {
            let a = action_filter.try_get().unwrap_or_default();
            if a.is_empty() {
                None
            } else {
                Some(a)
            }
        },
        status: {
            let s = status_filter.try_get().unwrap_or_default();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        },
        resource_type: {
            let r = resource_filter.try_get().unwrap_or_default();
            if r.is_empty() {
                None
            } else {
                Some(r)
            }
        },
        limit: Some(100),
        ..Default::default()
    });

    // Fetch audit logs
    let (logs, refetch_logs) = use_api_resource(move |client: Arc<ApiClient>| {
        let q = query.try_get().unwrap_or_default();
        async move { client.query_audit_logs(&q).await }
    });

    // Fetch chain verification (powers the summary strip)
    let (verification, refetch_verification) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.verify_audit_chain().await });

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref data)) = logs.try_get() {
            crate::debug_log!(
                "[list] audit logs: {} items (total: {})",
                data.logs.len(),
                data.total
            );
        }
    });

    let refetch_all = move || {
        refetch_logs.run(());
        refetch_verification.run(());
    };

    // Export state
    let (exporting, set_exporting) = signal(false);

    // Export handler
    let on_export = move |_| {
        let _ = set_exporting.try_set(true);
        let logs_data = logs.try_get().unwrap_or_default();
        wasm_bindgen_futures::spawn_local(async move {
            // Get current logs data
            if let LoadingState::Loaded(data) = logs_data {
                // Convert to JSONL format
                let mut jsonl = String::new();
                for log in &data.logs {
                    if let Ok(line) = serde_json::to_string(log) {
                        jsonl.push_str(&line);
                        jsonl.push('\n');
                    }
                }

                // Trigger browser download
                if let Err(e) = trigger_download(&jsonl, "audit_logs.jsonl") {
                    try_emit_global_error_with_details(
                        "Export Failed",
                        "Could not download audit logs",
                        &format!("{:?}", e),
                    );
                }
            }
            let _ = set_exporting.try_set(false);
        });
    };

    view! {
        <PageScaffold
            title="Audit Log"
            subtitle="Immutable record of all system events with cryptographic verification"
            breadcrumbs=vec![
                PageBreadcrumbItem::label("System"),
                PageBreadcrumbItem::current("Audit Log"),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                <Button variant=ButtonVariant::Outline on:click=move |_| refetch_all()>
                    "Refresh"
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on:click=on_export
                    disabled=Signal::derive(move || exporting.try_get().unwrap_or(false))
                >
                    <Show when=move || exporting.try_get().unwrap_or(false) fallback=move || view! { "Export" }>
                        <Spinner/>
                    </Show>
                </Button>
            </PageScaffoldActions>

            // Chain status summary
            <ChainStatusSummary verification=verification />

            // Filters
            <FilterSection
                action_filter=action_filter
                status_filter=status_filter
                resource_filter=resource_filter
                on_filters_changed=refetch_logs.as_callback()
            />

            // Event timeline
            <TimelineTab logs=logs on_retry=Callback::new(move |_| refetch_logs.run(())) />
        </PageScaffold>
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Trigger a browser file download with the given content and filename
fn trigger_download(content: &str, filename: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;

    // Create blob with content
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&wasm_bindgen::JsValue::from_str(content));

    let blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type("application/x-ndjson");

    let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &blob_options)?;

    // Create object URL
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;

    // Create temporary anchor element and trigger click
    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")?
        .dyn_into()
        .map_err(|_| "Failed to create anchor")?;
    anchor.set_href(&url);
    anchor.set_download(filename);

    // Append to body, click, and remove
    let body = document.body().ok_or("No body")?;
    body.append_child(&anchor)?;
    anchor.click();
    body.remove_child(&anchor)?;

    // Clean up object URL
    web_sys::Url::revoke_object_url(&url)?;

    Ok(())
}
