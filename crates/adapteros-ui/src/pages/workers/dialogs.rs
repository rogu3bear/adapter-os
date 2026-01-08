//! Workers page dialog components
//!
//! Modal dialogs for worker management actions.

use crate::components::{Dialog, Input, Select};
use adapteros_api_types::{NodeResponse, SpawnWorkerRequest};
use leptos::prelude::*;

use super::utils::{short_hash, short_id};

/// Local plan option type for spawn form
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanOption {
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub status: String,
}

#[component]
pub fn SpawnWorkerDialog(
    open: RwSignal<bool>,
    nodes: Vec<NodeResponse>,
    plans: Vec<PlanOption>,
    on_spawn: Callback<SpawnWorkerRequest>,
) -> impl IntoView {
    // Form state
    let tenant_id = RwSignal::new(String::new());
    let node_id = RwSignal::new(String::new());
    let plan_id = RwSignal::new(String::new());
    let uds_path = RwSignal::new(String::new());

    // Validation
    let is_valid = move || {
        !tenant_id.get().is_empty()
            && !node_id.get().is_empty()
            && !plan_id.get().is_empty()
            && !uds_path.get().is_empty()
    };

    // Build node options
    let node_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a node...".to_string()))
            .chain(
                nodes
                    .iter()
                    .map(|n| (n.id.clone(), format!("{} ({})", n.hostname, n.id))),
            )
            .collect();

    // Build plan options
    let plan_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a plan...".to_string()))
            .chain(plans.iter().map(|p| {
                (
                    p.id.clone(),
                    format!("{} ({})", short_hash(&p.manifest_hash_b3), p.id),
                )
            }))
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
            description="Configure and spawn a new inference worker".to_string()
        >
            <div class="space-y-4">
                <Input
                    value=tenant_id
                    label="Tenant ID".to_string()
                    placeholder="Enter tenant ID".to_string()
                />

                <Select
                    value=node_id
                    options=node_options
                    label="Node".to_string()
                />

                <Select
                    value=plan_id
                    options=plan_options
                    label="Plan".to_string()
                />

                <Input
                    value=uds_path
                    label="UDS Path".to_string()
                    placeholder="/tmp/aos-worker.sock".to_string()
                />

                <div class="flex justify-end gap-2 pt-4">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| {
                            open.set(false);
                            // Reset form
                            tenant_id.set(String::new());
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                        disabled=move || !is_valid()
                        on:click=move |_| {
                            let request = SpawnWorkerRequest {
                                tenant_id: tenant_id.get(),
                                node_id: node_id.get(),
                                plan_id: plan_id.get(),
                                uds_path: uds_path.get(),
                            };
                            on_spawn.run(request);
                            // Reset form
                            tenant_id.set(String::new());
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                        }
                    >
                        "Spawn Worker"
                    </button>
                </div>
            </div>
        </Dialog>
    }
}
