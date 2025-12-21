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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar() {
        assert_eq!(create_progress_bar(0.5, 10), "█████░░░░░");
        assert_eq!(create_progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(create_progress_bar(1.0, 10), "██████████");
    }
}
