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
    use adapteros_ui::components::BreadcrumbItem;

    #[wasm_bindgen_test]
    fn test_breadcrumb_item_link() {
        let item = BreadcrumbItem::new("Adapters", "/adapters");
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
    use adapteros_ui::components::{ButtonSize, ButtonType, ButtonVariant};

    #[wasm_bindgen_test]
    fn test_button_variant_default() {
        // Verify variants are constructible
        let _primary = ButtonVariant::Primary;
        let _secondary = ButtonVariant::Secondary;
        let _outline = ButtonVariant::Outline;
        let _ghost = ButtonVariant::Ghost;
        let _destructive = ButtonVariant::Destructive;
        let _link = ButtonVariant::Link;
    }

    #[wasm_bindgen_test]
    fn test_button_size_variants() {
        let _sm = ButtonSize::Sm;
        let _md = ButtonSize::Md;
        let _lg = ButtonSize::Lg;
    }

    #[wasm_bindgen_test]
    fn test_button_type_variants() {
        let _button = ButtonType::Button;
        let _submit = ButtonType::Submit;
        let _reset = ButtonType::Reset;
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
        let _secondary = BadgeVariant::Secondary;
        let _success = BadgeVariant::Success;
        let _warning = BadgeVariant::Warning;
        let _destructive = BadgeVariant::Destructive;
        let _outline = BadgeVariant::Outline;
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
        let _gray = StatusColor::Gray;
        let _green = StatusColor::Green;
        let _yellow = StatusColor::Yellow;
        let _red = StatusColor::Red;
        let _blue = StatusColor::Blue;
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
        let _empty = EmptyStateVariant::Empty;
        let _no_results = EmptyStateVariant::NoResults;
        let _no_permission = EmptyStateVariant::NoPermission;
        let _unavailable = EmptyStateVariant::Unavailable;
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
        let _one_two = TwoColumnRatio::OneTwo;
        let _two_one = TwoColumnRatio::TwoOne;
        let _one_one = TwoColumnRatio::OneOne;
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
        let _desktop = SplitMode::Desktop;
        let _stacked = SplitMode::Stacked;
    }

    #[wasm_bindgen_test]
    fn test_split_ratios() {
        let _half = SplitRatio::Half;
        let _third_two_thirds = SplitRatio::ThirdTwoThirds;
        let _two_fifths_three_fifths = SplitRatio::TwoFifthsThreeFifths;
    }
}
