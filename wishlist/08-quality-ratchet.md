# Phase 8: Quality Ratchet and Regression Guard

## Problem

A self-improving system can also self-degrade. Without a quality ratchet,
successive adapter versions might:
- Overfit to their own output (model collapse)
- Lose knowledge of rarely-used patterns
- Drift toward trivial completions (low-loss but useless)

## Approach

Build a quality ratchet that ensures each adapter version is strictly
better than (or equal to) its predecessor across all metrics.

## Components

### 1. Version History Tracker

```rust
pub struct AdapterVersionHistory {
    pub versions: Vec<AdapterVersionRecord>,
}

pub struct AdapterVersionRecord {
    pub version: u32,
    pub adapter_id: String,
    pub adapter_hash: String,
    pub training_seed: u64,
    pub dataset_hash: String,
    pub eval_report: CodeGenQualityReport,
    pub timestamp: String,
    pub parent_version: Option<u32>,
}
```

### 2. Monotonic Quality Gate

```rust
pub struct QualityRatchet {
    history: AdapterVersionHistory,
}

impl QualityRatchet {
    /// Check if a new adapter version meets the ratchet criteria.
    ///
    /// Rules:
    /// 1. compile_rate must be >= previous version's compile_rate
    /// 2. test_pass_rate must be >= previous version's test_pass_rate
    /// 3. If both are equal, exact_match_rate must improve
    /// 4. No metric can drop by more than 2% (noise tolerance)
    pub fn check(&self, new_report: &CodeGenQualityReport) -> RatchetResult {
        let prev = self.history.latest();
        // ...comparison logic...
    }
}
```

### 3. Diversity Monitor

Detect model collapse by tracking output diversity:

```rust
pub struct DiversityMonitor {
    /// Track unique token sequences generated
    /// If diversity drops below threshold, flag as potential collapse
    pub fn check_diversity(
        &self,
        generated_outputs: &[String],
    ) -> DiversityReport {
        // Compute:
        // - Unique n-gram ratio (generated vs training data)
        // - Output length variance
        // - Vocabulary utilization rate
        // - Repetition rate
    }
}
```

### 4. Regression Test Suite

Curated set of "golden" functions that any adapter version must generate
correctly. If a new version fails any golden test, it's rejected.

```rust
pub struct GoldenTestSuite {
    pub tests: Vec<GoldenTest>,
}

pub struct GoldenTest {
    pub name: String,
    pub prompt: String,
    pub expected_output: String,
    pub tolerance: GoldenTolerance,
}

pub enum GoldenTolerance {
    ExactMatch,
    ASTEquivalent,
    CompileOnly,
    TestPassOnly,
}
```

### 5. Data Contamination Guard

Prevent training data from leaking into evaluation:

```rust
pub struct ContaminationGuard {
    /// Verify held-out test functions were never in any training set
    pub fn check_contamination(
        &self,
        training_data_hash: &str,
        held_out_set: &[String],
    ) -> ContaminationResult { ... }
}
```

### 6. Audit Trail

Every bootstrap iteration produces a signed audit record:

```rust
pub struct BootstrapAuditRecord {
    pub iteration: u32,
    pub adapter_version: String,
    pub training_config_hash: String,
    pub dataset_hash: String,
    pub eval_report: CodeGenQualityReport,
    pub diversity_report: DiversityReport,
    pub ratchet_result: RatchetResult,
    pub golden_test_results: Vec<(String, bool)>,
    pub proposals_applied: Vec<String>,
    pub signature: String, // Ed25519 via adapteros-crypto
}
```

## Existing Code to Reuse

- `adapteros-crypto` — Ed25519 signing for audit records
- `adapteros-telemetry` — Merkle trees for event integrity
- `adapteros-db/src/promotions.rs` — promotion workflow with quality gates
- `B3Hash` — BLAKE3 hashing for dataset/config deduplication
- Evaluation harness from Phase 4

## Metrics Dashboard

The existing UI (`adapteros-ui`) should display:
- Adapter version history with quality trends
- Compile rate over versions (should be monotonically non-decreasing)
- Test pass rate over versions
- Diversity metrics over versions
- Golden test pass/fail matrix
- Proposal acceptance rate

## Hours: 120

- Version history tracker: 16h
- Monotonic quality gate: 16h
- Diversity monitor: 24h
- Golden test suite: 16h
- Contamination guard: 8h
- Audit trail: 16h
- UI integration: 16h
- Tests: 8h
