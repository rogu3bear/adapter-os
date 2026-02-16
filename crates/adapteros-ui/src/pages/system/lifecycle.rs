//! Admin lifecycle panel
//!
//! Controls for shutdown, maintenance, and safe-restart operations.
//! Requires Admin role.

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity, Dialog, DialogSize,
    FormField, ImpactItem, Input, Select,
};
use leptos::prelude::*;
use std::sync::Arc;

/// Admin lifecycle control panel
#[component]
pub fn AdminLifecyclePanel() -> impl IntoView {
    let client = expect_context::<Arc<ApiClient>>();

    // Feedback
    let action_error = RwSignal::new(Option::<String>::None);
    let action_success = RwSignal::new(Option::<String>::None);

    // ── Shutdown ────────────────────────────────────────────────────────
    let shutdown_open = RwSignal::new(false);
    let shutdown_loading = RwSignal::new(false);
    let shutdown_reason = RwSignal::new(String::new());
    let shutdown_mode = RwSignal::new("drain".to_string());

    let shutdown_dialog_open = RwSignal::new(false);

    let execute_shutdown = {
        let client = client.clone();
        Callback::new(move |_: ()| {
            let client = client.clone();
            let reason = shutdown_reason.get_untracked();
            let mode = shutdown_mode.get_untracked();
            shutdown_loading.set(true);
            action_error.set(None);
            action_success.set(None);
            shutdown_dialog_open.set(false);
            shutdown_open.set(false);

            leptos::task::spawn_local(async move {
                match client.request_shutdown(&reason, &mode).await {
                    Ok(resp) => {
                        let tracking = resp
                            .get("tracking_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let lifecycle = resp
                            .get("lifecycle")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        action_success.set(Some(format!(
                            "Shutdown accepted ({}). State: {}. Tracking: {}",
                            mode, lifecycle, tracking
                        )));
                    }
                    Err(e) => {
                        action_error.set(Some(format!("Shutdown failed: {}", e)));
                    }
                }
                shutdown_loading.set(false);
                shutdown_reason.set(String::new());
            });
        })
    };

    // ── Maintenance ────────────────────────────────────────────────────
    let maintenance_open = RwSignal::new(false);
    let maintenance_loading = RwSignal::new(false);
    let maintenance_reason = RwSignal::new(String::new());
    let maintenance_scope = RwSignal::new("controlplane".to_string());

    let execute_maintenance = {
        let client = client.clone();
        Callback::new(move |_: ()| {
            let client = client.clone();
            let reason = maintenance_reason.get_untracked();
            let scope = maintenance_scope.get_untracked();
            maintenance_loading.set(true);
            action_error.set(None);
            action_success.set(None);
            maintenance_open.set(false);

            leptos::task::spawn_local(async move {
                match client.request_maintenance(&reason, &scope).await {
                    Ok(resp) => {
                        let tracking = resp
                            .get("tracking_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let lifecycle = resp
                            .get("lifecycle")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        action_success.set(Some(format!(
                            "Maintenance accepted (scope: {}). State: {}. Tracking: {}",
                            scope, lifecycle, tracking
                        )));
                    }
                    Err(e) => {
                        action_error.set(Some(format!("Maintenance request failed: {}", e)));
                    }
                }
                maintenance_loading.set(false);
                maintenance_reason.set(String::new());
            });
        })
    };

    // ── Safe Restart ───────────────────────────────────────────────────
    let restart_open = RwSignal::new(false);
    let restart_loading = RwSignal::new(false);

    let execute_restart = {
        let client = client.clone();
        Callback::new(move |_: ()| {
            let client = client.clone();
            restart_loading.set(true);
            action_error.set(None);
            action_success.set(None);
            restart_open.set(false);

            leptos::task::spawn_local(async move {
                match client.safe_restart().await {
                    Ok(resp) => {
                        let lifecycle = resp
                            .get("lifecycle")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        action_success.set(Some(format!(
                            "Safe restart initiated. State: {}. Process will exit when drained; supervisor should restart.",
                            lifecycle
                        )));
                    }
                    Err(e) => {
                        action_error.set(Some(format!("Safe restart failed: {}", e)));
                    }
                }
                restart_loading.set(false);
            });
        })
    };

    let mode_options = vec![
        ("drain".to_string(), "Drain (graceful)".to_string()),
        ("immediate".to_string(), "Immediate".to_string()),
    ];

    let scope_options = vec![
        ("controlplane".to_string(), "Control Plane".to_string()),
        ("worker".to_string(), "Worker".to_string()),
        ("all".to_string(), "All".to_string()),
    ];

    view! {
        <Card
            title="Admin Lifecycle".to_string()
            description="Shutdown, maintenance, and restart controls (Admin only)".to_string()
        >
            // Feedback messages
            {move || {
                if let Some(msg) = action_error.get() {
                    Some(view! {
                        <div class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 mb-4">
                            <p class="text-sm text-destructive">{msg}</p>
                        </div>
                    })
                } else {
                    None
                }
            }}

            {move || {
                if let Some(msg) = action_success.get() {
                    Some(view! {
                        <div class="rounded-md border border-green-500/40 bg-green-500/5 px-3 py-2 mb-4">
                            <p class="text-sm text-green-600">{msg}</p>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Action buttons row
            <div class="flex flex-wrap items-center gap-3">
                // Shutdown
                <Button
                    variant=ButtonVariant::Destructive
                    disabled=Signal::derive(move || shutdown_loading.get() || maintenance_loading.get() || restart_loading.get())
                    on_click=Callback::new(move |_| {
                        shutdown_open.set(true);
                    })
                >
                    "Request Shutdown"
                </Button>

                // Maintenance
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=Signal::derive(move || shutdown_loading.get() || maintenance_loading.get() || restart_loading.get())
                    on_click=Callback::new(move |_| {
                        maintenance_open.set(true);
                    })
                >
                    "Maintenance Mode"
                </Button>

                // Safe Restart
                <Button
                    variant=ButtonVariant::Outline
                    disabled=Signal::derive(move || shutdown_loading.get() || maintenance_loading.get() || restart_loading.get())
                    on_click=Callback::new(move |_| {
                        restart_open.set(true);
                    })
                >
                    "Safe Restart"
                </Button>
            </div>
        </Card>

        // ── Shutdown Dialog ──────────────────────────────────────────────
        <Dialog
            open=shutdown_open
            title="Request Shutdown".to_string()
            description="Initiate a controlled shutdown of the system.".to_string()
            size=DialogSize::Md
        >
            <div class="space-y-4">
                <FormField label="Reason" name="shutdown_reason" required=true>
                    <Input
                        value=shutdown_reason
                        placeholder="Scheduled maintenance window".to_string()
                    />
                </FormField>
                <FormField label="Mode" name="shutdown_mode" required=true help="Drain completes in-flight work. Immediate aborts.".to_string()>
                    <Select value=shutdown_mode options=mode_options.clone() />
                </FormField>

                <div class="rounded-md border border-destructive/30 bg-destructive/5 p-3">
                    <h3 class="text-sm font-medium text-destructive mb-2">"Impact"</h3>
                    <ul class="space-y-1 text-sm text-muted-foreground">
                        <li>"Active connections will be terminated"</li>
                        <li>"In-flight inferences will complete (drain) or abort (immediate)"</li>
                    </ul>
                </div>

                <div class="flex justify-end gap-3 pt-2">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| shutdown_open.set(false))
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Destructive
                        disabled=Signal::derive(move || shutdown_reason.get().trim().is_empty() || shutdown_loading.get())
                        loading=Signal::derive(move || shutdown_loading.get())
                        on_click=Callback::new(move |_| {
                            shutdown_dialog_open.set(true);
                        })
                    >
                        "Continue"
                    </Button>
                </div>
            </div>
        </Dialog>

        // Final shutdown confirmation (typed)
        <ConfirmationDialog
            open=shutdown_dialog_open
            title="Confirm Shutdown".to_string()
            description="This action cannot be undone from the UI. The system will become unavailable.".to_string()
            severity=ConfirmationSeverity::Destructive
            typed_confirmation="SHUTDOWN".to_string()
            impact_items=vec![
                ImpactItem::new("System availability", "System will become unavailable"),
                ImpactItem::new("Active sessions", "All active sessions will end"),
            ]
            on_confirm=execute_shutdown
            loading=Signal::derive(move || shutdown_loading.get())
        />

        // ── Maintenance Dialog ───────────────────────────────────────────
        <Dialog
            open=maintenance_open
            title="Request Maintenance".to_string()
            description="Put components into maintenance mode.".to_string()
            size=DialogSize::Md
        >
            <div class="space-y-4">
                <FormField label="Reason" name="maint_reason" required=true>
                    <Input
                        value=maintenance_reason
                        placeholder="Applying security patches".to_string()
                    />
                </FormField>
                <FormField label="Scope" name="maint_scope" required=true help="Control Plane only, Workers only, or All components.".to_string()>
                    <Select value=maintenance_scope options=scope_options.clone() />
                </FormField>

                <div class="flex justify-end gap-3 pt-2">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| maintenance_open.set(false))
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=Signal::derive(move || maintenance_reason.get().trim().is_empty() || maintenance_loading.get())
                        loading=Signal::derive(move || maintenance_loading.get())
                        on_click=Callback::new(move |_| {
                            execute_maintenance.run(());
                        })
                    >
                        "Enter Maintenance"
                    </Button>
                </div>
            </div>
        </Dialog>

        // ── Safe Restart Confirmation ────────────────────────────────────
        <ConfirmationDialog
            open=restart_open
            title="Safe Restart".to_string()
            description="Drains active connections, then exits the process. An external supervisor must restart AdapterOS.".to_string()
            severity=ConfirmationSeverity::Warning
            typed_confirmation="RESTART".to_string()
            impact_items=vec![
                ImpactItem::new("Drain period", "Active inferences will complete before shutdown"),
                ImpactItem::new("Restart", "External supervisor must restart the process"),
            ]
            on_confirm=execute_restart
            loading=Signal::derive(move || restart_loading.get())
        />
    }
}
