//! Reusable delete confirmation dialog state hook
//!
//! Provides a standard pattern for delete confirmation dialogs with:
//! - Show/hide dialog state
//! - Pending item tracking (id + display name)
//! - Loading/deleting state
//! - Error handling
//!
//! # Example
//!
//! ```rust,ignore
//! let delete_state = use_delete_dialog();
//!
//! // Trigger delete confirmation
//! delete_state.confirm("item-123".to_string(), "My Item".to_string());
//!
//! // In the delete handler
//! let on_confirm = {
//!     let delete_state = delete_state.clone();
//!     Callback::new(move |_| {
//!         if let Some(id) = delete_state.pending_id.get() {
//!             delete_state.start_delete();
//!             // ... perform async delete ...
//!             delete_state.finish_delete(Ok(()));
//!         }
//!     })
//! };
//!
//! // In the view
//! <ConfirmationDialog
//!     open=delete_state.show
//!     loading=Signal::derive(move || delete_state.deleting.get())
//!     on_confirm=on_confirm
//!     on_cancel=Callback::new(move |_| delete_state.cancel())
//! />
//! ```

use leptos::prelude::*;

/// State for a delete confirmation dialog
#[derive(Clone)]
pub struct DeleteDialogState {
    /// Whether the dialog is visible
    pub show: RwSignal<bool>,
    /// ID of the item pending deletion
    pub pending_id: RwSignal<Option<String>>,
    /// Display name of the item pending deletion (for UI feedback)
    pub pending_name: RwSignal<String>,
    /// Whether a delete operation is in progress
    pub deleting: RwSignal<bool>,
    /// Error message from the last delete attempt
    pub error: RwSignal<Option<String>>,
}

impl DeleteDialogState {
    /// Create a new delete dialog state with all signals initialized
    pub fn new() -> Self {
        Self {
            show: RwSignal::new(false),
            pending_id: RwSignal::new(None),
            pending_name: RwSignal::new(String::new()),
            deleting: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    /// Reset the dialog state (clear pending item and error, but don't hide dialog)
    pub fn reset(&self) {
        self.pending_id.set(None);
        self.pending_name.set(String::new());
        self.error.set(None);
    }

    /// Request confirmation for deleting an item
    ///
    /// This shows the dialog and sets the pending item details.
    pub fn confirm(&self, id: String, name: String) {
        self.pending_id.set(Some(id));
        self.pending_name.set(name);
        self.error.set(None);
        self.show.set(true);
    }

    /// Cancel the delete operation
    ///
    /// Hides the dialog and resets state.
    pub fn cancel(&self) {
        self.show.set(false);
        self.reset();
    }

    /// Mark delete operation as started
    pub fn start_delete(&self) {
        self.deleting.set(true);
        self.error.set(None);
    }

    /// Complete the delete operation
    ///
    /// On success: hides dialog and resets state
    /// On error: shows error message and stops loading
    pub fn finish_delete(&self, result: Result<(), String>) {
        self.deleting.set(false);
        match result {
            Ok(()) => {
                self.show.set(false);
                self.reset();
            }
            Err(e) => {
                self.error.set(Some(e));
            }
        }
    }

    /// Get the pending item ID if one exists
    pub fn get_pending_id(&self) -> Option<String> {
        self.pending_id.get()
    }

    /// Get the pending item name
    pub fn get_pending_name(&self) -> String {
        self.pending_name.get()
    }

    /// Check if the dialog is currently showing
    pub fn is_showing(&self) -> bool {
        self.show.get()
    }

    /// Check if a delete operation is in progress
    pub fn is_deleting(&self) -> bool {
        self.deleting.get()
    }

    /// Get the current error message, if any
    pub fn get_error(&self) -> Option<String> {
        self.error.get()
    }
}

impl Default for DeleteDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new delete dialog state
///
/// This hook provides standardized state management for delete confirmation dialogs.
/// It handles the common pattern of:
/// 1. Showing a confirmation dialog
/// 2. Tracking which item is pending deletion
/// 3. Managing loading state during async delete
/// 4. Handling success/error outcomes
///
/// # Example
///
/// ```rust,ignore
/// let delete_state = use_delete_dialog();
///
/// // Request deletion confirmation
/// delete_state.confirm("item-id".to_string(), "Item Name".to_string());
///
/// // Handle confirmation
/// let on_confirm = {
///     let state = delete_state.clone();
///     Callback::new(move |_| {
///         if let Some(id) = state.get_pending_id() {
///             state.start_delete();
///             // ... async delete logic ...
///         }
///     })
/// };
///
/// // In view
/// <ConfirmationDialog
///     open=delete_state.show
///     loading=Signal::derive(move || delete_state.is_deleting())
///     on_confirm=on_confirm
///     on_cancel=Callback::new(move |_| delete_state.cancel())
/// />
/// ```
pub fn use_delete_dialog() -> DeleteDialogState {
    DeleteDialogState::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_dialog_state_new() {
        // Can't test signals in native tests, but we can verify the struct builds
        let _ = DeleteDialogState::default;
    }
}
