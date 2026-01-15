# Training Metrics System

Complete guide to the comprehensive training metrics infrastructure in adapterOS.

**Last Updated:** 2025-12-11
**Location:** `crates/adapteros-lora-worker/src/training/`

---

## Overview

The training metrics system provides detailed monitoring and analysis of the LoRA training process:

- **Loss Curve Tracking**: Epoch and batch-level loss history with trend analysis
- **Gradient Monitoring**: Gradient norm computation for stability detection
- **Learning Rate Scheduling**: Automatic LR adjustment suggestions based on training dynamics
- **Performance Metrics**: Throughput, batch timing, and memory tracking
- **Telemetry Integration**: Automatic export to centralized telemetry system
- **Visualization Helpers**: ASCII charts and progress indicators for real-time monitoring

---

## Architecture

### Core Components

```
training/
├── metrics.rs              # Core metrics collection
├── visualization.rs        # ASCII charts and progress UI
├── trainer_metrics_ext.rs  # Integration with MicroLoRATrainer
└── mod.rs                  # Module exports
```

### Data Flow

```
MicroLoRATrainer
    ↓
train_with_callback()
    ↓
TrainingMetrics (collects per-epoch/batch data)
    ↓
TelemetryWriter (exports to telemetry system)
    ↓
Centralized Metrics Store
```

---

## Quick Start

### Basic Metrics Collection

```rust
use adapteros_lora_worker::training::{
    MicroLoRATrainer, TrainingConfig, TrainingMetrics, MetricsConfig
};

// Create trainer
let config = TrainingConfig {
    rank: 4,
    alpha: 16.0,
    learning_rate: 1e-4,
    epochs: 10,
    ..Default::default()
};
let mut trainer = MicroLoRATrainer::new(config)?;

// Create metrics collector
let mut metrics = TrainingMetrics::new(MetricsConfig::default())?;
metrics.mark_training_start();

// Train with callback that updates metrics
let result = trainer.train_with_callback(&examples, |epoch, loss| {
    metrics.record_epoch_loss(epoch - 1, loss);
    metrics.record_learning_rate(0.0001);
}).await?;

// Export final report
let report = metrics.export_training_report();
println!("{}", report.final_snapshot.epoch_loss);
```

### With Session Management

```rust
use adapteros_lora_worker::training::TrainingMetricsSession;

let mut session = TrainingMetricsSession::new(
    TrainingMetrics::new(MetricsConfig::default())?,
);
session.mark_training_start();

// During training loop:
for epoch in 0..num_epochs {
    let epoch_loss = train_epoch(&mut trainer, &examples)?;
    session.record_epoch(epoch, epoch_loss, current_lr);

    // Check if LR adjustment needed
    if session.should_adjust_lr() {
        let factor = session.suggested_lr_factor();
        current_lr *= factor;
        info!("Adjusted LR by factor {}: {:.8}", factor, current_lr);
    }
}

let metrics = session.finalize()?;
```

### Visualization

```rust
use adapteros_lora_worker::training::TrainingCharts;

// ASCII charts
let loss_chart = TrainingCharts::loss_curve_chart(&metrics, 80, 20);
println!("{}", loss_chart);

let grad_chart = TrainingCharts::gradient_norm_chart(&metrics, 80, 20);
println!("{}", grad_chart);

// Progress bar
let bar = TrainingCharts::progress_bar(5, 10, 30);
println!("{}", bar); // [===============       ] 50%

// Summary card
let snapshot = metrics.snapshot();
println!("{}", TrainingCharts::summary_card(&snapshot));

// Detailed report
let report = metrics.export_training_report();
println!("{}", TrainingCharts::detailed_report(&report));
```

---

## API Reference

### TrainingMetrics

Core metrics collection class.

#### Creation

```rust
// With custom configuration
let config = MetricsConfig {
    max_loss_history: 100,
    max_batch_history: 500,
    track_gradients: true,
    track_memory: true,
    export_interval_epochs: 1,
};
let metrics = TrainingMetrics::new(config)?;

// With defaults
let metrics = TrainingMetrics::default_with_telemetry()?;
```

#### Recording Metrics

```rust
// Training lifecycle
metrics.mark_training_start();

// Loss tracking
metrics.record_epoch_loss(epoch: usize, loss: f32);
metrics.record_batch_loss(batch_idx: usize, loss: f32);

// Performance
metrics.record_batch_timing(elapsed_ms: u64);

// Gradient monitoring
metrics.record_gradient_norm(norm: f32);

// Learning rate
metrics.record_learning_rate(lr: f32);

// Memory
metrics.record_peak_memory(memory_mb: f32);
```

#### Analysis

```rust
// Stability checks
metrics.check_stability()?; // Detects NaN, Inf, exploding gradients

// Learning rate recommendations
metrics.should_adjust_learning_rate() -> bool;
metrics.suggest_learning_rate_factor() -> f32;

// Data export
metrics.loss_curve() -> Vec<f32>;
metrics.batch_loss_curve() -> Vec<f32>;
metrics.gradient_norm_curve() -> Vec<f32>;
metrics.learning_rate_history() -> Vec<f32>;
metrics.batch_timing_history() -> Vec<u64>;

// Snapshots
let snapshot = metrics.snapshot(); // Current state

// Reports
let report = metrics.export_training_report();
metrics.export_to_telemetry()?;
```

### MetricsSnapshot

Current metrics state at a point in time.

```rust
pub struct MetricsSnapshot {
    pub epoch: usize,                    // Current epoch
    pub epoch_loss: f32,                 // Loss this epoch
    pub avg_batch_loss: f32,             // Average batch loss
    pub min_loss: f32,                   // Best loss seen
    pub max_loss: f32,                   // Worst loss seen
    pub loss_trend: f32,                 // Improvement trend
    pub gradient_norm: Option<f32>,      // Current gradient norm
    pub learning_rate: f32,              // Current LR
    pub avg_batch_time_ms: f32,          // Average batch duration
    pub throughput_bps: f32,             // Batches per second
    pub peak_memory_mb: f32,             // Peak memory used
    pub total_time_ms: u64,              // Total training time
}
```

### TrainingReport

Complete training session analysis.

```rust
pub struct TrainingReport {
    pub total_epochs: usize,
    pub total_batches: usize,
    pub loss_curve: Vec<f32>,
    pub gradient_norm_curve: Vec<f32>,
    pub learning_rate_history: Vec<f32>,
    pub final_snapshot: MetricsSnapshot,
}

impl TrainingReport {
    pub fn loss_improvement_per_epoch(&self) -> f32;
    pub fn has_converged(&self) -> bool;
}
```

### TrainingCharts

Visualization utilities.

```rust
impl TrainingCharts {
    pub fn loss_curve_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String;
    pub fn gradient_norm_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String;
    pub fn learning_rate_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String;
    pub fn progress_bar(current: usize, total: usize, width: usize) -> String;
    pub fn summary_card(snapshot: &MetricsSnapshot) -> String;
    pub fn detailed_report(report: &TrainingReport) -> String;
}
```

### TrainingProgress

JSON-serializable progress tracking.

```rust
pub struct TrainingProgress {
    pub current_epoch: usize,
    pub total_epochs: usize,
    pub current_batch: usize,
    pub total_batches: usize,
    pub current_loss: f32,
    pub best_loss: f32,
    pub learning_rate: f32,
    pub eta_seconds: f32,
    pub progress_percent: f32,
}

impl TrainingProgress {
    pub fn from_snapshot(snapshot: &MetricsSnapshot, total_epochs: usize) -> Self;
    pub fn to_human_readable(&self) -> String;
}
```

---

## Integration Patterns

### Pattern 1: Simple Callback Integration

```rust
let mut metrics = TrainingMetrics::default_with_telemetry()?;
metrics.mark_training_start();

let result = trainer.train_with_callback(&examples, |epoch, loss| {
    metrics.record_epoch_loss(epoch - 1, loss);
}).await?;

println!("Training complete! Final loss: {:.6}", metrics.snapshot().epoch_loss);
```

### Pattern 2: Advanced Session Management

```rust
let mut session = TrainingMetricsSession {
    start_time: Instant::now(),
    metrics: TrainingMetrics::default_with_telemetry()?,
    batch_count: 0,
    epoch_count: 0,
};

for epoch in 0..num_epochs {
    for (batch_idx, batch) in examples.chunks(batch_size).enumerate() {
        let start = Instant::now();
        let loss = train_batch(&mut trainer, batch)?;
        let elapsed = start.elapsed().as_millis() as u64;

        session.record_batch(loss, elapsed, None);
    }

    let epoch_loss = calculate_epoch_loss()?;
    session.record_epoch(epoch, epoch_loss, config.learning_rate);

    if session.should_adjust_lr() {
        config.learning_rate *= session.suggested_lr_factor();
    }
}

let metrics = session.finalize()?;
```

### Pattern 3: Telemetry-Driven Monitoring

```rust
let mut metrics = TrainingMetrics::new(MetricsConfig {
    export_interval_epochs: 5,
    track_gradients: true,
    track_memory: true,
    ..Default::default()
})?;

for epoch in 0..num_epochs {
    let loss = train_epoch()?;
    metrics.record_epoch_loss(epoch, loss);

    // Metrics automatically exported every 5 epochs
    if epoch % 5 == 0 {
        metrics.export_to_telemetry()?;
    }

    // Real-time visualization
    let snapshot = metrics.snapshot();
    println!("{}", TrainingCharts::summary_card(&snapshot));
}

// Final report
let report = metrics.export_training_report();
println!("{}", TrainingCharts::detailed_report(&report));
```

---

## Features

### Loss Curve Tracking

Tracks loss at multiple granularities:

- **Epoch-level**: Average loss across all batches in an epoch
- **Batch-level**: Loss for individual batches (rolling window)
- **Trend analysis**: Compares early vs. recent losses to detect convergence

```rust
metrics.record_epoch_loss(epoch, loss);
metrics.record_batch_loss(batch_idx, loss);

let trend = metrics.snapshot().loss_trend; // Positive = improving
```

### Gradient Monitoring

Detects training instability through gradient norms:

```rust
// Compute L2 norm of all gradients
let grad_norm = compute_gradient_norm(&weights);
metrics.record_gradient_norm(grad_norm);

// Check for explosions (> 100) or vanishing (< 0.001)
metrics.check_stability()?;
```

**Stability Thresholds:**
- NaN/Inf: Immediate error
- Loss > 1e6: Warning
- Gradient norm > 100: Warning (exploding gradients)
- Gradient norm < 0.001: Consider LR increase

### Learning Rate Scheduling

Automatic LR adjustment recommendations:

```rust
// Detects plateau: loss improvement < 0.01% per step
if metrics.should_adjust_learning_rate() {
    let factor = metrics.suggest_learning_rate_factor();
    // Returns 0.5 for exploding gradients, 2.0 for vanishing, 1.0 otherwise
    new_lr = old_lr * factor;
}
```

### Performance Metrics

- **Throughput**: Batches per second
- **Batch timing**: Per-batch duration tracking
- **Memory**: Peak GPU memory usage
- **Training time**: Total elapsed time

### Telemetry Export

All metrics automatically exported to centralized telemetry system:

```rust
metrics.export_to_telemetry()?;
// Exports JSON event to "training.metrics_snapshot" channel
```

JSON Schema:
```json
{
    "event_type": "training.metrics_snapshot",
    "epoch": 5,
    "epoch_loss": 0.123456,
    "avg_batch_loss": 0.125,
    "min_loss": 0.100,
    "max_loss": 0.150,
    "loss_trend": 0.01,
    "gradient_norm": 0.05,
    "learning_rate": 0.0001,
    "avg_batch_time_ms": 50.2,
    "throughput_bps": 19.92,
    "peak_memory_mb": 512.5,
    "total_time_ms": 5000
}
```

---

## Configuration

### MetricsConfig

```rust
pub struct MetricsConfig {
    /// Maximum loss history to keep (default: 100)
    pub max_loss_history: usize,

    /// Maximum batch history (default: 500)
    pub max_batch_history: usize,

    /// Maximum gradient history (default: 100)
    pub max_gradient_history: usize,

    /// Enable gradient norm computation (default: true, expensive)
    pub track_gradients: bool,

    /// Enable memory tracking (default: true)
    pub track_memory: bool,

    /// Export metrics every N epochs (default: 1)
    pub export_interval_epochs: usize,
}
```

### Example Configurations

**Lightweight tracking** (minimal overhead):
```rust
MetricsConfig {
    max_loss_history: 50,
    max_batch_history: 100,
    track_gradients: false,
    track_memory: false,
    export_interval_epochs: 5,
}
```

**Full tracking** (maximum data):
```rust
MetricsConfig {
    max_loss_history: 200,
    max_batch_history: 1000,
    track_gradients: true,
    track_memory: true,
    export_interval_epochs: 1,
}
```

**Research/debugging**:
```rust
MetricsConfig {
    max_loss_history: 100,
    max_batch_history: 500,
    track_gradients: true,
    track_memory: true,
    export_interval_epochs: 1,
}
```

---

## Examples

### Complete Training Session

```rust
use adapteros_lora_worker::training::{
    MicroLoRATrainer, TrainingConfig, TrainingMetrics, MetricsConfig,
    TrainingCharts, TrainingExample
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Create examples
    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        // ... more examples
    ];

    // Create trainer
    let config = TrainingConfig {
        rank: 4,
        alpha: 16.0,
        learning_rate: 1e-4,
        epochs: 10,
        batch_size: 8,
        hidden_dim: 768,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config)?;

    // Create metrics
    let metrics_config = MetricsConfig::default();
    let mut metrics = TrainingMetrics::new(metrics_config)?;
    metrics.mark_training_start();

    // Train with monitoring
    let mut epoch_losses = Vec::new();
    let result = trainer.train_with_callback(&examples, |epoch, loss| {
        epoch_losses.push(loss);
        metrics.record_epoch_loss(epoch - 1, loss);
        metrics.record_learning_rate(config.learning_rate);

        // Real-time progress
        if epoch % 2 == 0 {
            let snapshot = metrics.snapshot();
            println!("{}", TrainingCharts::summary_card(&snapshot));
        }
    }).await?;

    // Final analysis
    let report = metrics.export_training_report();
    println!("{}", TrainingCharts::detailed_report(&report));

    println!("Training complete!");
    println!("  Final loss: {:.6}", result.final_loss);
    println!("  Training time: {:.2}s", report.final_snapshot.total_time_ms as f32 / 1000.0);
    println!("  Converged: {}", report.has_converged());

    Ok(())
}
```

### Web API Integration

```rust
// In server endpoint handler
async fn get_training_metrics(
    State(app_state): State<AppState>,
    Path(job_id): Path<String>,
) -> Json<MetricsSnapshot> {
    let metrics = app_state.training_metrics.get(&job_id).unwrap();
    Json(metrics.snapshot())
}

// Stream metrics for real-time UI updates
async fn stream_training_metrics(
    State(app_state): State<AppState>,
    Path(job_id): Path<String>,
) -> impl Stream<Item = Result<String>> {
    let metrics = app_state.training_metrics.get(&job_id).unwrap();

    interval(Duration::from_secs(1)).then(move |_| {
        let snapshot = metrics.snapshot();
        let progress = TrainingProgress::from_snapshot(&snapshot, 10);
        async move {
            Ok(serde_json::to_string(&progress).unwrap())
        }
    })
}
```

---

## Testing

### Unit Tests

```bash
cargo test -p adapteros-lora-worker training::metrics
cargo test -p adapteros-lora-worker training::visualization
```

### Example Test

```rust
#[test]
fn test_metrics_convergence() {
    let config = MetricsConfig::default();
    let mut metrics = TrainingMetrics::new(config).unwrap();

    // Simulate converging training
    for i in 0..10 {
        let loss = 1.0 / ((i + 1) as f32);
        metrics.record_epoch_loss(i, loss);
    }

    let report = metrics.export_training_report();
    assert!(report.has_converged());
}
```

---

## Performance Considerations

### Memory Usage

- **Default config**: ~50 KB (100 loss values, 500 batch losses, etc.)
- **Max config**: ~200 KB
- **Low overhead**: < 1% CPU for metrics collection

### Telemetry Overhead

- Non-blocking export via bounded channel
- Events dropped on overflow (tracked for monitoring)
- Default 1000 event buffer

### Optimization Tips

1. **Disable gradient tracking** if not needed:
   ```rust
   config.track_gradients = false;
   ```

2. **Increase export interval** for less frequent I/O:
   ```rust
   config.export_interval_epochs = 5;
   ```

3. **Reduce history sizes** for memory-constrained environments:
   ```rust
   config.max_loss_history = 25;
   config.max_batch_history = 100;
   ```

---

## Troubleshooting

### Issue: NaN Loss

```
Training stability check failed: NaN loss detected
```

**Causes:**
- Learning rate too high
- Poor data normalization
- Numerical overflow in loss computation

**Fix:**
```rust
// Reduce learning rate
config.learning_rate = 1e-5;

// Check data normalization
for example in &examples {
    assert!(!example.target.iter().any(|&v| v > 1e3));
}
```

### Issue: Exploding Gradients

```
Gradient norm exceeded 100.0: 523.5, exploding gradients
```

**Fix:**
```rust
// Metrics system suggests LR adjustment
if metrics.should_adjust_lr() {
    let factor = metrics.suggest_learning_rate_factor(); // 0.5
    config.learning_rate *= factor;
}
```

### Issue: Vanishing Gradients

Gradient norm consistently < 0.001

**Fix:**
```rust
// System suggests increase
let factor = metrics.suggest_learning_rate_factor(); // 2.0
config.learning_rate *= factor;
```

---

## Best Practices

1. **Always mark training start**:
   ```rust
   metrics.mark_training_start();
   ```

2. **Export to telemetry regularly**:
   ```rust
   if epoch % export_interval == 0 {
       metrics.export_to_telemetry()?;
   }
   ```

3. **Monitor stability checks**:
   ```rust
   if let Err(e) = metrics.check_stability() {
       warn!("Training issue: {}", e);
   }
   ```

4. **Use visualization for debugging**:
   ```rust
   println!("{}", TrainingCharts::loss_curve_chart(&metrics, 80, 20));
   ```

5. **Generate final report**:
   ```rust
   let report = metrics.export_training_report();
   println!("{}", TrainingCharts::detailed_report(&report));
   ```

---

## References

- [AGENTS.md - Training Pipeline](../AGENTS.md)
- [TRAINING.md](TRAINING.md) - Main training guide
- [crates/adapteros-lora-worker/src/training/](../crates/adapteros-lora-worker/src/training/)
- [crates/adapteros-telemetry/](../crates/adapteros-telemetry/)

---

MLNavigator Inc 2025-12-11
