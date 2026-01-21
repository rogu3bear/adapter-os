//! Organization section component

use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card};
use crate::signals::use_auth;
use leptos::prelude::*;

/// Organization section - placeholder
#[component]
pub fn OrgSection() -> impl IntoView {
    let (auth_state, _) = use_auth();

    view! {
        <div class="max-w-2xl">
            <Card title="Organization Settings".to_string() description="Configure your organization's adapterOS instance.".to_string()>
                <div class="space-y-4">
                    {move || {
                        let state = auth_state.get();
                        if let Some(user) = state.user() {
                            let tenant_id = user.tenant_id.clone();
                            let email = user.email.clone();
                            // Build tenant badges before the view! macro to avoid lifetime issues
                            let tenant_badges: Vec<_> = user.admin_tenants.iter().map(|t| {
                                let tenant = t.clone();
                                view! {
                                    <Badge variant=BadgeVariant::Outline>{tenant}</Badge>
                                }
                            }).collect();
                            view! {
                                <div class="grid gap-3 text-sm">
                                    <div class="flex justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Tenant ID"</span>
                                        <span class="font-mono text-xs">{tenant_id}</span>
                                    </div>
                                    <div class="flex justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Admin Email"</span>
                                        <span>{email}</span>
                                    </div>
                                    <div class="flex justify-between py-2">
                                        <span class="text-muted-foreground">"Admin Tenants"</span>
                                        <div class="flex gap-1">
                                            {tenant_badges}
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p class="text-muted-foreground">"Loading..."</p>
                            }.into_any()
                        }
                    }}
                </div>
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
                        <Button variant=ButtonVariant::Destructive disabled=true>
                            "Revoke All"
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
    }
}
