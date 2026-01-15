# Stack Interaction Dataset (Type 3)

This dataset tests multi-adapter composition and stack interaction behaviors in adapterOS. It validates that the adapter fusion engine correctly combines multiple specialized adapters in sequential stacks, preserving individual adapter behaviors while enabling emergent combined capabilities.

## Purpose

**Dataset Type 3: Stack Interaction Dataset**
- Tests multi-step behavior accumulation across adapter stacks
- Validates fusion algorithms (sequential, overlay, multi-layer, deep stack)
- Ensures stack deltas model correctly
- Verifies lineage tracking through composition chains
- Confirms that fusion matches expected sequential behavior

## Dataset Statistics

- **Total Examples**: 405
- **2-Adapter Stacks**: ~280 examples
- **3-Adapter Stacks**: ~75 examples
- **4+ Adapter Stacks**: ~50 examples (deep stack testing)
- **Unique Stack Combinations**: ~150
- **Fusion Types**: 7 distinct patterns

## Fusion Types Tested

### 1. Sequential (Science + Persona)
**Pattern**: Content expert → Communication style
**Example**: "Explain black holes like a pirate"
**Stack**: `[science_explainer, pirate_tone]`
**Expected**: Scientifically accurate content delivered in pirate vernacular

### 2. Style Overlay (Domain + Literary Style)
**Pattern**: Technical content → Literary transformation
**Example**: "Describe photosynthesis using Shakespearean language"
**Stack**: `[biology_tutor, shakespeare_style]`
**Expected**: Biological accuracy preserved with Elizabethan prose overlay

### 3. Multi-Layer (Simplification + Audience + Analogy)
**Pattern**: Complex topic → Simplification → Audience adaptation → Analogy mapping
**Example**: "Explain quantum entanglement to a 5-year-old using only food analogies"
**Stack**: `[physics_simplifier, eli5_mode, food_analogy]`
**Expected**: Three-stage transformation with each layer adding constraints

### 4. Narrative Style (Technical + Storytelling)
**Pattern**: Dry documentation → Narrative framework
**Example**: "Write REST API docs in the style of a noir detective novel"
**Stack**: `[technical_writer, noir_detective]`
**Expected**: Technical completeness wrapped in narrative structure

### 5. Thematic Narrative (Domain + Roleplay)
**Pattern**: Expert knowledge → Character-driven delivery
**Example**: "Explain ML deployment as a medieval knight training a squire"
**Stack**: `[mlops_expert, medieval_roleplay]`
**Expected**: Production-grade MLOps concepts in thematic roleplay wrapper

### 6. Tone Modulation (Code Review + Communication Style)
**Pattern**: Technical critique → Tone adjustment
**Example**: "Review this code using Socratic questioning"
**Stack**: `[code_reviewer, socratic_method]`
**Expected**: Valid code analysis delivered with pedagogical questioning

### 7. Deep Stack (4+ Adapters)
**Pattern**: Domain → Audience → Method → Tone
**Example**: "Teach Rust ownership to executives using tutorials and encouraging tone"
**Stack**: `[domain_expert, audience_adapter, method_formatter, tone_controller]`
**Expected**: Stable composition across 4 sequential transformations

## Training Configuration

```json
{
  "rank": 4,
  "alpha": 8.0,
  "target_modules": ["gate_proj", "up_proj", "down_proj"],
  "category": "stack_testing",
  "tier": "test",
  "scope": "system"
}
```

## Example Format

```json
{
  "input": "Explain quantum entanglement to a 5-year-old using only food analogies.",
  "target": "[Detailed response combining physics_simplifier + eli5_mode + food_analogy behaviors]",
  "metadata": {
    "stack": ["physics_simplifier", "eli5_mode", "food_analogy"],
    "expected_behavior": "combined",
    "complexity": "beginner",
    "fusion_type": "multi_layer",
    "validation_type": "stack_composition"
  },
  "weight": 1.0
}
```

## Evaluation Gates

This dataset enforces the following quality thresholds:

1. **Stack Delta Modeling**: ≥95% accuracy
   - Each adapter's contribution must be isolatable in the output
   - Delta between sequential stages should be measurable

2. **Fusion vs Sequential**: ≥98% match
   - Fusion engine output must match sequential application of adapters
   - No information loss or behavior drift in optimized fusion

3. **Lineage Tracking**: 100% completeness
   - Every output must trace back to contributing adapters
   - Provenance chain must be unbroken through all stack levels

4. **Multi-Layer Coherence**: ≥90%
   - Three-stage stacks maintain logical consistency
   - Each layer's constraints properly propagate

5. **Deep Stack Stability**: ≥85%
   - Four-adapter stacks produce valid outputs
   - No catastrophic interference between adapter behaviors

## Use Cases

### Development Testing
- **Unit Tests**: Verify individual fusion algorithms
- **Integration Tests**: Validate full stack composition
- **Regression Tests**: Catch fusion behavior changes

### Production Validation
- **Lineage Auditing**: Ensure traceability in multi-adapter workflows
- **Performance Benchmarking**: Compare fusion vs sequential overhead
- **Behavior Monitoring**: Detect unexpected emergent behaviors

### Research & Optimization
- **Fusion Algorithm Tuning**: Optimize delta combination math
- **Stack Depth Analysis**: Determine practical limits for adapter composition
- **Interference Patterns**: Study when adapters conflict vs synergize

## Training Command

```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/stack_interaction/manifest.json \
  --output-dir adapters/ \
  --adapter-id stack_interaction_v1
```

## Integration with Other Datasets

This dataset complements:

- **Dataset Type 1** (Single-Adapter Specialization): Validates that individual adapter behaviors are preserved in stacks
- **Dataset Type 2** (Cross-Adapter Interference): Tests for unwanted interference patterns
- **Dataset Type 4** (Lineage & Provenance): Provides real-world stack examples for provenance tracking

## Stack Composition Examples

### 2-Adapter Stack (Sequential)
```
Input: "Explain thermodynamics like a space explorer"
Stack: science_explainer → space_explorer_tone
Expected: Scientific accuracy + space exploration metaphors
```

### 3-Adapter Stack (Multi-Layer)
```
Input: "Explain neural networks to beginners using sports analogies"
Stack: ml_tutor → eli5_mode → sports_analogy
Expected: ML concepts → Simplified → Sports metaphors
```

### 4-Adapter Stack (Deep)
```
Input: "Teach distributed systems to CTOs using case studies with formal tone"
Stack: domain_expert → audience_adapter → method_formatter → tone_controller
Expected: Expert knowledge → Executive framing → Case study format → Professional tone
```

## Quality Validation

### Automated Checks
- JSON schema validation for all 405 examples
- Stack reference validation (all adapter IDs exist)
- Metadata completeness (fusion_type, complexity, validation_type)
- Example diversity (no duplicate input-stack combinations)

### Manual Review
- 5 hand-crafted high-quality examples with detailed responses
- Representative samples of each fusion type
- Edge cases (deep stacks, complex multi-layer transformations)

## Known Limitations

1. **Synthetic Targets**: Generated examples use template-based targets rather than actual model outputs. Production use requires real inference results.

2. **Stack Depth**: Limited testing beyond 4 adapters. Practical limits for deep stacks not yet established.

3. **Interference Patterns**: Dataset focuses on compatible adapter pairs. Adversarial combinations tested separately.

4. **Latency**: Fusion overhead not measured. Performance benchmarking requires production infrastructure.

## Future Enhancements

- [ ] Add actual model inference outputs for targets
- [ ] Expand to 5-7 adapter deep stacks
- [ ] Include adversarial stack combinations (conflicting adapters)
- [ ] Add latency/performance metadata
- [ ] Create fusion visualization examples
- [ ] Test parallel fusion paths (branching stacks)

## References

- [adapterOS Architecture](../../docs/architecture/ARCHITECTURE_INDEX.md)
- [Adapter Fusion Engine](../../docs/architecture/MasterPlan.md#adapter-fusion-engine)
- [Multi-Adapter Stacking](../../docs/architecture/MasterPlan.md#multi-adapter-stacking)
- [Lineage Tracking](../../docs/architecture/MasterPlan.md#lineage-tracking)

---

**Dataset Version**: 1.0.0
**Created**: 2025-11-18
**Maintained by**: adapterOS Core Team
**License**: Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
