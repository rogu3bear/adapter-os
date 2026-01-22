//! Global modal management context
//!
//! Provides centralized modal state management to avoid prop drilling
//! and enable showing modals from anywhere in the component tree.

use leptos::prelude::*;

use std::sync::Arc;

/// Modal identifiers for predefined modals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModalId {
    /// Confirm dialog
    Confirm,
    /// Create adapter
    CreateAdapter,
    /// Create stack
    CreateStack,
    /// Create training job
    CreateTrainingJob,
    /// Register repository
    RegisterRepository,
    /// Custom modal with arbitrary ID
    Custom(String),
}

impl ModalId {
    pub fn custom(id: impl Into<String>) -> Self {
        Self::Custom(id.into())
    }
}

/// Modal configuration for confirm dialogs
#[derive(Debug, Clone)]
pub struct ConfirmConfig {
    pub title: String,
    pub message: String,
    pub confirm_text: String,
    pub cancel_text: String,
    pub destructive: bool,
}

impl Default for ConfirmConfig {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: "Are you sure?".to_string(),
            confirm_text: "Confirm".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: false,
        }
    }
}

impl ConfirmConfig {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.confirm_text = text.into();
        self
    }

    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel_text = text.into();
        self
    }
}

/// Active modal state
#[derive(Clone, Default)]
pub struct ModalState {
    /// Currently open modal, if any
    pub active: Option<ModalId>,
    /// Confirm dialog config when active modal is Confirm
    pub confirm_config: Option<ConfirmConfig>,
    /// Callback to run when confirm is clicked
    confirm_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for ModalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModalState")
            .field("active", &self.active)
            .field("confirm_config", &self.confirm_config)
            .field(
                "confirm_callback",
                &self.confirm_callback.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

impl ModalState {
    pub fn is_open(&self, modal_id: &ModalId) -> bool {
        self.active.as_ref() == Some(modal_id)
    }

    pub fn is_any_open(&self) -> bool {
        self.active.is_some()
    }
}

/// Modal action helper
#[derive(Clone)]
pub struct ModalAction {
    state: RwSignal<ModalState>,
}

impl ModalAction {
    pub fn new(state: RwSignal<ModalState>) -> Self {
        Self { state }
    }

    /// Open a modal by ID
    pub fn open(&self, modal_id: ModalId) {
        self.state.update(|state| {
            state.active = Some(modal_id);
            state.confirm_config = None;
            state.confirm_callback = None;
        });
    }

    /// Close the active modal
    pub fn close(&self) {
        self.state.update(|state| {
            state.active = None;
            state.confirm_config = None;
            state.confirm_callback = None;
        });
    }

    /// Open a confirm dialog with a callback
    pub fn confirm<F>(&self, config: ConfirmConfig, on_confirm: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.state.update(|state| {
            state.active = Some(ModalId::Confirm);
            state.confirm_config = Some(config);
            state.confirm_callback = Some(Arc::new(on_confirm));
        });
    }

    /// Execute the confirm callback and close
    pub fn do_confirm(&self) {
        let callback = self.state.get_untracked().confirm_callback.clone();
        if let Some(cb) = callback {
            cb();
        }
        self.close();
    }

    /// Quick confirm dialog for destructive action
    pub fn confirm_destructive<F>(&self, title: &str, message: &str, on_confirm: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.confirm(
            ConfirmConfig::new(title, message)
                .destructive()
                .confirm_text("Delete"),
            on_confirm,
        );
    }

    /// Check if a specific modal is open
    pub fn is_open(&self, modal_id: &ModalId) -> bool {
        self.state.get_untracked().is_open(modal_id)
    }
}

/// Modal context type
pub type ModalContext = (ReadSignal<ModalState>, ModalAction);

/// Provide modal context at the app root
pub fn provide_modal_context() {
    let state = RwSignal::new(ModalState::default());
    let action = ModalAction::new(state);
    provide_context((state.read_only(), action));
}

/// Use modal context - panics if not provided
pub fn use_modal_context() -> ModalContext {
    expect_context::<ModalContext>()
}

/// Get the modal action for opening/closing modals
pub fn use_modal() -> ModalAction {
    use_modal_context().1
}

/// Get read-only modal state
pub fn use_modal_state() -> ReadSignal<ModalState> {
    use_modal_context().0
}

/// Create a signal that indicates if a specific modal is open
pub fn use_is_modal_open(modal_id: ModalId) -> Signal<bool> {
    let state = use_modal_state();
    Signal::derive(move || state.get().is_open(&modal_id))
}
