//! Routing pages for inspecting adapter routing behavior.
//!
//! Provides three views:
//! - **Rules (Management)**: View and manage routing rules that determine
//!   how requests are distributed across adapters.
//! - **Decisions**: Inspect recent routing decisions and their outcomes.
//! - **Weights**: View and adjust router feature importance weights.

pub mod decisions;
pub mod rules;
pub mod weights;

pub use decisions::RoutingDecisions;
pub use rules::RoutingRules;
pub use weights::RoutingWeights;

use crate::components::{PageBreadcrumbItem, PageScaffold, TabNav, TabPanel};
use leptos::prelude::*;

#[component]
pub fn Routing() -> impl IntoView {
    let active_tab = RwSignal::new("rules");

    view! {
        <div data-testid="routing-page">
            <PageScaffold
                title="Routing"
                subtitle="Inspect and manage how requests are routed across adapters."
                breadcrumbs=vec![
                    PageBreadcrumbItem::label("Route"),
                    PageBreadcrumbItem::current("Routing"),
                ]
            >
                <TabNav
                    tabs=vec![
                        ("rules", "Rules"),
                        ("decisions", "Decisions"),
                        ("weights", "Weights"),
                    ]
                    active=active_tab
                />

                <TabPanel tab="rules" active=active_tab>
                    <RoutingRules/>
                </TabPanel>

                <TabPanel tab="decisions" active=active_tab>
                    <RoutingDecisions/>
                </TabPanel>

                <TabPanel tab="weights" active=active_tab>
                    <RoutingWeights/>
                </TabPanel>
            </PageScaffold>
        </div>
    }
}
