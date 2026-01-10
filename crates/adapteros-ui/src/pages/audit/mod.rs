//! Audit page
//!
//! Immutable audit log viewer with hash chain visualization and verification.

mod components;
mod tabs;

use crate::api::{ApiClient, AuditLogsQuery};
use crate::components::{Button, ButtonVariant};
use crate::hooks::use_api_resource;
#[cfg(debug_assertions)]
use crate::hooks::LoadingState;
use leptos::prelude::*;
use std::sync::Arc;

use components::{ChainStatusSummary, FilterSection};
use tabs::{ComplianceTab, HashChainTab, MerkleTreeTab, TimelineTab};

// ============================================================================
// Tab types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditTab {
    Timeline,
    HashChain,
    MerkleTree,
    Compliance,
}

#[component]
fn TabButton(label: &'static str, tab: AuditTab, active_tab: RwSignal<AuditTab>) -> impl IntoView {
    let is_active = move || active_tab.get() == tab;

    view! {
        <button
            class=move || {
                let base = "py-4 px-1 border-b-2 font-medium text-sm transition-colors";
                if is_active() {
                    format!("{} border-primary text-primary", base)
                } else {
                    format!(
                        "{} border-transparent text-muted-foreground hover:text-foreground hover:border-border",
                        base,
                    )
                }
            }
            on:click=move |_| active_tab.set(tab)
        >
            {label}
        </button>
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
    let (compliance, _refetch_compliance) =
        use_api_resource(
            |client: Arc<ApiClient>| async move { client.get_compliance_audit().await },
        );

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let LoadingState::Loaded(ref data) = logs.get() {
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
        refetch_logs();
        refetch_chain();
        refetch_verification();
    };

    view! {
        <div class="p-6 space-y-6">
                // Header
                <div class="flex items-center justify-between">
                    <div>
                        <h1 class="text-3xl font-bold tracking-tight">"Audit Log"</h1>
                        <p class="text-muted-foreground mt-1">
                            "Immutable record of all system events with cryptographic verification"
                        </p>
                    </div>
                    <div class="flex items-center gap-2">
                        <Button variant=ButtonVariant::Outline on:click=move |_| refetch_all()>
                            "Refresh"
                        </Button>
                        <Button variant=ButtonVariant::Outline>"Export"</Button>
                    </div>
                </div>

                // Chain status summary
                <ChainStatusSummary
                    verification=verification
                    chain=chain
                    compliance=compliance
                />

                // Tab navigation
                <div class="border-b border-border">
                    <nav class="-mb-px flex space-x-8">
                        <TabButton label="Event Timeline" tab=AuditTab::Timeline active_tab=active_tab/>
                        <TabButton label="Hash Chain" tab=AuditTab::HashChain active_tab=active_tab/>
                        <TabButton label="Merkle Tree" tab=AuditTab::MerkleTree active_tab=active_tab/>
                        <TabButton label="Compliance" tab=AuditTab::Compliance active_tab=active_tab/>
                    </nav>
                </div>

                // Filters section
                <FilterSection
                    active_tab=active_tab
                    action_filter=action_filter
                    status_filter=status_filter
                    resource_filter=resource_filter
                />

                // Tab content
                {move || {
                    match active_tab.get() {
                        AuditTab::Timeline => {
                            view! { <TimelineTab logs=logs/> }.into_any()
                        }
                        AuditTab::HashChain => {
                            view! { <HashChainTab chain=chain/> }.into_any()
                        }
                        AuditTab::MerkleTree => {
                            view! { <MerkleTreeTab chain=chain verification=verification/> }.into_any()
                        }
                        AuditTab::Compliance => {
                            view! { <ComplianceTab compliance=compliance/> }.into_any()
                        }
                    }
                }}
        </div>
    }
}
