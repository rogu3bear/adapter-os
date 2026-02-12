//! UI profile configuration state.

use super::settings::use_settings;
use crate::api::ApiClient;
use crate::boot_log;
use adapteros_api_types::UiProfile;
use leptos::prelude::*;

#[derive(Debug, Clone)]
pub struct UiProfileState {
    pub runtime_profile: Option<UiProfile>,
    pub loaded: bool,
}

impl UiProfileState {
    fn new() -> Self {
        Self {
            runtime_profile: None,
            loaded: false,
        }
    }
}

pub type UiProfileContext = RwSignal<UiProfileState>;

/// Provide UI profile context and fetch runtime config.
pub fn provide_ui_profile_context() {
    let state = RwSignal::new(UiProfileState::new());
    provide_context(state);

    wasm_bindgen_futures::spawn_local(async move {
        let client = ApiClient::new();
        match client.get_ui_config().await {
            Ok(resp) => {
                state.update(|s| {
                    s.runtime_profile = Some(resp.ui_profile);
                    s.loaded = true;
                });
            }
            Err(err) => {
                boot_log("ui_profile", &format!("ui config fetch failed: {}", err));
                state.update(|s| s.loaded = true);
            }
        }
    });
}

/// Access UI profile context (creates it if missing).
pub fn use_ui_profile_state() -> UiProfileContext {
    use_context::<UiProfileContext>().unwrap_or_else(|| {
        provide_ui_profile_context();
        use_context::<UiProfileContext>().unwrap_or_else(|| RwSignal::new(UiProfileState::new()))
    })
}

/// Effective UI profile (runtime config overridden by local settings when set).
pub fn use_ui_profile() -> Signal<UiProfile> {
    let state = use_ui_profile_state();
    let settings = use_settings();

    Signal::derive(move || {
        let settings = settings.try_get().unwrap_or_default();
        if let Some(override_profile) = settings.ui_profile {
            override_profile
        } else if let Some(runtime_profile) = state.try_get().and_then(|s| s.runtime_profile) {
            runtime_profile
        } else {
            UiProfile::Primary
        }
    })
}
