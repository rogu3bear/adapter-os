//! Centralized icon components for adapterOS UI.
//!
//! All icons use Heroicons (MIT licensed) with consistent styling:
//! - Default size: 1rem (h-4 w-4)
//! - Stroke-based rendering with currentColor
//! - Configurable via `class` prop for size overrides
//! - Accessible by default: `aria-hidden="true"` for decorative icons
//!
//! # Usage
//! ```rust
//! use crate::components::icons::*;
//!
//! view! {
//!     <IconCheck />                           // Default size, decorative (aria-hidden)
//!     <IconCheck class="h-5 w-5" />          // Custom size
//!     <IconRefresh class="h-6 w-6 text-primary" /> // Custom size + color
//!     <IconWarning aria_label="Warning" />   // Meaningful icon (role="img", aria-label)
//! }
//! ```
//!
//! # Accessibility
//! By default, icons are decorative (`aria-hidden="true"`). To make an icon
//! meaningful for screen readers, pass an `aria_label` prop which:
//! - Removes `aria-hidden`
//! - Adds `role="img"`
//! - Sets `aria-label` to the provided value

use leptos::prelude::*;

// =============================================================================
// Core Icon Infrastructure
// =============================================================================

/// Base icon wrapper to standardize SVG boilerplate and default sizing.
///
/// By default, icons are decorative (`aria-hidden="true"`). To make an icon
/// meaningful for screen readers, pass an `aria_label` which adds
/// `role="img"` and removes `aria-hidden`.
#[component]
pub fn BaseIcon(
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// SVG inner content (paths, circles, etc.)
    children: Children,
    /// SVG viewBox (defaults to "0 0 24 24")
    #[prop(optional, default = "0 0 24 24")]
    view_box: &'static str,
    /// SVG fill (defaults to "none")
    #[prop(optional, default = "none")]
    fill: &'static str,
    /// SVG stroke width (defaults to 2)
    #[prop(optional, default = 2)]
    stroke_width: u32,
    /// Optional accessible label. When provided (non-empty), the icon becomes meaningful
    /// (role="img", aria-label set). When absent/empty, icon is decorative (aria-hidden="true").
    #[prop(optional, into)]
    aria_label: String,
) -> impl IntoView {
    let class = if class.is_empty() {
        "h-4 w-4".to_string()
    } else {
        class
    };

    // Determine accessibility attributes based on whether a label is provided
    let has_label = !aria_label.is_empty();

    view! {
        <svg
            class=class
            xmlns="http://www.w3.org/2000/svg"
            fill=fill
            viewBox=view_box
            stroke="currentColor"
            stroke-width=stroke_width.to_string()
            aria-hidden=move || (!has_label).then_some("true")
            role=move || has_label.then_some("img")
            aria-label=move || has_label.then(|| aria_label.clone())
        >
            {children()}
        </svg>
    }
}

// =============================================================================
// Action Icons
// =============================================================================

/// Checkmark icon - for success, completion, selection
#[component]
pub fn IconCheck(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <polyline points="20 6 9 17 4 12" stroke-linecap="round" stroke-linejoin="round"/>
        </BaseIcon>
    }
}

/// Checkmark in circle - for confirmed/verified states
#[component]
pub fn IconCheckCircle(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </BaseIcon>
    }
}

/// X/Close icon - for closing, canceling, errors
#[component]
pub fn IconX(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <line x1="18" y1="6" x2="6" y2="18" stroke-linecap="round" stroke-linejoin="round"/>
            <line x1="6" y1="6" x2="18" y2="18" stroke-linecap="round" stroke-linejoin="round"/>
        </BaseIcon>
    }
}

/// Plus icon - for adding, creating
#[component]
pub fn IconPlus(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
        </BaseIcon>
    }
}

/// Minus icon - for removing, decreasing
#[component]
pub fn IconMinus(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M20 12H4"/>
        </BaseIcon>
    }
}

/// Refresh/reload icon - for refreshing, syncing
#[component]
pub fn IconRefresh(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
        </BaseIcon>
    }
}

// =============================================================================
// Navigation Icons
// =============================================================================

/// Chevron down - for dropdowns, expand
#[component]
pub fn IconChevronDown(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
        </BaseIcon>
    }
}

/// Chevron up - for collapse, scroll up
#[component]
pub fn IconChevronUp(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 15l7-7 7 7"/>
        </BaseIcon>
    }
}

/// Chevron left - for back, previous
#[component]
pub fn IconChevronLeft(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/>
        </BaseIcon>
    }
}

/// Chevron right - for forward, next
#[component]
pub fn IconChevronRight(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7"/>
        </BaseIcon>
    }
}

/// Arrow left - for back navigation with line
#[component]
pub fn IconArrowLeft(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 19l-7-7m0 0l7-7m-7 7h18"/>
        </BaseIcon>
    }
}

// =============================================================================
// Status & Feedback Icons
// =============================================================================

/// Warning triangle - for warnings, cautions
#[component]
pub fn IconWarning(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
        </BaseIcon>
    }
}

/// Info circle - for information, help
#[component]
pub fn IconInfo(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </BaseIcon>
    }
}

/// Error/X in circle - for errors, failures
#[component]
pub fn IconError(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </BaseIcon>
    }
}

// =============================================================================
// Visibility Icons
// =============================================================================

/// Eye icon - for showing, visible state
#[component]
pub fn IconEye(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
            <circle cx="12" cy="12" r="3"/>
        </BaseIcon>
    }
}

/// Eye off icon - for hiding, hidden state
#[component]
pub fn IconEyeOff(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/>
            <line x1="1" y1="1" x2="23" y2="23"/>
        </BaseIcon>
    }
}

// =============================================================================
// Media Control Icons
// =============================================================================

/// Pause icon with circle - for pausing playback/processes
#[component]
pub fn IconPause(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 9v6m4-6v6m7-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </BaseIcon>
    }
}

/// Stop icon with circle - for stopping processes
#[component]
pub fn IconStop(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z"/>
        </BaseIcon>
    }
}

/// Play icon - for starting playback/processes
#[component]
pub fn IconPlay(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"/>
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </BaseIcon>
    }
}

// =============================================================================
// System & Infrastructure Icons
// =============================================================================

/// Server/computer icon - for workers, nodes
#[component]
pub fn IconServer(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/>
        </BaseIcon>
    }
}

/// Cog/gear icon - for settings
#[component]
pub fn IconCog(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
        </BaseIcon>
    }
}

/// Document/file icon
#[component]
pub fn IconDocument(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
        </BaseIcon>
    }
}

/// Folder icon
#[component]
pub fn IconFolder(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
        </BaseIcon>
    }
}

/// Search/magnifying glass icon
#[component]
pub fn IconSearch(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
        </BaseIcon>
    }
}

// =============================================================================
// Loading/Spinner Icon
// =============================================================================

/// Animated spinner icon - for loading states
#[component]
pub fn IconSpinner(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    let class = if class.is_empty() {
        "h-4 w-4 animate-spin".to_string()
    } else {
        format!("{} animate-spin", class)
    };
    let has_label = !aria_label.is_empty();
    view! {
        <svg class=class xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" aria-hidden=move || (!has_label).then_some("true") role=move || has_label.then_some("img") aria-label=move || has_label.then(|| aria_label.clone())>
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"/>
        </svg>
    }
}

// =============================================================================
// Misc UI Icons
// =============================================================================

/// External link icon
#[component]
pub fn IconExternalLink(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/>
        </BaseIcon>
    }
}

/// Copy/clipboard icon
#[component]
pub fn IconCopy(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"/>
        </BaseIcon>
    }
}

/// Trash/delete icon
#[component]
pub fn IconTrash(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
        </BaseIcon>
    }
}

/// Edit/pencil icon
#[component]
pub fn IconEdit(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
        </BaseIcon>
    }
}

/// Menu/hamburger icon
#[component]
pub fn IconMenu(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16"/>
        </BaseIcon>
    }
}

/// More/dots horizontal icon
#[component]
pub fn IconDotsHorizontal(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 12h.01M12 12h.01M19 12h.01M6 12a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0z"/>
        </BaseIcon>
    }
}

/// More/dots vertical icon
#[component]
pub fn IconDotsVertical(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 5v.01M12 12v.01M12 19v.01M12 6a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2z"/>
        </BaseIcon>
    }
}

/// Home icon
#[component]
pub fn IconHome(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6"/>
        </BaseIcon>
    }
}

/// User icon
#[component]
pub fn IconUser(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
        </BaseIcon>
    }
}

/// Logout icon
#[component]
pub fn IconLogout(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    view! {
        <BaseIcon class=class aria_label=aria_label>
            <path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"/>
        </BaseIcon>
    }
}
