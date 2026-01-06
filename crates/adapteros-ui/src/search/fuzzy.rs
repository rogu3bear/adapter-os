//! Fuzzy search implementation
//!
//! A simple, WASM-compatible fuzzy matching algorithm.

/// Calculate fuzzy match score between query and target.
///
/// Returns a score from 0.0 (no match) to 1.0 (exact match).
/// Implements a simple fuzzy matching algorithm that:
/// - Prefers prefix matches
/// - Prefers consecutive character matches
/// - Prefers word boundary matches
pub fn fuzzy_score(target: &str, query: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }

    let target_lower = target.to_lowercase();
    let query_lower = query.to_lowercase();

    // Exact match
    if target_lower == query_lower {
        return 1.0;
    }

    // Prefix match (highest priority after exact)
    if target_lower.starts_with(&query_lower) {
        let ratio = query.len() as f32 / target.len() as f32;
        return 0.9 + (ratio * 0.1);
    }

    // Contains match
    if target_lower.contains(&query_lower) {
        let ratio = query.len() as f32 / target.len() as f32;
        return 0.7 + (ratio * 0.1);
    }

    // Word boundary match (camelCase, snake_case, etc.)
    if word_boundary_match(&target_lower, &query_lower) {
        return 0.6;
    }

    // Character sequence match
    if let Some(score) = character_sequence_score(&target_lower, &query_lower) {
        return score * 0.5; // Scale down sequence matches
    }

    0.0
}

/// Check if query matches word boundaries in target
fn word_boundary_match(target: &str, query: &str) -> bool {
    let mut query_chars = query.chars().peekable();
    let mut at_boundary = true;

    for c in target.chars() {
        let is_boundary = !c.is_alphanumeric();

        if at_boundary && query_chars.peek() == Some(&c) {
            query_chars.next();
            if query_chars.peek().is_none() {
                return true;
            }
        }

        at_boundary = is_boundary || c.is_uppercase();
    }

    false
}

/// Calculate score for character sequence match
fn character_sequence_score(target: &str, query: &str) -> Option<f32> {
    let target_chars: Vec<char> = target.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    if query_chars.is_empty() {
        return Some(1.0);
    }

    if query_chars.len() > target_chars.len() {
        return None;
    }

    let mut target_idx = 0;
    let mut matched = 0;
    let mut consecutive = 0;
    let mut max_consecutive = 0;

    for &qc in &query_chars {
        let mut found = false;
        while target_idx < target_chars.len() {
            if target_chars[target_idx] == qc {
                matched += 1;
                consecutive += 1;
                max_consecutive = max_consecutive.max(consecutive);
                target_idx += 1;
                found = true;
                break;
            }
            target_idx += 1;
            consecutive = 0;
        }
        if !found {
            return None;
        }
    }

    if matched == query_chars.len() {
        // Score based on:
        // - Ratio of matched characters to target length
        // - Bonus for consecutive matches
        let base_score = matched as f32 / target_chars.len() as f32;
        let consecutive_bonus = (max_consecutive as f32 / query_chars.len() as f32) * 0.3;
        Some((base_score + consecutive_bonus).min(1.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert_eq!(fuzzy_score("Dashboard", "dashboard"), 1.0);
        assert_eq!(fuzzy_score("dashboard", "Dashboard"), 1.0);
    }

    #[test]
    fn test_prefix_match() {
        let score = fuzzy_score("Dashboard", "dash");
        assert!(score > 0.9 && score < 1.0);
    }

    #[test]
    fn test_contains_match() {
        let score = fuzzy_score("Dashboard", "board");
        assert!(score > 0.7 && score < 0.9);
    }

    #[test]
    fn test_no_match() {
        assert_eq!(fuzzy_score("Dashboard", "xyz"), 0.0);
    }

    #[test]
    fn test_character_sequence() {
        let score = fuzzy_score("Dashboard", "dsh");
        assert!(score > 0.0);
    }
}
