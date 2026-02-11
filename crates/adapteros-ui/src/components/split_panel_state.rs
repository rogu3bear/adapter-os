//! Shared state helpers for SplitPanel-driven list/detail pages.

use crate::signals::{RouteContext, SelectedEntity};
use leptos::prelude::*;

#[derive(Clone)]
pub struct SplitPanelSelectionState {
    pub selected_id: RwSignal<Option<String>>,
    pub has_selection: Signal<bool>,
    pub on_select: Callback<String>,
    pub on_close: Callback<()>,
}

/// Create canonical selection state for split panel pages.
pub fn use_split_panel_selection_state() -> SplitPanelSelectionState {
    let selected_id = RwSignal::new(None::<String>);
    let has_selection = Signal::derive(move || selected_id.get().is_some());

    let on_select = Callback::new(move |id: String| {
        selected_id.set(Some(id));
    });

    let on_close = Callback::new(move |_: ()| {
        selected_id.set(None);
    });

    SplitPanelSelectionState {
        selected_id,
        has_selection,
        on_select,
        on_close,
    }
}

/// Publish or clear selected route context entity in a consistent way.
pub fn publish_route_selection(
    route_ctx: &RouteContext,
    entity_type: &str,
    selected_id: Option<String>,
    display_name: Option<String>,
    status: Option<String>,
) {
    if let Some(id) = selected_id {
        match (display_name, status) {
            (Some(name), Some(status)) => {
                route_ctx.set_selected(SelectedEntity::with_status(entity_type, id, name, status));
            }
            (Some(name), None) => {
                route_ctx.set_selected(SelectedEntity::new(entity_type, id.clone(), name));
            }
            (None, _) => {
                route_ctx.set_selected(SelectedEntity::new(entity_type, id.clone(), id));
            }
        }
    } else {
        route_ctx.clear_selected();
    }
}
