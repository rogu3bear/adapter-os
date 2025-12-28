//! Dataset loading and weighted round-robin merge for multi-dataset training.

use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
use std::collections::VecDeque;

/// Deterministic weighted round-robin merge for multi-dataset training.
///
/// Takes a vector of (examples, weight) tuples and interleaves them according to weights.
/// Higher weights mean more slots in the round-robin schedule.
pub fn weighted_round_robin_merge(
    datasets: Vec<(Vec<WorkerTrainingExample>, f32)>,
) -> Vec<WorkerTrainingExample> {
    let weights: Vec<f32> = datasets.iter().map(|(_, w)| *w).collect();
    let mut queues: Vec<VecDeque<WorkerTrainingExample>> = datasets
        .into_iter()
        .map(|(examples, _)| VecDeque::from(examples))
        .collect();

    let mut schedule: Vec<usize> = Vec::new();
    for (idx, queue) in queues.iter().enumerate() {
        let weight = (*weights.get(idx).unwrap_or(&1.0)).max(0.0);
        let slots = weight.round() as usize;
        let slots = if slots == 0 { 1 } else { slots };
        if !queue.is_empty() {
            for _ in 0..slots {
                schedule.push(idx);
            }
        }
    }

    if schedule.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    loop {
        let mut progressed = false;
        for &idx in schedule.iter() {
            if let Some(ex) =
                queues
                    .get_mut(idx)
                    .and_then(|q| if q.is_empty() { None } else { q.pop_front() })
            {
                merged.push(ex);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    merged
}
