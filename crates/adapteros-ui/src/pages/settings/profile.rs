//! Profile section component

use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, DangerZone, DangerZoneItem,
    SimpleConfirmDialog,
};
use crate::signals::use_auth;
use leptos::prelude::*;

/// Profile section
#[component]
pub fn ProfileSection() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();

    // Logout handler
    let logout_loading = RwSignal::new(false);
    let show_logout_confirm = RwSignal::new(false);

    let handle_logout = Callback::new(move |_| {
        logout_loading.set(true);
        let action = auth_action.clone();
        wasm_bindgen_futures::spawn_local(async move {
            action.logout().await;
            // Redirect will happen via auth context.
            // Reset loading in case logout fails or navigation doesn't occur.
            let _ = logout_loading.try_set(false);
        });
    });

    view! {
        <div class="space-y-6 max-w-2xl">
            <Card title="User Profile".to_string() description="Your account information and session details.".to_string()>
                {move || {
                    if let Some(user) = auth_state.get().user() {
                        let user = user.clone();
                        view! {
                            <div class="space-y-4">
                                // Display Name
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Display Name"</span>
                                    <span class="col-span-2 text-sm">{user.display_name.clone()}</span>
                                </div>

                                // Email
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Email"</span>
                                    <span class="col-span-2 text-sm">{user.email.clone()}</span>
                                </div>

                                // User ID
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"User ID"</span>
                                    <span class="col-span-2 text-sm font-mono text-xs">{user.user_id.clone()}</span>
                                </div>

                                // Role
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Role"</span>
                                    <div class="col-span-2">
                                        <Badge variant=role_to_variant(&user.role)>
                                            {user.role.clone()}
                                        </Badge>
                                    </div>
                                </div>

                                // Tenant
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Tenant ID"</span>
                                    <span class="col-span-2 text-sm font-mono text-xs">{user.tenant_id.clone()}</span>
                                </div>

                                // Permissions
                                <div class="grid grid-cols-3 gap-4 items-start">
                                    <span class="text-sm font-medium text-muted-foreground">"Permissions"</span>
                                    <div class="col-span-2 flex flex-wrap gap-1">
                                        {if user.permissions.is_empty() {
                                            view! {
                                                <span class="text-sm text-muted-foreground">"No explicit permissions"</span>
                                            }.into_any()
                                        } else {
                                            let permissions = user.permissions.clone();
                                            view! {
                                                {permissions.into_iter().map(|p| {
                                                    view! {
                                                        <Badge variant=BadgeVariant::Outline>
                                                            {p}
                                                        </Badge>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            }.into_any()
                                        }}
                                    </div>
                                </div>

                                // MFA Status
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"MFA Status"</span>
                                    <div class="col-span-2">
                                        {if user.mfa_enabled.unwrap_or(false) {
                                            view! {
                                                <Badge variant=BadgeVariant::Success>"Enabled"</Badge>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <Badge variant=BadgeVariant::Secondary>"Disabled"</Badge>
                                            }.into_any()
                                        }}
                                    </div>
                                </div>

                                // Last Login
                                {user.last_login_at.clone().map(|last| view! {
                                    <div class="grid grid-cols-3 gap-4 items-center">
                                        <span class="text-sm font-medium text-muted-foreground">"Last Login"</span>
                                        <span class="col-span-2 text-sm">{last}</span>
                                    </div>
                                })}

                                // Member Since
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Member Since"</span>
                                    <span class="col-span-2 text-sm">{user.created_at.clone()}</span>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <p class="text-muted-foreground">"Loading user information..."</p>
                        }.into_any()
                    }
                }}
            </Card>

            // Session Actions (Danger Zone)
            <DangerZone>
                <DangerZoneItem
                    title="Sign Out"
                    description="End your current session. You will need to log in again."
                >
                    <Button
                        variant=ButtonVariant::Destructive
                        loading=Signal::from(logout_loading)
                        on_click=Callback::new(move |_| show_logout_confirm.set(true))
                    >
                        "Logout"
                    </Button>
                </DangerZoneItem>
            </DangerZone>

            // Logout confirmation dialog
            <SimpleConfirmDialog
                open=show_logout_confirm
                title="Sign Out"
                description="Are you sure you want to sign out? You will need to log in again."
                on_confirm=handle_logout
            />
        </div>
    }
}

/// Convert role string to badge variant
fn role_to_variant(role: &str) -> BadgeVariant {
    match role.to_lowercase().as_str() {
        "admin" => BadgeVariant::Destructive,
        "developer" | "sre" => BadgeVariant::Default,
        "operator" | "compliance" => BadgeVariant::Warning,
        "auditor" | "viewer" => BadgeVariant::Secondary,
        _ => BadgeVariant::Outline,
    }
}
