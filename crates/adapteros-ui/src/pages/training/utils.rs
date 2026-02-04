//! Training page utility functions
//!
//! Pure helper functions for formatting dates, numbers, and durations.

use adapteros_api_types::TrainingBackendKind;

// Re-export from canonical utils for backward compatibility
pub use crate::utils::format_datetime as format_date;

/// Format a large number with commas
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format duration in milliseconds to human readable
pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;

    if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

/// Format a backend string for display with proper capitalization.
///
/// Converts backend identifiers (e.g., "coreml", "mlx") to display-friendly
/// names (e.g., "CoreML", "MLX"). Returns the original string if unrecognized.
pub fn format_backend(backend: &str) -> String {
    // Match against known backend kinds using enum as source of truth
    if backend == TrainingBackendKind::CoreML.as_str() {
        "CoreML".to_string()
    } else if backend == TrainingBackendKind::Mlx.as_str() {
        "MLX".to_string()
    } else if backend == TrainingBackendKind::Metal.as_str() {
        "Metal".to_string()
    } else if backend == TrainingBackendKind::Cpu.as_str() {
        "CPU".to_string()
    } else if backend == TrainingBackendKind::Auto.as_str() {
        "Auto".to_string()
    } else {
        // Return original for unknown backends (graceful degradation)
        backend.to_string()
    }
}

/// Format an optional backend string, with a default for None.
pub fn format_backend_or(backend: Option<&str>, default: &str) -> String {
    match backend {
        Some(b) => format_backend(b),
        None => default.to_string(),
    }
}

/// Human-friendly label for preprocessing cache state.
#[allow(dead_code)] // Used by training preprocessing cache UI; keep for upcoming panels.
pub fn preprocess_state_label(cache_hit: bool, needs_reprocess: bool) -> &'static str {
    if needs_reprocess {
        "needs reprocess"
    } else if cache_hit {
        "cache hit"
    } else {
        "cache pending"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_label_reports_reprocess_first() {
        assert_eq!(preprocess_state_label(false, true), "needs reprocess");
        assert_eq!(preprocess_state_label(true, true), "needs reprocess");
    }

    #[test]
    fn preprocess_label_reports_cache_hit() {
        assert_eq!(preprocess_state_label(true, false), "cache hit");
        assert_eq!(preprocess_state_label(false, false), "cache pending");
    }

    #[test]
    fn format_backend_capitalizes_known_backends() {
        assert_eq!(format_backend("coreml"), "CoreML");
        assert_eq!(format_backend("mlx"), "MLX");
        assert_eq!(format_backend("metal"), "Metal");
        assert_eq!(format_backend("cpu"), "CPU");
        assert_eq!(format_backend("auto"), "Auto");
    }

    #[test]
    fn format_backend_preserves_unknown() {
        assert_eq!(format_backend("unknown"), "unknown");
        assert_eq!(format_backend("CustomBackend"), "CustomBackend");
    }

    #[test]
    fn format_backend_or_uses_default_for_none() {
        assert_eq!(format_backend_or(Some("mlx"), "N/A"), "MLX");
        assert_eq!(format_backend_or(None, "Pending"), "Pending");
        assert_eq!(format_backend_or(None, "Not specified"), "Not specified");
    }
}
