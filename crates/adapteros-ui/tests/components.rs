//! Component unit tests
//!
//! Tests for UI components using WASM test harness.
//! Run with: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================================
// Breadcrumb Component Tests
// ============================================================================

mod breadcrumb_tests {
    use super::*;
    use adapteros_ui::components::{humanize_segment, BreadcrumbItem};

    #[wasm_bindgen_test]
    fn test_breadcrumb_item_link() {
        let item = BreadcrumbItem::link("Adapters", "/adapters");
        assert_eq!(item.label, "Adapters");
        assert_eq!(item.href, Some("/adapters".to_string()));
    }

    #[wasm_bindgen_test]
    fn test_breadcrumb_item_current() {
        let item = BreadcrumbItem::current("My Adapter");
        assert_eq!(item.label, "My Adapter");
        assert!(item.href.is_none());
    }
}

// ============================================================================
// Button Component Tests
// ============================================================================

mod button_tests {
    use super::*;
    use adapteros_ui::components::{ButtonSize, ButtonVariant};

    #[wasm_bindgen_test]
    fn test_button_variant_default() {
        // Verify variants are constructible
        let _primary = ButtonVariant::Primary;
        let _secondary = ButtonVariant::Secondary;
        let _ghost = ButtonVariant::Ghost;
        let _danger = ButtonVariant::Danger;
    }

    #[wasm_bindgen_test]
    fn test_button_size_variants() {
        let _sm = ButtonSize::Sm;
        let _md = ButtonSize::Md;
        let _lg = ButtonSize::Lg;
    }
}

// ============================================================================
// Badge Component Tests
// ============================================================================

mod badge_tests {
    use super::*;
    use adapteros_ui::components::BadgeVariant;

    #[wasm_bindgen_test]
    fn test_badge_variants() {
        let _default = BadgeVariant::Default;
        let _success = BadgeVariant::Success;
        let _warning = BadgeVariant::Warning;
        let _error = BadgeVariant::Error;
        let _info = BadgeVariant::Info;
    }
}

// ============================================================================
// Status Indicator Tests
// ============================================================================

mod status_tests {
    use super::*;
    use adapteros_ui::components::StatusColor;

    #[wasm_bindgen_test]
    fn test_status_colors() {
        let _success = StatusColor::Success;
        let _warning = StatusColor::Warning;
        let _error = StatusColor::Error;
        let _info = StatusColor::Info;
        let _neutral = StatusColor::Neutral;
    }
}

// ============================================================================
// Empty State Tests
// ============================================================================

mod empty_state_tests {
    use super::*;
    use adapteros_ui::components::EmptyStateVariant;

    #[wasm_bindgen_test]
    fn test_empty_state_variants() {
        let _search = EmptyStateVariant::Search;
        let _data = EmptyStateVariant::Data;
        let _error = EmptyStateVariant::Error;
    }
}

// ============================================================================
// Dialog Component Tests
// ============================================================================

mod dialog_tests {
    use super::*;
    use adapteros_ui::components::DialogSize;

    #[wasm_bindgen_test]
    fn test_dialog_sizes() {
        let _sm = DialogSize::Sm;
        let _md = DialogSize::Md;
        let _lg = DialogSize::Lg;
        let _xl = DialogSize::Xl;
    }
}

// ============================================================================
// Workspace Layout Tests
// ============================================================================

mod workspace_tests {
    use super::*;
    use adapteros_ui::components::TwoColumnRatio;

    #[wasm_bindgen_test]
    fn test_two_column_ratios() {
        let _equal = TwoColumnRatio::Equal;
        let _wide_left = TwoColumnRatio::WideLeft;
        let _wide_right = TwoColumnRatio::WideRight;
    }
}

// ============================================================================
// Split Panel Tests
// ============================================================================

mod split_panel_tests {
    use super::*;
    use adapteros_ui::components::{SplitMode, SplitRatio};

    #[wasm_bindgen_test]
    fn test_split_modes() {
        let _horizontal = SplitMode::Horizontal;
        let _vertical = SplitMode::Vertical;
    }

    #[wasm_bindgen_test]
    fn test_split_ratios() {
        let _equal = SplitRatio::Equal;
        let _large_left = SplitRatio::LargeLeft;
        let _large_right = SplitRatio::LargeRight;
    }
}
