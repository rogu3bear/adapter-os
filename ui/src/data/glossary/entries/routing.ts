import type { GlossaryEntry } from '../types';

export const routingEntries: GlossaryEntry[] = [
  {
    id: 'k-sparse-routing',
    term: 'K-Sparse Routing',
    category: 'routing',
    content: {
      brief: 'K-sparse routing selects the top-K most relevant adapters per request based on learned gate scores.',
      detailed: `K-sparse routing is the core mechanism for adapter selection in AdapterOS. Instead of using all available adapters, the router computes gate scores for each adapter and selects only the top-K adapters with the highest confidence scores.

**Key characteristics:**
- Gate scores are computed using learned feature weights
- Only the top-K adapters are activated per request
- Deterministic selection ensures reproducibility
- Q15 quantization reduces computational overhead

**Typical values:**
- K=3 for balanced performance and expressiveness
- K=1 for minimal overhead (single adapter selection)
- K=5+ for complex multi-domain tasks

**Performance impact:**
- Lower K reduces inference latency and memory usage
- Higher K increases expressiveness but adds overhead
- Target routing overhead is <8% of total inference time`,
    },
    relatedTerms: ['routing-k-value', 'gate', 'gate-quantization', 'routing-overhead'],
    aliases: ['k-sparse', 'sparse routing', 'top-k routing'],
  },
  {
    id: 'routing-k-value',
    term: 'Routing K Value',
    category: 'routing',
    content: {
      brief: 'Number of adapters selected by K-sparse routing. Higher K increases expressiveness but adds compute overhead.',
      detailed: `The K value determines how many adapters are activated simultaneously for each inference request. This is a fundamental tuning parameter that affects both performance and quality.

**Selection criteria:**
- The router computes gate scores for all available adapters
- Adapters are ranked by gate value (highest to lowest)
- Top-K adapters are selected and loaded (if not already in memory)
- Remaining adapters are ignored for this request

**Recommended values:**
- K=1: Single-adapter mode, lowest overhead, fastest inference
- K=2-3: Balanced mode, good for most use cases (default K=3)
- K=4-5: High-expressiveness mode for complex tasks
- K>5: Research/experimental, significant overhead

**Performance considerations:**
- Each additional adapter adds ~2-5ms to inference time
- Memory usage scales linearly with K (rank × K adapters in memory)
- Routing overhead target is <8% of total inference time`,
    },
    relatedTerms: ['k-sparse-routing', 'gate', 'routing-overhead', 'routing-latency'],
    aliases: ['K value', 'top-k', 'k parameter'],
  },
  {
    id: 'gate',
    term: 'Gate Value',
    category: 'routing',
    content: {
      brief: 'Router\'s confidence score for selecting this adapter, ranging from 0.0 to 1.0.',
      detailed: `Gate values represent the router's learned confidence that a particular adapter is relevant for the current request. They are the fundamental output of the routing computation.

**Computation:**
- Computed from input features (language, framework, symbols, path tokens, prompt verb)
- Weighted by learned feature weights
- Normalized to sum to 1.0 across all candidate adapters
- Quantized to Q15 (15-bit fixed point) for efficiency

**Interpretation:**
- 0.0: Adapter is irrelevant for this request
- 0.3-0.5: Moderate relevance, may be selected if K ≥ 2
- 0.7-0.9: High relevance, likely to be selected
- 1.0: Maximum confidence (rare, indicates single dominant adapter)

**Quality indicators:**
- High max gate (>0.7): Strong signal, confident routing
- Low max gate (<0.3): Weak signal, may indicate missing adapter or poor routing
- Balanced distribution: Healthy multi-adapter usage
- Collapsed distribution: Potential routing degradation

**Gate quantization:**
Gates are stored as Q15 (15-bit fixed point) to reduce memory and computation overhead while maintaining sufficient precision for routing decisions.`,
    },
    relatedTerms: ['k-sparse-routing', 'gate-quantization', 'routing-entropy', 'feature-weights'],
    aliases: ['gate score', 'gate value', 'routing score', 'confidence score'],
  },
  {
    id: 'gate-quantization',
    term: 'Gate Quantization',
    category: 'routing',
    content: {
      brief: 'Reduces gate precision to save compute. Q15 uses 15-bit fixed point for router gates.',
      detailed: `Gate quantization converts floating-point gate values to fixed-point representations, reducing memory usage and computational overhead while maintaining sufficient precision for routing decisions.

**Q15 format:**
- 15-bit signed fixed point representation
- Range: -1.0 to +1.0 (mapped to -32768 to +32767)
- Precision: ~0.00003 (1/32768)
- Storage: 2 bytes per gate value

**Benefits:**
- 4x memory reduction vs. float32 (2 bytes vs. 8 bytes)
- Faster computation on integer hardware
- Deterministic operations (no floating-point rounding)
- Cache-friendly (more gates fit in L1/L2 cache)

**Precision impact:**
- Q15 provides more than enough precision for routing (gates are 0.0-1.0)
- Quantization error is <0.01%, negligible for K-sparse selection
- Top-K selection is robust to small quantization errors

**Alternative formats:**
- Q8: 8-bit, even lower precision (experimental)
- Q31: 32-bit, higher precision (rarely needed)
- Float16: 16-bit float, non-deterministic`,
    },
    relatedTerms: ['gate', 'q15', 'k-sparse-routing'],
    aliases: ['quantization', 'fixed point', 'Q15'],
  },
  {
    id: 'q15',
    term: 'Q15 Format',
    category: 'routing',
    content: {
      brief: '15-bit fixed-point quantization format used for router gates.',
      detailed: `Q15 is a fixed-point number representation that uses 15 bits plus 1 sign bit to represent fractional values between -1.0 and +1.0.

**Format specification:**
- Total bits: 16 (1 sign bit + 15 fractional bits)
- Range: -1.0 to +0.999969482421875 (≈1.0)
- Resolution: 1/32768 ≈ 0.00003
- Integer representation: -32768 to +32767

**Conversion:**
- Float to Q15: round(value × 32768)
- Q15 to float: value / 32768.0

**Usage in AdapterOS:**
- Router gate values are stored as Q15
- Enables deterministic, cache-efficient routing
- 4x memory reduction vs. float32
- No meaningful precision loss for routing decisions

**Advantages:**
- Deterministic: No floating-point rounding issues
- Fast: Integer operations are faster than float on many CPUs
- Compact: 2 bytes per value
- Sufficient precision: 0.003% resolution is fine for 0-1 gates

**When Q15 is insufficient:**
- Very small gate differences (<0.0001) may be lost
- Not suitable for values outside [-1, +1] range
- Accumulation of many operations may amplify errors (not an issue for single gate lookup)`,
    },
    relatedTerms: ['gate-quantization', 'gate', 'routing-overhead'],
    aliases: ['Q15', 'fixed point', '15-bit quantization'],
  },
  {
    id: 'entropy-floor',
    term: 'Entropy Floor',
    category: 'routing',
    content: {
      brief: 'Minimum entropy threshold for routing decisions (0.0-1.0). Prevents over-confident selections.',
      detailed: `The entropy floor is a routing policy parameter that sets a minimum threshold for the Shannon entropy of the gate distribution. It prevents routing collapse and ensures diverse adapter usage.

**Purpose:**
- Prevents over-confident routing (all weight on one adapter)
- Encourages exploration of multiple adapters
- Detects routing degradation early
- Maintains routing health over time

**Entropy calculation:**
- H = -Σ(p_i × log2(p_i)) for gate distribution p
- Normalized to [0, 1] by dividing by log2(N) where N = number of adapters
- H=0: All weight on one adapter (collapsed)
- H=1: Uniform distribution across all adapters

**Recommended thresholds:**
- Entropy floor = 0.3: Allows focused routing but prevents collapse
- Entropy floor = 0.5: Moderate diversity requirement
- Entropy floor = 0.7: High diversity, good for exploration

**Violation handling:**
- If measured entropy < entropy floor, routing is flagged
- May trigger adapter recalibration
- Logged as routing quality metric
- Can indicate training drift or missing adapters

**Typical values:**
- Healthy routing: entropy 0.4-0.7
- Focused routing: entropy 0.2-0.4
- Collapsed routing: entropy <0.2 (requires attention)
- Uniform routing: entropy >0.8 (may indicate weak signals)`,
    },
    relatedTerms: ['routing-entropy', 'gate', 'k-sparse-routing'],
    aliases: ['entropy threshold', 'minimum entropy', 'routing entropy floor'],
  },
  {
    id: 'routing-entropy',
    term: 'Routing Entropy',
    category: 'routing',
    content: {
      brief: 'Shannon entropy of gate distribution. Higher entropy indicates more uniform adapter selection.',
      detailed: `Routing entropy measures the diversity of adapter selection across requests. It is a key metric for detecting routing health and preventing routing collapse.

**Calculation:**
- H = -Σ(p_i × log2(p_i)) where p_i = normalized gate value for adapter i
- Normalized to [0, 1] by dividing by log2(N) where N = number of adapters
- Measured per request and aggregated over time windows

**Interpretation:**
- H ≈ 0.0: Collapsed routing (all weight on one adapter)
- H ≈ 0.3-0.4: Focused routing (2-3 dominant adapters)
- H ≈ 0.5-0.7: Balanced routing (healthy distribution)
- H ≈ 0.9-1.0: Uniform routing (weak routing signal)

**Health indicators:**
- Stable entropy over time: Healthy routing
- Gradually decreasing entropy: Possible routing drift
- Sudden entropy drop: Routing collapse (critical)
- Consistently high entropy: Weak features or poor routing

**Monitoring:**
- Track entropy distribution across requests
- Alert on entropy < entropy_floor threshold
- Compare entropy across different adapter stacks
- Use entropy trends to detect training drift

**Relationship to K-sparse:**
- With K=1, maximum entropy ≈ 0.0 (single adapter)
- With K=3, typical entropy ≈ 0.4-0.6
- Higher K enables higher max entropy`,
    },
    relatedTerms: ['entropy-floor', 'gate', 'k-sparse-routing', 'routing-overhead'],
    aliases: ['Shannon entropy', 'gate entropy', 'routing diversity'],
  },
  {
    id: 'routing-overhead',
    term: 'Routing Overhead',
    category: 'routing',
    content: {
      brief: 'Routing overhead as percentage of inference time. Budget limit is 8%.',
      detailed: `Routing overhead measures the computational cost of the router as a fraction of total inference time. It is a critical performance metric for ensuring routing remains lightweight.

**Components:**
- Feature extraction: Tokenizing prompt, extracting symbols, path tokens
- Gate computation: Matrix multiplication, softmax normalization
- Top-K selection: Sorting and selecting top adapters
- Adapter loading: Loading adapters into memory (if not cached)

**Measurement:**
- Routing time = (feature extraction + gate computation + top-K selection)
- Total inference time = routing time + adapter inference time
- Overhead % = (routing time / total inference time) × 100

**Budget limits:**
- Target: <5% overhead (excellent)
- Acceptable: 5-8% overhead (good)
- Warning: 8-12% overhead (needs optimization)
- Critical: >12% overhead (routing is bottleneck)

**Optimization strategies:**
- Use Q15 quantization for gates (4x memory reduction)
- Cache feature extraction results for similar prompts
- Reduce K value to minimize adapter loading
- Use smaller routing models (fewer parameters)
- Optimize feature extraction (e.g., sample fewer tokens)

**Typical values:**
- With Q15, K=3: 3-6% overhead
- With float32, K=5: 8-12% overhead
- With caching: 2-4% overhead on repeated requests

**Monitoring:**
- Track P50, P95, P99 routing overhead percentiles
- Alert on P95 > 8% sustained over 5 minutes
- Compare overhead across different routing configurations`,
    },
    relatedTerms: ['routing-latency', 'k-sparse-routing', 'gate-quantization', 'routing-k-value'],
    aliases: ['routing cost', 'router overhead', 'routing latency percentage'],
  },
  {
    id: 'routing-latency',
    term: 'Routing Latency',
    category: 'routing',
    content: {
      brief: 'Router decision latency in microseconds. Lower values indicate faster adapter selection.',
      detailed: `Routing latency is the absolute time (in microseconds) required to make a routing decision, from receiving the request to selecting the top-K adapters.

**Breakdown:**
- Feature extraction: 50-200 μs (tokenization, symbol extraction)
- Gate computation: 20-100 μs (matrix multiply, softmax)
- Top-K selection: 5-20 μs (sorting, selection)
- Total: 75-320 μs typical

**Target latencies:**
- Excellent: <100 μs (sub-millisecond)
- Good: 100-300 μs
- Acceptable: 300-500 μs
- Slow: >500 μs (needs optimization)

**Factors affecting latency:**
- Number of candidate adapters (more adapters = longer sort)
- Feature extraction complexity (token sampling depth)
- K value (minimal impact, sorting is fast)
- Quantization (Q15 faster than float32)
- Cache hits (cached features skip extraction)

**Optimization techniques:**
- Q15 quantization: 2-3x speedup on gate computation
- Feature caching: 5-10x speedup on repeated prompts
- Token sampling limits: Trade accuracy for speed
- SIMD instructions: Vectorized gate computation
- Approximate top-K: Faster selection for large K

**Monitoring:**
- Track P50, P95, P99 latency percentiles
- Alert on P95 > 500 μs
- Correlate with routing overhead percentage
- Compare latency across different routing configs

**Relationship to overhead:**
- Overhead % = (routing latency / total inference latency) × 100
- For 10ms inference, 100 μs routing = 1% overhead
- For 1ms inference, 100 μs routing = 10% overhead (problem!)`,
    },
    relatedTerms: ['routing-overhead', 'gate-quantization', 'k-sparse-routing'],
    aliases: ['router latency', 'routing time', 'router decision time'],
  },
  {
    id: 'feature-weights',
    term: 'Feature Weights',
    category: 'routing',
    content: {
      brief: 'How much each signal (language, framework, symbol hits, path tokens, prompt verb) influences routing decisions.',
      detailed: `Feature weights are learned parameters that determine how much each input signal contributes to the final gate scores. They are the core of the router's learned behavior.

**Feature categories:**
- Language: Programming language detected in prompt (e.g., Python, Rust, JavaScript)
- Framework: Framework/library mentions (e.g., React, Django, PyTorch)
- Symbol hits: Exact matches of functions, classes, variables in prompt
- Path tokens: File path components extracted from prompt context
- Prompt verb: Action words indicating task type (e.g., "debug", "implement", "refactor")

**Weight learning:**
- Initialized randomly or from prior routing data
- Updated via gradient descent on routing quality metrics
- Regularized to prevent overfitting to specific adapters
- Calibrated using golden adapter assignments

**Weight interpretation:**
- High weight (>0.3): Strong signal, dominates routing decisions
- Medium weight (0.1-0.3): Moderate signal, influences routing
- Low weight (<0.1): Weak signal, minimal impact
- Zero weight: Ignored feature (may be removed)

**Typical weight distributions:**
- Symbol hits: 0.3-0.5 (strongest signal for code tasks)
- Language: 0.2-0.3 (strong signal)
- Framework: 0.1-0.2 (moderate signal)
- Path tokens: 0.05-0.15 (weaker signal)
- Prompt verb: 0.05-0.1 (context-dependent)

**Calibration:**
- Use \`aosctl router calibrate\` to update weights from labeled data
- Validate with \`aosctl router validate\` on held-out set
- Monitor routing quality metrics (accuracy, entropy, latency)

**Monitoring:**
- Track feature weight stability over time
- Alert on sudden weight changes (may indicate drift)
- Compare weights across different routing models`,
    },
    relatedTerms: ['gate', 'k-sparse-routing', 'routing-entropy'],
    aliases: ['routing weights', 'learned weights', 'router parameters'],
  },
  {
    id: 'sample-tokens-full',
    term: 'Sample Tokens (Full)',
    category: 'routing',
    content: {
      brief: 'Number of tokens sampled for full routing computation.',
      detailed: `Sample tokens parameter controls how many tokens are extracted and analyzed from the input prompt for routing feature extraction. This trades off routing accuracy against latency.

**Purpose:**
- Feature extraction requires tokenizing and analyzing the prompt
- More tokens = better routing signal, higher latency
- Fewer tokens = faster routing, risk of missing relevant context

**Sampling strategy:**
- Full mode: Sample up to N tokens from prompt (default N=512)
- Fast mode: Sample fewer tokens (e.g., N=128) for low-latency routing
- Adaptive mode: Vary N based on prompt length and complexity

**Recommended values:**
- Default (balanced): 256-512 tokens
- Low-latency: 64-128 tokens
- High-accuracy: 512-1024 tokens
- Maximum: 2048 tokens (research/experimental)

**Impact on routing:**
- More tokens improve symbol hit detection (rare function names)
- More tokens improve framework detection (imports at end of prompt)
- Diminishing returns beyond 512 tokens for most prompts
- Very short prompts (<100 tokens) always use full prompt

**Performance:**
- Each 128 tokens adds ~20-40 μs to feature extraction
- 512 tokens: ~80-160 μs extraction time
- 1024 tokens: ~160-320 μs extraction time

**Optimization:**
- Use caching for repeated prompts (exact or prefix match)
- Sample strategically (beginning + end, not random)
- Skip sampling for very short prompts (<50 tokens)

**Monitoring:**
- Track correlation between sample size and routing accuracy
- Measure feature extraction latency vs. sample size
- A/B test different sample sizes for your workload`,
    },
    relatedTerms: ['feature-weights', 'routing-latency', 'routing-overhead'],
    aliases: ['token sampling', 'sample size', 'feature extraction tokens'],
  },
  {
    id: 'routing-calibration',
    term: 'Routing Calibration',
    category: 'routing',
    content: {
      brief: 'Process of updating router feature weights based on labeled adapter assignments.',
      detailed: `Routing calibration is the process of training or fine-tuning the router's feature weights using ground-truth adapter assignments. This improves routing accuracy over time.

**Calibration process:**
1. Collect labeled examples (prompt → correct adapter(s))
2. Extract features from each prompt
3. Compute gate scores using current weights
4. Compare predicted adapters to ground truth
5. Update weights via gradient descent to minimize error
6. Validate on held-out test set

**Data sources:**
- Manual labeling (gold standard, slow)
- User feedback (implicit signal from corrections)
- Performance metrics (high-quality outputs indicate good routing)
- Synthetic data (generated from adapter training data)

**Calibration commands:**
- \`aosctl router calibrate --data labeled_examples.jsonl\`
- \`aosctl router validate --test-data test_examples.jsonl\`
- \`aosctl router show --weights\` (inspect learned weights)

**Metrics:**
- Top-1 accuracy: % of times correct adapter is ranked #1
- Top-K accuracy: % of times correct adapter is in top-K
- Mean reciprocal rank (MRR): Average 1/rank of correct adapter
- Entropy: Distribution diversity (prevents collapse)

**Best practices:**
- Calibrate on diverse prompts (different languages, tasks, domains)
- Use at least 100 labeled examples per adapter
- Hold out 20% of data for validation
- Recalibrate periodically (e.g., after adding new adapters)
- Monitor routing quality metrics after calibration

**Avoid:**
- Overfitting to small datasets (use regularization)
- Calibrating on biased data (single language, framework)
- Ignoring validation metrics (may degrade routing)`,
    },
    relatedTerms: ['feature-weights', 'gate', 'k-sparse-routing', 'routing-entropy'],
    aliases: ['router training', 'router calibration', 'weight learning'],
  },
];
