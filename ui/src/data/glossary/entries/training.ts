import type { GlossaryEntry } from '../types';

export const trainingEntries: GlossaryEntry[] = [
  {
    id: 'lora-rank',
    term: 'LoRA Rank',
    category: 'training',
    content: {
      brief: 'LoRA rank determines the adaptation capacity of your model. Higher ranks (16, 32) capture more complex patterns but consume more memory. Typical values: 4-8 for lightweight, 16 for balanced, 32-64 for high capacity.',
      detailed: `The rank parameter in Low-Rank Adaptation (LoRA) defines the dimensionality of the low-rank matrices used to adapt the base model. It directly controls the model's capacity to learn new patterns while fine-tuning.

**How It Works:**
LoRA decomposes weight updates into two smaller matrices (A and B) where the rank determines their inner dimension. A rank of 16 means each adapter uses 16-dimensional representations.

**Choosing the Right Rank:**
- **Rank 4-8**: Lightweight tasks, minimal memory footprint, fast training
- **Rank 16**: Balanced choice for most use cases, good capacity vs efficiency
- **Rank 32-64**: Complex tasks requiring high capacity, more memory intensive

**Trade-offs:**
- Higher rank = more parameters = more capacity but slower inference and more memory
- Lower rank = fewer parameters = faster but may underfit complex patterns

**Memory Impact:**
A rank 32 adapter typically uses 4x more memory than rank 8 for the same model.`,
    },
    relatedTerms: ['lora-alpha', 'target-modules', 'training-template', 'quantization'],
    aliases: ['rank', 'r', 'lora-r', 'adapter-rank'],
  },
  {
    id: 'lora-alpha',
    term: 'LoRA Alpha',
    category: 'training',
    content: {
      brief: 'Scaling factor for LoRA weight updates. Typically set to 2x the rank value (e.g., alpha=32 for rank=16). Higher alpha values produce stronger adaptations.',
      detailed: `Alpha is a scaling hyperparameter that controls the magnitude of LoRA adaptations applied to the base model. It works in conjunction with the rank parameter.

**Relationship to Rank:**
The effective scaling of LoRA weights is alpha/rank. Common practice is to set alpha = 2 * rank, which gives a scaling factor of 2.

**Common Configurations:**
- Rank 8, Alpha 16 (scaling factor: 2)
- Rank 16, Alpha 32 (scaling factor: 2)
- Rank 32, Alpha 64 (scaling factor: 2)

**When to Adjust:**
- **Increase alpha**: When adaptations seem too weak or training loss plateaus early
- **Decrease alpha**: When the model overfits or diverges during training
- **Keep alpha/rank constant**: Maintains consistent scaling when changing rank

**Advanced Usage:**
Some practitioners use alpha = rank for more conservative adaptations or alpha = 4 * rank for aggressive fine-tuning on domain-specific tasks.`,
    },
    relatedTerms: ['lora-rank', 'learning-rate', 'weight-decay'],
    aliases: ['alpha', 'lora-scaling', 'scaling-factor'],
  },
  {
    id: 'learning-rate',
    term: 'Learning Rate',
    category: 'training',
    content: {
      brief: 'Step size for gradient descent optimization. Controls how much weights change per update. Typical range: 1e-4 to 1e-3. Smaller values provide more stable training but slower convergence.',
      detailed: `The learning rate is one of the most critical hyperparameters in neural network training. It determines how aggressively the optimizer updates model weights based on the computed gradients.

**Typical Values:**
- **1e-5 to 3e-5**: Conservative, very stable, good for large models
- **1e-4 to 3e-4**: Balanced choice for most LoRA fine-tuning
- **5e-4 to 1e-3**: Aggressive, faster convergence but risk of instability

**Learning Rate Scheduling:**
AdapterOS supports warmup and decay strategies:
- **Warmup**: Gradually increases LR from 0 to target over N steps
- **Cosine decay**: Smoothly decreases LR following cosine curve
- **Linear decay**: Linearly reduces LR over training

**Signs of Problems:**
- **Too high**: Loss spikes, NaN values, training diverges
- **Too low**: Extremely slow convergence, loss barely decreases

**Best Practices:**
- Start with 1e-4 for LoRA fine-tuning
- Use warmup (10-20% of total steps) for stability
- Monitor early training loss - adjust if unstable or too slow`,
    },
    relatedTerms: ['warmup-steps', 'epochs', 'gradient-accumulation', 'training-loss'],
    aliases: ['lr', 'learning-rate-schedule', 'step-size'],
  },
  {
    id: 'epochs',
    term: 'Epochs',
    category: 'training',
    content: {
      brief: 'Number of complete passes through the entire training dataset. More epochs allow better learning but risk overfitting. Typical: 3-10 epochs for fine-tuning.',
      detailed: `An epoch represents one complete iteration through all training examples. The total number of epochs determines how many times the model sees each training sample.

**Choosing Epoch Count:**
- **1-3 epochs**: Large datasets (>10K examples), risk of overfitting
- **3-5 epochs**: Medium datasets (1K-10K examples), balanced approach
- **5-10 epochs**: Small datasets (<1K examples), need more exposure
- **10+ epochs**: Very small datasets or specific convergence requirements

**Monitoring Training:**
Watch validation metrics across epochs:
- Loss should generally decrease
- Validation loss diverging from training loss = overfitting
- Both losses plateauing = convergence achieved

**Early Stopping:**
AdapterOS supports stopping training early if:
- Validation loss stops improving for N consecutive epochs
- Target loss threshold is reached
- Training time exceeds limits

**Calculation:**
Total training steps = (dataset_size / batch_size) * epochs * gradient_accumulation_steps`,
    },
    relatedTerms: ['batch-size', 'training-progress', 'training-loss', 'validation-status'],
    aliases: ['epoch', 'training-epochs', 'num-epochs'],
  },
  {
    id: 'batch-size',
    term: 'Batch Size',
    category: 'training',
    content: {
      brief: 'Number of training examples processed together in one forward/backward pass. Larger batches train faster and use more memory. Typical: 4-32 depending on GPU memory.',
      detailed: `Batch size controls how many training examples are processed simultaneously before updating model weights. It significantly impacts both training speed and memory usage.

**Memory vs Speed Trade-off:**
- **Small batches (1-4)**: Minimal memory, fits on limited hardware, noisier gradients
- **Medium batches (8-16)**: Balanced memory/speed, stable gradients
- **Large batches (32-64)**: Maximum speed, requires significant memory, smoother gradients

**Hardware Considerations:**
- **8GB GPU**: Batch size 2-4 for 7B models
- **16GB GPU**: Batch size 4-8 for 7B models
- **24GB+ GPU**: Batch size 8-16+ for 7B models

**Effective Batch Size:**
Combine batch_size with gradient_accumulation_steps:
Effective batch = batch_size * gradient_accumulation_steps

Example: batch_size=4, accumulation=4 → effective batch of 16

**Impact on Training:**
- Larger batches provide more stable gradient estimates
- Smaller batches add beneficial noise that can improve generalization
- Very large batches may require learning rate adjustments

**OOM Errors:**
If you hit out-of-memory errors, reduce batch_size or enable gradient_accumulation.`,
    },
    relatedTerms: ['gradient-accumulation', 'max-seq-length', 'quantization', 'data-type'],
    aliases: ['batch', 'per-device-batch-size', 'micro-batch-size'],
  },
  {
    id: 'gradient-accumulation',
    term: 'Gradient Accumulation',
    category: 'training',
    content: {
      brief: 'Accumulates gradients over multiple forward/backward passes before updating weights. Simulates larger batch sizes without increasing memory usage. Typical: 2-8 steps.',
      detailed: `Gradient accumulation is a technique to achieve the benefits of large batch training while staying within memory constraints. It splits a large effective batch into smaller micro-batches.

**How It Works:**
1. Process micro-batch 1, compute gradients, don't update weights
2. Process micro-batch 2, accumulate gradients, don't update weights
3. Repeat for N accumulation steps
4. Apply accumulated gradients, update weights, zero gradients

**Effective Batch Size:**
effective_batch = batch_size * gradient_accumulation_steps * num_gpus

Example: batch=4, accumulation=4, gpus=1 → effective batch of 16

**When to Use:**
- Limited GPU memory prevents desired batch size
- Want stable training with large effective batches
- Matching batch size from reference implementation

**Trade-offs:**
- **Pros**: Same results as large batches, less memory
- **Cons**: Slower training (more forward/backward passes per update)

**Typical Configurations:**
- accumulation=2: Double effective batch, minimal slowdown
- accumulation=4: Quadruple effective batch, moderate slowdown
- accumulation=8+: Very large effective batches, significant slowdown

**Best Practice:**
Set batch_size to maximum that fits in memory, then use accumulation to reach target effective batch size.`,
    },
    relatedTerms: ['batch-size', 'learning-rate', 'tokens-per-second'],
    aliases: ['accumulation-steps', 'grad-accum', 'accumulation'],
  },
  {
    id: 'warmup-steps',
    term: 'Warmup Steps',
    category: 'training',
    content: {
      brief: 'Number of initial training steps where learning rate gradually increases from 0 to the target value. Stabilizes early training. Typical: 10-20% of total steps.',
      detailed: `Learning rate warmup is a training stabilization technique that gradually increases the learning rate from near-zero to the target value over the first N training steps.

**Why Warmup Matters:**
Early in training, model weights are randomly initialized or pretrained for different data. Starting with a high learning rate can cause:
- Unstable gradients and loss spikes
- Premature convergence to poor local minima
- Numeric instability (NaN or Inf values)

**Warmup Schedule:**
Most common is linear warmup:
- Step 0: lr = 0
- Step N/2: lr = target_lr / 2
- Step N: lr = target_lr
- Step N+1 onward: lr = target_lr (or decay schedule)

**Calculating Warmup Steps:**
Rule of thumb: 10-20% of total training steps

Example for 1000 total steps:
- 100 warmup steps (10%)
- 200 warmup steps (20%)

**Total Steps Calculation:**
total_steps = (dataset_size / batch_size) * epochs / gradient_accumulation

**Impact:**
- Too few warmup steps: May still see early instability
- Too many warmup steps: Wastes training time on suboptimal learning rates
- No warmup: Risk of training divergence, especially with high learning rates

**Best Practice:**
Use 10% warmup for stable datasets, 20% for noisy or challenging data.`,
    },
    relatedTerms: ['learning-rate', 'epochs', 'training-loss'],
    aliases: ['warmup', 'lr-warmup', 'warmup-ratio'],
  },
  {
    id: 'weight-decay',
    term: 'Weight Decay',
    category: 'training',
    content: {
      brief: 'L2 regularization that prevents overfitting by penalizing large weights. Adds weight_decay * weight² to the loss. Typical: 0.01 for balanced regularization.',
      detailed: `Weight decay is a regularization technique that discourages the model from relying too heavily on any single feature by penalizing large weight values.

**How It Works:**
During optimization, weights are decayed by a small factor:
new_weight = old_weight - learning_rate * (gradient + weight_decay * old_weight)

This effectively adds L2 regularization to the loss function.

**Typical Values:**
- **0.0**: No regularization, maximum model flexibility
- **0.01**: Standard choice for most fine-tuning (balanced)
- **0.001**: Light regularization, large datasets
- **0.1**: Heavy regularization, small datasets or overfitting

**When to Increase Weight Decay:**
- Model overfits training data (train loss << validation loss)
- Small training dataset
- Want to preserve more base model behavior

**When to Decrease Weight Decay:**
- Model underfits (high train and validation loss)
- Large, diverse training dataset
- Want stronger adaptation from base model

**LoRA-Specific Considerations:**
Since LoRA only trains adapter weights, not the full model, weight decay primarily affects the adapter parameters. This helps prevent the adapter from making overly aggressive changes to the base model.

**AdamW Optimizer:**
AdapterOS uses AdamW which implements weight decay correctly by decoupling it from gradient-based updates.`,
    },
    relatedTerms: ['lora-rank', 'lora-alpha', 'learning-rate', 'training-loss'],
    aliases: ['l2-regularization', 'regularization', 'decay'],
  },
  {
    id: 'max-seq-length',
    term: 'Maximum Sequence Length',
    category: 'training',
    content: {
      brief: 'Maximum number of tokens in a training example. Longer sequences capture more context but require more memory. Typical: 512-2048 tokens.',
      detailed: `Maximum sequence length defines the longest input the model will process during training. It directly impacts memory usage and the model's ability to learn from long-context examples.

**Common Values by Use Case:**
- **512 tokens**: Short-form content, code snippets, simple Q&A
- **1024 tokens**: Medium-form content, documentation, conversations
- **2048 tokens**: Long-form content, technical documents, extended dialogues
- **4096+ tokens**: Very long documents, extensive code files

**Memory Impact:**
Memory usage scales quadratically with sequence length due to attention mechanisms:
- 512 tokens: baseline memory
- 1024 tokens: ~4x baseline memory
- 2048 tokens: ~16x baseline memory

**Truncation and Padding:**
- Sequences longer than max_seq_length are truncated
- Sequences shorter than max_seq_length are padded
- Padding tokens don't contribute to loss calculation

**Best Practices:**
1. Analyze your training data distribution
2. Set max_seq_length to cover 90-95% of examples
3. Extremely long outliers can be split into multiple examples
4. Balance context needs vs memory constraints

**Impact on Inference:**
Models trained with max_seq_length=N can generally handle sequences up to N tokens at inference time, though base model context limits still apply.`,
    },
    relatedTerms: ['batch-size', 'tokens-per-second', 'dataset-preview'],
    aliases: ['max-length', 'context-length', 'seq-length', 'sequence-length'],
  },
  {
    id: 'target-modules',
    term: 'Target Modules',
    category: 'training',
    content: {
      brief: 'Specifies which neural network layers receive LoRA adapters. Common choices: attention layers (q_proj, v_proj, k_proj) or all linear layers. Affects adapter capacity and size.',
      detailed: `Target modules determine which parts of the base model are adapted with LoRA. This is a critical decision that affects both adapter effectiveness and parameter count.

**Common Strategies:**

**Attention-Only (Recommended):**
- Modules: q_proj, v_proj (query and value projections)
- Use case: Most efficient, good for general fine-tuning
- Parameters: Lowest count, fastest inference

**Attention-Extended:**
- Modules: q_proj, k_proj, v_proj, o_proj
- Use case: Stronger adaptation of attention mechanism
- Parameters: Moderate count

**All Linear Layers:**
- Modules: All linear transformations in the model
- Use case: Maximum adaptation capacity, domain-specific tasks
- Parameters: Highest count, slower inference

**Module Naming:**
Module names are model-specific. For transformer models:
- q_proj: Query projection in self-attention
- k_proj: Key projection in self-attention
- v_proj: Value projection in self-attention
- o_proj: Output projection in self-attention
- gate_proj, up_proj, down_proj: MLP layers

**Trade-offs:**
- More modules = more parameters = stronger adaptation = more memory
- Fewer modules = fewer parameters = faster training/inference = less capacity

**Finding Module Names:**
AdapterOS can auto-detect available modules for your base model. Use the model introspection tools to list all linear layers.`,
    },
    relatedTerms: ['lora-rank', 'lora-alpha', 'training-template'],
    aliases: ['modules', 'target-layers', 'lora-modules'],
  },
  {
    id: 'training-job',
    term: 'Training Job',
    category: 'training',
    content: {
      brief: 'A training job executes the LoRA fine-tuning process on a dataset. Jobs progress through queued, running, completed, failed, or cancelled states.',
      detailed: `A training job represents a single LoRA fine-tuning run in AdapterOS. It orchestrates the entire training pipeline from data loading to adapter packaging.

**Job Lifecycle:**
1. **Queued**: Job created, waiting for worker availability
2. **Running**: Worker assigned, training in progress
3. **Completed**: Training finished successfully, adapter ready
4. **Failed**: Error occurred, check logs for details
5. **Cancelled**: User or system cancelled the job

**Job Components:**
- Dataset reference (must be validated)
- Training configuration (rank, alpha, learning rate, etc.)
- Resource allocation (GPU, memory limits)
- Progress tracking (epoch, loss, tokens/sec)
- Output artifacts (adapter weights, logs, metrics)

**Monitoring:**
Real-time metrics available during training:
- Current epoch and step
- Training loss (should decrease)
- Tokens per second (throughput)
- Estimated time remaining
- Memory usage

**Job Management:**
- List all jobs: Filter by status, tenant, dataset
- View details: Logs, metrics, configuration
- Cancel job: Gracefully stops training, cleanup resources
- Restart failed: Retry with same or adjusted configuration

**Output:**
Successful jobs produce:
- .aos adapter file (packaged weights + manifest)
- Training metrics and loss curves
- Validation results (if validation set provided)
- Audit trail with deterministic execution evidence`,
    },
    relatedTerms: ['training-dataset', 'training-status', 'training-progress', 'training-loss'],
    aliases: ['job', 'fine-tuning-job', 'lora-job'],
  },
  {
    id: 'training-dataset',
    term: 'Training Dataset',
    category: 'training',
    content: {
      brief: 'Collection of training examples used for fine-tuning. Datasets must be validated before use. Stored in NDJSON format with input/output pairs.',
      detailed: `A training dataset in AdapterOS is a curated collection of examples that teach the model new behaviors or knowledge through LoRA fine-tuning.

**Dataset Format:**
NDJSON (newline-delimited JSON) with structured examples:
- Input: The prompt or context
- Output: The desired completion
- Optional metadata: source, category, difficulty

**Dataset States:**
1. **Draft**: Being assembled, not yet validated
2. **Validating**: Quality checks in progress
3. **Valid**: Ready for training
4. **Invalid**: Failed validation, cannot be used

**Validation Checks:**
- Format correctness (valid JSON, required fields)
- Token length within limits
- No duplicate examples
- Minimum quality thresholds
- Tokenization compatibility

**Size Recommendations:**
- **100-500 examples**: Basic task adaptation
- **500-2000 examples**: Solid fine-tuning for specific domain
- **2000-10000 examples**: Comprehensive domain coverage
- **10000+ examples**: Large-scale specialization

**Data Quality > Quantity:**
Better to have 500 high-quality, diverse examples than 5000 repetitive or low-quality ones.

**Dataset Organization:**
- Group by collection for logical organization
- Tag with metadata for filtering
- Version datasets to track improvements
- Link to source documents for provenance

**Best Practices:**
- Balance input/output lengths
- Include diverse examples covering edge cases
- Remove personal or sensitive information
- Test with small dataset first, then scale up`,
    },
    relatedTerms: ['validation-status', 'dataset-preview', 'training-job', 'max-seq-length'],
    aliases: ['dataset', 'training-data', 'fine-tuning-dataset'],
  },
  {
    id: 'training-status',
    term: 'Training Status',
    category: 'training',
    content: {
      brief: 'Current state of a training job: queued (waiting), running (in progress), completed (success), failed (error), or cancelled (stopped). Real-time status updates via SSE.',
      detailed: `Training status indicates the current phase of a training job in the AdapterOS pipeline. Status changes are broadcast in real-time to connected clients.

**Status Values:**

**Queued:**
- Job accepted and validated
- Waiting for worker availability
- Position in queue visible
- Can be cancelled without resource impact

**Running:**
- Worker assigned and training started
- Real-time metrics available (loss, progress, tokens/sec)
- Can be cancelled (graceful shutdown)
- Resources actively allocated

**Completed:**
- Training finished successfully
- Adapter packaged and ready for use
- Full metrics and logs available
- Can be registered as adapter

**Failed:**
- Error occurred during training
- Check error logs for diagnostic information
- Common causes: OOM, invalid config, data corruption
- Can retry with adjusted configuration

**Cancelled:**
- User or system stopped the job
- Partial results may be available
- Resources freed immediately
- Can restart from beginning

**Status Transitions:**
- Queued → Running → Completed
- Queued → Cancelled
- Running → Failed
- Running → Cancelled
- No status moves backward (except restart as new job)

**Monitoring:**
Subscribe to SSE endpoint /v1/stream/training/{job_id} for real-time updates.`,
    },
    relatedTerms: ['training-job', 'training-progress', 'tokens-per-second'],
    aliases: ['job-status', 'status', 'state'],
  },
  {
    id: 'training-progress',
    term: 'Training Progress',
    category: 'training',
    content: {
      brief: 'Percentage of training completed, calculated from current epoch and step. Updated in real-time. Complemented by metrics like loss and tokens/sec.',
      detailed: `Training progress provides real-time visibility into how far along a training job has advanced toward completion.

**Progress Calculation:**
progress = (current_step / total_steps) * 100

Where:
total_steps = (dataset_size / batch_size) * epochs / gradient_accumulation

**Progress Components:**
- **Current epoch**: Which pass through the dataset (e.g., 2/5)
- **Current step**: Global step counter across all epochs
- **Percentage**: 0-100% overall completion
- **Estimated time**: Remaining time based on current throughput

**Real-Time Updates:**
Progress updates broadcast every:
- Completed batch (frequent, may be rate-limited for UI)
- Completed epoch (always sent)
- Significant metric changes

**Interpreting Progress:**

**Early Training (0-20%):**
- Loss may be volatile
- Throughput stabilizing
- Warmup phase if configured

**Mid Training (20-80%):**
- Steady loss decrease expected
- Stable throughput
- Main learning phase

**Late Training (80-100%):**
- Loss should plateau or decrease slowly
- Possible learning rate decay
- Final convergence

**Progress vs Success:**
100% progress doesn't guarantee good results. Always check:
- Final loss value
- Validation metrics
- Sample generations

**Monitoring Dashboard:**
AdapterOS UI shows progress with:
- Progress bar with percentage
- Current/total epoch display
- Live loss chart
- Throughput metrics`,
    },
    relatedTerms: ['training-status', 'epochs', 'training-loss', 'tokens-per-second'],
    aliases: ['progress', 'completion', 'training-percentage'],
  },
  {
    id: 'training-loss',
    term: 'Training Loss',
    category: 'training',
    content: {
      brief: 'Measures how well the model predicts training examples. Lower is better. Typical range: 0.1-3.0 depending on task. Should generally decrease over time.',
      detailed: `Training loss quantifies the difference between the model's predictions and the actual target outputs. It's the primary metric for monitoring training progress.

**Loss Function:**
AdapterOS uses cross-entropy loss for language modeling:
- Measures prediction error per token
- Averages across all tokens in the batch
- Lower loss = better predictions

**Typical Loss Ranges:**
- **2.0-3.0**: Early training, model still learning patterns
- **1.0-2.0**: Mid training, decent performance
- **0.5-1.0**: Late training, strong performance
- **0.1-0.5**: Excellent fit, possible overfitting if validation loss diverges
- **< 0.1**: Likely overfit unless dataset is very small

**Healthy Loss Curve:**
- Starts high (2-3)
- Decreases steadily
- May plateau as training converges
- Smooth curve (no wild fluctuations)

**Warning Signs:**

**Loss Spikes:**
- Learning rate too high
- Batch contains outlier examples
- Numeric instability

**Loss Plateau:**
- May indicate convergence (good)
- Or learning rate too low (bad)
- Check if more epochs help

**Loss Increases:**
- Serious problem: divergence
- Lower learning rate
- Check for data corruption

**Training vs Validation Loss:**
- Both decreasing: healthy training
- Training low, validation high: overfitting
- Both high: underfitting, need more capacity or epochs

**Loss Monitoring:**
Track loss at:
- Every N steps (e.g., every 10 steps)
- End of each epoch
- Export for analysis and comparison`,
    },
    relatedTerms: ['epochs', 'learning-rate', 'training-progress', 'validation-status'],
    aliases: ['loss', 'cross-entropy', 'training-error'],
  },
  {
    id: 'tokens-per-second',
    term: 'Tokens Per Second',
    category: 'training',
    content: {
      brief: 'Training throughput measured in tokens processed per second. Higher is faster. Typical: 1000-10000 tokens/sec depending on hardware and configuration.',
      detailed: `Tokens per second (tokens/sec) measures training speed by counting how many tokens the system processes each second during forward and backward passes.

**Typical Throughput:**

**Consumer Hardware:**
- M1/M2 Mac (16GB): 500-2000 tokens/sec
- M3 Mac (24GB): 1500-3000 tokens/sec
- GTX 3060 (12GB): 1000-2500 tokens/sec
- RTX 3090 (24GB): 3000-6000 tokens/sec

**Server Hardware:**
- RTX 4090 (24GB): 5000-10000 tokens/sec
- A100 (40GB/80GB): 8000-15000 tokens/sec
- H100: 15000-30000 tokens/sec

**Factors Affecting Throughput:**

**Increase Throughput:**
- Larger batch sizes (more parallelism)
- Shorter max sequence length
- Quantization (int8, int4)
- Fewer target modules
- Lower LoRA rank
- More powerful hardware

**Decrease Throughput:**
- Gradient accumulation (more passes per update)
- Very long sequences
- Higher precision (float32 vs float16)
- More target modules
- Higher LoRA rank

**Throughput vs Quality:**
Higher throughput doesn't mean better results. Focus on:
- Batch size large enough for stable gradients
- Sequence length sufficient for your data
- Balance speed with model quality needs

**Time Estimation:**
total_time = (total_tokens / tokens_per_sec)

Example:
- 1M tokens, 2000 tokens/sec = 500 seconds ≈ 8 minutes

**Monitoring:**
Throughput should be stable during training. Fluctuations may indicate:
- Variable sequence lengths in dataset
- Memory pressure causing swapping
- Background system activity`,
    },
    relatedTerms: ['batch-size', 'gradient-accumulation', 'max-seq-length', 'training-progress'],
    aliases: ['throughput', 'tokens-sec', 'tps', 'training-speed'],
  },
  {
    id: 'training-template',
    term: 'Training Template',
    category: 'training',
    content: {
      brief: 'Predefined configuration preset for training jobs. Templates package rank, alpha, learning rate, and other hyperparameters for specific use cases.',
      detailed: `Training templates are curated configuration presets that bundle proven hyperparameter combinations for common fine-tuning scenarios.

**Built-in Templates:**

**general-code:**
- Rank: 16, Alpha: 32
- Learning rate: 2e-4
- Warmup: 10%
- Use case: General code generation and understanding
- Best for: Balanced code tasks, multiple languages

**framework-specific:**
- Rank: 12, Alpha: 24
- Learning rate: 1e-4
- Warmup: 15%
- Use case: Deep specialization in one framework
- Best for: React, Django, Rust-specific tasks

**documentation:**
- Rank: 8, Alpha: 16
- Learning rate: 3e-4
- Warmup: 5%
- Use case: Technical writing, docs generation
- Best for: API docs, tutorials, explanations

**conversational:**
- Rank: 16, Alpha: 32
- Learning rate: 1e-4
- Warmup: 20%
- Use case: Chat, dialogue, interactive systems
- Best for: Customer support, assistants

**Template Structure:**
Each template defines:
- LoRA configuration (rank, alpha, modules)
- Optimizer settings (learning rate, weight decay)
- Schedule parameters (warmup, epochs)
- Data parameters (max sequence length, batch size)

**Customizing Templates:**
- Start with closest matching template
- Override specific parameters as needed
- Save custom configurations as new templates
- Share templates across team/organization

**Best Practices:**
- Use templates as starting points, not strict rules
- Adjust based on dataset size and quality
- Monitor first few epochs, tune if needed
- Document successful configurations for reuse`,
    },
    relatedTerms: ['lora-rank', 'lora-alpha', 'learning-rate', 'training-job'],
    aliases: ['template', 'config-preset', 'training-preset'],
  },
  {
    id: 'quantization',
    term: 'Quantization',
    category: 'training',
    content: {
      brief: 'Reduces numerical precision of model weights to save memory and increase speed. Common formats: float16 (half precision), int8 (8-bit), int4 (4-bit). Trade-off: memory vs quality.',
      detailed: `Quantization compresses model weights from high-precision formats to lower-precision representations, dramatically reducing memory usage with minimal quality loss.

**Precision Formats:**

**float32 (Full Precision):**
- 32 bits per parameter
- Maximum quality, maximum memory
- Rarely needed for inference

**float16/bfloat16 (Half Precision):**
- 16 bits per parameter
- 2x memory reduction vs float32
- Negligible quality loss
- Standard for training and inference

**int8 (8-bit Integer):**
- 8 bits per parameter
- 4x memory reduction vs float32
- Minimal quality loss with proper calibration
- Good balance for most use cases

**int4 (4-bit Integer):**
- 4 bits per parameter
- 8x memory reduction vs float32
- Noticeable but acceptable quality loss
- Enables running larger models on limited hardware

**Memory Impact Example (7B model):**
- float32: ~28GB
- float16: ~14GB
- int8: ~7GB
- int4: ~3.5GB

**Quality Trade-offs:**
- float16 → int8: Usually <1% quality degradation
- int8 → int4: 2-5% quality degradation, task-dependent
- Larger models tolerate quantization better

**When to Use:**

**float16:**
- Training (standard)
- High-quality inference
- Sufficient memory available

**int8:**
- Production inference
- Memory-constrained environments
- Good quality requirements

**int4:**
- Extremely limited memory
- Exploration and development
- Acceptable quality trade-off

**AdapterOS Support:**
- Automatic format detection
- Dynamic quantization during loading
- Mixed-precision training (float16)
- Quantized inference (int8/int4)`,
    },
    relatedTerms: ['data-type', 'batch-size', 'max-seq-length'],
    aliases: ['weight-quantization', 'precision', 'compression'],
  },
  {
    id: 'data-type',
    term: 'Data Type',
    category: 'training',
    content: {
      brief: 'Numerical format for representing weights and activations. Common types: float32 (full precision), float16 (half precision), bfloat16 (brain float). Affects memory and precision.',
      detailed: `Data type (dtype) specifies the numerical representation format used for model computations, affecting both memory usage and numerical precision.

**Common Data Types:**

**float32 (FP32):**
- 32-bit floating point
- Range: ±3.4e38, Precision: ~7 decimal digits
- Use case: Maximum precision, rarely needed
- Memory: Baseline (100%)

**float16 (FP16):**
- 16-bit floating point
- Range: ±65,504, Precision: ~3 decimal digits
- Use case: Standard for most training
- Memory: 50% of FP32
- Risk: Can underflow/overflow on extreme values

**bfloat16 (BF16):**
- 16-bit brain floating point (Google)
- Range: Same as FP32, Precision: ~3 decimal digits
- Use case: Training large models, better stability than FP16
- Memory: 50% of FP32
- Benefit: Wider range prevents overflow issues

**Mixed Precision:**
Modern training uses multiple dtypes:
- Master weights: float32 (high precision)
- Forward/backward: float16/bfloat16 (fast computation)
- Gradients: float32 (accumulation precision)

**Hardware Support:**
- **Apple Neural Engine**: Optimized for float16
- **NVIDIA Ampere+**: Native bfloat16 support
- **NVIDIA Tensor Cores**: Accelerated float16/bfloat16
- **CPU**: All formats, but float32 most optimized

**Choosing Data Type:**

**float16:**
- Standard choice for most hardware
- Good balance speed/precision
- Watch for numeric instability

**bfloat16:**
- Large models (>1B parameters)
- NVIDIA Ampere or newer GPUs
- More stable than float16

**float32:**
- Numeric instability issues
- Research requiring maximum precision
- Rarely needed in practice

**AdapterOS Defaults:**
- Training: mixed precision (float16 compute, float32 master weights)
- Inference: float16 for speed
- Can override per job/adapter`,
    },
    relatedTerms: ['quantization', 'batch-size', 'training-loss'],
    aliases: ['dtype', 'precision', 'numerical-format', 'float-type'],
  },
  {
    id: 'validation-status',
    term: 'Validation Status',
    category: 'training',
    content: {
      brief: 'Dataset validation state: draft (editing), validating (checks running), valid (ready for training), invalid (failed checks). Only valid datasets can be used for training.',
      detailed: `Validation status indicates whether a dataset has passed AdapterOS quality and format checks required for training.

**Status Flow:**

**1. Draft:**
- Dataset created but not submitted for validation
- Can be edited, examples added/removed
- Not usable for training
- No resource allocation

**2. Validating:**
- Validation checks in progress
- Cannot be edited during validation
- Typically completes in seconds to minutes
- Progress indicators available

**3. Valid:**
- Passed all validation checks
- Ready for training job creation
- Locked from editing (create new version to modify)
- Metadata and statistics available

**4. Invalid:**
- Failed one or more validation checks
- Error details specify what failed
- Must be corrected and revalidated
- Cannot be used for training

**Validation Checks:**

**Format Validation:**
- Valid NDJSON structure
- Required fields present (input, output)
- Proper encoding (UTF-8)
- No malformed JSON

**Content Validation:**
- Token counts within limits
- No empty inputs/outputs
- Character encoding valid
- No excessive repetition

**Quality Checks:**
- Minimum number of examples (typically 50+)
- Input/output length distribution reasonable
- No duplicate examples
- Toxicity screening (if enabled)

**Statistical Analysis:**
- Token count distribution
- Input/output length ratios
- Vocabulary diversity
- Example complexity distribution

**Fixing Invalid Datasets:**
Common issues:
- Truncate examples exceeding max_seq_length
- Remove duplicates
- Fix JSON formatting errors
- Add more examples if below minimum

**Best Practice:**
Preview dataset before validation to catch obvious issues early.`,
    },
    relatedTerms: ['training-dataset', 'dataset-preview', 'max-seq-length'],
    aliases: ['dataset-status', 'validation-state', 'status'],
  },
  {
    id: 'dataset-preview',
    term: 'Dataset Preview',
    category: 'training',
    content: {
      brief: 'Sample view of training examples before starting a job. Shows input/output pairs, token counts, and format. Helps verify data quality and catch issues early.',
      detailed: `Dataset preview provides a representative sample of your training data, enabling quality verification before committing computational resources to training.

**Preview Contents:**

**Example Display:**
- First N examples (typically 10-50)
- Random sampling for large datasets
- Input and output text
- Token counts per example
- Metadata if available

**Statistics:**
- Total example count
- Token count distribution (min, max, median, mean)
- Input/output length ratio
- Vocabulary size estimate
- Example categories/types

**Quality Indicators:**
- Formatting consistency
- Length distribution chart
- Potential issues flagged:
  - Very short/long examples
  - Repetitive content
  - Encoding problems
  - Missing fields

**Preview Actions:**
- Inspect individual examples
- Filter by length, category
- Identify outliers
- Download sample for external analysis

**What to Look For:**

**Good Signs:**
- Diverse, natural examples
- Consistent formatting
- Appropriate lengths (not too short/long)
- Clear input-output relationships
- Balanced distribution

**Warning Signs:**
- Many duplicates or near-duplicates
- Inconsistent formats
- Outputs much longer than inputs (or vice versa)
- Truncated or corrupted text
- Excessive special characters

**Preview vs Validation:**
- Preview: Human inspection, subjective quality
- Validation: Automated checks, objective criteria
- Both important for successful training

**Best Practice:**
Always preview datasets before validation and training. Catching issues early saves time and compute resources.

**Export Preview:**
Download preview sample for:
- Team review
- External validation tools
- Documentation
- Comparison with other datasets`,
    },
    relatedTerms: ['training-dataset', 'validation-status', 'max-seq-length'],
    aliases: ['preview', 'dataset-sample', 'example-preview'],
  },
  {
    id: 'optimizer',
    term: 'Optimizer',
    category: 'training',
    content: {
      brief: 'Algorithm that updates model weights based on gradients. AdapterOS uses AdamW (Adam with weight decay) for stable, efficient training.',
      detailed: `The optimizer is responsible for updating model parameters during training based on computed gradients. AdapterOS uses AdamW by default.

**AdamW (Recommended):**
- Adaptive learning rates per parameter
- Momentum for faster convergence
- Decoupled weight decay for proper regularization
- Works well with minimal tuning

**Why AdamW:**
- **Adaptive**: Automatically adjusts learning rates for each parameter
- **Stable**: Maintains moving averages of gradients and squared gradients
- **Efficient**: Faster convergence than SGD on most tasks
- **Robust**: Less sensitive to learning rate choice than vanilla Adam

**Optimizer Parameters:**

**Learning Rate (lr):**
- Primary tuning knob
- Typical: 1e-4 to 3e-4

**Betas (β1, β2):**
- β1=0.9: Momentum coefficient (typical: 0.9)
- β2=0.999: Second moment coefficient (typical: 0.999)
- Rarely need adjustment

**Epsilon (ε):**
- Numerical stability term
- Default: 1e-8
- Almost never needs changing

**Weight Decay:**
- Regularization strength
- Typical: 0.01
- See weight-decay entry for details

**Optimizer State:**
Optimizer maintains per-parameter state:
- First moment (momentum)
- Second moment (adaptive learning rate)
- This doubles memory usage during training

**Alternative Optimizers:**
- **SGD**: Simpler, needs careful learning rate tuning
- **Adam**: Like AdamW but couples weight decay with gradients
- **Lion**: Newer, memory-efficient alternative

**AdapterOS Default Configuration:**
AdamW with lr=2e-4, β1=0.9, β2=0.999, weight_decay=0.01`,
    },
    relatedTerms: ['learning-rate', 'weight-decay', 'gradient-accumulation'],
    aliases: ['adamw', 'adam', 'optimization-algorithm'],
  },
  {
    id: 'checkpoint',
    term: 'Training Checkpoint',
    category: 'training',
    content: {
      brief: 'Snapshot of model state during training. Includes weights, optimizer state, and training progress. Enables resuming interrupted training or selecting best epoch.',
      detailed: `Training checkpoints are periodic snapshots of the complete training state, enabling recovery, resumption, and model selection.

**Checkpoint Contents:**
- Model weights (LoRA adapters)
- Optimizer state (momentum, adaptive rates)
- Training metadata (epoch, step, loss)
- Random number generator state
- Configuration parameters

**Checkpoint Strategy:**

**Every Epoch:**
- Save at end of each epoch
- Enables selecting best epoch by validation loss
- Moderate storage overhead

**Every N Steps:**
- More granular recovery
- Higher storage costs
- Useful for very long training runs

**Best Only:**
- Keep only checkpoint with lowest validation loss
- Minimal storage
- Requires validation set

**Checkpoint Uses:**

**Resume Training:**
- Recover from crashes or interruptions
- Continue training with adjusted hyperparameters
- No loss of progress

**Model Selection:**
- Compare checkpoints from different epochs
- Choose best performing on validation set
- Avoid overfitting by early stopping

**Experimentation:**
- Branch from mid-training checkpoint
- Test different hyperparameters
- A/B test training strategies

**Storage Considerations:**
Checkpoint size = model weights + optimizer state
- Rank 16 adapter: ~50-200MB per checkpoint
- 10 epochs = 500MB-2GB total
- Auto-cleanup old checkpoints to manage storage

**AdapterOS Checkpointing:**
- Automatic checkpointing at epoch boundaries
- Configurable retention policy
- Resume from latest checkpoint on failure
- Export checkpoints as standalone adapters

**Best Practices:**
- Enable checkpointing for training >1 hour
- Keep checkpoints for important production models
- Use validation metrics for checkpoint selection
- Clean up intermediate checkpoints after training`,
    },
    relatedTerms: ['epochs', 'training-progress', 'training-loss'],
    aliases: ['checkpoint', 'snapshot', 'model-checkpoint'],
  },
  {
    id: 'early-stopping',
    term: 'Early Stopping',
    category: 'training',
    content: {
      brief: 'Automatically stops training when validation loss stops improving. Prevents overfitting and saves compute. Typical patience: 2-3 epochs without improvement.',
      detailed: `Early stopping is a regularization technique that halts training when the model stops improving on a validation set, preventing overfitting and wasting compute resources.

**How It Works:**
1. Monitor validation loss after each epoch
2. Track best validation loss seen so far
3. Count epochs since last improvement
4. Stop if no improvement for N consecutive epochs (patience)

**Patience Parameter:**
Number of epochs to wait for improvement:
- **Patience 1**: Very aggressive, may stop too early
- **Patience 2-3**: Balanced, recommended for most cases
- **Patience 5+**: Conservative, allows more exploration

**Validation Set:**
Early stopping requires a validation set:
- Typically 10-20% of training data
- Must be representative of full data distribution
- Should not be used for training

**Benefits:**

**Prevents Overfitting:**
- Stops before model memorizes training data
- Maintains good generalization
- Automatically finds optimal training duration

**Saves Resources:**
- No wasted epochs on overfit model
- Automatic training duration tuning
- Reduces compute costs

**Improves Workflow:**
- Less manual monitoring needed
- Consistent stopping criteria
- Reproducible training duration

**Configuration Example:**
\`\`\`json
{
  "early_stopping": {
    "enabled": true,
    "patience": 3,
    "min_delta": 0.001,  // Minimum improvement to count
    "metric": "validation_loss"
  }
}
\`\`\`

**Min Delta:**
Minimum improvement to reset patience counter:
- Prevents stopping on tiny fluctuations
- Typical: 0.001 to 0.01
- Task-dependent

**When Not to Use:**
- No validation set available
- Very small datasets (risk stopping too early)
- Exploring maximum model capacity
- Validation loss is noisy

**AdapterOS Implementation:**
- Configurable per training job
- Logs early stopping events
- Returns best checkpoint, not final
- Supports multiple stopping criteria`,
    },
    relatedTerms: ['epochs', 'training-loss', 'validation-status', 'checkpoint'],
    aliases: ['early-stop', 'stopping-criterion', 'validation-stopping'],
  },
  {
    id: 'overfitting',
    term: 'Overfitting',
    category: 'training',
    content: {
      brief: 'Model memorizes training data instead of learning general patterns. Signs: training loss much lower than validation loss. Fix: more data, regularization, early stopping, or fewer epochs.',
      detailed: `Overfitting occurs when a model learns training data too well, including noise and irrelevant patterns, resulting in poor generalization to new examples.

**Symptoms:**

**Primary Indicator:**
Training loss << Validation loss
- Training loss: 0.3
- Validation loss: 1.2
- Gap indicates overfitting

**Other Signs:**
- Perfect or near-perfect training accuracy
- Validation metrics plateau or worsen while training improves
- Model outputs training examples verbatim
- Poor performance on real-world data

**Causes:**

**Insufficient Data:**
- Too few training examples
- Not diverse enough
- Not representative of use cases

**Excessive Capacity:**
- Model too large for task
- LoRA rank too high
- Too many target modules

**Too Much Training:**
- Too many epochs
- No early stopping
- Continued training past convergence

**Prevention Strategies:**

**More Data:**
- Collect more examples
- Augment existing data
- Synthesize additional examples

**Regularization:**
- Increase weight_decay (try 0.01 → 0.1)
- Add dropout (if supported)
- Use smaller LoRA rank

**Early Stopping:**
- Monitor validation loss
- Stop when validation stops improving
- Use patience of 2-3 epochs

**Architecture Choices:**
- Reduce LoRA rank (32 → 16 → 8)
- Target fewer modules
- Use less capacity overall

**Training Adjustments:**
- Fewer epochs
- Higher learning rate (converges faster, less overfitting)
- Smaller batch size (adds noise, regularizes)

**Monitoring:**
Track gap between training and validation loss:
- Gap < 0.2: Healthy, possibly underfitting
- Gap 0.2-0.5: Normal, acceptable
- Gap 0.5-1.0: Mild overfitting, consider adjustments
- Gap > 1.0: Significant overfitting, action needed

**Underfitting vs Overfitting:**
- Underfitting: Both losses high, need more capacity/epochs
- Sweet spot: Both losses low, small gap
- Overfitting: Training loss low, validation loss high, large gap

**AdapterOS Tools:**
- Automatic validation loss tracking
- Early stopping support
- Loss curve visualization
- Configurable regularization`,
    },
    relatedTerms: ['training-loss', 'epochs', 'weight-decay', 'early-stopping'],
    aliases: ['overfit', 'memorization', 'generalization-gap'],
  },
  {
    id: 'base-model',
    term: 'Base Model',
    category: 'training',
    content: {
      brief: 'The foundation model that LoRA adapters modify. Common examples: Llama, Qwen, Mistral. Adapters must match the base model architecture and tokenizer.',
      detailed: `The base model is the pretrained foundation language model that LoRA adapters augment with new capabilities. All adapters are trained relative to a specific base model.

**Common Base Models:**

**Llama 3 / 3.1:**
- Sizes: 7B, 13B, 70B parameters
- Strong general capabilities
- Good code understanding
- Open weights

**Qwen 2.5:**
- Sizes: 0.5B, 1.5B, 3B, 7B, 14B, 32B, 72B
- Excellent multilingual support
- Strong reasoning
- Code-optimized variants

**Mistral:**
- Sizes: 7B, Mixtral 8x7B
- High quality outputs
- Efficient architecture
- Good instruction following

**Compatibility Requirements:**

**Architecture Match:**
- Adapter trained on Llama 3 7B only works with Llama 3 7B
- Cannot mix architectures (Llama + Qwen)
- Cannot mix sizes (7B + 13B)

**Tokenizer Match:**
- Must use same tokenizer as training
- Token IDs must align
- Vocabulary must match

**Version Match:**
- Llama 2 ≠ Llama 3
- Different versions = different base models
- Check exact model ID/hash

**Selecting a Base Model:**

**For Code:**
- Qwen 2.5 Coder
- Llama 3.1 (70B for complex tasks)
- Codestral

**For Chat/General:**
- Llama 3.1 Instruct
- Qwen 2.5 Instruct
- Mistral Instruct

**For Specialized Domains:**
- Start with general base model
- Fine-tune with LoRA for domain
- May need larger base for complex domains

**Base Model in AdapterOS:**
- Stored in registry with hash verification
- Tracked for each adapter
- Enforced compatibility checks
- Support for multiple bases simultaneously

**Memory Considerations:**
Base model loaded once, adapters swap on top:
- 7B model: ~14GB (float16)
- Multiple adapters: +50-200MB each
- Efficient for serving many adapters

**Base Model Updates:**
New base model version = retrain all adapters:
- Plan migration strategy
- Validate adapter performance on new base
- May see quality improvements`,
    },
    relatedTerms: ['lora-rank', 'target-modules', 'training-job'],
    aliases: ['foundation-model', 'pretrained-model', 'base'],
  },
];
