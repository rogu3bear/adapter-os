# CoreML Power Management Quick Start

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

## 5-Minute Quick Start

### 1. Basic Setup (No Power Management)

```rust
use adapteros_lora_kernel_coreml::CoreMLBackend;
use std::path::Path;

let backend = CoreMLBackend::new(Path::new("model.mlpackage"))?;
```

### 2. Enable Power Management

```rust
use adapteros_lora_kernel_coreml::{CoreMLBackend, PowerMode};

let backend = CoreMLBackend::new_with_power_mode(
    Path::new("model.mlpackage"),
    PowerMode::Balanced  // or Performance, Efficiency, LowPower
)?;
```

### 3. Check Power Status

```rust
// Battery state
if let Some(battery) = backend.get_battery_state() {
    println!("Battery: {:.1}%, plugged: {}",
             battery.level_percent, battery.is_plugged_in);
}

// Thermal state
if let Some(thermal) = backend.get_thermal_state() {
    println!("Thermal: {:?}", thermal);
}

// Power metrics
if let Some(metrics) = backend.get_power_metrics() {
    println!("Avg watts/token: {:.3}", metrics.avg_watts_per_token);
    println!("Battery drain: {:.2}%/hr", metrics.battery_drain_rate_pct_per_hour);
}
```

### 4. Dynamic Mode Switching

```rust
// Switch based on conditions
if let Some(battery) = backend.get_battery_state() {
    if battery.level_percent < 20.0 {
        backend.set_power_mode(PowerMode::LowPower);
    }
}

if let Some(thermal) = backend.get_thermal_state() {
    if thermal == ThermalState::Critical {
        backend.set_power_mode(PowerMode::Efficiency);
    }
}
```

## Power Mode Cheat Sheet

| Mode | When to Use | Battery Life | Performance |
|------|-------------|--------------|-------------|
| **Performance** | Benchmarking, max speed | 1.0x | 100% |
| **Balanced** | General production | 1.25x | 85% |
| **Efficiency** | On battery | 1.5x | 70% |
| **LowPower** | Low battery (<20%) | 1.9x | 45% |

## Common Patterns

### Pattern 1: Production Server (Plugged In)

```rust
let backend = CoreMLBackend::new_with_power_mode(
    model_path,
    PowerMode::Performance
)?;
```

### Pattern 2: Edge Device (Battery Powered)

```rust
let backend = CoreMLBackend::new_with_power_mode(
    model_path,
    PowerMode::Efficiency
)?;

// Auto-adjust based on battery
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Some(battery) = backend.get_battery_state() {
            if battery.level_percent < 30.0 {
                backend.set_power_mode(PowerMode::LowPower);
            } else if battery.is_plugged_in {
                backend.set_power_mode(PowerMode::Balanced);
            }
        }
    }
});
```

### Pattern 3: Adaptive (Auto-Switch)

```rust
let backend = CoreMLBackend::new_with_power_mode(
    model_path,
    PowerMode::Balanced
)?;

// The backend automatically:
// - Switches to LowPower when battery < 10%
// - Throttles when thermal = Critical
// - Defers operations when on battery
// No manual intervention needed!
```

### Pattern 4: Development/Testing

```rust
// No power management overhead
let backend = CoreMLBackend::new(model_path)?;
```

## Monitoring Template

```rust
use adapteros_lora_kernel_coreml::{CoreMLBackend, PowerMode, ThermalState};
use std::time::Duration;

async fn monitor_power(backend: &CoreMLBackend) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        // Health check
        match backend.health_check() {
            Ok(health) if !matches!(health, BackendHealth::Healthy) => {
                warn!("Backend degraded: {:?}", health);
            }
            Err(e) => error!("Health check failed: {}", e),
            _ => {}
        }

        // Power metrics
        if let Some(metrics) = backend.get_power_metrics() {
            info!("Power: {:.2}W avg, {:.2}%/hr drain",
                  metrics.avg_watts_per_token * 1000.0,
                  metrics.battery_drain_rate_pct_per_hour);
        }

        // Thermal management
        if let Some(thermal) = backend.get_thermal_state() {
            if matches!(thermal, ThermalState::Serious | ThermalState::Critical) {
                warn!("High thermal state: {:?}, throttling", thermal);
            }
        }
    }
}
```

## Troubleshooting

### Q: High power consumption?
```rust
let metrics = backend.get_power_metrics().unwrap();
if metrics.avg_watts_per_token > 0.5 {
    backend.set_power_mode(PowerMode::Efficiency);
}
```

### Q: Device getting hot?
```rust
if let Some(thermal) = backend.get_thermal_state() {
    match thermal {
        ThermalState::Serious => backend.set_power_mode(PowerMode::Efficiency),
        ThermalState::Critical => backend.set_power_mode(PowerMode::LowPower),
        _ => {}
    }
}
```

### Q: Battery draining fast?
```rust
let metrics = backend.get_power_metrics().unwrap();
if metrics.battery_drain_rate_pct_per_hour > 10.0 {
    backend.set_power_mode(PowerMode::LowPower);
}
```

### Q: How to disable power management?
```rust
// Just use new() instead of new_with_power_mode()
let backend = CoreMLBackend::new(model_path)?;
```

## Metrics Reference

```rust
let metrics = backend.get_metrics();

// Standard metrics
metrics.total_operations          // Total inference ops
metrics.avg_latency_us            // Average latency in μs
metrics.error_count               // Error count

// Power-specific custom metrics
metrics.custom_metrics["battery_level_pct"]              // Battery %
metrics.custom_metrics["thermal_state"]                  // 0-3
metrics.custom_metrics["avg_watts_per_token"]            // Power efficiency
metrics.custom_metrics["battery_drain_rate_pct_per_hour"] // Drain rate
metrics.custom_metrics["thermal_throttle_events"]        // Throttle count
```

## Performance Impact

| Mode | Overhead | Latency Impact |
|------|----------|----------------|
| No power management | 0μs | 0% |
| Performance | <1μs | <1% |
| Balanced | ~5μs | ~2% |
| Efficiency | ~10μs | ~5% |
| LowPower | ~50μs | ~10% |

*Overhead includes monitoring, throttling checks, and deferral logic*

## Complete Example

```rust
use adapteros_lora_kernel_coreml::{CoreMLBackend, PowerMode};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    // Create backend with power management
    let mut backend = CoreMLBackend::new_with_power_mode(
        Path::new("models/qwen2.5-7b.mlpackage"),
        PowerMode::Balanced
    )?;

    // Check initial state
    if let Some(battery) = backend.get_battery_state() {
        println!("Starting with battery at {:.1}%", battery.level_percent);
    }

    // Run inference
    let mut io = IoBuffers::new(32000);
    io.input_ids = vec![1, 2, 3, 4, 5];

    let ring = RouterRing::new(0);
    backend.run_step(&ring, &mut io)?;

    // Check power consumption
    if let Some(metrics) = backend.get_power_metrics() {
        println!("Power consumption: {:.3} watts/token", metrics.avg_watts_per_token);
    }

    // Health check
    let health = backend.health_check()?;
    println!("Backend health: {:?}", health);

    Ok(())
}
```

---

**Next Steps:**
- Read [README.md](README.md) for detailed documentation
- See [IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md) for architecture details
- Check [../../docs/COREML_INTEGRATION.md](../../docs/COREML_INTEGRATION.md) for CoreML setup

**Signed:** James KC Auchterlonie
**Date:** 2025-11-19
