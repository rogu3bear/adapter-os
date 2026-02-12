//! Hash display component.
//!
//! Thin wrapper around [`CopyableId`] for BLAKE3 hash display.
//! Shows `b3:{8 hex}` via [`adapteros_id::format_hash_short`],
//! with full hash available in tooltip and clipboard.

use leptos::prelude::*;

use crate::components::CopyableId;

/// Displays a BLAKE3 hash in short form (`b3:abcd1234`) with copy-to-clipboard.
///
/// The full hash is preserved in the tooltip and clipboard; only the display
/// is shortened via `format_hash_short`.
#[component]
pub fn HashDisplay(
    /// Full hash string (stored in clipboard on copy).
    hash: String,
    /// Optional label shown above the hash.
    #[prop(optional)]
    label: String,
) -> impl IntoView {
    let formatted = adapteros_id::format_hash_short(&hash);

    view! {
        <CopyableId id=hash display_name=formatted label=label />
    }
}
