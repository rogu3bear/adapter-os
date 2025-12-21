import type { GlossaryEntry } from '@/data/glossary/types';

export const coreConceptsEntries: GlossaryEntry[] = [
  {
    id: 'tenant',
    term: 'Tenant',
    category: 'core-concepts',
    content: {
      brief: 'A tenant is the top-level isolation unit in AdapterOS, representing a user, organization, or environment with complete resource separation.',
      detailed: `A tenant provides strong isolation boundaries for adapters, stacks, training jobs, policies, and telemetry. Each tenant operates independently with its own namespace, permissions, and resource quotas.

Tenants enable multi-user deployments where different organizations or teams can share the same AdapterOS installation without interference. All resources (adapters, datasets, execution plans) are scoped to a tenant.

Common tenant patterns include: production/staging environments, per-customer deployments in SaaS scenarios, or departmental isolation within an organization. Tenant IDs follow the format \`tenant/domain/purpose/revision\` for hierarchical organization.`,
    },
    relatedTerms: ['adapter', 'stack', 'isolation', 'rbac'],
    aliases: ['tenancy', 'tenant-id', 'tenant isolation'],
  },
  {
    id: 'adapter',
    term: 'Adapter',
    category: 'core-concepts',
    content: {
      brief: 'An adapter is a LoRA (Low-Rank Adaptation) module that specializes a base model for a specific task, domain, or style without modifying the original model weights.',
      detailed: `Adapters are lightweight fine-tuning modules that inject learned behavior into a base model through low-rank matrix decompositions. They enable task-specific customization while maintaining the base model's general capabilities.

Each adapter is packaged in the \`.aos\` format with weights, metadata, and training provenance. Adapters can be combined in stacks for multi-domain inference, loaded/unloaded dynamically, and versioned independently.

Typical adapter sizes range from 10-50MB (compared to multi-GB base models), enabling rapid iteration and deployment. Adapters progress through lifecycle states: Unloaded → Cold → Warm → Hot → Resident, managed by the lifecycle subsystem for optimal memory usage.`,
    },
    relatedTerms: ['lora', 'base-model', 'stack', 'lifecycle', 'aos-format'],
    aliases: ['LoRA adapter', 'adapter module', 'fine-tuned adapter'],
  },
  {
    id: 'stack',
    term: 'Stack',
    category: 'core-concepts',
    content: {
      brief: 'A stack is a tenant-scoped collection of adapters with execution rules (workflow type, policies) that defines how adapters are combined for inference.',
      detailed: `Stacks orchestrate multiple adapters to handle complex tasks requiring different specializations. Each stack specifies a workflow type (Sequential, Parallel, or UpstreamDownstream) that controls execution order and data flow.

Stacks are the primary unit of inference deployment. When you send an inference request, you target a specific stack, and the router selects the most relevant adapters from that stack's membership. Stacks can be activated/deactivated for deployment control.

Example stacks: "code-review" (syntax checker + security scanner + style guide), "customer-support" (sentiment analyzer + FAQ retriever + response generator), "data-pipeline" (validator → transformer → summarizer).`,
    },
    relatedTerms: ['adapter', 'workflow-type', 'router', 'inference'],
    aliases: ['adapter stack', 'stack configuration', 'execution stack'],
  },
  {
    id: 'router',
    term: 'Router',
    category: 'core-concepts',
    content: {
      brief: 'The router is the K-sparse gating mechanism that dynamically selects the top-K most relevant adapters from a stack for each inference request.',
      detailed: `The router analyzes input embeddings and uses learned gating weights (Q15 quantized) to score adapter relevance. Only the top-K adapters are activated, reducing computation while maintaining quality. This sparse activation is key to efficient multi-adapter inference.

Router weights are calibrated during training and can be fine-tuned based on production usage patterns. The K value is configurable per stack (typical range: 2-8 adapters). Router decisions are logged in telemetry for debugging and optimization.

The router enables dynamic specialization: different parts of a request may activate different adapters. For example, a code review request might activate "python-syntax" for Python files and "javascript-lint" for JS files based on content analysis.`,
    },
    relatedTerms: ['stack', 'adapter', 'inference', 'k-sparse-routing'],
    aliases: ['adapter router', 'gating network', 'K-sparse router'],
  },
  {
    id: 'kernel',
    term: 'Kernel',
    category: 'core-concepts',
    content: {
      brief: 'Kernels are precompiled Metal compute shaders that execute LoRA operations on the GPU with deterministic, reproducible computation guaranteed by hardware.',
      detailed: `Metal kernels implement the low-level matrix operations for LoRA inference: matrix multiplication, addition, and activation functions. They are compiled ahead-of-time and cryptographically signed to ensure integrity.

The kernel manifest (\`metallib_manifest.json\`) specifies entry points, memory layouts, and determinism guarantees. Kernels leverage Apple Neural Engine (ANE) when available, falling back gracefully to GPU or CPU.

Deterministic kernels are critical for reproducible inference: identical inputs produce bit-identical outputs, enabling golden run verification and compliance auditing. The kernel signature chain prevents unauthorized modifications.`,
    },
    relatedTerms: ['metal', 'ane', 'determinism', 'backend'],
    aliases: ['Metal kernel', 'compute shader', 'GPU kernel'],
  },
  {
    id: 'base-model',
    term: 'Base Model',
    category: 'core-concepts',
    content: {
      brief: 'The base model is the foundation large language model (e.g., Qwen, Llama) that adapters specialize without modifying its frozen weights.',
      detailed: `Base models provide general language understanding and generation capabilities. In AdapterOS, base models remain immutable during inference and training—only adapter weights are learned or modified.

Supported base models include Qwen 2.5 (7B/14B/32B), Llama 3, and compatible architectures. Base models are stored separately from adapters and shared across all tenants to minimize storage.

The base model's architecture determines adapter compatibility: rank dimensions, attention heads, and layer counts must match. AdapterOS validates adapter-to-base-model compatibility during registration to prevent runtime errors.`,
    },
    relatedTerms: ['adapter', 'lora', 'training-job'],
    aliases: ['foundation model', 'pretrained model', 'LLM'],
  },
  {
    id: 'lora',
    term: 'LoRA',
    category: 'core-concepts',
    content: {
      brief: 'Low-Rank Adaptation: a parameter-efficient fine-tuning technique that modifies model behavior by injecting low-rank matrix decompositions into attention layers.',
      detailed: `LoRA decomposes weight updates into two smaller matrices (A and B) such that ΔW = B × A, where A and B have much lower rank than the original weight matrix. This reduces trainable parameters by 100-1000x while maintaining quality.

Key hyperparameters: rank (r, typically 8-32) controls expressiveness, alpha (α) controls learning rate scaling. Higher rank = more capacity but larger files and slower inference.

AdapterOS implements Micro-LoRA: highly efficient LoRA with quantization (Q15), sparse routing, and optimized kernels. Training completes in minutes on M-series hardware, and adapters deploy in milliseconds.`,
    },
    relatedTerms: ['adapter', 'base-model', 'training-job', 'lora-rank'],
    aliases: ['Low-Rank Adaptation', 'LoRA fine-tuning', 'micro-LoRA'],
  },
  {
    id: 'control-plane',
    term: 'Control Plane',
    category: 'core-concepts',
    content: {
      brief: 'The management layer that orchestrates adapter lifecycle, enforces policies, manages resources, and provides APIs for system configuration and monitoring.',
      detailed: `The control plane is the central coordination authority in AdapterOS. It handles adapter registration, lifecycle transitions (loading/unloading), stack management, policy enforcement, and telemetry collection.

Control plane operations are exposed via REST APIs (~190 endpoints) and the CLI (51 command groups). All operations are authenticated (JWT Ed25519), authorized (RBAC with 55 permissions), and logged in telemetry.

The control plane runs in the \`adapteros-server-api\` crate and delegates to subsystems: lifecycle manager for adapter states, policy engine for validation, registry for metadata, and orchestrator for distributed coordination.`,
    },
    relatedTerms: ['lifecycle', 'policy', 'rbac'],
    aliases: ['management plane', 'orchestration layer', 'CP'],
  },
  {
    id: 'workflow-type',
    term: 'Workflow Type',
    category: 'core-concepts',
    content: {
      brief: 'Workflow type defines how adapters in a stack are executed: Sequential (ordered pipeline), Parallel (concurrent), or UpstreamDownstream (two-phase processing).',
      detailed: `Sequential workflows execute adapters in order, passing outputs as inputs to the next stage. Use for pipelines like validation → transformation → summarization.

Parallel workflows execute all selected adapters concurrently and merge results (voting, averaging, or concatenation). Use for ensemble methods or multi-perspective analysis.

UpstreamDownstream workflows split execution into two phases: upstream adapters process inputs, downstream adapters refine outputs. Use for retrieval-then-generation or planning-then-execution patterns.`,
    },
    relatedTerms: ['stack', 'adapter', 'inference', 'execution-plan'],
    aliases: ['execution mode', 'workflow mode', 'stack workflow'],
  },
  {
    id: 'telemetry',
    term: 'Telemetry',
    category: 'core-concepts',
    content: {
      brief: 'Telemetry is the structured event logging system that creates an immutable audit trail of all inference, training, and system operations in canonical JSON format.',
      detailed: `Every significant operation in AdapterOS emits telemetry events: adapter loads, inference requests, policy checks, training steps. Events are timestamped, signed, and linked in a Merkle chain for integrity.

Telemetry events follow a canonical schema with strict versioning. They include: event type, timestamp, actor (user/system), resources (adapter IDs, tenant), operation details, and outcome (success/error).

Telemetry enables: compliance auditing (who did what when), debugging (trace request flow), performance analysis (latency breakdowns), and determinism verification (replay golden runs). Events are stored in SQLite WAL mode and archived in signed bundles.`,
    },
    relatedTerms: ['telemetry-bundle', 'merkle-chain', 'audit-log', 'golden-run'],
    aliases: ['telemetry events', 'event logging', 'audit trail'],
  },
  {
    id: 'telemetry-bundle',
    term: 'Telemetry Bundle',
    category: 'core-concepts',
    content: {
      brief: 'A telemetry bundle is a compressed, cryptographically signed archive of telemetry events used for replay, audit, and compliance verification.',
      detailed: `Bundles group related telemetry events (e.g., all events from a single inference request or training job) into a portable, tamper-evident package. The bundle includes events, signatures, and metadata.

Bundles enable offline analysis and regulatory compliance: export a bundle for external audit, replay it in a staging environment to verify determinism, or archive it for long-term retention.

Bundle format: gzip-compressed NDJSON with SHA-256 manifest and Ed25519 signature. Bundles can be validated independently without access to the live system, ensuring auditability even after deployment changes.`,
    },
    relatedTerms: ['telemetry', 'golden-run', 'replay', 'audit-log'],
    aliases: ['telemetry archive', 'event bundle', 'audit bundle'],
  },
  {
    id: 'golden-run',
    term: 'Golden Run',
    category: 'core-concepts',
    content: {
      brief: 'A golden run (also called a Verified Run) is a verified, deterministic inference execution whose telemetry bundle serves as a canonical reference for validating future executions through replay.',
      detailed: `Golden runs establish ground truth for determinism testing. When you create a golden run, AdapterOS executes an inference request, captures all telemetry, and stores the bundle with outputs.

To verify determinism, replay the golden run: load the same adapters, execute the same input, and compare outputs byte-for-byte. Any divergence indicates non-deterministic behavior (floating-point drift, random seeding issues, etc.).

Golden runs are created with \`aosctl golden create\` and verified with \`aosctl replay\`. They are essential for: regression testing (ensure updates don't break determinism), compliance (prove reproducible outputs), and debugging (isolate divergence sources).`,
    },
    relatedTerms: ['replay', 'determinism', 'telemetry-bundle', 'divergence'],
    aliases: ['golden execution', 'reference run', 'baseline execution', 'verified run'],
  },
  {
    id: 'replay',
    term: 'Replay',
    category: 'core-concepts',
    content: {
      brief: 'Replay (also called Run History) re-executes a golden run using its telemetry bundle to verify determinism by comparing outputs and intermediate states byte-for-byte.',
      detailed: `Replay loads a telemetry bundle, reconstructs the execution environment (same adapters, same inputs, same random seeds), and re-runs the operation. Every output tensor, every intermediate activation, and every decision must match the golden run exactly.

Replay detects: kernel changes (different Metal shaders), adapter drift (weights modified), environmental differences (different hardware), or logic bugs (uninitialized state).

The replay system uses HKDF-seeded randomness to ensure reproducibility across runs. Divergence reports show exactly where outputs differ (layer, tensor index, bit offset), enabling rapid debugging.`,
    },
    relatedTerms: ['golden-run', 'determinism', 'divergence', 'telemetry-bundle'],
    aliases: ['replay execution', 'determinism verification', 'golden replay', 'run history'],
  },
  {
    id: 'divergence',
    term: 'Divergence',
    category: 'core-concepts',
    content: {
      brief: 'A divergence is a mismatch between golden run and replay execution, indicating non-deterministic behavior or environmental differences.',
      detailed: `Divergences are detected during replay by comparing outputs at multiple granularities: final response tokens, intermediate layer activations, attention weights, and router decisions.

Common divergence causes: floating-point non-associativity (GPU thread scheduling), uninitialized variables, time-dependent logic, or hardware differences (ANE vs GPU). AdapterOS minimizes divergences through deterministic kernels and controlled execution.

Divergence reports include: location (layer, operation), magnitude (L1/L2 distance), and context (input that triggered it). Small divergences (<1e-6) are often acceptable; large divergences (>1e-3) indicate bugs.`,
    },
    relatedTerms: ['replay', 'golden-run', 'determinism'],
    aliases: ['output divergence', 'non-determinism', 'replay mismatch'],
  },
  {
    id: 'merkle-chain',
    term: 'Merkle Chain',
    category: 'core-concepts',
    content: {
      brief: 'A Merkle chain is a cryptographically linked sequence of hashed telemetry events that creates an immutable, verifiable audit trail.',
      detailed: `Each telemetry event includes the hash of the previous event, forming a tamper-evident chain. Any modification to historical events breaks the chain, enabling integrity verification.

Merkle chains provide: append-only guarantees (can't delete events), ordering guarantees (events are causally linked), and non-repudiation (signatures prove authorship).

The chain is stored in the database and included in telemetry bundles. External auditors can verify the entire chain without access to the live system, ensuring compliance with regulatory requirements for immutable audit logs.`,
    },
    relatedTerms: ['telemetry', 'audit-log', 'telemetry-bundle'],
    aliases: ['event chain', 'hash chain', 'audit chain'],
  },
  {
    id: 'cpid',
    term: 'CPID',
    category: 'core-concepts',
    content: {
      brief: 'Control Plane ID: a unique identifier that groups related policies, execution plans, and telemetry events for a specific deployment or configuration.',
      detailed: `CPIDs provide a stable reference for versioned deployments. When you update a stack's policies or workflow, a new CPID is generated. All inference requests using that configuration are tagged with the CPID.

CPIDs enable: configuration tracking (which policies were active), A/B testing (compare CPIDs for different configurations), and rollback (revert to previous CPID).

CPID format: UUID v4 or deterministic hash of configuration. CPIDs appear in telemetry events, execution plans, and policy validation results, enabling correlation across system components.`,
    },
    relatedTerms: ['policy', 'execution-plan', 'telemetry'],
    aliases: ['control plane ID', 'configuration ID', 'deployment ID'],
  },
  {
    id: 'determinism',
    term: 'Determinism',
    category: 'core-concepts',
    content: {
      brief: 'System behavior that produces bit-identical outputs for identical inputs, ensuring reproducible results across executions, hardware, and time.',
      detailed: `AdapterOS guarantees determinism through: HKDF-seeded randomness (reproducible RNG), deterministic kernels (hardware-guaranteed computation), deterministic execution order (FIFO task queue), and controlled floating-point operations.

Determinism is critical for: compliance (prove outputs are reproducible), debugging (eliminate non-deterministic flakiness), and testing (golden runs must replay exactly).

The deterministic execution subsystem (\`adapteros-deterministic-exec\`) provides \`spawn_deterministic\` for tasks requiring reproducibility. All inference, training, and routing operations use deterministic execution. Background tasks (monitoring, logging) may use non-deterministic \`tokio::spawn\`.`,
    },
    relatedTerms: ['golden-run', 'replay'],
    aliases: ['deterministic execution', 'reproducibility', 'deterministic behavior'],
  },
  {
    id: 'zero-egress',
    term: 'Zero Egress',
    category: 'core-concepts',
    content: {
      brief: 'Security mode that blocks all outbound network connections during inference and training to prevent data exfiltration and ensure air-gapped operation.',
      detailed: `Zero-egress mode enforces network isolation at the policy level. All external communication (HTTP, DNS, sockets) is blocked. Only Unix domain sockets (UDS) for local IPC are permitted.

This prevents: model weight exfiltration, training data leakage, prompt injection attacks that attempt network calls, and supply chain attacks via malicious dependencies.

Zero-egress is validated at runtime by the egress policy pack. Violations are logged in telemetry and can trigger automatic adapter unloading. Enable with \`--enforce-zero-egress\` flag or in stack policies.`,
    },
    relatedTerms: ['policy', 'egress-policy'],
    aliases: ['network isolation', 'air-gapped mode', 'egress blocking'],
  },
  {
    id: 'execution-plan',
    term: 'Execution Plan',
    category: 'core-concepts',
    content: {
      brief: 'A compiled specification that defines which adapters to load, in what order, with which policies, for a specific inference or training task.',
      detailed: `Execution plans are generated from stack configurations and router decisions. They specify: adapter IDs and lifecycle states required, workflow type and execution order, memory allocation and eviction policies, and policy checks to enforce.

Plans are compiled once and cached for performance. When an inference request arrives, the plan is retrieved (or compiled), validated against current system state, and executed.

Plans enable: pre-flight validation (check resource availability before execution), optimization (batch-load adapters), and debugging (inspect planned vs actual execution). Plans are included in telemetry for audit trails.`,
    },
    relatedTerms: ['stack', 'workflow-type', 'lifecycle', 'policy'],
    aliases: ['execution strategy', 'adapter plan', 'inference plan'],
  },
  {
    id: 'lifecycle',
    term: 'Lifecycle',
    category: 'core-concepts',
    content: {
      brief: 'The adapter lifecycle state machine manages transitions through Unloaded, Cold, Warm, Hot, and Resident states to optimize memory usage and inference latency.',
      detailed: `Lifecycle states represent readiness levels:
- **Unloaded**: Not in memory (disk only)
- **Cold**: Metadata loaded, weights on disk
- **Warm**: Weights in RAM, not GPU-resident
- **Hot**: Weights on GPU, ready for inference
- **Resident**: Pinned in GPU memory, never evicted

Transitions are managed by the lifecycle subsystem based on usage patterns, memory pressure, and pinning policies. Hot adapters serve requests with minimal latency; warm adapters require GPU upload (milliseconds); cold adapters require disk read + GPU upload (seconds).

Use \`aosctl adapter pin\` to keep critical adapters resident, preventing eviction. The lifecycle manager automatically promotes frequently-used adapters to hot state and demotes idle adapters to conserve memory.`,
    },
    relatedTerms: ['adapter', 'hot-swap'],
    aliases: ['adapter lifecycle', 'state machine', 'lifecycle management'],
  },
  {
    id: 'hot-swap',
    term: 'Hot Swap',
    category: 'core-concepts',
    content: {
      brief: 'Hot swap enables replacing a running adapter with a new version without interrupting inference, using atomic pointer updates and memory pooling.',
      detailed: `Hot swap updates an adapter's weights while it remains loaded in GPU memory. The new version is loaded into a staging area, validated, then atomically swapped with the active version using pointer CAS (compare-and-swap).

In-flight requests complete using the old version; new requests use the new version. No downtime, no request failures. Hot swap is critical for: zero-downtime deployments, A/B testing (swap between versions), and emergency rollbacks.

The hot swap system (\`adapteros-lora-worker/adapter_hotswap.rs\`) manages memory pools to avoid allocation overhead. Swaps complete in <10ms. Failed swaps automatically roll back to the previous version.`,
    },
    relatedTerms: ['lifecycle', 'adapter'],
    aliases: ['adapter swap', 'hot reload', 'zero-downtime update'],
  },
  // NOTE: 'rbac' is defined in security.ts
  {
    id: 'policy',
    term: 'Guardrails',
    category: 'core-concepts',
    content: {
      brief: 'Guardrails (policies) are declarative rules enforced at runtime that govern adapter behavior, resource usage, security, and compliance.',
      detailed: `AdapterOS includes 24 canonical guardrails (technically called policy packs or policies): Egress (network isolation), Determinism (reproducibility), Router (K-sparse gating), Evidence (audit requirements), Memory (resource limits), Secrets (credential management), and more.

Policies are defined in TOML, cryptographically signed (Ed25519), and enforced by the policy engine. Violations can trigger: request rejection, adapter unloading, alerts, or logging.

Policies are scoped to tenants or stacks. Stack policies override tenant policies for specific workflows. Policy evaluation is logged in telemetry for audit trails.

Use \`aosctl policy list\` to view active policies, \`aosctl policy validate\` to check compliance, and \`aosctl policy sign\` to approve new policies (requires Admin role).`,
    },
    relatedTerms: ['policy-pack', 'compliance', 'stack'],
    aliases: ['policy', 'policies', 'policy enforcement', 'policy pack', 'governance', 'rules', 'security rules'],
  },
  {
    id: 'inference',
    term: 'Inference',
    category: 'core-concepts',
    content: {
      brief: 'Inference is the process of executing a trained model (base model + adapters) on input data to generate predictions, completions, or classifications.',
      detailed: `AdapterOS inference flow:
1. Client sends request to \`/v1/infer\` with prompt, stack ID, and parameters
2. Router selects top-K adapters from the stack
3. Lifecycle manager ensures selected adapters are Hot (GPU-resident)
4. Backend (CoreML/MLX/Metal) executes base model + adapter forward passes
5. Response is streamed back to client; telemetry is logged

Inference supports: batch requests (multiple prompts), streaming (SSE), structured outputs (JSON schema validation), and multi-turn conversations (session state).

Inference requests are deterministic (identical inputs → identical outputs) when using deterministic backends and seeded randomness. Golden runs can verify this property through replay.`,
    },
    relatedTerms: ['adapter', 'router', 'stack', 'backend'],
    aliases: ['model inference', 'inference execution', 'prediction'],
  },
  {
    id: 'aos-format',
    term: 'AOS Format',
    category: 'core-concepts',
    content: {
      brief: 'The .aos file format is a binary container for adapter packages, with a 64-byte header, memory-mapped weights, and JSON metadata for zero-copy loading.',
      detailed: `AOS format structure:
- **Header (64 bytes)**: Magic number (4), flags (4), weights offset/size (16), manifest offset/size (16), reserved (24)
- **Weights section**: Raw tensor data (float16/int8), page-aligned for mmap
- **Manifest section**: JSON metadata (architecture, training config, provenance)

The format enables zero-copy loading: mmap the file, parse the header, point to weight offsets—no deserialization overhead. Adapters load in <10ms.

AOS files are cryptographically signed (manifest hash + Ed25519 signature) to prevent tampering. The packaging tool (\`AdapterPackager\`) validates format compliance and generates signatures.`,
    },
    relatedTerms: ['adapter', 'lifecycle'],
    aliases: ['.aos file', 'adapter package', 'AOS container'],
  },
];
