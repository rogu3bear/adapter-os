//! Workers page dialog components
//!
//! Modal dialogs for worker management actions.

use crate::components::{Button, ButtonVariant, Dialog, FormField, Input, Select};
use adapteros_api_types::{NodeResponse, SpawnWorkerRequest};
use leptos::prelude::*;

use super::utils::format_relative_date;

/// Local plan option type for spawn form
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanOption {
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub status: String,
    #[serde(default)]
    pub created_at: String,
}

/// Format a deployment config for display
fn format_config_label(plan: &PlanOption) -> String {
    let date_part = if !plan.created_at.is_empty() {
        format!(" ({})", format_relative_date(&plan.created_at))
    } else {
        String::new()
    };

    // Use a short ID prefix for identification
    let id_prefix = adapteros_id::short_id(&plan.id);

    format!("Config {}{}", id_prefix, date_part)
}

/// Format a node for display in quick summaries and select options
fn format_node_label(node: &NodeResponse) -> String {
    format!(
        "{} ({})",
        node.node.hostname,
        adapteros_id::short_id(&node.node.id)
    )
}

/// Deterministic UDS path helper shared by quick and advanced submit paths
fn auto_socket_path(node_id: &str, timestamp: u64) -> String {
    format!(
        "var/run/aos-worker-{}-{}.sock",
        adapteros_id::short_id(node_id),
        timestamp
    )
}

/// Quick mode node selection: prefer active nodes, fallback to first entry.
fn select_quick_node(nodes: &[NodeResponse]) -> Option<NodeResponse> {
    nodes
        .iter()
        .find(|n| n.node.status.eq_ignore_ascii_case("active"))
        .cloned()
        .or_else(|| nodes.first().cloned())
}

/// Quick mode plan selection: prefer ready/active plans, fallback to first entry.
fn select_quick_plan(plans: &[PlanOption]) -> Option<PlanOption> {
    plans
        .iter()
        .find(|p| {
            p.status.eq_ignore_ascii_case("ready")
                || p.status.eq_ignore_ascii_case("active")
                || p.status.eq_ignore_ascii_case("built")
        })
        .cloned()
        .or_else(|| plans.first().cloned())
}

#[component]
pub fn SpawnWorkerDialog(
    open: RwSignal<bool>,
    nodes: Vec<NodeResponse>,
    plans: Vec<PlanOption>,
    on_spawn: Callback<SpawnWorkerRequest>,
    #[prop(optional, into)] loading: Signal<bool>,
) -> impl IntoView {
    const MODE_QUICK: &str = "quick";
    const MODE_ADVANCED: &str = "advanced";

    // Form state
    let mode = RwSignal::new(MODE_QUICK.to_string());
    let node_id = RwSignal::new(String::new());
    let plan_id = RwSignal::new(String::new());
    let uds_path = RwSignal::new(String::new());

    // Clone data for use in closures
    let plans_for_lookup = plans.clone();
    let plans_for_tenant = plans.clone();

    // Quick-mode defaults (deterministic across renders for the same list ordering)
    let quick_node = StoredValue::new(select_quick_node(&nodes));
    let quick_plan = StoredValue::new(select_quick_plan(&plans));

    // Derive tenant_id from selected plan
    let tenant_id = Memo::new(move |_| {
        let selected_plan_id = plan_id.get();
        plans_for_lookup
            .iter()
            .find(|p| p.id == selected_plan_id)
            .map(|p| p.tenant_id.clone())
            .unwrap_or_default()
    });

    // Inline readiness checks for both modes.
    let quick_missing = Memo::new({
        move |_| {
            let mut missing = Vec::new();
            let node = quick_node.get_value();
            let plan = quick_plan.get_value();

            if node.is_none() {
                missing.push("No nodes are available. Register a node first.".to_string());
            }
            if plan.is_none() {
                missing.push("No deployment configs are available. Create one first.".to_string());
            }
            if let Some(selected_plan) = plan {
                if selected_plan.tenant_id.trim().is_empty() {
                    missing.push("Default deployment config is missing tenant scope.".to_string());
                }
            }
            missing
        }
    });

    let advanced_missing = Memo::new({
        move |_| {
            let mut missing = Vec::new();
            let selected_node = node_id.get();
            let selected_plan = plan_id.get();
            let selected_socket = uds_path.get();

            if selected_node.is_empty() {
                missing.push("Choose a node.".to_string());
            }
            if selected_plan.is_empty() {
                missing.push("Choose a deployment config.".to_string());
            }
            if selected_socket.is_empty() {
                missing.push("Socket path is required in Advanced mode.".to_string());
            }
            if !selected_plan.is_empty() && tenant_id.get().is_empty() {
                missing.push("Selected deployment config is missing tenant scope.".to_string());
            }
            missing
        }
    });

    let can_submit = Signal::derive(move || {
        if mode.get() == MODE_ADVANCED {
            advanced_missing.get().is_empty()
        } else {
            quick_missing.get().is_empty()
        }
    });

    // Build node options
    let node_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a node...".to_string()))
            .chain(
                nodes
                    .iter()
                    .map(|n| (n.node.id.clone(), format_node_label(n))),
            )
            .collect();

    // Build deployment config options with friendly labels
    let plan_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a deployment config...".to_string()))
            .chain(plans.iter().map(|p| (p.id.clone(), format_config_label(p))))
            .collect();
    let node_options = StoredValue::new(node_options);
    let plan_options = StoredValue::new(plan_options);

    // Mode options
    let mode_options = vec![
        (MODE_QUICK.to_string(), "Quick".to_string()),
        (MODE_ADVANCED.to_string(), "Advanced".to_string()),
    ];

    // Auto-generate UDS path when node is selected or changed.
    // Track the last auto-generated path so we only overwrite auto-generated values,
    // not user-edited ones.
    let last_auto_path = RwSignal::new(String::new());
    Effect::new(move || {
        let Some(node) = node_id.try_get() else {
            return;
        };
        if node.is_empty() {
            return;
        }
        let current = uds_path.get_untracked();
        let prev_auto = last_auto_path.get_untracked();
        // Only overwrite if empty or still matches the last auto-generated value
        if current.is_empty() || current == prev_auto {
            let timestamp = js_sys::Date::now() as u64;
            let generated = auto_socket_path(&node, timestamp);
            let _ = last_auto_path.try_set(generated.clone());
            let _ = uds_path.try_set(generated);
        }
    });

    // If user switches to Advanced with empty fields, seed controls from quick defaults.
    Effect::new({
        move || {
            let Some(current_mode) = mode.try_get() else {
                return;
            };
            if current_mode != MODE_ADVANCED {
                return;
            }

            if node_id.get_untracked().is_empty() {
                if let Some(default_node) = quick_node.get_value() {
                    node_id.set(default_node.node.id);
                }
            }

            if plan_id.get_untracked().is_empty() {
                if let Some(default_plan) = quick_plan.get_value() {
                    plan_id.set(default_plan.id);
                }
            }
        }
    });

    view! {
        <Dialog
            open=open
            title="Spawn New Worker".to_string()
            description="Start a new inference worker on a cluster node".to_string()
        >
            <div class="space-y-4 overflow-y-auto" style="max-height: 60vh">
                <FormField
                    label="Mode"
                    name="spawn_mode"
                    required=true
                    help="Quick auto-picks defaults. Advanced lets you choose node/config/socket.".to_string()
                >
                    <Select value=mode options=mode_options />
                </FormField>

                {move || {
                    if mode.get() == MODE_QUICK {
                        let chosen_node = quick_node.get_value();
                        let chosen_plan = quick_plan.get_value();
                        let node_label = chosen_node
                            .as_ref()
                            .map(format_node_label)
                            .unwrap_or_else(|| "No available nodes".to_string());
                        let config_label = chosen_plan
                            .as_ref()
                            .map(format_config_label)
                            .unwrap_or_else(|| "No available deployment configs".to_string());
                        let tenant = chosen_plan
                            .as_ref()
                            .map(|p| p.tenant_id.clone())
                            .unwrap_or_default();
                        let socket_preview = chosen_node
                            .as_ref()
                            .map(|n| {
                                format!(
                                    "var/run/aos-worker-{}-<timestamp>.sock",
                                    adapteros_id::short_id(&n.node.id)
                                )
                            })
                            .unwrap_or_else(|| "var/run/aos-worker-<node>-<timestamp>.sock".to_string());

                        Some(view! {
                            <div class="rounded-md bg-muted/50 px-3 py-2 space-y-1">
                                <p class="text-xs font-medium text-muted-foreground">"Quick will choose"</p>
                                <p class="text-sm">
                                    <span class="text-muted-foreground">"Node: "</span>
                                    {node_label}
                                </p>
                                <p class="text-sm">
                                    <span class="text-muted-foreground">"Deployment Config: "</span>
                                    {config_label}
                                </p>
                                <p class="text-sm">
                                    <span class="text-muted-foreground">"Tenant: "</span>
                                    <span class="font-mono">{if tenant.is_empty() { "-".to_string() } else { tenant }}</span>
                                </p>
                                <p class="text-xs text-muted-foreground">
                                    <span>"Socket path: "</span>
                                    <span class="font-mono">{socket_preview}</span>
                                </p>
                                <p class="text-xs text-muted-foreground">
                                    "Preference order: active entries first, then first in each list."
                                </p>
                            </div>
                        })
                    } else {
                        None
                    }
                }}

                {move || {
                    let current_mode = mode.get();
                    let missing = if current_mode == MODE_ADVANCED {
                        advanced_missing.get()
                    } else {
                        quick_missing.get()
                    };

                    if missing.is_empty() {
                        let ready_text = if current_mode == MODE_ADVANCED {
                            "Advanced mode is ready to spawn."
                        } else {
                            "Quick mode is ready to spawn."
                        };
                        view! {
                            <div class="rounded-md border border-border/60 bg-muted/20 px-3 py-2">
                                <p class="text-xs text-muted-foreground">{ready_text}</p>
                            </div>
                        }.into_any()
                    } else {
                        let heading = if current_mode == MODE_ADVANCED {
                            "Advanced mode is missing prerequisites:"
                        } else {
                            "Quick mode is missing prerequisites:"
                        };
                        view! {
                            <div class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2">
                                <p class="text-xs font-medium text-destructive">{heading}</p>
                                <div class="mt-1 space-y-1">
                                    {missing
                                        .into_iter()
                                        .map(|msg| view! { <p class="text-xs text-destructive">{msg}</p> })
                                        .collect_view()}
                                </div>
                                {if current_mode == MODE_QUICK {
                                    view! {
                                        <div class="mt-2 flex flex-wrap items-center gap-3 text-xs">
                                            <a class="text-primary hover:underline" href="/system">
                                                "Open System (register node)"
                                            </a>
                                            <a class="text-primary hover:underline" href="/stacks">
                                                "Open Stacks (create deployment config)"
                                            </a>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <div></div> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                }}

                {move || {
                    if mode.get() == MODE_ADVANCED {
                        Some(view! {
                            <div class="border-t pt-4 space-y-4">
                                // Node selection (required)
                                <FormField label="Node" name="node" required=true help="The cluster node where the worker will run".to_string()>
                                    <Select value=node_id options=node_options.get_value() />
                                </FormField>

                                // Deployment config selection (required)
                                <FormField label="Deployment Config" name="deployment_config" required=true help="Defines which adapters and routing rules the worker uses".to_string()>
                                    <Select value=plan_id options=plan_options.get_value() />
                                </FormField>

                                // Show tenant ID as read-only info when a plan is selected
                                {move || {
                                    let tid = tenant_id.get();
                                    if !tid.is_empty() {
                                        Some(view! {
                                            <div class="rounded-md bg-muted/50 px-3 py-2">
                                                <p class="text-xs text-muted-foreground">"Tenant"</p>
                                                <p class="text-sm font-mono">{tid}</p>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }
                                }}

                                <FormField label="Socket Path" name="socket_path" required=true help="Auto-generated from the selected node; edit only if you need a custom path.".to_string()>
                                    <Input
                                        value=uds_path
                                        placeholder="var/run/aos-worker.sock".to_string()
                                    />
                                </FormField>
                            </div>
                        })
                    } else {
                        None
                    }
                }}

                <div class="flex justify-end gap-2 pt-4">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| {
                            open.set(false);
                            // Reset form
                            mode.set(MODE_QUICK.to_string());
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                            last_auto_path.set(String::new());
                        })
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=Signal::derive(move || !can_submit.get() || loading.get())
                        loading=loading
                        on_click=Callback::new({
                            let plans_ref = plans_for_tenant.clone();
                            move |_| {
                                let request = if mode.get() == MODE_ADVANCED {
                                    if !advanced_missing.get().is_empty() {
                                        return;
                                    }

                                    let selected_plan_id = plan_id.get();
                                    let selected_node_id = node_id.get();
                                    let selected_socket = uds_path.get();
                                    let tid = plans_ref
                                        .iter()
                                        .find(|p| p.id == selected_plan_id)
                                        .map(|p| p.tenant_id.clone())
                                        .unwrap_or_default();

                                    if tid.is_empty()
                                        || selected_node_id.is_empty()
                                        || selected_plan_id.is_empty()
                                        || selected_socket.is_empty()
                                    {
                                        return;
                                    }

                                    SpawnWorkerRequest {
                                        tenant_id: tid,
                                        node_id: selected_node_id,
                                        plan_id: selected_plan_id,
                                        uds_path: selected_socket,
                                    }
                                } else {
                                    if !quick_missing.get().is_empty() {
                                        return;
                                    }

                                    let Some(default_node) = quick_node.get_value() else {
                                        return;
                                    };
                                    let Some(default_plan) = quick_plan.get_value() else {
                                        return;
                                    };
                                    let tid = default_plan.tenant_id.trim().to_string();
                                    if tid.is_empty() {
                                        return;
                                    }

                                    let timestamp = js_sys::Date::now() as u64;
                                    let default_node_id = default_node.node.id.clone();
                                    SpawnWorkerRequest {
                                        tenant_id: tid,
                                        node_id: default_node_id.clone(),
                                        plan_id: default_plan.id,
                                        uds_path: auto_socket_path(&default_node_id, timestamp),
                                    }
                                };

                                on_spawn.run(request);
                                // Reset form
                                mode.set(MODE_QUICK.to_string());
                                node_id.set(String::new());
                                plan_id.set(String::new());
                                uds_path.set(String::new());
                                last_auto_path.set(String::new());
                            }
                        })
                    >
                        "Spawn Worker"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}
