//! Audit page
//!
//! Immutable audit log viewer with hash chain visualization and verification.

use crate::api::{
    ApiClient, AuditChainEntry, AuditChainResponse, AuditLogEntry, AuditLogsQuery,
    AuditLogsResponse, ChainVerificationResponse, ComplianceAuditResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Shell, Spinner, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

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
    let (chain, refetch_chain) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get_audit_chain(Some(50)).await
    });

    // Fetch chain verification
    let (verification, refetch_verification) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client.verify_audit_chain().await
        });

    // Fetch compliance
    let (compliance, _refetch_compliance) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client.get_compliance_audit().await
        });

    let refetch_all = move || {
        refetch_logs();
        refetch_chain();
        refetch_verification();
    };

    view! {
        <Shell>
            <div class="space-y-6">
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
        </Shell>
    }
}

// ============================================================================
// Tab types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditTab {
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
// Filter Section
// ============================================================================

#[component]
fn FilterSection(
    active_tab: RwSignal<AuditTab>,
    action_filter: RwSignal<String>,
    status_filter: RwSignal<String>,
    resource_filter: RwSignal<String>,
) -> impl IntoView {
    let show_filters = move || matches!(active_tab.get(), AuditTab::Timeline | AuditTab::HashChain);

    view! {
        <div class=move || {
            if show_filters() { "block" } else { "hidden" }
        }>
            <Card>
                <div class="flex items-end gap-4">
                    <div class="flex-1">
                        <label class="text-sm font-medium mb-2 block">"Action Type"</label>
                        <select
                            class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                            on:change=move |ev| action_filter.set(event_target_value(&ev))
                        >
                            <option value="">"All Actions"</option>
                            <option value="create">"Create"</option>
                            <option value="update">"Update"</option>
                            <option value="delete">"Delete"</option>
                            <option value="login">"Login"</option>
                            <option value="logout">"Logout"</option>
                            <option value="inference">"Inference"</option>
                            <option value="training">"Training"</option>
                        </select>
                    </div>
                    <div class="flex-1">
                        <label class="text-sm font-medium mb-2 block">"Status"</label>
                        <select
                            class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                            on:change=move |ev| status_filter.set(event_target_value(&ev))
                        >
                            <option value="">"All Statuses"</option>
                            <option value="success">"Success"</option>
                            <option value="failure">"Failure"</option>
                            <option value="pending">"Pending"</option>
                        </select>
                    </div>
                    <div class="flex-1">
                        <label class="text-sm font-medium mb-2 block">"Resource Type"</label>
                        <select
                            class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                            on:change=move |ev| resource_filter.set(event_target_value(&ev))
                        >
                            <option value="">"All Resources"</option>
                            <option value="adapter">"Adapter"</option>
                            <option value="model">"Model"</option>
                            <option value="stack">"Stack"</option>
                            <option value="policy">"Policy"</option>
                            <option value="training_job">"Training Job"</option>
                            <option value="user">"User"</option>
                        </select>
                    </div>
                </div>
            </Card>
        </div>
    }
}

// ============================================================================
// Chain Status Summary
// ============================================================================

#[component]
fn ChainStatusSummary(
    verification: ReadSignal<LoadingState<ChainVerificationResponse>>,
    chain: ReadSignal<LoadingState<AuditChainResponse>>,
    compliance: ReadSignal<LoadingState<ComplianceAuditResponse>>,
) -> impl IntoView {
    view! {
        <div class="grid gap-4 md:grid-cols-4">
            // Chain Integrity
            <Card>
                <div class="flex items-center gap-3">
                    {move || {
                        match verification.get() {
                            LoadingState::Loaded(v) => {
                                if v.chain_valid {
                                    view! {
                                        <div class="h-10 w-10 rounded-full bg-green-500/10 flex items-center justify-center">
                                            <svg
                                                class="h-5 w-5 text-green-500"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                            >
                                                <path d="M9 12l2 2 4-4"/>
                                                <circle cx="12" cy="12" r="10"/>
                                            </svg>
                                        </div>
                                    }
                                    .into_any()
                                } else {
                                    view! {
                                        <div class="h-10 w-10 rounded-full bg-red-500/10 flex items-center justify-center">
                                            <svg
                                                class="h-5 w-5 text-red-500"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                            >
                                                <circle cx="12" cy="12" r="10"/>
                                                <line x1="15" y1="9" x2="9" y2="15"/>
                                                <line x1="9" y1="9" x2="15" y2="15"/>
                                            </svg>
                                        </div>
                                    }
                                    .into_any()
                                }
                            }
                            _ => {
                                view! {
                                    <div class="h-10 w-10 rounded-full bg-muted flex items-center justify-center">
                                        <Spinner/>
                                    </div>
                                }
                                .into_any()
                            }
                        }
                    }}
                    <div>
                        <p class="text-sm font-medium">"Chain Integrity"</p>
                        {move || {
                            match verification.get() {
                                LoadingState::Loaded(v) => {
                                    let text = if v.chain_valid { "Verified" } else { "Invalid" };
                                    let class = if v.chain_valid {
                                        "text-green-500"
                                    } else {
                                        "text-red-500"
                                    };
                                    view! {
                                        <p class=format!("text-lg font-bold {}", class)>{text}</p>
                                    }
                                    .into_any()
                                }
                                LoadingState::Loading => {
                                    view! {
                                        <p class="text-lg font-bold text-muted-foreground">
                                            "Checking..."
                                        </p>
                                    }
                                    .into_any()
                                }
                                _ => {
                                    view! {
                                        <p class="text-lg font-bold text-muted-foreground">"--"</p>
                                    }
                                    .into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            </Card>

            // Total Entries
            <Card>
                <div class="flex items-center gap-3">
                    <div class="h-10 w-10 rounded-full bg-blue-500/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-blue-500"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                            <polyline points="14 2 14 8 20 8"/>
                            <line x1="16" y1="13" x2="8" y2="13"/>
                            <line x1="16" y1="17" x2="8" y2="17"/>
                        </svg>
                    </div>
                    <div>
                        <p class="text-sm font-medium">"Total Events"</p>
                        {move || {
                            match chain.get() {
                                LoadingState::Loaded(c) => {
                                    view! { <p class="text-lg font-bold">{c.total_entries}</p> }
                                        .into_any()
                                }
                                _ => {
                                    view! {
                                        <p class="text-lg font-bold text-muted-foreground">"--"</p>
                                    }
                                    .into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            </Card>

            // Merkle Root
            <Card>
                <div class="flex items-center gap-3">
                    <div class="h-10 w-10 rounded-full bg-purple-500/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-purple-500"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
                            <path d="M2 17l10 5 10-5"/>
                            <path d="M2 12l10 5 10-5"/>
                        </svg>
                    </div>
                    <div>
                        <p class="text-sm font-medium">"Merkle Root"</p>
                        {move || {
                            match chain.get() {
                                LoadingState::Loaded(c) => {
                                    let root = c
                                        .merkle_root
                                        .clone()
                                        .unwrap_or_else(|| "N/A".to_string());
                                    let short = if root.len() > 12 {
                                        format!("{}...", &root[..12])
                                    } else {
                                        root
                                    };
                                    view! {
                                        <p
                                            class="text-sm font-mono text-muted-foreground"
                                            title=c.merkle_root.clone().unwrap_or_default()
                                        >
                                            {short}
                                        </p>
                                    }
                                    .into_any()
                                }
                                _ => {
                                    view! {
                                        <p class="text-sm font-mono text-muted-foreground">"--"</p>
                                    }
                                    .into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            </Card>

            // Compliance Rate
            <Card>
                <div class="flex items-center gap-3">
                    <div class="h-10 w-10 rounded-full bg-yellow-500/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-yellow-500"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                        </svg>
                    </div>
                    <div>
                        <p class="text-sm font-medium">"Compliance"</p>
                        {move || {
                            match compliance.get() {
                                LoadingState::Loaded(c) => {
                                    let rate = format!("{:.0}%", c.compliance_rate * 100.0);
                                    let class = if c.compliance_rate >= 0.95 {
                                        "text-green-500"
                                    } else if c.compliance_rate >= 0.8 {
                                        "text-yellow-500"
                                    } else {
                                        "text-red-500"
                                    };
                                    view! {
                                        <p class=format!("text-lg font-bold {}", class)>{rate}</p>
                                    }
                                    .into_any()
                                }
                                _ => {
                                    view! {
                                        <p class="text-lg font-bold text-muted-foreground">"--"</p>
                                    }
                                    .into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            </Card>
        </div>
    }
}

// ============================================================================
// Timeline Tab
// ============================================================================

#[component]
fn TimelineTab(logs: ReadSignal<LoadingState<AuditLogsResponse>>) -> impl IntoView {
    view! {
        <Card>
            {move || {
                match logs.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }
                        .into_any()
                    }
                    LoadingState::Loaded(data) => {
                        if data.logs.is_empty() {
                            view! {
                                <div class="text-center py-12">
                                    <p class="text-muted-foreground">"No audit events found"</p>
                                </div>
                            }
                            .into_any()
                        } else {
                            let log_count = data.logs.len();
                            let total = data.total;
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Timestamp"</TableHead>
                                            <TableHead>"Action"</TableHead>
                                            <TableHead>"Resource"</TableHead>
                                            <TableHead>"User"</TableHead>
                                            <TableHead>"Status"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {data
                                            .logs
                                            .into_iter()
                                            .map(|entry| {
                                                view! { <TimelineRow entry=entry/> }
                                            })
                                            .collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                                <div class="flex items-center justify-between mt-4 pt-4 border-t">
                                    <p class="text-sm text-muted-foreground">
                                        {format!("Showing {} of {} events", log_count, total)}
                                    </p>
                                </div>
                            }
                            .into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }
                        .into_any()
                    }
                }
            }}
        </Card>
    }
}

#[component]
fn TimelineRow(entry: AuditLogEntry) -> impl IntoView {
    let status_variant = match entry.status.as_str() {
        "success" => BadgeVariant::Success,
        "failure" => BadgeVariant::Destructive,
        "pending" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    };

    view! {
        <TableRow>
            <TableCell>
                <div>
                    <p class="text-sm font-mono">{entry.timestamp.clone()}</p>
                    <p class="text-xs text-muted-foreground font-mono">{entry.id.clone()}</p>
                </div>
            </TableCell>
            <TableCell>
                <Badge variant=BadgeVariant::Outline>{entry.action.clone()}</Badge>
            </TableCell>
            <TableCell>
                <div>
                    <p class="text-sm">{entry.resource_type.clone()}</p>
                    {entry
                        .resource_id
                        .clone()
                        .map(|id| {
                            view! { <p class="text-xs text-muted-foreground font-mono">{id}</p> }
                        })}
                </div>
            </TableCell>
            <TableCell>
                <div>
                    <p class="text-sm">{entry.user_id.clone()}</p>
                    <p class="text-xs text-muted-foreground">{entry.user_role.clone()}</p>
                </div>
            </TableCell>
            <TableCell>
                <Badge variant=status_variant>{entry.status.clone()}</Badge>
            </TableCell>
        </TableRow>
    }
}

// ============================================================================
// Hash Chain Tab
// ============================================================================

#[component]
fn HashChainTab(chain: ReadSignal<LoadingState<AuditChainResponse>>) -> impl IntoView {
    view! {
        <Card>
            {move || {
                match chain.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }
                        .into_any()
                    }
                    LoadingState::Loaded(data) => {
                        if data.entries.is_empty() {
                            view! {
                                <div class="text-center py-12">
                                    <p class="text-muted-foreground">"No chain entries found"</p>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="space-y-0">
                                    {data
                                        .entries
                                        .into_iter()
                                        .enumerate()
                                        .map(|(idx, entry)| {
                                            let is_last = idx == 0;
                                            view! { <ChainEntryRow entry=entry is_first=is_last/> }
                                        })
                                        .collect::<Vec<_>>()}
                                </div>
                            }
                            .into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }
                        .into_any()
                    }
                }
            }}
        </Card>
    }
}

#[component]
fn ChainEntryRow(entry: AuditChainEntry, is_first: bool) -> impl IntoView {
    let verification_class = if entry.verified {
        "text-green-500"
    } else {
        "text-red-500"
    };

    let hash_short = if entry.entry_hash.len() > 16 {
        format!("{}...", &entry.entry_hash[..16])
    } else {
        entry.entry_hash.clone()
    };

    let prev_hash_display = entry
        .previous_hash
        .clone()
        .map(|h| {
            if h.len() > 16 {
                format!("{}...", &h[..16])
            } else {
                h
            }
        })
        .unwrap_or_else(|| "GENESIS".to_string());

    let border_class = if entry.verified {
        "border-green-500 bg-green-500/10"
    } else {
        "border-red-500 bg-red-500/10"
    };

    let prev_hash_class = if entry.previous_hash.is_none() {
        "text-purple-500"
    } else {
        "text-foreground"
    };

    view! {
        <div class="relative">
            // Connector line
            {move || {
                if !is_first {
                    view! { <div class="absolute left-6 -top-4 w-0.5 h-4 bg-border"></div> }
                        .into_any()
                } else {
                    view! {}.into_any()
                }
            }}

            <div class="flex items-start gap-4 p-4 hover:bg-muted/50 rounded-lg transition-colors">
                // Chain link icon with verification status
                <div class=format!(
                    "flex-shrink-0 h-12 w-12 rounded-full border-2 flex items-center justify-center {}",
                    border_class,
                )>
                    <svg
                        class=format!("h-5 w-5 {}", verification_class)
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
                        <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
                    </svg>
                </div>

                // Entry details
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2 mb-1">
                        <span class="text-sm font-medium">
                            {format!("#{}", entry.chain_sequence)}
                        </span>
                        <Badge variant=BadgeVariant::Outline>{entry.action.clone()}</Badge>
                        <Badge variant=BadgeVariant::Secondary>{entry.resource_type.clone()}</Badge>
                        {move || {
                            if entry.verified {
                                view! { <Badge variant=BadgeVariant::Success>"Verified"</Badge> }
                                    .into_any()
                            } else {
                                view! { <Badge variant=BadgeVariant::Destructive>"Invalid"</Badge> }
                                    .into_any()
                            }
                        }}
                    </div>

                    <p class="text-xs text-muted-foreground mb-2">{entry.timestamp.clone()}</p>

                    // Hash visualization
                    <div class="grid grid-cols-2 gap-4 p-3 bg-muted/30 rounded-md font-mono text-xs">
                        <div>
                            <p class="text-muted-foreground mb-1">"Entry Hash"</p>
                            <p class="text-foreground" title=entry.entry_hash.clone()>
                                {hash_short}
                            </p>
                        </div>
                        <div>
                            <p class="text-muted-foreground mb-1">"Previous Hash"</p>
                            <p
                                class=prev_hash_class
                                title=entry.previous_hash.clone().unwrap_or_default()
                            >
                                {prev_hash_display}
                            </p>
                        </div>
                    </div>
                </div>

                // Arrow indicating chain direction
                <div class="flex-shrink-0 text-muted-foreground">
                    <svg
                        class="h-5 w-5"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <line x1="12" y1="5" x2="12" y2="19"/>
                        <polyline points="19 12 12 19 5 12"/>
                    </svg>
                </div>
            </div>
        </div>
    }
}

// ============================================================================
// Merkle Tree Tab
// ============================================================================

#[component]
fn MerkleTreeTab(
    chain: ReadSignal<LoadingState<AuditChainResponse>>,
    verification: ReadSignal<LoadingState<ChainVerificationResponse>>,
) -> impl IntoView {
    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Merkle Tree Visualization
            <Card title="Merkle Tree Structure".to_string()>
                {move || {
                    match chain.get() {
                        LoadingState::Loaded(data) => {
                            let merkle_root = data
                                .merkle_root
                                .clone()
                                .unwrap_or_else(|| "Not Available".to_string());
                            let entry_count = data.total_entries;
                            view! {
                                <div class="space-y-4">
                                    // Root visualization
                                    <div class="flex flex-col items-center">
                                        <div class="p-4 bg-purple-500/10 border-2 border-purple-500 rounded-lg text-center">
                                            <p class="text-xs text-muted-foreground mb-1">
                                                "Merkle Root"
                                            </p>
                                            <p class="font-mono text-sm break-all">{merkle_root}</p>
                                        </div>

                                        // Tree branches (simplified visualization)
                                        <div class="w-0.5 h-8 bg-border"></div>

                                        <div class="flex items-center gap-4">
                                            <div class="w-12 h-0.5 bg-border"></div>
                                            <div class="w-0.5 h-8 bg-border"></div>
                                            <div class="w-12 h-0.5 bg-border"></div>
                                        </div>

                                        <div class="flex items-center gap-8 mt-2">
                                            <div class="p-3 bg-blue-500/10 border border-blue-500 rounded text-center">
                                                <p class="text-xs text-muted-foreground">
                                                    "Left Subtree"
                                                </p>
                                                <p class="text-sm font-mono">
                                                    {format!("{} entries", entry_count / 2)}
                                                </p>
                                            </div>
                                            <div class="p-3 bg-blue-500/10 border border-blue-500 rounded text-center">
                                                <p class="text-xs text-muted-foreground">
                                                    "Right Subtree"
                                                </p>
                                                <p class="text-sm font-mono">
                                                    {format!("{} entries", entry_count - entry_count / 2)}
                                                </p>
                                            </div>
                                        </div>
                                    </div>

                                    // Legend
                                    <div class="flex items-center gap-4 justify-center pt-4 border-t">
                                        <div class="flex items-center gap-2">
                                            <div class="w-3 h-3 bg-purple-500 rounded"></div>
                                            <span class="text-xs text-muted-foreground">"Root"</span>
                                        </div>
                                        <div class="flex items-center gap-2">
                                            <div class="w-3 h-3 bg-blue-500 rounded"></div>
                                            <span class="text-xs text-muted-foreground">
                                                "Internal"
                                            </span>
                                        </div>
                                        <div class="flex items-center gap-2">
                                            <div class="w-3 h-3 bg-green-500 rounded"></div>
                                            <span class="text-xs text-muted-foreground">"Leaf"</span>
                                        </div>
                                    </div>
                                </div>
                            }
                            .into_any()
                        }
                        LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }
                            .into_any()
                        }
                        _ => {
                            view! {
                                <div class="text-center py-12 text-muted-foreground">
                                    "No data available"
                                </div>
                            }
                            .into_any()
                        }
                    }
                }}
            </Card>

            // Verification Details
            <Card title="Verification Status".to_string()>
                {move || {
                    match verification.get() {
                        LoadingState::Loaded(v) => {
                            let status_class = if v.chain_valid {
                                "bg-green-500/10 border border-green-500"
                            } else {
                                "bg-red-500/10 border border-red-500"
                            };
                            let text_class = if v.chain_valid {
                                "text-green-500"
                            } else {
                                "text-red-500"
                            };
                            view! {
                                <div class="space-y-4">
                                    // Status indicator
                                    <div class=format!("p-4 rounded-lg {}", status_class)>
                                        <div class="flex items-center gap-3">
                                            {if v.chain_valid {
                                                view! {
                                                    <svg
                                                        class="h-8 w-8 text-green-500"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                    >
                                                        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
                                                        <polyline points="22 4 12 14.01 9 11.01"/>
                                                    </svg>
                                                }
                                                .into_any()
                                            } else {
                                                view! {
                                                    <svg
                                                        class="h-8 w-8 text-red-500"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                    >
                                                        <circle cx="12" cy="12" r="10"/>
                                                        <line x1="15" y1="9" x2="9" y2="15"/>
                                                        <line x1="9" y1="9" x2="15" y2="15"/>
                                                    </svg>
                                                }
                                                .into_any()
                                            }}
                                            <div>
                                                <p class=format!("text-lg font-bold {}", text_class)>
                                                    {if v.chain_valid {
                                                        "Chain Verified"
                                                    } else {
                                                        "Verification Failed"
                                                    }}
                                                </p>
                                                <p class="text-sm text-muted-foreground">
                                                    {format!(
                                                        "{} of {} entries verified",
                                                        v.verified_entries,
                                                        v.total_entries,
                                                    )}
                                                </p>
                                            </div>
                                        </div>
                                    </div>

                                    // Details
                                    <div class="space-y-2">
                                        <div class="flex justify-between py-2 border-b">
                                            <span class="text-muted-foreground">"Total Entries"</span>
                                            <span class="font-medium">{v.total_entries}</span>
                                        </div>
                                        <div class="flex justify-between py-2 border-b">
                                            <span class="text-muted-foreground">
                                                "Verified Entries"
                                            </span>
                                            <span class="font-medium">{v.verified_entries}</span>
                                        </div>
                                        {v
                                            .first_invalid_sequence
                                            .map(|seq| {
                                                view! {
                                                    <div class="flex justify-between py-2 border-b">
                                                        <span class="text-muted-foreground">
                                                            "First Invalid"
                                                        </span>
                                                        <span class="font-medium text-red-500">
                                                            {format!("#{}", seq)}
                                                        </span>
                                                    </div>
                                                }
                                            })}
                                        <div class="flex justify-between py-2 border-b">
                                            <span class="text-muted-foreground">"Verified At"</span>
                                            <span class="font-mono text-sm">
                                                {v.verification_timestamp.clone()}
                                            </span>
                                        </div>
                                        {v
                                            .merkle_root
                                            .clone()
                                            .map(|root| {
                                                view! {
                                                    <div class="py-2">
                                                        <span class="text-muted-foreground block mb-1">
                                                            "Merkle Root"
                                                        </span>
                                                        <span class="font-mono text-xs break-all">
                                                            {root}
                                                        </span>
                                                    </div>
                                                }
                                            })}
                                    </div>

                                    {v
                                        .error_message
                                        .clone()
                                        .map(|err| {
                                            view! {
                                                <div class="p-3 bg-red-500/10 border border-red-500 rounded-md">
                                                    <p class="text-sm text-red-500">{err}</p>
                                                </div>
                                            }
                                        })}
                                </div>
                            }
                            .into_any()
                        }
                        LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }
                            .into_any()
                        }
                        _ => {
                            view! {
                                <div class="text-center py-12 text-muted-foreground">
                                    "No verification data"
                                </div>
                            }
                            .into_any()
                        }
                    }
                }}
            </Card>
        </div>
    }
}

// ============================================================================
// Compliance Tab
// ============================================================================

#[component]
fn ComplianceTab(compliance: ReadSignal<LoadingState<ComplianceAuditResponse>>) -> impl IntoView {
    view! {
        <Card>
            {move || {
                match compliance.get() {
                    LoadingState::Loaded(data) => {
                        view! {
                            <div class="space-y-6">
                                // Summary stats
                                <div class="grid gap-4 md:grid-cols-4">
                                    <div class="p-4 bg-muted/30 rounded-lg">
                                        <p class="text-sm text-muted-foreground">"Compliance Rate"</p>
                                        <p class="text-2xl font-bold">
                                            {format!("{:.1}%", data.compliance_rate * 100.0)}
                                        </p>
                                    </div>
                                    <div class="p-4 bg-muted/30 rounded-lg">
                                        <p class="text-sm text-muted-foreground">"Total Controls"</p>
                                        <p class="text-2xl font-bold">{data.total_controls}</p>
                                    </div>
                                    <div class="p-4 bg-muted/30 rounded-lg">
                                        <p class="text-sm text-muted-foreground">"Compliant"</p>
                                        <p class="text-2xl font-bold text-green-500">
                                            {data.compliant_controls}
                                        </p>
                                    </div>
                                    <div class="p-4 bg-muted/30 rounded-lg">
                                        <p class="text-sm text-muted-foreground">"Violations"</p>
                                        <p class="text-2xl font-bold text-red-500">
                                            {data.active_violations}
                                        </p>
                                    </div>
                                </div>

                                // Controls list
                                <div>
                                    <h3 class="text-lg font-semibold mb-4">"Compliance Controls"</h3>
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>"Control ID"</TableHead>
                                                <TableHead>"Name"</TableHead>
                                                <TableHead>"Status"</TableHead>
                                                <TableHead>"Last Checked"</TableHead>
                                                <TableHead>"Findings"</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {data
                                                .controls
                                                .into_iter()
                                                .map(|control| {
                                                    let status_variant = match control.status.as_str() {
                                                        "compliant" => BadgeVariant::Success,
                                                        "non_compliant" => BadgeVariant::Destructive,
                                                        "pending" => BadgeVariant::Warning,
                                                        _ => BadgeVariant::Secondary,
                                                    };
                                                    view! {
                                                        <TableRow>
                                                            <TableCell>
                                                                <span class="font-mono text-sm">
                                                                    {control.control_id.clone()}
                                                                </span>
                                                            </TableCell>
                                                            <TableCell>{control.control_name.clone()}</TableCell>
                                                            <TableCell>
                                                                <Badge variant=status_variant>
                                                                    {control.status.clone()}
                                                                </Badge>
                                                            </TableCell>
                                                            <TableCell>
                                                                <span class="text-sm text-muted-foreground">
                                                                    {control.last_checked.clone()}
                                                                </span>
                                                            </TableCell>
                                                            <TableCell>
                                                                <span class="text-sm">
                                                                    {control.findings.len()}{" findings"}
                                                                </span>
                                                            </TableCell>
                                                        </TableRow>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </TableBody>
                                    </Table>
                                </div>

                                <p class="text-xs text-muted-foreground">
                                    {format!("Last updated: {}", data.timestamp)}
                                </p>
                            </div>
                        }
                        .into_any()
                    }
                    LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }
                        .into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }
                        .into_any()
                    }
                    _ => {
                        view! {
                            <div class="text-center py-12 text-muted-foreground">
                                "No compliance data available"
                            </div>
                        }
                        .into_any()
                    }
                }
            }}
        </Card>
    }
}
