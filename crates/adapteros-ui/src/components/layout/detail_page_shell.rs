//! DetailPageShell - Reusable scaffold for entity detail pages
//!
//! Provides the common structure shared by standalone detail pages:
//! URL param extraction, PageScaffold with 3-level breadcrumbs,
//! back-button navigation, and empty-ID guard.
//!
//! Usage:
//! ```rust,ignore
//! view! {
//!     <DetailPageShell
//!         title="Run Detail"
//!         section="Observe"
//!         section_href="/runs"
//!         entity_plural="Runs"
//!         list_href="/runs"
//!     >
//!         // children rendered only when ID is non-empty
//!         // use expect_context::<DetailEntityId>() to get the ID
//!         <RunDetailHub run_id=expect_context::<DetailEntityId>().get()/>
//!     </DetailPageShell>
//! }
//! ```

use crate::api::ApiError;
use crate::components::{Button, ButtonVariant, ErrorDisplay};

use super::{BreadcrumbItem, PageScaffold, PageScaffoldActions};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};

/// Reactive entity ID extracted from URL parameters by [`DetailPageShell`].
///
/// Children of `DetailPageShell` retrieve the current entity ID via:
/// ```rust,ignore
/// let id = expect_context::<DetailEntityId>();
/// let current_id: String = id.get();
/// ```
#[derive(Clone, Copy)]
pub struct DetailEntityId(Memo<String>);

impl DetailEntityId {
    /// Get the current entity ID value.
    pub fn get(&self) -> String {
        self.0.get()
    }
}

/// Reusable scaffold for entity detail pages.
///
/// Handles URL param extraction, breadcrumb construction, back-button
/// navigation, and empty-ID validation. Children are rendered only when
/// the entity ID is non-empty and can access it via
/// `expect_context::<DetailEntityId>()`.
#[component]
pub fn DetailPageShell(
    /// Page title shown in the scaffold header
    #[prop(into)]
    title: String,
    /// Nav-group label for the first breadcrumb (e.g. "Observe")
    section: &'static str,
    /// Href for the first breadcrumb
    section_href: &'static str,
    /// Entity collection label (e.g. "Runs", "Repositories")
    entity_plural: &'static str,
    /// Href for the entity list (also used for back-button navigation)
    list_href: &'static str,
    /// URL param key (defaults to "id")
    #[prop(default = "id")]
    param_key: &'static str,
    /// Body content, rendered when the entity ID is non-empty.
    /// Access the ID via `expect_context::<DetailEntityId>()`.
    children: ChildrenFn,
) -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let entity_id = Memo::new(move |_| params.get().get(param_key).unwrap_or_default());

    provide_context(DetailEntityId(entity_id));

    let back_label = format!("Back to {entity_plural}");
    let list_href_owned = list_href.to_string();

    view! {
        <PageScaffold
            title=title
            breadcrumbs=vec![
                BreadcrumbItem::new(section, section_href),
                BreadcrumbItem::new(entity_plural, list_href),
                BreadcrumbItem::current(entity_id.get()),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new({
                        let navigate = navigate.clone();
                        let href = list_href_owned.clone();
                        move |_| navigate(&href, Default::default())
                    })
                >
                    {back_label}
                </Button>
            </PageScaffoldActions>
            {move || {
                let id = entity_id.get();
                if id.is_empty() {
                    view! {
                        <ErrorDisplay error=ApiError::Validation(
                            format!("Missing {param_key} in URL")
                        )/>
                    }.into_any()
                } else {
                    children().into_any()
                }
            }}
        </PageScaffold>
    }
}
