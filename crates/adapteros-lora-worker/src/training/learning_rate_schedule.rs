//! Learning rate schedules for training optimization
//!
//! Implements common learning rate schedules:
//! - Constant: Fixed learning rate throughout training
//! - Linear decay: Linear decrease from initial LR to final LR
//! - Cosine annealing: Smooth cosine decay curve
//! - Warmup + decay: Gradual warmup followed by decay schedule

use serde::{Deserialize, Serialize};

/// Learning rate schedule type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LRScheduleType {
    /// Constant learning rate
    #[default]
    Constant,
    /// Linear decay from initial to final learning rate
    Linear,
    /// Cosine annealing decay
    Cosine,
}

/// Learning rate scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LRSchedulerConfig {
    /// Schedule type
    pub schedule_type: LRScheduleType,
    /// Initial learning rate
    pub initial_lr: f32,
    /// Final learning rate (for decay schedules)
    pub final_lr: f32,
    /// Number of warmup steps (gradual increase from 0 to initial_lr)
    pub warmup_steps: u32,
    /// Total training steps (for scheduling)
    pub total_steps: u32,
}

impl Default for LRSchedulerConfig {
    fn default() -> Self {
        Self {
            schedule_type: LRScheduleType::Constant,
            initial_lr: 0.001,
            final_lr: 0.0001,
            warmup_steps: 0,
            total_steps: 1000,
        }
    }
}

impl LRSchedulerConfig {
    /// Create constant learning rate schedule
    pub fn constant(lr: f32) -> Self {
        Self {
            schedule_type: LRScheduleType::Constant,
            initial_lr: lr,
            final_lr: lr,
            warmup_steps: 0,
            total_steps: 1,
        }
    }

    /// Create linear decay schedule
    pub fn linear(initial_lr: f32, final_lr: f32, total_steps: u32) -> Self {
        Self {
            schedule_type: LRScheduleType::Linear,
            initial_lr,
            final_lr,
            warmup_steps: 0,
            total_steps,
        }
    }

    /// Create cosine annealing schedule
    pub fn cosine(initial_lr: f32, final_lr: f32, total_steps: u32) -> Self {
        Self {
            schedule_type: LRScheduleType::Cosine,
            initial_lr,
            final_lr,
            warmup_steps: 0,
            total_steps,
        }
    }

    /// Add warmup steps to any schedule
    pub fn with_warmup(mut self, warmup_steps: u32) -> Self {
        self.warmup_steps = warmup_steps;
        self
    }
}

/// Learning rate scheduler
pub struct LRScheduler {
    config: LRSchedulerConfig,
    current_step: u32,
}

impl LRScheduler {
    /// Create a new learning rate scheduler
    pub fn new(config: LRSchedulerConfig) -> Self {
        Self {
            config,
            current_step: 0,
        }
    }

    /// Get current learning rate for the current step
    pub fn get_lr(&self) -> f32 {
        // Warmup phase: linear increase from 0 to initial_lr
        if self.current_step < self.config.warmup_steps {
            let warmup_progress = self.current_step as f32 / self.config.warmup_steps as f32;
            return self.config.initial_lr * warmup_progress;
        }

        // Main schedule (after warmup)
        let steps_after_warmup = self.current_step - self.config.warmup_steps;
        let total_decay_steps = self
            .config
            .total_steps
            .saturating_sub(self.config.warmup_steps);

        if total_decay_steps == 0 {
            return self.config.initial_lr;
        }

        let progress = (steps_after_warmup as f32 / total_decay_steps as f32).min(1.0);

        match self.config.schedule_type {
            LRScheduleType::Constant => self.config.initial_lr,
            LRScheduleType::Linear => {
                // Linear interpolation
                self.config.initial_lr + progress * (self.config.final_lr - self.config.initial_lr)
            }
            LRScheduleType::Cosine => {
                // Cosine annealing: lr = final_lr + 0.5 * (initial_lr - final_lr) * (1 + cos(π * progress))
                let cosine_factor = 0.5 * (1.0 + (std::f32::consts::PI * progress).cos());
                self.config.final_lr
                    + cosine_factor * (self.config.initial_lr - self.config.final_lr)
            }
        }
    }

    /// Step the scheduler (call after each training step)
    pub fn step(&mut self) {
        self.current_step += 1;
    }

    /// Get current step count
    pub fn current_step(&self) -> u32 {
        self.current_step
    }

    /// Reset scheduler to step 0
    pub fn reset(&mut self) {
        self.current_step = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_schedule() {
        let config = LRSchedulerConfig::constant(0.001);
        let mut scheduler = LRScheduler::new(config);

        assert_eq!(scheduler.get_lr(), 0.001);
        scheduler.step();
        assert_eq!(scheduler.get_lr(), 0.001);
        scheduler.step();
        assert_eq!(scheduler.get_lr(), 0.001);
    }

    #[test]
    fn test_linear_schedule() {
        let config = LRSchedulerConfig::linear(0.01, 0.001, 100);
        let mut scheduler = LRScheduler::new(config);

        // Initial LR
        assert_eq!(scheduler.get_lr(), 0.01);

        // Midpoint
        for _ in 0..50 {
            scheduler.step();
        }
        let mid_lr = scheduler.get_lr();
        assert!((mid_lr - 0.0055).abs() < 0.0001); // ~halfway between 0.01 and 0.001

        // Final
        for _ in 50..100 {
            scheduler.step();
        }
        assert!((scheduler.get_lr() - 0.001).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_schedule() {
        let config = LRSchedulerConfig::cosine(0.01, 0.001, 100);
        let mut scheduler = LRScheduler::new(config);

        // Initial LR
        assert_eq!(scheduler.get_lr(), 0.01);

        // Midpoint (cosine should be smoother than linear)
        for _ in 0..50 {
            scheduler.step();
        }
        let mid_lr = scheduler.get_lr();
        // At 50% progress, cos(π*0.5) = 0, so lr = final_lr + 0.5*(initial_lr - final_lr)
        let expected_mid = 0.001 + 0.5 * (0.01 - 0.001);
        assert!((mid_lr - expected_mid).abs() < 0.0001);

        // Final
        for _ in 50..100 {
            scheduler.step();
        }
        assert!((scheduler.get_lr() - 0.001).abs() < 0.0001);
    }

    #[test]
    fn test_warmup() {
        let config = LRSchedulerConfig::constant(0.001).with_warmup(10);
        let mut scheduler = LRScheduler::new(config);

        // Step 0: should be 0
        assert_eq!(scheduler.get_lr(), 0.0);

        // Step 5: should be 0.0005 (halfway)
        for _ in 0..5 {
            scheduler.step();
        }
        assert!((scheduler.get_lr() - 0.0005).abs() < 0.0001);

        // Step 10: should be 0.001 (full LR)
        for _ in 5..10 {
            scheduler.step();
        }
        assert!((scheduler.get_lr() - 0.001).abs() < 0.0001);

        // After warmup: constant
        scheduler.step();
        assert_eq!(scheduler.get_lr(), 0.001);
    }

    #[test]
    fn test_warmup_with_linear_decay() {
        let config = LRSchedulerConfig::linear(0.01, 0.001, 100).with_warmup(20);
        let mut scheduler = LRScheduler::new(config);

        // Warmup phase
        for _ in 0..20 {
            scheduler.step();
        }
        assert!((scheduler.get_lr() - 0.01).abs() < 0.0001);

        // Decay phase
        for _ in 20..60 {
            scheduler.step();
        }
        // At step 60 (40 steps after warmup, halfway through 80 decay steps)
        let mid_lr = scheduler.get_lr();
        let expected = 0.01 + 0.5 * (0.001 - 0.01);
        assert!((mid_lr - expected).abs() < 0.001);
    }

    #[test]
    fn test_reset() {
        let config = LRSchedulerConfig::linear(0.01, 0.001, 100);
        let mut scheduler = LRScheduler::new(config);

        for _ in 0..50 {
            scheduler.step();
        }
        assert_eq!(scheduler.current_step(), 50);

        scheduler.reset();
        assert_eq!(scheduler.current_step(), 0);
        assert_eq!(scheduler.get_lr(), 0.01);
    }
}
