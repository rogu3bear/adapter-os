//! Design tokens and theme constants for adapterOS UI
//!
//! This module provides typed design tokens that match the original Tailwind theme.
//! Colors use CSS custom properties for dark mode support.

/// CSS custom property references for colors
/// These reference the CSS variables defined in base.css
pub mod colors {
    // Semantic colors (reference CSS variables for dark mode support)
    pub const BACKGROUND: &str = "var(--color-background)";
    pub const FOREGROUND: &str = "var(--color-foreground)";
    pub const CARD: &str = "var(--color-card)";
    pub const CARD_FOREGROUND: &str = "var(--color-card-foreground)";
    pub const POPOVER: &str = "var(--color-popover)";
    pub const POPOVER_FOREGROUND: &str = "var(--color-popover-foreground)";
    pub const PRIMARY: &str = "var(--color-primary)";
    pub const PRIMARY_FOREGROUND: &str = "var(--color-primary-foreground)";
    pub const SECONDARY: &str = "var(--color-secondary)";
    pub const SECONDARY_FOREGROUND: &str = "var(--color-secondary-foreground)";
    pub const MUTED: &str = "var(--color-muted)";
    pub const MUTED_FOREGROUND: &str = "var(--color-muted-foreground)";
    pub const ACCENT: &str = "var(--color-accent)";
    pub const ACCENT_FOREGROUND: &str = "var(--color-accent-foreground)";
    pub const DESTRUCTIVE: &str = "var(--color-destructive)";
    pub const DESTRUCTIVE_FOREGROUND: &str = "var(--color-destructive-foreground)";
    pub const BORDER: &str = "var(--color-border)";
    pub const INPUT: &str = "var(--color-input)";
    pub const RING: &str = "var(--color-ring)";

    // Status colors (semantic, auto-adapt to dark mode via CSS variables)
    pub const STATUS_INFO: &str = "var(--color-status-info)";
    pub const STATUS_SUCCESS: &str = "var(--color-status-success)";
    pub const STATUS_WARNING: &str = "var(--color-status-warning)";
    pub const STATUS_ERROR: &str = "var(--color-status-error)";

    // Legacy raw color values (prefer STATUS_* for new code)
    pub const GREEN_500: &str = "rgb(34 197 94)";
    pub const YELLOW_500: &str = "rgb(234 179 8)";
    pub const RED_500: &str = "rgb(239 68 68)";
    pub const BLUE_500: &str = "rgb(59 130 246)";
    pub const PURPLE_500: &str = "rgb(168 85 247)";
    pub const ORANGE_500: &str = "rgb(249 115 22)";
}

/// Spacing scale (matches Tailwind's 0.25rem base)
pub mod spacing {
    pub const S0: &str = "0";
    pub const S0_5: &str = "0.125rem"; // 2px
    pub const S1: &str = "0.25rem"; // 4px
    pub const S1_5: &str = "0.375rem"; // 6px
    pub const S2: &str = "0.5rem"; // 8px
    pub const S2_5: &str = "0.625rem"; // 10px
    pub const S3: &str = "0.75rem"; // 12px
    pub const S3_5: &str = "0.875rem"; // 14px
    pub const S4: &str = "1rem"; // 16px
    pub const S5: &str = "1.25rem"; // 20px
    pub const S6: &str = "1.5rem"; // 24px
    pub const S7: &str = "1.75rem"; // 28px
    pub const S8: &str = "2rem"; // 32px
    pub const S9: &str = "2.25rem"; // 36px
    pub const S10: &str = "2.5rem"; // 40px
    pub const S11: &str = "2.75rem"; // 44px
    pub const S12: &str = "3rem"; // 48px
    pub const S14: &str = "3.5rem"; // 56px
    pub const S16: &str = "4rem"; // 64px
    pub const S20: &str = "5rem"; // 80px
    pub const S24: &str = "6rem"; // 96px
}

/// Border radius values
pub mod radius {
    pub const NONE: &str = "0";
    pub const SM: &str = "calc(var(--radius) - 4px)";
    pub const DEFAULT: &str = "calc(var(--radius) - 2px)";
    pub const MD: &str = "calc(var(--radius) - 2px)";
    pub const LG: &str = "var(--radius)";
    pub const XL: &str = "calc(var(--radius) + 4px)";
    pub const XXL: &str = "calc(var(--radius) + 8px)";
    pub const FULL: &str = "9999px";
}

/// Font sizes
pub mod font_size {
    pub const XS: &str = "0.75rem"; // 12px
    pub const SM: &str = "0.875rem"; // 14px
    pub const BASE: &str = "1rem"; // 16px
    pub const LG: &str = "1.125rem"; // 18px
    pub const XL: &str = "1.25rem"; // 20px
    pub const XXL: &str = "1.5rem"; // 24px
    pub const XXXL: &str = "1.875rem"; // 30px
}

/// Font weights
pub mod font_weight {
    pub const NORMAL: &str = "400";
    pub const MEDIUM: &str = "500";
    pub const SEMIBOLD: &str = "600";
    pub const BOLD: &str = "700";
}

/// Line heights
pub mod line_height {
    pub const NONE: &str = "1";
    pub const TIGHT: &str = "1.25";
    pub const SNUG: &str = "1.375";
    pub const NORMAL: &str = "1.5";
    pub const RELAXED: &str = "1.625";
    pub const LOOSE: &str = "2";
}

/// Shadows
pub mod shadow {
    pub const SM: &str = "0 1px 2px 0 rgb(0 0 0 / 0.05)";
    pub const DEFAULT: &str = "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)";
    pub const MD: &str = "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)";
    pub const LG: &str = "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)";
    pub const XL: &str = "0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)";
}

/// Transitions
pub mod transition {
    pub const COLORS: &str =
        "color, background-color, border-color, text-decoration-color, fill, stroke";
    pub const ALL: &str = "all";
    pub const OPACITY: &str = "opacity";
    pub const TRANSFORM: &str = "transform";
    pub const DURATION_150: &str = "150ms";
    pub const DURATION_200: &str = "200ms";
    pub const DURATION_300: &str = "300ms";

    /// Default easing function (CSS variable for consistency)
    pub const EASE_DEFAULT: &str = "var(--ease-default)";
    /// Legacy: direct cubic-bezier value (prefer EASE_DEFAULT for new code)
    pub const EASE_IN_OUT: &str = "cubic-bezier(0.4, 0, 0.2, 1)";
}

/// Z-index layers
pub mod z_index {
    pub const AUTO: &str = "auto";
    pub const Z0: &str = "0";
    pub const Z10: &str = "10";
    pub const Z20: &str = "20";
    pub const Z30: &str = "30";
    pub const Z40: &str = "40";
    pub const Z50: &str = "50";
}

/// Glass morphism design tokens (PRD-UI-100)
///
/// CSS custom property references for the liquid glass theme.
/// These reference the CSS variables defined in glass.css.
pub mod glass {
    // Glass background layers
    pub const BG_1: &str = "var(--glass-bg-1)";
    pub const BG_2: &str = "var(--glass-bg-2)";
    pub const BG_3: &str = "var(--glass-bg-3)";

    // Glass border
    pub const BORDER: &str = "var(--glass-border)";

    // Glass shadows
    pub const SHADOW_SM: &str = "var(--glass-shadow-sm)";
    pub const SHADOW_MD: &str = "var(--glass-shadow-md)";
    pub const SHADOW_LG: &str = "var(--glass-shadow-lg)";

    // Glass effects
    pub const BLUR: &str = "var(--glass-blur)";
    pub const SATURATION: &str = "var(--glass-saturation)";
    pub const NOISE_OPACITY: &str = "var(--glass-noise-opacity)";
    pub const HIGHLIGHT: &str = "var(--glass-highlight)";
    pub const GLOW: &str = "var(--glass-glow)";

    // Elevation blur multipliers
    pub const BLUR_1: &str = "var(--glass-blur-1)";
    pub const BLUR_2: &str = "var(--glass-blur-2)";
    pub const BLUR_3: &str = "var(--glass-blur-3)";
}

/// Letter spacing tokens
pub mod letter_spacing {
    pub const TIGHTER: &str = "var(--tracking-tighter)";
    pub const TIGHT: &str = "var(--tracking-tight)";
    pub const NORMAL: &str = "var(--tracking-normal)";
    pub const WIDE: &str = "var(--tracking-wide)";
    pub const WIDER: &str = "var(--tracking-wider)";
}

/// Font family tokens
pub mod font_family {
    pub const SANS: &str = "var(--font-sans)";
    pub const MONO: &str = "var(--font-mono)";
}

/// Animation duration tokens
pub mod animation {
    pub const DURATION_FAST: &str = "150ms";
    pub const DURATION_NORMAL: &str = "200ms";
    pub const DURATION_SLOW: &str = "300ms";
    pub const SHIMMER_DURATION: &str = "1.5s";
    pub const STAGGER_DELAY: &str = "50ms";
}

/// Visual emphasis levels for content hierarchy
pub mod emphasis {
    pub const HIGH: &str = "1";
    pub const MEDIUM: &str = "0.85";
    pub const LOW: &str = "0.6";
    pub const DISABLED: &str = "0.38";
}
