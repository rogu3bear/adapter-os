pub mod decisions;
pub mod rules;

pub use decisions::RoutingDecisions;
pub use rules::RoutingRules;

use crate::components::{TabNav, TabPanel};
use leptos::prelude::*;

#[component]
pub fn Routing() -> impl IntoView {
    let active_tab = RwSignal::new("rules".to_string());

    view! {
        <div class="space-y-6">
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
