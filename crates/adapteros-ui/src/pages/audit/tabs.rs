//! Audit page tab components
//!
//! Individual tab views for timeline, hash chain, merkle tree, and compliance.

use crate::api::{
    AuditChainEntry, AuditChainResponse, AuditLogEntry, AuditLogsResponse,
    ChainVerificationResponse, ComplianceAuditResponse,
};
use crate::components::{
    Badge, BadgeVariant, Card, ErrorDisplay, Spinner, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::hooks::LoadingState;
use leptos::prelude::*;

/// Page size for client-side pagination (reduces initial DOM nodes)
const AUDIT_PAGE_SIZE: usize = 25;

// ============================================================================
// Timeline Tab
// ============================================================================

#[component]
pub fn TimelineTab(logs: ReadSignal<LoadingState<AuditLogsResponse>>) -> impl IntoView {
    // Client-side pagination to reduce DOM nodes
    let visible_count = RwSignal::new(AUDIT_PAGE_SIZE);

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
                            let logs_data = data.logs;

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
                                        {move || {
                                            let count = visible_count.get().min(log_count);
                                            logs_data
                                                .iter()
                                                .take(count)
                                                .map(|entry| {
                                                    view! { <TimelineRow entry=entry.clone()/> }
                                                })
                                                .collect::<Vec<_>>()
                                        }}
                                    </TableBody>
                                </Table>

                                // Show more button if there are hidden items
                                {move || {
                                    let count = visible_count.get();
                                    let remaining = log_count.saturating_sub(count);
                                    if remaining > 0 {
                                        view! {
                                            <div class="flex items-center justify-center py-4 border-t">
                                                <button
                                                    class="text-sm text-primary hover:underline"
                                                    on:click=move |_| {
                                                        visible_count.update(|c| *c = (*c + AUDIT_PAGE_SIZE).min(log_count));
                                                    }
                                                >
                                                    {format!("Show more ({} remaining)", remaining)}
                                                </button>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }
                                }}

                                <div class="flex items-center justify-between mt-4 pt-4 border-t">
                                    <p class="text-sm text-muted-foreground">
                                        {format!("Showing {} of {} events", visible_count.get().min(log_count), total)}
                                    </p>
                                </div>
                            }
                            .into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay error=e/>
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
pub fn HashChainTab(chain: ReadSignal<LoadingState<AuditChainResponse>>) -> impl IntoView {
    // Client-side pagination to reduce DOM nodes (same pattern as TimelineTab)
    let visible_count = RwSignal::new(AUDIT_PAGE_SIZE);

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
                            let entry_count = data.entries.len();
                            let entries_data = data.entries;

                            view! {
                                <div class="space-y-0">
                                    {move || {
                                        let count = visible_count.get().min(entry_count);
                                        entries_data
                                            .iter()
                                            .take(count)
                                            .enumerate()
                                            .map(|(idx, entry)| {
                                                let is_first = idx == 0;
                                                view! { <ChainEntryRow entry=entry.clone() is_first=is_first/> }
                                            })
                                            .collect::<Vec<_>>()
                                    }}
                                </div>

                                // Show more button if there are hidden items
                                {move || {
                                    let count = visible_count.get();
                                    let remaining = entry_count.saturating_sub(count);
                                    if remaining > 0 {
                                        view! {
                                            <div class="flex items-center justify-center py-4 border-t">
                                                <button
                                                    class="text-sm text-primary hover:underline"
                                                    on:click=move |_| {
                                                        visible_count.update(|c| *c = (*c + AUDIT_PAGE_SIZE).min(entry_count));
                                                    }
                                                >
                                                    {format!("Show more ({} remaining)", remaining)}
                                                </button>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }
                                }}

                                <div class="flex items-center justify-between mt-4 pt-4 border-t">
                                    <p class="text-sm text-muted-foreground">
                                        {format!("Showing {} of {} entries", visible_count.get().min(entry_count), entry_count)}
                                    </p>
                                </div>
                            }
                            .into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay error=e/>
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
pub fn MerkleTreeTab(
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
pub fn ComplianceTab(
    compliance: ReadSignal<LoadingState<ComplianceAuditResponse>>,
) -> impl IntoView {
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
                            <ErrorDisplay error=e/>
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
