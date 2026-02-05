//! Audit page shared components
//!
//! Filter and summary components used across tabs.

use crate::api::{AuditChainResponse, ChainVerificationResponse, ComplianceAuditResponse};
use crate::components::{Card, Skeleton, Spinner};
use crate::hooks::LoadingState;
use leptos::prelude::*;

use super::AuditTab;

// ============================================================================
// Filter Section
// ============================================================================

#[component]
pub fn FilterSection(
    active_tab: RwSignal<AuditTab>,
    action_filter: RwSignal<String>,
    status_filter: RwSignal<String>,
    resource_filter: RwSignal<String>,
    on_filters_changed: Callback<()>,
) -> impl IntoView {
    let show_filters = move || matches!(active_tab.get(), AuditTab::Timeline);

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
                            prop:value=move || action_filter.get()
                            on:change=move |ev| {
                                action_filter.set(event_target_value(&ev));
                                on_filters_changed.run(());
                            }
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
                            prop:value=move || status_filter.get()
                            on:change=move |ev| {
                                status_filter.set(event_target_value(&ev));
                                on_filters_changed.run(());
                            }
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
                            prop:value=move || resource_filter.get()
                            on:change=move |ev| {
                                resource_filter.set(event_target_value(&ev));
                                on_filters_changed.run(());
                            }
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
pub fn ChainStatusSummary(
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
                                        <div class="h-10 w-10 rounded-full bg-status-success/10 flex items-center justify-center">
                                            <svg
                                                class="h-5 w-5 text-status-success"
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
                                        <div class="h-10 w-10 rounded-full bg-status-error/10 flex items-center justify-center">
                                            <svg
                                                class="h-5 w-5 text-status-error"
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
                                        "text-status-success"
                                    } else {
                                        "text-status-error"
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
                                        <Skeleton class="h-6 w-20"/>
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
                    <div class="h-10 w-10 rounded-full bg-status-info/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-status-info"
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
                                        <Skeleton class="h-6 w-12"/>
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
                    <div class="h-10 w-10 rounded-full bg-primary/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-primary"
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
                                        <Skeleton class="h-4 w-24"/>
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
                    <div class="h-10 w-10 rounded-full bg-status-warning/10 flex items-center justify-center">
                        <svg
                            class="h-5 w-5 text-status-warning"
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
                                    // Handle both formats: decimal (0-1) and percentage (0-100)
                                    // Normalize to decimal for color comparison
                                    let rate_decimal = if c.compliance_rate > 1.0 {
                                        c.compliance_rate / 100.0
                                    } else {
                                        c.compliance_rate
                                    };
                                    let rate_percent = (rate_decimal * 100.0).min(100.0);
                                    let rate = format!("{:.0}%", rate_percent);
                                    let class = if rate_decimal >= 0.95 {
                                        "text-status-success"
                                    } else if rate_decimal >= 0.8 {
                                        "text-status-warning"
                                    } else {
                                        "text-status-error"
                                    };
                                    view! {
                                        <p class=format!("text-lg font-bold {}", class)>{rate}</p>
                                    }
                                    .into_any()
                                }
                                _ => {
                                    view! {
                                        <Skeleton class="h-6 w-14"/>
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
