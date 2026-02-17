//! Organization section component

use crate::api::{report_error_with_toast, ApiClient, ApiError, TenantSummary};
use crate::components::{
    Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity, EmptyState,
    EmptyStateVariant, ErrorDisplay, SkeletonCard,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_auth, use_notifications, use_refetch_signal, RefetchTopic};
use crate::utils::{format_datetime, humanize};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

/// Organization section - tenant metadata
#[component]
pub fn OrgSection() -> impl IntoView {
    let (auth_state, _) = use_auth();
    let notifications = use_notifications();
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

    // Revoke all sessions state
    let show_revoke_confirm = RwSignal::new(false);
    let revoking = RwSignal::new(false);

    // Derive the org name from tenant data for the confirmation dialog
    let org_name_for_confirm = Memo::new(move |_| {
        if let LoadingState::Loaded(data) = tenants.get() {
            let tenant_id = auth_state.get().user().map(|u| u.tenant_id.clone());
            select_tenant(&data.tenants, tenant_id.as_deref())
                .map(|t| t.name)
                .unwrap_or_default()
        } else {
            String::new()
        }
    });

    let tenant_id_for_revoke =
        Memo::new(move |_| auth_state.get().user().map(|u| u.tenant_id.clone()));

    let do_revoke = {
        let notifications = notifications.clone();
        Callback::new(move |_| {
            let Some(tid) = tenant_id_for_revoke.get_untracked() else {
                return;
            };
            revoking.set(true);
            show_revoke_confirm.set(false);
            let notifications = notifications.clone();
            spawn_local(async move {
                let client = ApiClient::new();
                let url = format!("/v1/tenants/{}/revoke-all-tokens", tid);
                match client.post_empty::<serde_json::Value>(&url).await {
                    Ok(_) => {
                        notifications.success(
                            "Sessions revoked",
                            "All user sessions have been invalidated.",
                        );
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to revoke sessions",
                            Some("/admin?tab=org"),
                            true,
                        );
                    }
                }
                revoking.set(false);
            });
        })
    };

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
                            }.into_any()
                        }
                    }
                    LoadingState::Error(err) => match err {
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
                            <ErrorDisplay error=other on_retry=refetch.as_callback()/>
                        }.into_any(),
                    },
                }}
            </Card>

            <Card title="Danger Zone".to_string() class="mt-6 border-destructive".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="font-medium">"Revoke All Sessions"</p>
                            <p class="text-sm text-muted-foreground">
                                "Force all users to re-authenticate. Use with caution."
                            </p>
                        </div>
                        <Button
                            variant=ButtonVariant::Destructive
                            on_click=Callback::new(move |_| show_revoke_confirm.set(true))
                            loading=Signal::from(revoking)
                            disabled=Signal::from(revoking)
                        >
                            "Revoke All"
                        </Button>
                    </div>
                </div>
            </Card>

            {move || {
                let name = org_name_for_confirm.get();
                let display_name = if name.is_empty() { "this organization".to_string() } else { name.clone() };
                let description = format!(
                    "This will force all users in '{}' to re-authenticate. Active sessions will be terminated immediately.",
                    display_name,
                );
                view! {
                    <ConfirmationDialog
                        open=show_revoke_confirm
                        title="Revoke All Sessions"
                        description=description
                        severity=ConfirmationSeverity::Destructive
                        confirm_text="Revoke All Sessions"
                        typed_confirmation=display_name
                        on_confirm=do_revoke
                        on_cancel=Callback::new(move |_| show_revoke_confirm.set(false))
                        loading=Signal::derive(move || revoking.get())
                    />
                }
            }}
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
