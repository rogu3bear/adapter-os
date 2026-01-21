//! Roles section component

use crate::components::{Badge, BadgeVariant, Card};
use leptos::prelude::*;

/// Roles section
#[component]
pub fn RolesSection() -> impl IntoView {
    // Define the roles with their descriptions
    let roles = vec![
        (
            "Admin",
            "Full access to all features including user management, policies, and system settings",
            vec![
                "Manage users and roles",
                "Configure policies",
                "Access audit logs",
                "Manage federation",
            ],
        ),
        (
            "Operator",
            "Can run inference, training, and manage adapters. Cannot modify system settings",
            vec![
                "Create/cancel training jobs",
                "Load/unload models",
                "Create adapter stacks",
                "View system metrics",
            ],
        ),
        (
            "Viewer",
            "Read-only access to dashboards and status. Cannot modify any resources",
            vec![
                "View dashboard",
                "View system status",
                "Run approved inferences",
                "View training jobs",
            ],
        ),
    ];

    view! {
        <div class="grid gap-4">
            {roles.into_iter().map(|(name, desc, perms)| {
                let variant = match name {
                    "Admin" => BadgeVariant::Destructive,
                    "Operator" => BadgeVariant::Default,
                    _ => BadgeVariant::Secondary,
                };

                view! {
                    <Card>
                        <div class="flex items-start justify-between">
                            <div class="flex-1">
                                <div class="flex items-center gap-2 mb-2">
                                    <Badge variant=variant>{name}</Badge>
                                </div>
                                <p class="text-sm text-muted-foreground mb-4">{desc}</p>
                                <div class="space-y-1">
                                    {perms.into_iter().map(|perm| view! {
                                        <div class="flex items-center gap-2 text-sm">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-4 w-4 text-status-success"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                            >
                                                <polyline points="20 6 9 17 4 12"/>
                                            </svg>
                                            <span>{perm}</span>
                                        </div>
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        </div>
                    </Card>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
