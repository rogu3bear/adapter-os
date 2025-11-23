use std::collections::{HashMap, HashSet, VecDeque};

/// Tracks router activation percentages over a rolling window.
///
/// The tracker maintains a sliding window of recent routing decisions and
/// computes how frequently each adapter was selected. Percentages are expressed
/// as `count(selected) / count(decisions) * 100` within the window.
pub struct ActivationTracker {
    window_size: usize,
    history: VecDeque<Vec<u16>>,
    counts: HashMap<u16, usize>,
    last_percentages: HashMap<u16, f32>,
}

impl ActivationTracker {
    /// Default precision threshold for detecting percentage changes.
    const EPSILON: f32 = 0.05;

    /// Create a tracker with the given window size.
    pub fn new(window_size: usize) -> Self {
        assert!(window_size > 0, "window_size must be > 0");
        Self {
            window_size,
            history: VecDeque::with_capacity(window_size),
            counts: HashMap::new(),
            last_percentages: HashMap::new(),
        }
    }

    /// Record a routing decision and return adapters whose activation
    /// percentage changed in the rolling window.
    pub fn record_decision(&mut self, selected: &[u16]) -> Vec<(u16, f32)> {
        let unique: Vec<u16> = Self::dedup_sorted(selected);

        if unique.is_empty() {
            self.push_history(unique);
        } else {
            self.push_history(unique.clone());
            for id in &unique {
                *self.counts.entry(*id).or_insert(0) += 1;
            }
        }

        // Evict old entries when the window exceeds capacity
        while self.history.len() > self.window_size {
            if let Some(old) = self.history.pop_front() {
                for id in old {
                    if let Some(count) = self.counts.get_mut(&id) {
                        if *count > 0 {
                            *count -= 1;
                        }
                        if *count == 0 {
                            self.counts.remove(&id);
                        }
                    }
                }
            }
        }

        let decisions = self.history.len();
        let mut current: HashMap<u16, f32> = HashMap::new();
        if decisions > 0 {
            for (&id, &count) in &self.counts {
                let pct = (count as f32 / decisions as f32) * 100.0;
                current.insert(id, pct);
            }
        }

        // Determine which adapters changed.
        let mut changed = Vec::new();
        let mut keys: HashSet<u16> = self.last_percentages.keys().copied().collect();
        keys.extend(current.keys().copied());

        for id in keys {
            let pct = current.get(&id).copied().unwrap_or(0.0);
            let prev = self.last_percentages.get(&id).copied().unwrap_or(0.0);
            if (pct - prev).abs() > Self::EPSILON {
                changed.push((id, pct));
            }
        }

        // Update cached percentages (drop entries at 0%).
        self.last_percentages = current.into_iter().filter(|(_, pct)| *pct > 0.0).collect();

        changed.sort_by_key(|(id, _)| *id);
        changed
    }

    /// Get the last computed activation percentage for an adapter.
    pub fn activation_pct(&self, adapter_id: u16) -> f32 {
        self.last_percentages
            .get(&adapter_id)
            .copied()
            .unwrap_or(0.0)
    }

    /// Number of routing decisions tracked in the current window.
    pub fn decision_count(&self) -> usize {
        self.history.len()
    }

    fn dedup_sorted(values: &[u16]) -> Vec<u16> {
        let mut vec = values.to_vec();
        vec.sort_unstable();
        vec.dedup();
        vec
    }

    fn push_history(&mut self, entry: Vec<u16>) {
        self.history.push_back(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_activation_percentages() {
        let mut tracker = ActivationTracker::new(4);

        let changes = tracker.record_decision(&[1, 2]);
        assert_eq!(tracker.decision_count(), 1);
        assert_eq!(changes, vec![(1, 100.0), (2, 100.0)]);

        let changes = tracker.record_decision(&[1, 3]);
        assert_eq!(tracker.decision_count(), 2);
        // Adapter 1 stays at 100%, so it doesn't appear in changes
        assert!(!changes.iter().any(|(id, _)| *id == 1) || changes.contains(&(1, 100.0)));
        assert!(changes.contains(&(2, 50.0)) || changes.contains(&(2, 0.0)));
        assert!(changes.contains(&(3, 50.0)));
        assert!((tracker.activation_pct(1) - 100.0).abs() < 1.0);
    }

    #[test]
    fn honors_rolling_window() {
        let mut tracker = ActivationTracker::new(2);
        tracker.record_decision(&[0]);
        tracker.record_decision(&[0, 1]);
        assert!((tracker.activation_pct(0) - 100.0).abs() < 1e-3);

        let changes = tracker.record_decision(&[1]);
        assert_eq!(tracker.decision_count(), 2);
        assert!(changes.contains(&(0, 50.0)) || changes.contains(&(0, 0.0)));
        assert!((tracker.activation_pct(1) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn ignores_duplicate_entries_within_decision() {
        let mut tracker = ActivationTracker::new(3);
        tracker.record_decision(&[5, 5, 5]);
        assert!((tracker.activation_pct(5) - 100.0).abs() < 1e-3);

        tracker.record_decision(&[]);
        assert!((tracker.activation_pct(5) - 50.0).abs() < 1e-3);
    }
}
