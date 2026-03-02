//! Audit page shared components
//!
//! Filter and summary components used across the audit page.

use crate::api::ChainVerificationResponse;
use crate::components::{Badge, BadgeVariant, Card, Skeleton};
use crate::hooks::LoadingState;
use leptos::prelude::*;

// ============================================================================
// Filter Section
// ============================================================================

#[component]
pub fn FilterSection(
    action_filter: RwSignal<String>,
    status_filter: RwSignal<String>,
    resource_filter: RwSignal<String>,
    on_filters_changed: Callback<()>,
) -> impl IntoView {
    view! {
        <Card>
            <div class="flex items-end gap-4">
                <div class="flex-1">
                    <label for="audit-action-filter" class="text-sm font-medium mb-2 block">
                        "Action Type"
                    </label>
                    <select
                        id="audit-action-filter"
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
                    <label for="audit-status-filter" class="text-sm font-medium mb-2 block">
                        "Status"
                    </label>
                    <select
                        id="audit-status-filter"
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
                    <label for="audit-resource-filter" class="text-sm font-medium mb-2 block">
                        "Resource Type"
                    </label>
                    <select
                        id="audit-resource-filter"
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
    }
}

// ============================================================================
// Chain Status Summary (minimal)
// ============================================================================

#[component]
pub fn ChainStatusSummary(
    verification: ReadSignal<LoadingState<ChainVerificationResponse>>,
) -> impl IntoView {
    view! {
        <div class="grid gap-4 md:grid-cols-3">
            // Total Events
            <Card>
                <div>
                    <p class="text-sm font-medium">"Total Events"</p>
                    {move || {
                        match verification.get() {
                            LoadingState::Loaded(v) => {
                                view! { <p class="text-lg font-bold">{v.total_entries}</p> }.into_any()
                            }
                            _ => view! { <Skeleton class="h-6 w-12"/> }.into_any(),
                        }
                    }}
                </div>
            </Card>

            // Last Verified
            <Card>
                <div>
                    <p class="text-sm font-medium">"Last Verified"</p>
                    {move || {
                        match verification.get() {
                            LoadingState::Loaded(v) => {
                                view! {
                                    <p class="text-sm font-mono text-muted-foreground">{v.verification_timestamp.clone()}</p>
                                }.into_any()
                            }
                            _ => view! { <Skeleton class="h-4 w-24"/> }.into_any(),
                        }
                    }}
                </div>
            </Card>

            // Chain Status
            <Card>
                <div>
                    <p class="text-sm font-medium">"Chain Status"</p>
                    {move || {
                        match verification.get() {
                            LoadingState::Loaded(v) => {
                                if v.chain_valid {
                                    view! { <Badge variant=BadgeVariant::Success>"Valid"</Badge> }.into_any()
                                } else {
                                    view! { <Badge variant=BadgeVariant::Destructive>"Invalid"</Badge> }.into_any()
                                }
                            }
                            _ => view! { <Skeleton class="h-6 w-16"/> }.into_any(),
                        }
                    }}
                </div>
            </Card>
        </div>
    }
}
