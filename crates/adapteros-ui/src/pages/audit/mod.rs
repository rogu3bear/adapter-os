//! Audit page
//!
//! Immutable audit log viewer with hash chain visualization and verification.

mod components;
mod tabs;

use crate::api::{ApiClient, AuditLogsQuery};
use crate::components::{
    Button, ButtonVariant, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Spinner, TabNav,
};
use crate::hooks::use_api_resource;
use crate::hooks::LoadingState;
use leptos::prelude::*;
use std::fmt;
use std::sync::Arc;

use components::{ChainStatusSummary, FilterSection};
use tabs::{ComplianceTab, EmbeddingsTab, HashChainTab, MerkleTreeTab, TimelineTab};

// ============================================================================
// Tab types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditTab {
    Timeline,
    HashChain,
    MerkleTree,
    Compliance,
    Embeddings,
}

impl fmt::Display for AuditTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditTab::Timeline => write!(f, "timeline"),
            AuditTab::HashChain => write!(f, "hashchain"),
            AuditTab::MerkleTree => write!(f, "merkletree"),
            AuditTab::Compliance => write!(f, "compliance"),
            AuditTab::Embeddings => write!(f, "embeddings"),
        }
    }
}

// ============================================================================
// Audit page - main component
// ============================================================================

/// Audit log viewer page with chain visualization
#[component]
pub fn Audit() -> impl IntoView {
    // Active tab state
    let active_tab = RwSignal::new(AuditTab::Timeline);

    // Filter state
    let action_filter = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::new());
    let resource_filter = RwSignal::new(String::new());

    // Build query from filters
    let query = Memo::new(move |_| AuditLogsQuery {
        action: {
            let a = action_filter.get();
            if a.is_empty() {
                None
            } else {
                Some(a)
            }
        },
        status: {
            let s = status_filter.get();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        },
        resource_type: {
            let r = resource_filter.get();
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
        let q = query.get();
        async move { client.query_audit_logs(&q).await }
    });

    // Fetch audit chain
    let (chain, refetch_chain) =
        use_api_resource(
            |client: Arc<ApiClient>| async move { client.get_audit_chain(Some(50)).await },
        );

    // Fetch chain verification
    let (verification, refetch_verification) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.verify_audit_chain().await });

    // Fetch compliance
    let (compliance, refetch_compliance) =
        use_api_resource(
            |client: Arc<ApiClient>| async move { client.get_compliance_audit().await },
        );

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref data)) = logs.try_get() {
            web_sys::console::log_1(
                &format!(
                    "[list] audit logs: {} items (total: {})",
                    data.logs.len(),
                    data.total
                )
                .into(),
            );
        }
    });

    let refetch_all = move || {
        refetch_logs.run(());
        refetch_chain.run(());
        refetch_verification.run(());
        refetch_compliance.run(());
    };

    // Export state
    let (exporting, set_exporting) = signal(false);

    // Export handler
    let on_export = move |_| {
        set_exporting.set(true);
        let logs_data = logs.get();
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
                    web_sys::console::error_1(&format!("Export failed: {:?}", e).into());
                }
            }
            set_exporting.set(false);
        });
    };

    view! {
        <PageScaffold
            title="Audit Log"
            subtitle="Immutable record of all system events with cryptographic verification"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Govern", "/audit"),
                PageBreadcrumbItem::current("Audit Log"),
            ]
        >
            <PageScaffoldActions slot>
                <Button variant=ButtonVariant::Outline on:click=move |_| refetch_all()>
                    "Refresh"
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on:click=on_export
                    disabled=Signal::derive(move || exporting.get())
                >
                    {move || if exporting.get() {
                        view! { <Spinner/> }.into_any()
                    } else {
                        view! { "Export" }.into_any()
                    }}
                </Button>
            </PageScaffoldActions>

            // Chain status summary
            <ChainStatusSummary
                verification=verification
                chain=chain
                compliance=compliance
            />

            // Tab navigation
            <TabNav
                tabs=vec![
                    (AuditTab::Timeline, "Event Timeline"),
                    (AuditTab::HashChain, "Hash Chain"),
                    (AuditTab::MerkleTree, "Merkle Tree"),
                    (AuditTab::Compliance, "Compliance"),
                    (AuditTab::Embeddings, "Embeddings"),
                ]
                active=active_tab
            />

            // Filters section
            <FilterSection
                active_tab=active_tab
                action_filter=action_filter
                status_filter=status_filter
                resource_filter=resource_filter
                on_filters_changed=refetch_logs.as_callback()
            />

            // Tab content
            {move || {
                match active_tab.get() {
                    AuditTab::Timeline => {
                        view! { <TimelineTab logs=logs on_retry=Callback::new(move |_| refetch_logs.run(()))/> }.into_any()
                    }
                    AuditTab::HashChain => {
                        view! { <HashChainTab chain=chain on_retry=Callback::new(move |_| refetch_chain.run(()))/> }.into_any()
                    }
                    AuditTab::MerkleTree => {
                        view! { <MerkleTreeTab chain=chain verification=verification /> }.into_any()
                    }
                    AuditTab::Compliance => {
                        view! { <ComplianceTab compliance=compliance on_retry=Callback::new(move |_| refetch_compliance.run(()))/> }.into_any()
                    }
                    AuditTab::Embeddings => {
                        view! { <EmbeddingsTab/> }.into_any()
                    }
                }
            }}
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
