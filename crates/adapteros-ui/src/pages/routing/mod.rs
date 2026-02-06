//! Routing debug pages for inspecting adapter routing behavior.
//!
//! Provides two views:
//! - **Rules (Management)**: View and manage routing rules that determine
//!   how requests are distributed across adapters.
//! - **Decisions**: Inspect recent routing decisions and their outcomes.

pub mod decisions;
pub mod rules;

pub use decisions::RoutingDecisions;
pub use rules::RoutingRules;

use crate::components::{TabNav, TabPanel};
use leptos::prelude::*;

#[component]
pub fn Routing() -> impl IntoView {
    let active_tab = RwSignal::new("rules");

    view! {
        <div class="shell-page space-y-6">
            <div>
                <h1 class="heading-1">"Routing Debug"</h1>
                <p class="text-sm text-muted-foreground">
                    "Inspect and manage how requests are routed across adapters."
                </p>
            </div>
            <TabNav
                tabs=vec![
                    ("rules", "Management"),
                    ("decisions", "Decisions"),
                ]
                active=active_tab
            />

            <TabPanel tab="rules" active=active_tab>
                <RoutingRules/>
            </TabPanel>

            <TabPanel tab="decisions" active=active_tab>
                <RoutingDecisions/>
            </TabPanel>
        </div>
    }
}
