//! Workers page dialog components
//!
//! Modal dialogs for worker management actions.

use crate::components::{Button, ButtonVariant, Dialog, Input, Select};
use adapteros_api_types::{NodeResponse, SpawnWorkerRequest};
use leptos::prelude::*;

use super::utils::{format_relative_date, short_id};

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
    let id_prefix = if plan.id.len() > 8 {
        &plan.id[..8]
    } else {
        &plan.id
    };

    format!("Config {}...{}", id_prefix, date_part)
}

#[component]
pub fn SpawnWorkerDialog(
    open: RwSignal<bool>,
    nodes: Vec<NodeResponse>,
    plans: Vec<PlanOption>,
    on_spawn: Callback<SpawnWorkerRequest>,
) -> impl IntoView {
    // Form state
    let node_id = RwSignal::new(String::new());
    let plan_id = RwSignal::new(String::new());
    let uds_path = RwSignal::new(String::new());

    // Clone plans for use in closures
    let plans_for_lookup = plans.clone();
    let plans_for_tenant = plans.clone();

    // Derive tenant_id from selected plan
    let tenant_id = Memo::new(move |_| {
        let selected_plan_id = plan_id.get();
        plans_for_lookup
            .iter()
            .find(|p| p.id == selected_plan_id)
            .map(|p| p.tenant_id.clone())
            .unwrap_or_default()
    });

    // Validation - tenant_id is derived from plan, so just check plan is selected
    let is_valid = move || !node_id.get().is_empty() && !plan_id.get().is_empty();

    // Build node options
    let node_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a node...".to_string()))
            .chain(nodes.iter().map(|n| {
                (
                    n.node.id.clone(),
                    format!("{} ({})", n.node.hostname, short_id(&n.node.id)),
                )
            }))
            .collect();

    // Build deployment config options with friendly labels
    let plan_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a deployment config...".to_string()))
            .chain(plans.iter().map(|p| (p.id.clone(), format_config_label(p))))
            .collect();

    // Auto-generate UDS path when node is selected
    Effect::new(move || {
        let node = node_id.get();
        if !node.is_empty() && uds_path.get().is_empty() {
            let timestamp = js_sys::Date::now() as u64;
            uds_path.set(format!(
                "/tmp/aos-worker-{}-{}.sock",
                short_id(&node),
                timestamp
            ));
        }
    });

    view! {
        <Dialog
            open=open
            title="Spawn New Worker".to_string()
            description="Start a new inference worker on a cluster node".to_string()
        >
            <div class="space-y-4">
                // Node selection (required)
                <Select
                    value=node_id
                    options=node_options
                    label="Node".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "The cluster node where the worker will run"
                </p>

                // Deployment config selection (required) - renamed from "Plan"
                <Select
                    value=plan_id
                    options=plan_options
                    label="Deployment Config".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "Defines which adapters and routing rules the worker uses"
                </p>

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

                // Advanced option - Socket Path (auto-generated, rarely needs changing)
                <div class="border-t pt-4">
                    <p class="text-xs font-medium text-muted-foreground mb-2">"Advanced (optional)"</p>
                    <Input
                        value=uds_path
                        label="Socket Path".to_string()
                        placeholder="/var/run/aos-worker.sock".to_string()
                    />
                    <p class="text-xs text-muted-foreground -mt-2">
                        "Auto-generated when a node is selected. Most users won't need to change this."
                    </p>
                </div>

                <div class="flex justify-end gap-2 pt-4">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| {
                            open.set(false);
                            // Reset form
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                        })
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=Signal::derive(move || !is_valid())
                        on_click=Callback::new({
                            let plans_ref = plans_for_tenant.clone();
                            move |_| {
                                let selected_plan_id = plan_id.get();
                                let tid = plans_ref
                                    .iter()
                                    .find(|p| p.id == selected_plan_id)
                                    .map(|p| p.tenant_id.clone())
                                    .unwrap_or_default();

                                // Ensure UDS path has a value
                                let socket_path = if uds_path.get().is_empty() {
                                    let timestamp = js_sys::Date::now() as u64;
                                    format!("/var/run/aos-worker-{}-{}.sock", short_id(&node_id.get()), timestamp)
                                } else {
                                    uds_path.get()
                                };

                                let request = SpawnWorkerRequest {
                                    tenant_id: tid,
                                    node_id: node_id.get(),
                                    plan_id: selected_plan_id,
                                    uds_path: socket_path,
                                };
                                on_spawn.run(request);
                                // Reset form
                                node_id.set(String::new());
                                plan_id.set(String::new());
                                uds_path.set(String::new());
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
