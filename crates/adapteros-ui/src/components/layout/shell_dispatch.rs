//! Shell dispatcher — routes between Shell and HudShell based on UI profile.

use super::hud_shell::HudShell;
use super::shell::Shell;
use crate::signals::use_ui_profile;
use adapteros_api_types::UiProfile;
use leptos::prelude::*;
use leptos::tachys::view::any_view::IntoAny;

/// Reads the effective UI profile and renders the matching shell.
///
/// Shell and HudShell are siblings — neither nests inside the other.
/// Both provide the same contexts child pages expect (route context,
/// SSE subscriptions, StatusCenterProvider) so pages work identically
/// regardless of which shell is active.
#[component]
pub fn ShellDispatch() -> impl IntoView {
    let profile = use_ui_profile();
    move || match profile.get() {
        UiProfile::Hud => view! { <HudShell/> }.into_any(),
        _ => view! { <Shell/> }.into_any(),
    }
}
