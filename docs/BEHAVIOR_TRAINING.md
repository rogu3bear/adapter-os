# Behavior Training Data Pipeline

## Introduction

This document provides comprehensive details on generating training data for adapter lifecycle behaviors. The pipeline supports both exporting historical telemetry events and generating synthetic examples to train adapters on state transitions, memory management, and operational patterns.

## Dataset Schema

### BehaviorExample Structure

Each training example follows this JSONL format:

```json
{
  "input": {
    "adapter_id": "tenant-a/engineering/code-review/r001",
    "load_state": "warm",
    "activation_pct": 0.65,
    "memory_mb": 150,
    "last_used": "2025-11-18T05:00:00Z"
  },
  "target": {
    "next_state": "hot",
    "action": "promote",
    "reason": "activation_threshold_crossed",
    "memory_delta": 50
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive",
    "policy_compliant": true,
    "category": "promotion",
    "source": "telemetry_export"
  }
}
```

**Fields:**

- **input.adapter_id:** Semantic adapter name (required)
- **input.load_state:** Current runtime state (Unloaded, Cold, Warm, Hot, Resident)
- **input.activation_pct:** Activation percentage (0.0-1.0)
- **input.memory_mb:** Current memory usage in MB
- **input.last_used:** ISO8601 timestamp of last activation
- **target.next_state:** Expected next state
- **target.action:** Transition action (promote, demote, evict, pin, recover)
- **target.reason:** Reason for transition
- **target.memory_delta:** Expected memory change (positive/negative MB)
- **metadata.quality:** Relevance score (0.0-1.0)
- **metadata.label:** positive/negative for RLHF
- **metadata.policy_compliant:** Boolean policy validation
- **metadata.category:** One of 6 behavior categories
- **metadata.source:** "telemetry_export" or "synthetic"

### Behavior Categories

1. **Promotion:** State advancement based on activation thresholds
   - Cold → Warm (activation_pct > 0.1)
   - Warm → Hot (activation_pct > 0.5)
   - Hot → Resident (pinned or consistent high quality)

2. **Demotion:** State regression due to inactivity
   - Hot → Warm (inactivity > 1 hour)
   - Warm → Cold (inactivity > 24 hours)
   - Cold → Unloaded (memory pressure or TTL)

3. **Eviction:** Memory pressure or policy-based removal
   - Any loaded state → Unloaded when memory > 85%
   - Prioritizes Cold/Warm adapters first

4. **Pinning:** Manual protection from eviction
   - Any state → Resident (manual pin request)
   - Resident → Any (unpin or TTL expiration)

5. **Recovery:** Heartbeat timeout or crash recovery
   - Stale state → Reset (heartbeat_age > 300s)
   - Failed load → Retry or Unloaded

6. **TTL Enforcement:** Expiration-based eviction
   - Any state → Unloaded (expires_at < now)

## Quality Criteria

All generated examples must meet these thresholds:

- **Minimum Examples:** 500 per category (total: 3,000)
- **Relevance Score:** ≥ 0.85 (semantic similarity to real events)
- **Confidence:** ≥ 0.90 (validation against lifecycle rules)
- **Transition Validity:** 100% (must use valid promote/demote paths)
- **Data Distribution:** Balanced across categories and states
- **Temporal Realism:** Synthetic examples respect real time deltas

### Validation Rules

1. **State Transitions:** Use `AdapterState::promote()` / `demote()` methods
2. **Activation Percentages:** [0.0, 1.0] range, realistic distributions
3. **Memory Values:** Match adapter tier sizes (e.g., Cold: 50-200MB)
4. **Timestamps:** ISO8601, realistic intervals (e.g., 5-60s between heartbeats)
5. **Policy Compliance:** Generated examples must pass 24 canonical policies

## Generation Strategies

### 1. Telemetry Export (Historical Data)

**Purpose:** Extract real events from production telemetry for authentic training.

**Process:**
1. Query `behavior_events` table for events in time range
2. Filter by category, tenant, adapter
3. Reconstruct input/target from event data
4. Validate and enrich with metadata
5. Output as JSONL

**CLI Usage:**
```bash
aosctl behavior-export \
  --output ./exported.jsonl \
  --categories promotion,eviction \
  --since 2025-01-01 \
  --until 2025-12-01 \
  --tenant system
```

**Example Output (promotion event):**
```jsonl
{"input":{"adapter_id":"system/core/router/r001","load_state":"warm","activation_pct":0.55,"memory_mb":120,"last_used":"2025-11-18T10:30:00Z"},"target":{"next_state":"hot","action":"promote","reason":"activation_threshold","memory_delta":30},"metadata":{"quality":0.92,"label":"positive","policy_compliant":true,"category":"promotion","source":"telemetry_export"}}
```

### 2. Synthetic Generation (Rule-Based)

**Purpose:** Generate diverse examples to fill gaps in historical data.

**Process:**
1. Define rules per category (e.g., activation thresholds)
2. Generate random but realistic inputs (states, percentages, timestamps)
3. Apply lifecycle rules to determine valid targets
4. Validate synthetic examples against quality criteria
5. Output as JSONL with `source: "synthetic"`

**CLI Usage:**
```bash
aosctl behavior-export \
  --output ./synthetic.jsonl \
  --synthetic-count 1000 \
  --categories all \
  --seed 42
```

**Example Output (synthetic eviction):**
```jsonl
{"input":{"adapter_id":"tenant-b/docs/manual/r002","load_state":"cold","activation_pct":0.02,"memory_mb":80,"last_used":"2025-11-18T09:15:00Z"},"target":{"next_state":"unloaded","action":"evict","reason":"memory_pressure_85pct","memory_delta":-80},"metadata":{"quality":0.88,"label":"positive","policy_compliant":true,"category":"eviction","source":"synthetic"}}
```

### 3. Combined Pipeline

**Purpose:** Mix historical and synthetic data for balanced datasets.

**Process:**
1. Export historical data (if available)
2. Generate synthetic to meet minimum counts
3. Balance categories
4. Apply quality filtering
5. Save as unified JSONL

**CLI Usage:**
```bash
aosctl behavior-export \
  --output ./mixed.jsonl \
  --categories all \
  --since 2025-01-01 \
  --synthetic-count 500 \
  --min-per-category 100
```

## CLI Usage Examples

### Export Historical Data
```bash
# Export all promotion events from last month
aosctl behavior-export \
  --output behaviors/promotion.jsonl \
  --categories promotion \
  --since 2025-10-01

# Export tenant-specific eviction events
aosctl behavior-export \
  --output behaviors/tenant-evictions.jsonl \
  --categories eviction \
  --tenant tenant-a \
  --since 2025-11-01
```

### Generate Synthetic Data
```bash
# Generate 1000 balanced synthetic examples
aosctl behavior-export \
  --output behaviors/synthetic-balanced.jsonl \
  --synthetic-count 1000 \
  --categories all \
  --seed 42

# Generate category-specific synthetic data
aosctl behavior-export \
  --output behaviors/pinning-synthetic.jsonl \
  --categories pinning \
  --synthetic-count 200 \
  --seed 123
```

### Combined Pipeline
```bash
# Export historical + generate synthetic to meet quotas
aosctl behavior-export \
  --output behaviors/full-dataset.jsonl \
  --categories all \
  --since 2025-09-01 \
  --synthetic-count 800 \
  --min-per-category 100 \
  --tenant system
```

### Validation
```bash
# Validate generated dataset
aosctl behavior-validate \
  --input behaviors/full-dataset.jsonl \
  --check-min-examples \
  --check-transitions \
  --check-quality
```

## Example Datasets

### 1. Promotion Examples

**File:** `behaviors/tier_promotion/positive.jsonl`

```jsonl
{"input":{"adapter_id":"system/core/router/r001","load_state":"warm","activation_pct":0.55,"memory_mb":120,"last_used":"2025-11-18T10:30:00Z"},"target":{"next_state":"hot","action":"promote","reason":"activation_threshold","memory_delta":30},"metadata":{"quality":0.92,"label":"positive","policy_compliant":true,"category":"promotion","source":"telemetry_export"}}
{"input":{"adapter_id":"tenant-a/engineering/code-review/r002","load_state":"cold","activation_pct":0.15,"memory_mb":80,"last_used":"2025-11-18T11:00:00Z"},"target":{"next_state":"warm","action":"promote","reason":"initial_activation","memory_delta":20},"metadata":{"quality":0.88,"label":"positive","policy_compliant":true,"category":"promotion","source":"synthetic"}}
```

### 2. Demotion Examples

**File:** `behaviors/state_transitions/demotion.jsonl`

```jsonl
{"input":{"adapter_id":"tenant-b/docs/manual/r003","load_state":"hot","activation_pct":0.05,"memory_mb":200,"last_used":"2025-11-17T14:00:00Z"},"target":{"next_state":"warm","action":"demote","reason":"inactivity_1h","memory_delta":-40},"metadata":{"quality":0.91,"label":"positive","policy_compliant":true,"category":"demotion","source":"telemetry_export"}}
{"input":{"adapter_id":"system/security/policy-check/r004","load_state":"warm","activation_pct":0.01,"memory_mb":100,"last_used":"2025-11-16T09:00:00Z"},"target":{"next_state":"cold","action":"demote","reason":"inactivity_24h","memory_delta":-20},"metadata":{"quality":0.87,"label":"positive","policy_compliant":true,"category":"demotion","source":"synthetic"}}
```

### 3. Eviction Examples

**File:** `behaviors/memory_eviction/positive.jsonl`

```jsonl
{"input":{"adapter_id":"tenant-c/analytics/reporting/r005","load_state":"cold","activation_pct":0.02,"memory_mb":60,"last_used":"2025-11-18T08:00:00Z"},"target":{"next_state":"unloaded","action":"evict","reason":"memory_pressure_85pct","memory_delta":-60},"metadata":{"quality":0.95,"label":"positive","policy_compliant":true,"category":"eviction","source":"telemetry_export"}}
{"input":{"adapter_id":"system/test/temp-adapter/r006","load_state":"warm","activation_pct":0.00,"memory_mb":90,"last_used":"2025-11-18T10:00:00Z"},"target":{"next_state":"unloaded","action":"evict","reason":"ttl_expired","memory_delta":-90},"metadata":{"quality":0.89,"label":"positive","policy_compliant":true,"category":"eviction","source":"synthetic"}}
```

### 4. Pinning Examples

**File:** `behaviors/pinning_patterns/positive.jsonl`

```jsonl
{"input":{"adapter_id":"tenant-a/core/production-router/r007","load_state":"hot","activation_pct":0.80,"memory_mb":250,"last_used":"2025-11-18T12:00:00Z"},"target":{"next_state":"resident","action":"pin","reason":"manual_production_pin","memory_delta":0},"metadata":{"quality":0.94,"label":"positive","policy_compliant":true,"category":"pinning","source":"telemetry_export"}}
{"input":{"adapter_id":"system/admin/golden-adapter/r008","load_state":"resident","activation_pct":1.00,"memory_mb":300,"last_used":"2025-11-18T13:00:00Z"},"target":{"next_state":"hot","action":"unpin","reason":"ttl_expired","memory_delta":-50},"metadata":{"quality":0.92,"label":"positive","policy_compliant":true,"category":"pinning","source":"synthetic"}}
```

### 5. Recovery Examples

**File:** `behaviors/heartbeat_recovery/positive.jsonl`

```jsonl
{"input":{"adapter_id":"tenant-d/inference/live-model/r009","load_state":"hot","activation_pct":0.70,"memory_mb":180,"last_used":"2025-11-18T11:45:00Z"},"target":{"next_state":"hot","action":"recover","reason":"heartbeat_timeout_300s","memory_delta":0},"metadata":{"quality":0.93,"label":"positive","policy_compliant":true,"category":"recovery","source":"telemetry_export"}}
{"input":{"adapter_id":"system/backup/crash-prone/r010","load_state":"cold","activation_pct":0.10,"memory_mb":70,"last_used":"2025-11-18T07:00:00Z"},"target":{"next_state":"unloaded","action":"recover","reason":"load_failure_retry_exhausted","memory_delta":-70},"metadata":{"quality":0.86,"label":"positive","policy_compliant":true,"category":"recovery","source":"synthetic"}}
```

### 6. TTL Enforcement Examples

**File:** `behaviors/ttl_enforcement/positive.jsonl`

```jsonl
{"input":{"adapter_id":"tenant-e/temp/debug-adapter/r011","load_state":"warm","activation_pct":0.20,"memory_mb":110,"last_used":"2025-11-18T09:30:00Z"},"target":{"next_state":"unloaded","action":"evict","reason":"ttl_expired_7d","memory_delta":-110},"metadata":{"quality":0.90,"label":"positive","policy_compliant":true,"category":"ttl_enforcement","source":"telemetry_export"}}
{"input":{"adapter_id":"system/test/short-lived/r012","load_state":"cold","activation_pct":0.00,"memory_mb":50,"last_used":"2025-11-18T10:00:00Z"},"target":{"next_state":"unloaded","action":"evict","reason":"ttl_expired_1h","memory_delta":-50},"metadata":{"quality":0.88,"label":"positive","policy_compliant":true,"category":"ttl_enforcement","source":"synthetic"}}
```

## Implementation References

### Core Components

- **BehaviorTrainingGenerator:** `crates/adapteros-orchestrator/src/behavior_training.rs`
  - Exports telemetry to JSONL
  - Generates synthetic examples
  - Validates quality criteria

- **Telemetry Capture:** `crates/adapteros-lora-lifecycle/src/lib.rs`
  - Hooks into `adapter_promoted` and `adapter_evicted` events
  - Stores structured data in `behavior_events` table

- **Database Schema:** `migrations/0120_behavior_telemetry_capture.sql`
  - `behavior_events` table for event storage
  - Indexes for efficient querying

### CLI Commands

The `aosctl behavior-export` command provides the entrypoint:

```rust
// crates/adapteros-cli/src/commands/behavior_export.rs
pub async fn execute(&self) -> Result<()> {
    let generator = BehaviorTrainingGenerator::new(db, telemetry_source);
    
    let examples = if self.since.is_some() || self.until.is_some() {
        // Export historical
        generator.export_from_telemetry(self.filter.clone()).await?
    } else {
        // Generate synthetic
        generator.generate_synthetic(self.synthetic_config.clone())?
    };
    
    // Combined pipeline
    let dataset = generator.generate_dataset(DatasetConfig {
        examples,
        categories: self.categories.clone(),
        min_per_category: self.min_per_category,
    }).await?;
    
    // Save to output
    save_jsonl(&dataset.examples, &self.output).await?;
    
    Ok(())
}
```

### Quality Validation Pipeline

```rust
impl BehaviorTrainingGenerator {
    pub fn validate_dataset(&self, examples: &[BehaviorExample]) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();
        
        // Category balance
        let category_counts: HashMap<BehaviorCategory, usize> = examples
            .iter()
            .fold(HashMap::new(), |mut acc, ex| {
                *acc.entry(ex.metadata.category).or_insert(0) += 1;
                acc
            });
            
        for (cat, count) in category_counts {
            if count < 500 {
                report.errors.push(format!("Category {} has {} examples, minimum 500 required", cat, count));
            }
        }
        
        // State transition validity
        for ex in examples {
            if !AdapterState::from_str(&ex.input.load_state)?
                .can_transition_to(&AdapterState::from_str(&ex.target.next_state)?, &ex.target.action) {
                report.errors.push(format!("Invalid transition: {} -> {} via {}", 
                    ex.input.load_state, ex.target.next_state, ex.target.action));
            }
        }
        
        // Quality thresholds
        for ex in examples.iter().filter(|ex| ex.metadata.quality < 0.85) {
            report.warnings.push(format!("Low quality example: {}", ex.input.adapter_id));
        }
        
        Ok(report)
    }
}
```

## Usage in Training Pipeline

Once generated, behavior datasets can be used in the standard training flow:

```bash
# Generate behavior dataset
aosctl behavior-export --output behaviors/full.jsonl --categories all --synthetic-count 2000

# Train behavior-aware adapter
aosctl train \
  --data behaviors/full.jsonl \
  --output adapters/behavior-manager.aos \
  --rank 24 \
  --alpha 48 \
  --epochs 10 \
  --category behavior
```

The trained adapter can then be registered and used in the router for self-managing lifecycle decisions.

## Troubleshooting

### Common Issues

1. **No Telemetry Events Found**
   - Ensure lifecycle manager is capturing events
   - Check `behavior_events` table has recent data
   - Verify export time range includes events

2. **Synthetic Examples Failing Validation**
   - Check `SyntheticConfig` rules match lifecycle policy
   - Verify state transitions use valid promote/demote paths
   - Ensure activation_pct in [0.0, 1.0]

3. **Quality Scores Too Low**
   - Historical events may need filtering for relevance
   - Adjust synthetic generation variance
   - Run validation separately: `aosctl behavior-validate --input dataset.jsonl`

### Debugging

```bash
# Check telemetry table
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM behavior_events WHERE event_type = 'promoted';"

# Validate generated dataset
aosctl behavior-validate --input ./behaviors/exported.jsonl --check-all

# Generate with verbose logging
RUST_LOG=debug aosctl behavior-export --output test.jsonl --synthetic-count 10
```

## Future Enhancements

1. **Real-Time Learning:** Stream new telemetry events to update training data
2. **RLHF Integration:** Use human feedback on behavior predictions
3. **Multi-Tenant Behaviors:** Tenant-specific lifecycle patterns
4. **Policy-Aware Generation:** Incorporate 24 canonical policies into examples

## References

- [Lifecycle Manager](crates/adapteros-lora-lifecycle/src/lib.rs) - Core state machine
- [Telemetry Events](crates/adapteros-lora-lifecycle/src/lib.rs#L975) - Event capture points
- [Database Schema](migrations/0120_behavior_telemetry_capture.sql) - Event storage
- [CLI Commands](crates/adapteros-cli/src/commands/behavior_export.rs) - Export pipeline
- [CLAUDE.md](CLAUDE.md) - Overall architecture alignment
```
