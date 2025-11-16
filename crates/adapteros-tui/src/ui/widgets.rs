use ratatui::{
    style::{Color, Style},
    text::Span,
};

/// Creates a progress bar without emojis, using only box-drawing characters
/// Returns a string representing the progress bar
pub fn create_progress_bar(percentage: f32, width: usize) -> String {
    let filled = (percentage * width as f32) as usize;
    let empty = width.saturating_sub(filled);

    let mut bar = String::new();

    // Use full blocks for filled portion
    for _ in 0..filled {
        bar.push('█');
    }

    // Use light shade for empty portion
    for _ in 0..empty {
        bar.push('░');
    }

    bar
}

/// Creates a gradient progress bar with color transitions
/// Returns a vector of styled spans for rendering
pub fn create_gradient_progress_bar(percentage: f32, width: usize) -> Vec<Span<'static>> {
    let filled = (percentage * width as f32) as usize;
    let empty = width.saturating_sub(filled);

    let mut spans = Vec::new();

    // Determine colors based on percentage
    let color = if percentage >= 0.85 {
        Color::Red
    } else if percentage >= 0.70 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Create the filled portion with color
    let mut filled_str = String::new();
    for _ in 0..filled {
        filled_str.push('█');
    }
    spans.push(Span::styled(filled_str, Style::default().fg(color)));

    // Create the empty portion
    let mut empty_str = String::new();
    for _ in 0..empty {
        empty_str.push('░');
    }
    spans.push(Span::styled(empty_str, Style::default().fg(Color::Gray)));

    spans
}

/// Creates a simple text-based progress indicator for loading states
pub fn create_loading_spinner(frame: usize) -> &'static str {
    const SPINNERS: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    SPINNERS[frame % SPINNERS.len()]
}

/// Format bytes into human-readable format
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let base = 1024_f64;
    let exponent = (bytes as f64).ln() / base.ln();
    let exponent = exponent.floor() as usize;

    if exponent >= UNITS.len() {
        return format!("{} {}", bytes, UNITS[UNITS.len() - 1]);
    }

    let value = bytes as f64 / base.powi(exponent as i32);

    if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[exponent])
    } else if value >= 10.0 {
        format!("{:.1} {}", value, UNITS[exponent])
    } else {
        format!("{:.2} {}", value, UNITS[exponent])
    }
}

/// Format duration into human-readable format
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Create an aligned table row with fixed column widths
pub fn create_aligned_row(columns: &[(&str, usize)]) -> String {
    columns
        .iter()
        .map(|(text, width)| {
            if text.len() > *width {
                format!("{:.*}", width, text)
            } else {
                format!("{:<width$}", text, width = width)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar() {
        assert_eq!(create_progress_bar(0.5, 10), "█████░░░░░");
        assert_eq!(create_progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(create_progress_bar(1.0, 10), "██████████");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(86461), "1d 0h");
    }
}
