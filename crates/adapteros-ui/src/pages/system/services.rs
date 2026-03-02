//! Service control panel
//!
//! Interactive service management: start, stop, restart individual services
//! and view service logs. Requires NodeManage permission.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity,
    Dialog, DialogSize,
};
use adapteros_api_types::{ServiceHealthStatus, ServiceState};
use leptos::prelude::*;
use std::sync::Arc;

/// Service control panel with start/stop/restart and log viewing
#[component]
pub fn ServiceControlPanel(services: Vec<ServiceState>) -> impl IntoView {
    let client = expect_context::<Arc<ApiClient>>();

    // Action state
    let action_loading = RwSignal::new(false);
    let action_error = RwSignal::new(Option::<String>::None);
    let action_success = RwSignal::new(Option::<String>::None);

    // Confirmation dialog state
    let confirm_open = RwSignal::new(false);
    let confirm_service_id = RwSignal::new(String::new());
    let confirm_action = RwSignal::new(String::new()); // "start", "stop", "restart"

    // Logs dialog state
    let logs_open = RwSignal::new(false);
    let logs_service_id = RwSignal::new(String::new());
    let logs_content = RwSignal::new(Vec::<String>::new());
    let logs_loading = RwSignal::new(false);

    // Bulk action state
    let bulk_confirm_open = RwSignal::new(false);
    let bulk_action = RwSignal::new(String::new()); // "start-essential", "stop-essential"

    let services_data = StoredValue::new(services);

    // Execute service action
    let execute_action = {
        let client = client.clone();
        Callback::new(move |_: ()| {
            let client = client.clone();
            let service_id = confirm_service_id.get_untracked();
            let action = confirm_action.get_untracked();
            action_loading.set(true);
            action_error.set(None);
            action_success.set(None);
            confirm_open.set(false);

            leptos::task::spawn_local(async move {
                let result = match action.as_str() {
                    "start" => client.start_service(&service_id).await,
                    "stop" => client.stop_service(&service_id).await,
                    "restart" => client.restart_service(&service_id).await,
                    _ => return,
                };

                let _ = action_loading.try_set(false);
                match result {
                    Ok(resp) if resp.success => {
                        let _ = action_success.try_set(Some(resp.message));
                    }
                    Ok(resp) => {
                        let _ = action_error.try_set(Some(resp.message));
                    }
                    Err(e) => {
                        let _ = action_error.try_set(Some(format!("{}", e)));
                    }
                }
            });
        })
    };

    // Execute bulk action
    let execute_bulk = {
        let client = client.clone();
        Callback::new(move |_: ()| {
            let client = client.clone();
            let action = bulk_action.get_untracked();
            action_loading.set(true);
            action_error.set(None);
            action_success.set(None);
            bulk_confirm_open.set(false);

            leptos::task::spawn_local(async move {
                let result = match action.as_str() {
                    "start-essential" => client.start_essential_services().await,
                    "stop-essential" => client.stop_essential_services().await,
                    _ => return,
                };

                let _ = action_loading.try_set(false);
                match result {
                    Ok(resp) if resp.success => {
                        let _ = action_success.try_set(Some(resp.message));
                    }
                    Ok(resp) => {
                        let _ = action_error.try_set(Some(resp.message));
                    }
                    Err(e) => {
                        let _ = action_error.try_set(Some(format!("{}", e)));
                    }
                }
            });
        })
    };

    // Fetch logs
    let fetch_logs = {
        let client = client.clone();
        Callback::new(move |service_id: String| {
            let client = client.clone();
            logs_service_id.set(service_id.clone());
            logs_loading.set(true);
            logs_content.set(Vec::new());
            logs_open.set(true);

            leptos::task::spawn_local(async move {
                match client.get_service_logs(&service_id, Some(200)).await {
                    Ok(lines) => {
                        let _ = logs_content.try_set(lines);
                    }
                    Err(e) => {
                        let _ = logs_content.try_set(vec![format!("Error fetching logs: {}", e)]);
                    }
                }
                let _ = logs_loading.try_set(false);
            });
        })
    };

    let confirm_title = Memo::new(move |_| {
        let action = confirm_action.get();
        let service = confirm_service_id.get();
        match action.as_str() {
            "start" => format!("Start {}", service),
            "stop" => format!("Stop {}", service),
            "restart" => format!("Restart {}", service),
            _ => "Confirm Action".to_string(),
        }
    });

    let confirm_description = Memo::new(move |_| {
        let action = confirm_action.get();
        let service = confirm_service_id.get();
        match action.as_str() {
            "start" => format!(
                "Start the {} service. This will attempt to launch the service process.",
                service
            ),
            "stop" => format!(
                "Stop the {} service. Active connections will be terminated.",
                service
            ),
            "restart" => format!(
                "Restart the {} service. The service will briefly be unavailable during restart.",
                service
            ),
            _ => "Confirm this action.".to_string(),
        }
    });

    let confirm_severity = Memo::new(move |_| match confirm_action.get().as_str() {
        "stop" => ConfirmationSeverity::Warning,
        _ => ConfirmationSeverity::Normal,
    });

    let bulk_title = Memo::new(move |_| match bulk_action.get().as_str() {
        "start-essential" => "Start Essential Services".to_string(),
        "stop-essential" => "Stop Essential Services".to_string(),
        _ => "Confirm Action".to_string(),
    });

    let bulk_description = Memo::new(move |_| match bulk_action.get().as_str() {
        "start-essential" => "Start all essential services (backend, worker).".to_string(),
        "stop-essential" => {
            "Stop all essential services. The system will become unavailable.".to_string()
        }
        _ => "Confirm this action.".to_string(),
    });

    let bulk_severity = Memo::new(move |_| match bulk_action.get().as_str() {
        "stop-essential" => ConfirmationSeverity::Warning,
        _ => ConfirmationSeverity::Normal,
    });

    view! {
        <Card
            title="Service Control".to_string()
            description="Manage system services".to_string()
        >
            // Feedback messages
            {move || {
                action_error.get().map(|msg| view! {
                        <div class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 mb-4">
                            <p class="text-sm text-destructive">{msg}</p>
                        </div>
                    })
            }}

            {move || {
                action_success.get().map(|msg| view! {
                        <div class="rounded-md border border-status-success/40 bg-status-success/5 px-3 py-2 mb-4">
                            <p class="text-sm text-status-success">{msg}</p>
                        </div>
                    })
            }}

            // Service list
            <div class="space-y-3">
                {services_data.get_value().into_iter().map(|svc| {
                    let name = svc.name.clone();
                    let name_for_start = svc.name.clone();
                    let name_for_stop = svc.name.clone();
                    let name_for_restart = svc.name.clone();
                    let name_for_logs = svc.name.clone();
                    let (variant, label) = match svc.status {
                        ServiceHealthStatus::Healthy => (BadgeVariant::Success, "Healthy"),
                        ServiceHealthStatus::Degraded => (BadgeVariant::Warning, "Degraded"),
                        ServiceHealthStatus::Unhealthy => (BadgeVariant::Destructive, "Unhealthy"),
                        ServiceHealthStatus::Unknown => (BadgeVariant::Secondary, "Unknown"),
                    };
                    let last_check = svc.last_check.clone();

                    view! {
                        <div class="flex items-center justify-between rounded-md border border-border/60 bg-card/50 px-4 py-3">
                            <div class="flex items-center gap-3">
                                <span class="text-sm font-medium">{name}</span>
                                <Badge variant=variant>{label}</Badge>
                                <span class="text-xs text-muted-foreground">{last_check}</span>
                            </div>
                            <div class="flex items-center gap-2">
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=crate::components::ButtonSize::Sm
                                    on_click=Callback::new(move |_| {
                                        fetch_logs.run(name_for_logs.clone());
                                    })
                                >
                                    "Logs"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Primary
                                    size=crate::components::ButtonSize::Sm
                                    disabled=Signal::derive(move || action_loading.get())
                                    on_click=Callback::new(move |_| {
                                        confirm_service_id.set(name_for_start.clone());
                                        confirm_action.set("start".to_string());
                                        confirm_open.set(true);
                                    })
                                >
                                    "Start"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Secondary
                                    size=crate::components::ButtonSize::Sm
                                    disabled=Signal::derive(move || action_loading.get())
                                    on_click=Callback::new(move |_| {
                                        confirm_service_id.set(name_for_restart.clone());
                                        confirm_action.set("restart".to_string());
                                        confirm_open.set(true);
                                    })
                                >
                                    "Restart"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=crate::components::ButtonSize::Sm
                                    disabled=Signal::derive(move || action_loading.get())
                                    on_click=Callback::new(move |_| {
                                        confirm_service_id.set(name_for_stop.clone());
                                        confirm_action.set("stop".to_string());
                                        confirm_open.set(true);
                                    })
                                >
                                    "Stop"
                                </Button>
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>

            // Bulk actions
            <div class="flex items-center gap-3 mt-4 pt-4 border-t border-border/40">
                <span class="text-sm text-muted-foreground">"Essential services:"</span>
                <Button
                    variant=ButtonVariant::Primary
                    size=crate::components::ButtonSize::Sm
                    disabled=Signal::derive(move || action_loading.get())
                    on_click=Callback::new(move |_| {
                        bulk_action.set("start-essential".to_string());
                        bulk_confirm_open.set(true);
                    })
                >
                    "Start All"
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    size=crate::components::ButtonSize::Sm
                    disabled=Signal::derive(move || action_loading.get())
                    on_click=Callback::new(move |_| {
                        bulk_action.set("stop-essential".to_string());
                        bulk_confirm_open.set(true);
                    })
                >
                    "Stop All"
                </Button>
            </div>
        </Card>

        // Confirmation dialog for individual service actions
        <ConfirmationDialog
            open=confirm_open
            title=confirm_title.get_untracked()
            description=confirm_description.get_untracked()
            severity=confirm_severity.get_untracked()
            on_confirm=execute_action
            loading=Signal::derive(move || action_loading.get())
        />

        // Confirmation dialog for bulk actions
        <ConfirmationDialog
            open=bulk_confirm_open
            title=bulk_title.get_untracked()
            description=bulk_description.get_untracked()
            severity=bulk_severity.get_untracked()
            on_confirm=execute_bulk
            loading=Signal::derive(move || action_loading.get())
        />

        // Logs dialog
        <Dialog
            open=logs_open
            title=format!("Logs: {}", logs_service_id.get_untracked())
            description="Recent service log output".to_string()
            size=DialogSize::Lg
        >
            <div class="max-h-96 overflow-y-auto">
                {move || {
                    if logs_loading.get() {
                        view! {
                            <div class="flex items-center justify-center py-8">
                                <span class="text-sm text-muted-foreground">"Loading logs..."</span>
                            </div>
                        }.into_any()
                    } else {
                        let lines = logs_content.get();
                        if lines.is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground py-4">"No log output available."</p>
                            }.into_any()
                        } else {
                            view! {
                                <pre class="text-xs font-mono bg-muted/30 rounded-md p-3 whitespace-pre-wrap break-all">
                                    {lines.join("\n")}
                                </pre>
                            }.into_any()
                        }
                    }
                }}
            </div>
            <div class="flex justify-end mt-4">
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| {
                        logs_open.set(false);
                    })
                >
                    "Close"
                </Button>
            </div>
        </Dialog>
    }
}
