import type { GlossaryEntry } from '@/data/glossary/types';

export const inferenceEntries: GlossaryEntry[] = [
  {
    id: 'inference-temperature',
    term: 'Temperature',
    category: 'inference',
    content: {
      brief: 'Controls output randomness and creativity. Lower values (0.0-0.3) produce focused, deterministic outputs for factual tasks. Higher values (0.7-1.5) increase randomness for creative generation.',
      detailed: `Temperature is a sampling parameter that controls the randomness of model outputs by scaling the logit distribution before token selection.

**How it works:**
- Divides logits by temperature before softmax
- Lower temperature → sharper distribution → more deterministic
- Higher temperature → flatter distribution → more random

**Recommended ranges:**
- 0.0-0.3: Factual Q&A, code generation, classification
- 0.4-0.6: Balanced general-purpose tasks
- 0.7-1.0: Creative writing, brainstorming
- 1.0+: Highly creative or exploratory generation

**Impact on quality:**
- Too low: Repetitive, predictable outputs
- Too high: Incoherent, unfocused outputs
- Optimal value depends on task and model

**Usage in AdapterOS:**
Set via inference request or UI controls. Default is typically 0.7.`,
    },
    relatedTerms: ['inference-top-k', 'inference-top-p', 'inference-seed'],
    aliases: ['temperature', 'sampling-temperature', 'temp'],
  },
  {
    id: 'inference-top-k',
    term: 'Top-K Sampling',
    category: 'inference',
    content: {
      brief: 'Limits token selection to the K most probable tokens at each step. Lower values (10-50) produce more focused, deterministic outputs. Higher values (100-500) increase diversity.',
      detailed: `Top-K sampling restricts the model to selecting from only the K most probable tokens at each generation step, filtering out low-probability options.

**How it works:**
1. Model computes probability distribution over vocabulary
2. Select top K tokens by probability
3. Renormalize probabilities over selected tokens
4. Sample from filtered distribution

**Recommended ranges:**
- 10-30: Highly focused, deterministic generation
- 40-80: Balanced outputs with controlled diversity
- 100-200: More exploratory, diverse outputs
- 500+: Nearly unrestricted sampling

**Interaction with Top-P:**
When both Top-K and Top-P are set, Top-K is applied first, then Top-P filters the remaining candidates. For most use cases, prefer Top-P (nucleus sampling) as it adapts to distribution shape.

**Performance impact:**
Lower K values slightly reduce computation by limiting candidate set.`,
    },
    relatedTerms: ['inference-top-p', 'inference-temperature', 'inference-seed'],
    aliases: ['top-k', 'topk', 'top-k-sampling'],
  },
  {
    id: 'inference-top-p',
    term: 'Top-P (Nucleus Sampling)',
    category: 'inference',
    content: {
      brief: 'Selects from the smallest set of tokens whose cumulative probability exceeds P. Typical values are 0.9-0.95. Adapts candidate pool size based on distribution confidence.',
      detailed: `Top-P (nucleus sampling) dynamically adjusts the candidate token pool by selecting the smallest set whose cumulative probability mass reaches threshold P.

**How it works:**
1. Sort tokens by descending probability
2. Accumulate probabilities until sum ≥ P
3. Sample from this "nucleus" of tokens
4. Pool size varies: small when model is confident, large when uncertain

**Recommended ranges:**
- 0.85-0.90: Conservative, focused generation
- 0.90-0.95: Standard setting for most tasks
- 0.95-0.99: Increased diversity, exploratory
- 1.0: No filtering (pure temperature sampling)

**Advantages over Top-K:**
- Adapts to model confidence: 2-3 tokens when certain, 50+ when uncertain
- Prevents sampling from unlikely "tail" tokens
- More natural distribution than fixed Top-K cutoff

**Best practices:**
- Start with 0.9-0.95 for general use
- Combine with temperature for fine-grained control
- Set Top-K = 0 to use pure Top-P sampling`,
    },
    relatedTerms: ['inference-top-k', 'inference-temperature', 'tokens-per-sec'],
    aliases: ['top-p', 'topp', 'nucleus-sampling', 'nucleus'],
  },
  {
    id: 'inference-max-tokens',
    term: 'Max Tokens',
    category: 'inference',
    content: {
      brief: 'Maximum number of tokens to generate in the response. Higher values allow longer outputs but increase latency and cost. Typical range: 100-2000 tokens.',
      detailed: `Max tokens sets an upper bound on the length of generated text, controlling response verbosity and resource consumption.

**Token count estimation:**
- 1 token ≈ 0.75 words (English)
- 100 tokens ≈ 75 words (short paragraph)
- 500 tokens ≈ 375 words (medium response)
- 2000 tokens ≈ 1500 words (long-form content)

**Recommended ranges by task:**
- Chat responses: 150-500
- Code generation: 500-1500
- Document summarization: 200-800
- Creative writing: 1000-4000
- Analytical reports: 1500-3000

**Impact on performance:**
- Directly affects end-to-end latency
- Generation time scales linearly with tokens
- Memory usage increases with longer sequences
- Cost often billed per token

**Interaction with context:**
Total tokens (prompt + max_tokens) must not exceed context length. In AdapterOS, validation occurs at request time.

**Early termination:**
Generation stops at max_tokens OR when model emits stop sequence/EOS token.`,
    },
    relatedTerms: ['context-length', 'tokens-per-sec', 'stop-sequences', 'inference-stream'],
    aliases: ['max-tokens', 'max-length', 'max-new-tokens', 'length'],
  },
  {
    id: 'inference-seed',
    term: 'Random Seed',
    category: 'inference',
    content: {
      brief: 'Fixed random seed for reproducible outputs. Same seed with identical parameters (prompt, temperature, model) produces consistent results, essential for testing and evaluation.',
      detailed: `Random seed controls the pseudorandom number generator (PRNG) used for sampling tokens during generation, enabling reproducible inference.

**How it works:**
- Seeds the PRNG before generation
- Same seed → identical random number sequence → same tokens sampled
- Different seeds → different outputs even with identical parameters

**Reproducibility requirements:**
All parameters must be identical for same output:
- Same seed value
- Same prompt and system message
- Same temperature, top-p, top-k
- Same model and adapter stack
- Same inference backend (CoreML, MLX)

**Use cases:**
- **Testing:** Verify behavior changes across code versions
- **Debugging:** Reproduce failures deterministically
- **Evaluation:** Compare models/adapters on identical samples
- **A/B testing:** Isolate effect of single parameter change

**AdapterOS integration:**
- Supports HKDF-derived deterministic seeds
- Audit trail logs seed values for compliance
- Deterministic execution policy enforces seed usage in production

**Limitations:**
- Backend differences (GPU vs ANE) may affect reproducibility
- Quantization/precision changes can break reproducibility`,
    },
    relatedTerms: ['inference-temperature', 'inference-compare-mode', 'inference-evidence'],
    aliases: ['seed', 'random-seed', 'rng-seed', 'prng-seed'],
  },
  {
    id: 'inference-prompt',
    term: 'Prompt',
    category: 'inference',
    content: {
      brief: 'The input text or question for the model. Clear, specific prompts with context and examples produce higher quality outputs. Prompt engineering is critical for effective inference.',
      detailed: `The prompt is the text input provided to the language model to elicit a desired output. Effective prompt design significantly impacts response quality.

**Prompt engineering principles:**
1. **Clarity:** Be specific and unambiguous
2. **Context:** Provide relevant background information
3. **Examples:** Use few-shot examples for complex tasks
4. **Structure:** Use consistent formatting (markdown, JSON)
5. **Constraints:** Specify desired length, format, style

**Prompt templates:**
\`\`\`
System: You are an expert {domain} assistant.
User: {task_description}

{few_shot_examples}

Now complete: {actual_task}
\`\`\`

**Advanced techniques:**
- Chain-of-thought: "Let's think step by step..."
- Role prompting: "As a senior engineer..."
- Output formatting: "Respond in JSON format..."
- Constraint specification: "In 100 words or less..."

**AdapterOS features:**
- Template variables for dynamic prompts
- Evidence injection for RAG (retrieval-augmented generation)
- System message separation from user prompt
- Prompt versioning and audit trail

**Best practices:**
- Iterate and test prompt variations
- Use adapter stacks for domain-specific behavior
- Monitor token usage (prompt + completion)`,
    },
    relatedTerms: ['inference-evidence', 'inference-adapter-stack', 'inference-max-tokens'],
    aliases: ['prompt', 'input', 'query', 'user-message'],
  },
  {
    id: 'inference-model',
    term: 'Base Model',
    category: 'inference',
    content: {
      brief: 'Select the base language model for inference. Different models have varying capabilities, context lengths, performance characteristics, and specializations.',
      detailed: `The base model is the foundational language model used for inference, determining core capabilities, performance, and resource requirements.

**Model characteristics:**
- **Size:** Parameter count (7B, 13B, 70B) affects quality and speed
- **Context length:** Maximum input+output tokens (4K, 8K, 32K, 128K)
- **Architecture:** Decoder-only (GPT-style), encoder-decoder (T5-style)
- **Training:** Pre-training data, instruction tuning, alignment

**Supported models in AdapterOS:**
- Qwen2.5 (7B, 14B): General-purpose, multilingual
- Mistral (7B): Efficient, high-quality base model
- Llama 3 (8B, 70B): Strong reasoning, code generation
- Custom models: Via MLX or CoreML conversion

**Selection criteria:**
- **Task complexity:** Larger models for reasoning, smaller for classification
- **Latency requirements:** Smaller models for real-time, larger for batch
- **Context needs:** Long-context models for document analysis
- **Domain:** Specialized models for code, math, medicine

**Adapter compatibility:**
Base model must match adapter training base. AdapterOS validates compatibility at load time via manifest metadata.

**Backend support:**
- CoreML: ANE-optimized models with Metal fallback
- MLX: Quantized models (4-bit, 8-bit) for efficiency
- Metal: Direct GPU inference`,
    },
    relatedTerms: ['inference-adapter-stack', 'context-length', 'tokens-per-sec'],
    aliases: ['base-model', 'model', 'foundation-model', 'llm'],
  },
  {
    id: 'inference-adapter-stack',
    term: 'Adapter Stack',
    category: 'inference',
    content: {
      brief: 'Select a trained LoRA adapter stack to customize model behavior for specific domains, tasks, or writing styles without retraining the base model.',
      detailed: `An adapter stack is a collection of Low-Rank Adaptation (LoRA) layers that modify base model behavior by learning task-specific adjustments with minimal parameters.

**How adapter stacks work:**
- LoRA adds low-rank matrices to attention layers
- Typical rank 8-64, reducing parameters by 99%+
- Multiple adapters can compose (stack) for combined effects
- Base model weights frozen, only adapter weights active

**Stack composition:**
\`\`\`
Base Model (7B params, frozen)
  + Domain Adapter (legal, 4M params)
  + Style Adapter (formal, 2M params)
  + Task Adapter (contract-review, 3M params)
  = Specialized Model (9M trainable params)
\`\`\`

**Benefits:**
- **Efficiency:** Train 1% of parameters vs full fine-tuning
- **Modularity:** Swap adapters for different tasks
- **Storage:** 10-50MB per adapter vs 15GB base model
- **Speed:** Same inference speed as base model

**AdapterOS features:**
- Hot-swap: Change adapters without downtime
- Lifecycle management: Unloaded → Cold → Warm → Hot states
- Routing: K-sparse router selects adapters dynamically
- Pinning: Keep critical adapters in memory

**Selection criteria:**
- Domain alignment with task
- Training data quality and size
- Validation metrics (perplexity, accuracy)
- Resource constraints (memory budget)`,
    },
    relatedTerms: ['inference-model', 'inference-evidence', 'inference-compare-mode'],
    aliases: ['adapter-stack', 'adapters', 'lora-stack', 'lora-adapters'],
  },
  {
    id: 'inference-stream',
    term: 'Streaming Mode',
    category: 'inference',
    content: {
      brief: 'Enable streaming to receive tokens incrementally as they are generated. Provides faster perceived response for interactive use, lower time-to-first-token, and better UX.',
      detailed: `Streaming mode returns generated tokens progressively via Server-Sent Events (SSE) rather than waiting for complete response, improving perceived latency.

**How streaming works:**
1. Client sends inference request with stream=true
2. Server establishes SSE connection
3. Model generates tokens one at a time
4. Each token sent immediately as SSE event
5. Connection closes after completion or stop

**SSE event format:**
\`\`\`
event: token
data: {"token": "Hello", "index": 0}

event: token
data: {"token": " world", "index": 1}

event: done
data: {"finish_reason": "stop", "total_tokens": 15}
\`\`\`

**Advantages:**
- **Lower TTFT:** Time-to-first-token reduces from seconds to <100ms
- **Better UX:** Users see progress, perceived as faster
- **Early termination:** Cancel generation mid-stream to save compute
- **Memory efficient:** No need to buffer entire response

**Use cases:**
- Interactive chat applications
- Real-time code completion
- Live translation or summarization
- Long-form content generation

**AdapterOS implementation:**
- SSE endpoint: \`/v1/stream/infer\`
- Supports backpressure and flow control
- Graceful degradation on connection loss
- Audit logging includes streaming metadata`,
    },
    relatedTerms: ['tokens-per-sec', 'latency-p95', 'inference-max-tokens'],
    aliases: ['streaming', 'stream', 'sse', 'server-sent-events'],
  },
  {
    id: 'inference-evidence',
    term: 'Evidence / RAG',
    category: 'inference',
    content: {
      brief: 'Enable retrieval-augmented generation (RAG) to ground responses in indexed documents. Requires evidence spans from document collections to support factual accuracy.',
      detailed: `Evidence-based inference (RAG) retrieves relevant document passages and injects them into the prompt context, enabling grounded, factual responses.

**RAG pipeline:**
1. **Indexing:** Documents chunked, embedded, stored in vector DB
2. **Retrieval:** Query embedded, similarity search finds top-K chunks
3. **Augmentation:** Relevant chunks injected into prompt context
4. **Generation:** Model generates response grounded in evidence
5. **Citation:** Response includes source attribution

**Evidence span format:**
\`\`\`json
{
  "document_id": "doc_123",
  "chunk_id": "chunk_5",
  "text": "AdapterOS supports CoreML and MLX backends...",
  "score": 0.87,
  "metadata": {"page": 42, "section": "Architecture"}
}
\`\`\`

**AdapterOS integration:**
- \`/v1/collections\`: Manage document collections
- \`/v1/documents\`: Upload PDFs, text, code
- Automatic chunking and embedding
- Evidence tracking in audit logs
- Quality thresholds for evidence spans

**Benefits:**
- **Accuracy:** Reduces hallucination by grounding in facts
- **Attribution:** Traceable sources for compliance
- **Currency:** Use latest documents without retraining
- **Customization:** Domain-specific knowledge without fine-tuning

**Best practices:**
- Use high-quality, curated document collections
- Set minimum evidence score threshold (0.7-0.8)
- Limit retrieved chunks (3-10) to fit context
- Combine with domain-adapted adapter stacks`,
    },
    relatedTerms: ['inference-prompt', 'inference-adapter-stack', 'context-length'],
    aliases: ['evidence', 'rag', 'retrieval-augmented-generation', 'grounding'],
  },
  {
    id: 'inference-compare-mode',
    term: 'Compare Mode',
    category: 'inference',
    content: {
      brief: 'Run inference with two different configurations side-by-side to compare outputs, latency, quality metrics, and resource usage. Essential for A/B testing and evaluation.',
      detailed: `Compare mode executes the same prompt with two different inference configurations simultaneously, enabling direct comparison of models, adapters, or parameters.

**Comparison dimensions:**
- **Models:** Base model A vs base model B
- **Adapters:** Adapter stack 1 vs adapter stack 2
- **Parameters:** Temperature 0.3 vs 0.7
- **Backends:** CoreML vs MLX performance
- **Versions:** Adapter v1 vs v2 after retraining

**Output comparison:**
\`\`\`
Configuration A          | Configuration B
Model: qwen2.5-7b       | Model: mistral-7b
Adapter: legal-v2       | Adapter: legal-v3
Temperature: 0.5        | Temperature: 0.5
---
Output: [response A]    | Output: [response B]
Tokens: 247             | Tokens: 312
Latency: 1.2s          | Latency: 1.5s
Quality: 8.5/10        | Quality: 9.1/10
\`\`\`

**Evaluation metrics:**
- **Semantic similarity:** Embedding cosine similarity
- **Factual accuracy:** Evidence grounding, hallucination rate
- **Performance:** Latency, throughput, memory
- **Quality:** Human ratings, automated scoring

**Use cases:**
- **Model selection:** Choose best base model for task
- **Adapter evaluation:** Validate training improvements
- **Parameter tuning:** Find optimal temperature, top-p
- **Regression testing:** Ensure updates don't degrade quality

**AdapterOS features:**
- Side-by-side UI comparison view
- Batch comparison across test sets
- Metric export for analysis
- Deterministic seeds for fair comparison`,
    },
    relatedTerms: ['inference-seed', 'inference-adapter-stack', 'inference-model'],
    aliases: ['compare', 'comparison', 'ab-test', 'eval-mode'],
  },
  {
    id: 'tokens-per-sec',
    term: 'Tokens Per Second',
    category: 'inference',
    content: {
      brief: 'Throughput metric measuring the number of tokens generated per second. Higher values indicate faster generation. Typical range: 20-100 tokens/sec depending on model size and hardware.',
      detailed: `Tokens per second (tok/s) measures inference throughput, indicating how quickly the model generates output text.

**Typical performance:**
- **7B models on ANE:** 40-80 tok/s
- **7B models on GPU:** 30-60 tok/s
- **13B models on GPU:** 15-35 tok/s
- **70B models (quantized):** 5-15 tok/s

**Factors affecting tok/s:**
1. **Model size:** Larger models slower (more compute per token)
2. **Hardware:** ANE > GPU > CPU
3. **Batch size:** Higher batches amortize overhead
4. **Quantization:** 4-bit models 2-3x faster than FP16
5. **Sequence length:** Longer sequences slower (KV cache growth)
6. **Adapter overhead:** Minimal impact (<5%)

**Calculation:**
\`\`\`
tokens_per_sec = total_tokens_generated / generation_time_seconds
\`\`\`

**Relationship to latency:**
- **Time-to-first-token (TTFT):** Prompt processing time
- **Inter-token latency:** 1 / tok/s (e.g., 50 tok/s = 20ms/token)
- **Total latency:** TTFT + (num_tokens / tok/s)

**Optimization strategies:**
- Use quantized models (4-bit, 8-bit)
- Batch multiple requests together
- Enable KV cache for multi-turn chat
- Use smaller models for latency-critical tasks
- Prefer ANE-optimized CoreML backend

**Monitoring in AdapterOS:**
Real-time metrics available via \`/v1/metrics/system\` and streaming telemetry.`,
    },
    relatedTerms: ['latency-p95', 'latency-p99', 'inference-stream', 'inference-max-tokens'],
    aliases: ['tokens-per-sec', 'throughput', 'tok-s', 'generation-speed'],
  },
  {
    id: 'latency-p95',
    term: 'Latency P95',
    category: 'inference',
    content: {
      brief: '95th percentile end-to-end response latency in milliseconds. 95% of requests complete faster than this value. Key SLA metric for production inference systems.',
      detailed: `P95 latency is the 95th percentile of end-to-end request latency, meaning 95% of requests complete faster than this threshold.

**Why P95 matters:**
- **User experience:** Captures typical user experience
- **SLA targets:** Common performance guarantee (e.g., "P95 < 2s")
- **Outlier tolerant:** Ignores worst 5% (network glitches, GC pauses)
- **Actionable:** More stable than max latency for optimization

**P95 vs other percentiles:**
- **P50 (median):** Half of requests faster, misses tail latency
- **P95:** Industry standard for user-facing services
- **P99:** Stricter guarantee, more sensitive to outliers
- **Max:** Captures worst case, too noisy for SLAs

**Typical P95 targets:**
- **Chat applications:** 1-2 seconds
- **Code completion:** 200-500ms
- **Batch processing:** 10-30 seconds
- **Real-time assistants:** <1 second

**Latency breakdown:**
\`\`\`
Total P95 = Queue time + Prompt processing + Generation + Network
           = 50ms      + 200ms             + 1500ms      + 50ms
           = 1.8s
\`\`\`

**Optimization strategies:**
- Reduce queue time: Scale workers, load balancing
- Faster prompt processing: Quantization, batch prefill
- Faster generation: Smaller models, speculative decoding
- Reduce network: CDN, edge deployment

**AdapterOS monitoring:**
- Real-time P95 tracking per adapter, model, tenant
- Alerting on P95 SLA violations
- Telemetry export for analysis`,
    },
    relatedTerms: ['latency-p99', 'tokens-per-sec', 'inference-stream'],
    aliases: ['latency-p95', 'p95', 'p95-latency', '95th-percentile'],
  },
  {
    id: 'latency-p99',
    term: 'Latency P99',
    category: 'inference',
    content: {
      brief: '99th percentile response latency. Only 1% of requests exceed this value. Stricter performance guarantee than P95, important for latency-sensitive applications.',
      detailed: `P99 latency is the 99th percentile of request latency, representing a stricter performance guarantee where 99% of requests complete faster.

**When to use P99:**
- **Critical applications:** Real-time assistants, safety systems
- **Premium SLAs:** High-tier service guarantees
- **Latency-sensitive:** Interactive applications where delays hurt UX
- **Debugging tail latency:** Identify rare performance issues

**P99 vs P95:**
- **Sensitivity:** P99 more affected by outliers
- **Stability:** P99 more variable, harder to optimize
- **Coverage:** P99 guarantees better worst-case experience
- **Cost:** Optimizing P99 often requires over-provisioning

**Typical P99 targets:**
- **Real-time chat:** 2-3 seconds
- **Code completion:** 500-800ms
- **Interactive tools:** <2 seconds
- **Batch processing:** 30-60 seconds

**Common P99 degradation causes:**
- **Cold starts:** Loading adapters from disk
- **Memory pressure:** Swapping, GC pauses
- **Resource contention:** CPU/GPU sharing
- **Network issues:** Timeouts, retries
- **Long prompts:** Quadratic attention scaling

**Monitoring and alerting:**
\`\`\`
P99 spike detected:
- Baseline: 1.8s
- Current: 4.2s (+133%)
- Potential causes: Memory thrashing, adapter load
- Recommended action: Scale workers, unload cold adapters
\`\`\`

**AdapterOS optimization:**
- Adapter pinning keeps critical adapters hot
- Memory management prevents thrashing
- Lifecycle optimization (Warm → Hot transition)
- Deterministic execution reduces variance`,
    },
    relatedTerms: ['latency-p95', 'tokens-per-sec', 'inference-adapter-stack'],
    aliases: ['latency-p99', 'p99', 'p99-latency', '99th-percentile'],
  },
  {
    id: 'context-length',
    term: 'Context Length',
    category: 'inference',
    content: {
      brief: 'Maximum number of tokens the model can process in a single request (prompt + generated output). Common values: 4K, 8K, 32K, 128K tokens.',
      detailed: `Context length (context window) is the maximum total tokens a model can process, including both input prompt and generated output.

**Token budget:**
\`\`\`
context_length = prompt_tokens + max_new_tokens
8192 = 6000 (prompt) + 2192 (generation)
\`\`\`

**Common context lengths:**
- **4K (4096 tokens):** Early GPT-3 models, ~3K words
- **8K (8192 tokens):** Standard for many 7B models, ~6K words
- **32K (32768 tokens):** Extended context models, ~24K words
- **128K (131072 tokens):** Long-context models, ~98K words
- **200K+ tokens:** Cutting-edge models, full books

**Use cases by context length:**
- **4-8K:** Chat, Q&A, code completion, short documents
- **16-32K:** Long documents, codebase analysis, multi-turn chat
- **64-128K:** Full research papers, large codebases, extensive context
- **200K+:** Books, entire repositories, comprehensive analysis

**Memory scaling:**
\`\`\`
KV cache size ≈ 2 × layers × hidden_dim × context_length × precision
For 7B model at 32K context: ~8GB KV cache
\`\`\`

**Performance implications:**
- **Latency:** Longer context → quadratic attention cost
- **Memory:** Linear scaling for KV cache
- **Throughput:** Fewer concurrent requests with long context

**AdapterOS handling:**
- Validates total tokens against model context length
- Returns error if prompt + max_tokens exceeds limit
- Memory management accounts for KV cache size
- Supports dynamic context length per model`,
    },
    relatedTerms: ['inference-max-tokens', 'inference-prompt', 'tokens-per-sec'],
    aliases: ['context-length', 'context-window', 'max-context', 'sequence-length'],
  },
  {
    id: 'stop-sequences',
    term: 'Stop Sequences',
    category: 'inference',
    content: {
      brief: 'Strings that cause generation to halt when encountered. Used to terminate output at natural boundaries like sentences, paragraphs, or specific markers.',
      detailed: `Stop sequences are predefined strings that immediately terminate generation when the model produces them, enabling precise control over output boundaries.

**Common stop sequences:**
- ["\\n\\n"]: Stop at paragraph breaks
- ["</s>", "<|endoftext|>"]: Model-specific EOS tokens
- ["User:", "Assistant:"]: Stop at conversation turns
- ["---"]: Stop at section dividers

**Use cases:**
1. **Conversation turns:** Set stop to ["\\nUser:", "\\nAssistant:"] to generate only the next assistant turn.

2. **Structured output:** Set stop to ["}"] to stop after complete JSON object.

3. **Format constraints:** Set stop to ["\\n\\n"] to stop after paragraph breaks.

**Behavior:**
- Generation stops immediately when sequence detected
- Stop sequence included or excluded based on API config
- Multiple stop sequences: first match triggers stop
- Case-sensitive exact string matching
- No regex support (use exact strings)

**Interaction with max_tokens:**
Generation stops at earliest of:
1. Stop sequence encountered
2. Max tokens reached
3. Model emits EOS token`,
    },
    relatedTerms: ['inference-max-tokens', 'inference-prompt', 'inference-stream'],
    aliases: ['stop-sequences', 'stop-tokens', 'stop-strings', 'terminators'],
  },
  {
    id: 'inference-batch',
    term: 'Batch Inference',
    category: 'inference',
    content: {
      brief: 'Process multiple prompts in a single request to amortize overhead and improve throughput. Efficient for non-interactive workloads like dataset evaluation or batch processing.',
      detailed: `Batch inference processes multiple independent prompts together, sharing model loading and initialization overhead to maximize throughput.

**How batching works:**
1. Collect N prompts into batch
2. Pad/truncate to uniform length (or dynamic batching)
3. Process together in single forward pass
4. Return N independent outputs

**Benefits:**
- **Throughput:** 2-10x higher tokens/sec vs sequential
- **Efficiency:** Amortizes model load, memory allocation
- **Cost:** Lower per-request cost for batch processing
- **Utilization:** Better GPU/ANE utilization

**Batch size tradeoffs:**
\`\`\`
Batch=1:  40 tok/s, 1.0s latency
Batch=4:  120 tok/s, 2.5s latency
Batch=16: 280 tok/s, 8.0s latency
\`\`\`

**Use cases:**
- **Dataset evaluation:** Score 1000s of examples
- **Batch translation:** Process document corpus
- **Embeddings:** Generate vectors for search index
- **Offline analysis:** Non-time-sensitive tasks

**Dynamic batching:**
- Collects requests over time window (e.g., 100ms)
- Processes accumulated batch together
- Balances latency and throughput
- Adaptive batch size based on load

**AdapterOS implementation:**
- \`/v1/infer/batch\` endpoint
- Configurable batch size limits
- Automatic padding/truncation
- Per-request status tracking
- Memory-aware batch sizing

**Not suitable for:**
- Real-time interactive chat (use streaming)
- Latency-critical applications
- Mixed prompt lengths with strict latency SLA`,
    },
    relatedTerms: ['tokens-per-sec', 'latency-p95', 'inference-stream'],
    aliases: ['batch-inference', 'batching', 'batch-processing'],
  },
  {
    id: 'inference-repetition-penalty',
    term: 'Repetition Penalty',
    category: 'inference',
    content: {
      brief: 'Penalizes tokens that have already appeared in the generated text to reduce repetition. Values >1.0 discourage repetition. Typical range: 1.0-1.2.',
      detailed: `Repetition penalty reduces the probability of tokens that have already been generated, preventing repetitive or looping outputs.

**How it works:**
1. Track all tokens generated so far
2. For each candidate token at step t:
   - If token appeared previously: logit = logit / penalty
   - If token is new: logit unchanged
3. Apply softmax and sample

**Penalty values:**
- **1.0:** No penalty (default, allows natural repetition)
- **1.05-1.1:** Mild penalty, reduces obvious loops
- **1.1-1.2:** Moderate penalty, standard for most tasks
- **1.2-1.5:** Strong penalty, may hurt coherence
- **>1.5:** Extreme penalty, often produces unnatural text

**Use cases:**
- **Creative writing:** Prevent word/phrase repetition (1.1-1.15)
- **Code generation:** Allow intentional repetition (1.0-1.05)
- **Summarization:** Reduce redundancy (1.1-1.2)
- **Lists/enumerations:** Prevent duplicate items (1.15-1.25)

**Tradeoffs:**
- **Too low:** Repetitive, boring, stuck in loops
- **Too high:** Unnatural vocabulary, forced variety, lower quality
- **Optimal value:** Task and model dependent

**Variants:**
- **Frequency penalty:** Scale by occurrence count
- **Presence penalty:** Binary (appeared or not)
- **Range-limited:** Only penalize last N tokens

**AdapterOS default:** 1.0 (no penalty), configurable per request.

**Example:**
Without penalty (1.0): "The model is good. The model is very good. The model..."
With penalty (1.15): "The model is good. It performs well. Quality is high..."`,
    },
    relatedTerms: ['inference-temperature', 'inference-top-p', 'inference-top-k'],
    aliases: ['repetition-penalty', 'frequency-penalty', 'presence-penalty'],
  },
  {
    id: 'inference-system-message',
    term: 'System Message',
    category: 'inference',
    content: {
      brief: 'Initial instruction that sets model behavior, role, and constraints. Processed before user prompt. Used to establish persona, formatting rules, and safety guidelines.',
      detailed: `System message (system prompt) is a special instruction prepended to every conversation that defines the model's role, behavior, and operational constraints.

**Purpose:**
- **Role definition:** "You are a helpful coding assistant..."
- **Behavior rules:** "Always respond in JSON format..."
- **Safety constraints:** "Never generate harmful content..."
- **Domain knowledge:** "You have expertise in Rust and systems programming..."
- **Output formatting:** "Use markdown for code blocks..."

**Message structure:**
\`\`\`
[System] You are an expert Rust developer. Provide concise, idiomatic code.
[User] How do I read a file?
[Assistant] Use std::fs::read_to_string()...
\`\`\`

**Best practices:**
1. **Concise:** Keep under 200 tokens to preserve context
2. **Specific:** Clear instructions over vague guidance
3. **Consistent:** Reuse templates for predictable behavior
4. **Tested:** Validate system message effectiveness on examples

**Common patterns:**
\`\`\`
Role: "You are {role} with expertise in {domain}."
Format: "Respond in {format}. Use {style}."
Constraints: "Never {forbidden}. Always {required}."
Context: "Current date: {date}. User: {user_info}."
\`\`\`

**AdapterOS integration:**
- Separate system_message field in API
- Template variables for dynamic content
- Adapter-specific system message overrides
- Audit logging includes system message hash

**Limitations:**
- Models may ignore complex or contradictory instructions
- No guarantee of perfect adherence
- Use adapters for stronger behavioral changes
- Validate outputs match system message intent

**Security note:**
System messages can be overridden by clever user prompts (jailbreaking). Combine with adapters and output filtering for security.`,
    },
    relatedTerms: ['inference-prompt', 'inference-adapter-stack', 'inference-temperature'],
    aliases: ['system-message', 'system-prompt', 'instruction', 'preamble'],
  },
];
