//! Organization section component

use crate::api::{use_api_client, ApiClient, ApiError, TenantSummary};
use crate::components::{Card, EmptyState, EmptyStateVariant, ErrorDisplay, SkeletonCard};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_auth, use_refetch_signal, RefetchTopic};
use crate::utils::{format_datetime, humanize};
use leptos::prelude::*;
use std::sync::Arc;

/// Organization section - tenant metadata display
#[component]
pub fn OrgSection() -> impl IntoView {
    let (auth_state, _) = use_auth();
    let _client = use_api_client();
    let (tenants, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_user_tenants().await });

    // Periodic polling for data freshness (30s)
    let refetch_poll = refetch;
    let _cancel_poll = use_polling(30_000, move || {
        refetch_poll.run(());
        async {}
    });

    // SSE-driven refetch for user-related changes
    let users_counter = use_refetch_signal(RefetchTopic::Users);
    Effect::new(move || {
        let _ = users_counter.get();
        refetch.run(());
    });

    view! {
        <div class="max-w-2xl">
            <Card title="Organization Settings".to_string() description="Configure your organization's adapterOS instance.".to_string()>
                {move || match tenants.get() {
                    LoadingState::Idle | LoadingState::Loading => view! {
                        <SkeletonCard has_header=true/>
                    }.into_any(),
                    LoadingState::Loaded(data) => {
                        let user = auth_state.get().user().map(|u| {
                            (Some(u.tenant_id.clone()), Some(u.email.clone()), Some(u.display_name.clone()))
                        });
                        let (tenant_id, contact_email, contact_name) = user.unwrap_or((None, None, None));
                        let tenant_list = data.tenants;

                        if tenant_list.is_empty() {
                            view! {
                                <EmptyState
                                    title="No organization data"
                                    description="Tenant metadata is not available for this account."
                                    variant=EmptyStateVariant::Empty
                                    action_label="Retry"
                                    on_action=refetch.as_callback()
                                />
                            }.into_any()
                        } else {
                            let tenant = select_tenant(&tenant_list, tenant_id.as_deref());
                            let org_name = format_value(tenant.as_ref().map(|t| t.name.clone()));
                            let tenant_id = format_value(
                                tenant.as_ref().map(|t| t.id.clone()).or(tenant_id),
                            );
                            let created_at = tenant.as_ref().and_then(|t| t.created_at.as_deref().map(format_datetime)).unwrap_or_else(|| "Not available".to_string());
                            let status = format_value(tenant.as_ref().and_then(|t| t.status.clone()));
                            let contact_name = format_value(contact_name);
                            let contact_email = format_value(contact_email);

                            view! {
                                <div class="space-y-3">
                                    <p class="text-xs text-muted-foreground">
                                        "Tenants are workspace contexts. You can see multiple tenants under one user account."
                                    </p>
                                    <div class="grid gap-3 text-sm">
                                    <div class="flex items-center justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Organization Name"</span>
                                        <span class="font-medium">{org_name}</span>
                                    </div>
                                    <div class="flex items-center justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Tenant ID"</span>
                                        <span class="font-mono text-xs">{tenant_id}</span>
                                    </div>
                                    <div class="flex items-center justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Status"</span>
                                        <span class="text-muted-foreground">{humanize(&status)}</span>
                                    </div>
                                    <div class="flex items-center justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Created"</span>
                                        <span>{created_at}</span>
                                    </div>
                                    <div class="flex items-center justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Contact Name"</span>
                                        <span>{contact_name}</span>
                                    </div>
                                    <div class="flex items-center justify-between py-2">
                                        <span class="text-muted-foreground">"Contact Email"</span>
                                        <span>{contact_email}</span>
                                    </div>
                                </div>
                                </div>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(err) => match err.as_ref() {
                        ApiError::NotFound(_) => view! {
                            <EmptyState
                                title="Organization info unavailable"
                                description="This server does not expose tenant metadata yet."
                                variant=EmptyStateVariant::Unavailable
                                action_label="Retry"
                                on_action=refetch.as_callback()
                            />
                        }.into_any(),
                          other => view! {
                            <ErrorDisplay error=other.clone() on_retry=refetch.as_callback()/>
                        }.into_any(),
                    },
                }}
            </Card>
        </div>
    }
}

fn select_tenant(tenants: &[TenantSummary], tenant_id: Option<&str>) -> Option<TenantSummary> {
    tenant_id
        .and_then(|id| tenants.iter().find(|t| t.id == id))
        .cloned()
        .or_else(|| tenants.first().cloned())
}

fn format_value(value: Option<String>) -> String {
    value
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| "Not available".to_string())
}
