import type { GlossaryEntry } from '../types';

/**
 * UI Fields Glossary Entries
 *
 * Form-specific help text for all pages and modals in AdapterOS UI.
 * Extracted from help-tooltip.tsx fallbacks and consolidated for reuse.
 */

export const uiFieldsEntries: GlossaryEntry[] = [
  // ===== Core System Fields =====
  {
    id: 'cpid',
    term: 'Control Plane ID',
    category: 'ui-fields',
    content: {
      brief: 'Control Plane ID: identifier that groups policies, plans, and telemetry.',
      detailed: 'Unique identifier for a control plane instance. The CPID groups related policies, execution plans, and telemetry bundles together for governance and audit purposes. Used in promotion workflows and policy management.',
    },
    relatedTerms: ['policy-cpid', 'promotion-cpid'],
    aliases: ['cpid', 'control plane id'],
  },
  {
    id: 'merkle-root',
    term: 'Merkle Root',
    category: 'ui-fields',
    content: {
      brief: 'Root hash of a Merkle tree used to attest integrity of bundled events.',
      detailed: 'Cryptographic hash at the root of a Merkle tree structure. Provides tamper-evident integrity verification for bundled telemetry events and audit logs. Any modification to underlying events changes the root hash.',
    },
    relatedTerms: ['telemetry-event', 'audit-events'],
    aliases: ['merkle root', 'root hash'],
  },
  {
    id: 'schema-hash',
    term: 'Schema Hash',
    category: 'ui-fields',
    content: {
      brief: 'Content hash of the policy schema version applied to a policy pack.',
      detailed: 'BLAKE3 hash of the policy schema definition. Ensures that policies are validated against the correct schema version. Used to detect schema drift and enforce policy compatibility.',
    },
    relatedTerms: ['policy-schema-hash', 'policy-version'],
    aliases: ['schema hash', 'policy schema'],
  },
  {
    id: 'tokens-per-second',
    term: 'Tokens Per Second',
    category: 'ui-fields',
    content: {
      brief: 'Throughput: number of tokens processed per second across the system.',
      detailed: 'System-wide or per-adapter metric measuring token processing throughput. Higher values indicate better performance. Typical values: 100-500 tokens/sec for inference, 1000+ for training on GPU.',
    },
    relatedTerms: ['training-tokens-per-sec', 'inference-max-tokens'],
    aliases: ['throughput', 'tokens/sec', 'tps'],
  },
  {
    id: 'latency-p95',
    term: 'Latency P95',
    category: 'ui-fields',
    content: {
      brief: 'Latency p95: 95th percentile end-to-end response latency in milliseconds.',
      detailed: '95th percentile latency measurement - 95% of requests complete faster than this value. More representative of user experience than average latency. Target: <100ms for interactive, <500ms for batch.',
    },
    relatedTerms: ['routing-latency', 'routing-overhead'],
    aliases: ['p95', '95th percentile', 'latency'],
  },
  {
    id: 'adapter-count',
    term: 'Adapter Count',
    category: 'ui-fields',
    content: {
      brief: 'Total number of active code adapters loaded in the system.',
      detailed: 'Number of LoRA adapters currently loaded in memory and available for inference. Includes adapters in Warm, Hot, and Resident states across all compute nodes.',
    },
    relatedTerms: ['adapter-state', 'tenant-adapters', 'node-adapters'],
    aliases: ['active adapters', 'loaded adapters'],
  },
  {
    id: 'active-sessions',
    term: 'Active Sessions',
    category: 'ui-fields',
    content: {
      brief: 'Concurrent active user or service sessions currently using the system.',
      detailed: 'Number of authenticated sessions with recent activity. Includes user UI sessions, API clients, and service-to-service connections. Used for capacity planning and license compliance.',
    },
    relatedTerms: ['tenant-users', 'audit-user'],
    aliases: ['sessions', 'concurrent users'],
  },

  // ===== Permission and Access =====
  {
    id: 'requires-admin',
    term: 'Requires Admin',
    category: 'ui-fields',
    content: {
      brief: 'This action requires the Admin role. Contact an administrator for access.',
      detailed: 'Admin-only operation. Only users with the Admin role (full system access) can perform this action. If you need access, contact your system administrator to review your role assignment.',
    },
    relatedTerms: ['tenant-actions', 'node-actions'],
    aliases: ['admin only', 'admin required'],
  },

  // ===== Navigation and Settings =====
  {
    id: 'operations',
    term: 'Operations',
    category: 'ui-fields',
    content: {
      brief: 'Runtime management, plan execution, and system monitoring.',
      detailed: 'Operations section provides runtime management capabilities including adapter operations, plan execution, system monitoring, and health checks.',
    },
    aliases: ['operations section', 'ops'],
  },
  {
    id: 'settings',
    term: 'Settings',
    category: 'ui-fields',
    content: {
      brief: 'System configuration and administration.',
      detailed: 'Settings section for configuring system parameters, managing users, tenants, policies, and administrative functions.',
    },
    aliases: ['settings section', 'config'],
  },
  {
    id: 'compute-nodes',
    term: 'Compute Nodes',
    category: 'ui-fields',
    content: {
      brief: 'Worker nodes in the cluster running inference and training workloads.',
      detailed: 'Physical or virtual machines that execute inference requests and training jobs. Each node has dedicated CPU, memory, and optionally GPU resources.',
    },
    relatedTerms: ['node-name', 'node-status', 'node-cpu'],
    aliases: ['nodes', 'workers', 'cluster nodes'],
  },
  {
    id: 'active-tenants',
    term: 'Active Tenants',
    category: 'ui-fields',
    content: {
      brief: 'Isolated tenant environments with their own adapters and policies.',
      detailed: 'Number of tenant environments currently in Active state. Each tenant has isolated resources, dedicated adapters, and independent policy enforcement.',
    },
    relatedTerms: ['tenant-name', 'tenant-status', 'tenant-isolation'],
    aliases: ['tenants', 'tenant count'],
  },

  // ===== Data Classification and Compliance =====
  {
    id: 'data-classification',
    term: 'Data Classification',
    category: 'ui-fields',
    content: {
      brief: 'Sensitivity level of data (Public, Internal, Confidential, Restricted) that determines access controls and handling requirements.',
      detailed: 'Data sensitivity classification following industry standards:\n- **Public**: No restrictions, can be shared externally\n- **Internal**: Internal use only, not for external sharing\n- **Confidential**: Sensitive business data, restricted access\n- **Restricted**: Highly sensitive, strictest controls and audit\n\nClassification drives policy enforcement and access controls.',
    },
    relatedTerms: ['itar-compliance', 'tenant-isolation'],
    aliases: ['classification', 'data sensitivity'],
  },
  {
    id: 'itar-compliance',
    term: 'ITAR Compliance',
    category: 'ui-fields',
    content: {
      brief: 'International Traffic in Arms Regulations compliance flag. When enabled, enforces strict US export control requirements for defense-related data.',
      detailed: 'ITAR flag indicates data subject to US export control laws (defense articles, technical data). When enabled:\n- Restricts access to US persons only\n- Enforces additional audit logging\n- Applies egress controls\n- Requires special handling procedures\n\nConsult legal/compliance before enabling.',
    },
    relatedTerms: ['data-classification', 'policy-compliance'],
    aliases: ['itar', 'export control'],
  },

  // ===== Policy Fields =====
  {
    id: 'policy-name',
    term: 'Policy Name',
    category: 'ui-fields',
    content: {
      brief: 'Human-readable name for the policy pack identifying its purpose and scope.',
      detailed: 'Descriptive name following convention: {domain}-{purpose}-{version} (e.g., egress-prod-v2, determinism-strict-v1). Should clearly indicate what the policy governs.',
    },
    relatedTerms: ['policy-version', 'policy-cpid'],
    aliases: ['policy identifier'],
  },
  {
    id: 'policy-version',
    term: 'Policy Version',
    category: 'ui-fields',
    content: {
      brief: 'Semantic version number (e.g., v1.2.3) tracking policy revisions and updates.',
      detailed: 'Semantic version (MAJOR.MINOR.PATCH):\n- **MAJOR**: Breaking changes, incompatible with previous version\n- **MINOR**: New rules added, backward compatible\n- **PATCH**: Bug fixes, clarifications\n\nVersion history is immutable for audit compliance.',
    },
    relatedTerms: ['policy-name', 'schema-hash'],
    aliases: ['version'],
  },
  {
    id: 'policy-status',
    term: 'Policy Status',
    category: 'ui-fields',
    content: {
      brief: 'Current state of the policy: Active (enforced), Draft (pending review), or Disabled.',
      detailed: 'Policy lifecycle state:\n- **Active**: Currently enforced on matching tenants/adapters\n- **Draft**: Under review, not yet enforced\n- **Disabled**: Temporarily deactivated, not enforced\n- **Archived**: Retired, read-only for audit\n\nOnly Active policies are enforced at runtime.',
    },
    relatedTerms: ['policy-signed', 'policy-actions'],
    aliases: ['status', 'state'],
  },
  {
    id: 'policy-signed',
    term: 'Policy Signed',
    category: 'ui-fields',
    content: {
      brief: 'Cryptographic signature status: indicates if the policy has been digitally signed for authenticity.',
      detailed: 'Digital signature using Ed25519 cryptography. Signed policies:\n- Cannot be modified without invalidating signature\n- Provide non-repudiation (proof of authorship)\n- Required for production deployment\n\nSigning requires Admin or Compliance role.',
    },
    relatedTerms: ['policy-status', 'policy-actions'],
    aliases: ['signature', 'signed'],
  },
  {
    id: 'policy-cpid',
    term: 'Policy CPID',
    category: 'ui-fields',
    content: {
      brief: 'Control Plane ID: unique identifier that groups policies, plans, and telemetry.',
      detailed: 'Control Plane ID linking this policy to execution plans and telemetry. Used for end-to-end tracking of policy enforcement and compliance reporting.',
    },
    relatedTerms: ['cpid', 'policy-schema-hash'],
    aliases: ['policy control plane id'],
  },
  {
    id: 'policy-schema-hash',
    term: 'Policy Schema Hash',
    category: 'ui-fields',
    content: {
      brief: 'BLAKE3 hash of the policy schema ensuring integrity and version tracking.',
      detailed: 'Cryptographic hash of the JSON schema used to validate this policy. Ensures policies are validated against correct schema version. Mismatch indicates schema drift or tampering.',
    },
    relatedTerms: ['schema-hash', 'policy-version'],
    aliases: ['policy hash'],
  },
  {
    id: 'policy-actions',
    term: 'Policy Actions',
    category: 'ui-fields',
    content: {
      brief: 'Available operations: Edit, Sign, Compare versions, and Export policy configurations.',
      detailed: 'Policy management actions:\n- **Edit**: Modify policy rules (creates new version)\n- **Sign**: Cryptographically sign for production use\n- **Compare**: Diff two policy versions side-by-side\n- **Export**: Download as JSON for backup/migration\n\nPermissions vary by role (Admin, Compliance).',
    },
    relatedTerms: ['policy-status', 'policy-signed'],
    aliases: ['policy operations'],
  },

  // ===== Inference Playground Fields =====
  {
    id: 'inference-model',
    term: 'Inference Model',
    category: 'ui-fields',
    content: {
      brief: 'Select the base model for inference. Different models have varying capabilities, context lengths, and performance characteristics.',
      detailed: 'Base language model powering inference. Available models:\n- **Qwen2.5 7B**: Fast, efficient, 32K context\n- **Llama 3.1 8B**: Balanced performance, 128K context\n- **Mistral 7B**: Strong reasoning, 32K context\n\nChoose based on latency requirements and task complexity.',
    },
    relatedTerms: ['base-model-name', 'inference-adapter-stack'],
    aliases: ['base model', 'model selection'],
  },
  {
    id: 'inference-adapter-stack',
    term: 'Inference Adapter Stack',
    category: 'ui-fields',
    content: {
      brief: 'Select a trained LoRA adapter to customize model behavior. Adapters add domain-specific knowledge without retraining the base model.',
      detailed: 'LoRA adapter stack to apply on top of base model. Adapters:\n- Add domain-specific knowledge (code, docs, etc.)\n- Fine-tune behavior for specific tasks\n- Stack multiple adapters via K-sparse routing\n\nLeave empty for base model only (no adaptation).',
    },
    relatedTerms: ['adapter-name', 'adapter-tier'],
    aliases: ['adapter stack', 'lora adapter'],
  },
  {
    id: 'inference-prompt',
    term: 'Inference Prompt',
    category: 'ui-fields',
    content: {
      brief: 'The input text or question for the model. Clear, specific prompts produce better results. Supports multi-turn conversations.',
      detailed: 'Text input to the model. Best practices:\n- Be specific and clear about desired output\n- Provide examples for complex tasks\n- Use system prompts for behavior/persona\n- Keep context within model limits\n\nSupports markdown and code blocks.',
    },
    relatedTerms: ['inference-max-tokens', 'inference-temperature'],
    aliases: ['prompt', 'input'],
  },
  {
    id: 'inference-max-tokens',
    term: 'Inference Max Tokens',
    category: 'ui-fields',
    content: {
      brief: 'Maximum number of tokens to generate in the response. Higher values allow longer responses but increase latency and cost.',
      detailed: 'Maximum output length in tokens (roughly 4 chars/token):\n- **100-500**: Short answers, summaries\n- **500-1000**: Detailed explanations\n- **1000-2000**: Long-form content\n- **2000+**: Documents, extensive code\n\nHigher limits increase latency and memory usage.',
    },
    relatedTerms: ['tokens-per-second', 'inference-temperature'],
    aliases: ['max tokens', 'output length'],
  },
  {
    id: 'inference-temperature',
    term: 'Inference Temperature',
    category: 'ui-fields',
    content: {
      brief: 'Controls output randomness. Lower values (0.0-0.3) for factual tasks, higher values (0.7-1.5) for creative tasks.',
      detailed: 'Sampling temperature controlling randomness:\n- **0.0**: Deterministic (always same output)\n- **0.1-0.3**: Factual, focused (code, data extraction)\n- **0.5-0.7**: Balanced (general QA, summaries)\n- **0.8-1.2**: Creative (writing, brainstorming)\n- **1.3+**: Very diverse, potentially incoherent\n\nDefault: 0.7 for most tasks.',
    },
    relatedTerms: ['inference-top-k', 'inference-top-p', 'inference-seed'],
    aliases: ['temperature', 'sampling temperature'],
  },
  {
    id: 'inference-top-k',
    term: 'Inference Top-K',
    category: 'ui-fields',
    content: {
      brief: 'Limits token selection to top K most probable tokens. Lower values (10-50) make output more focused and deterministic.',
      detailed: 'Top-K sampling: consider only K most probable tokens:\n- **1**: Greedy (always most probable)\n- **10-50**: Focused, coherent\n- **50-100**: Balanced diversity\n- **100+**: High diversity\n\nCombined with temperature for fine-grained control.',
    },
    relatedTerms: ['inference-top-p', 'inference-temperature'],
    aliases: ['top k', 'top-k sampling'],
  },
  {
    id: 'inference-top-p',
    term: 'Inference Top-P',
    category: 'ui-fields',
    content: {
      brief: 'Nucleus sampling threshold. Selects from smallest set of tokens whose cumulative probability exceeds P. Typically 0.9-0.95.',
      detailed: 'Top-P (nucleus) sampling: include tokens until cumulative probability reaches P:\n- **0.9**: Focused, excludes unlikely tokens\n- **0.95**: Balanced (recommended default)\n- **0.98**: More diverse\n- **1.0**: No filtering (all tokens considered)\n\nAlternative to top-K, often more stable.',
    },
    relatedTerms: ['inference-top-k', 'inference-temperature'],
    aliases: ['top p', 'nucleus sampling'],
  },
  {
    id: 'inference-seed',
    term: 'Inference Seed',
    category: 'ui-fields',
    content: {
      brief: 'Fixed random seed for reproducible outputs. Same seed with identical parameters produces consistent results for testing.',
      detailed: 'Random seed for HKDF-based determinism:\n- Set seed for reproducible outputs (testing, debugging)\n- Same seed + params = identical output\n- Leave empty for varied outputs\n\nRequired for golden run testing and determinism audits.',
    },
    relatedTerms: ['inference-temperature', 'testing-epsilon'],
    aliases: ['random seed', 'deterministic seed'],
  },
  {
    id: 'inference-evidence',
    term: 'Inference Evidence',
    category: 'ui-fields',
    content: {
      brief: 'Enable retrieval-augmented generation (RAG). Requires evidence spans from indexed documents to support the response.',
      detailed: 'RAG mode: augment responses with document evidence:\n- Retrieves relevant document chunks\n- Grounds responses in factual data\n- Provides citation spans\n- Reduces hallucination\n\nRequires documents indexed in collections.',
    },
    relatedTerms: ['inference-prompt'],
    aliases: ['rag', 'evidence mode', 'retrieval'],
  },
  {
    id: 'inference-stream',
    term: 'Inference Stream',
    category: 'ui-fields',
    content: {
      brief: 'Enable streaming mode to receive tokens as they are generated. Provides faster perceived response for interactive use.',
      detailed: 'Streaming mode (SSE):\n- Tokens arrive progressively as generated\n- Lower perceived latency for long outputs\n- Better UX for interactive chat\n- Allows early cancellation\n\nDisable for batch processing or when full response needed.',
    },
    relatedTerms: ['inference-max-tokens'],
    aliases: ['streaming', 'sse'],
  },
  {
    id: 'inference-compare-mode',
    term: 'Inference Compare Mode',
    category: 'ui-fields',
    content: {
      brief: 'Run inference with two different configurations side-by-side to compare outputs, latency, and quality.',
      detailed: 'A/B comparison mode:\n- Run same prompt with different adapters/params\n- Compare outputs, latency, quality metrics\n- Useful for adapter evaluation\n- Golden run validation\n\nHelps choose best adapter/configuration for task.',
    },
    relatedTerms: ['golden-comparison', 'inference-adapter-stack'],
    aliases: ['compare', 'a/b test'],
  },

  // ===== Adapter Fields =====
  {
    id: 'adapter-name',
    term: 'Adapter Name',
    category: 'ui-fields',
    content: {
      brief: 'Unique identifier for this adapter using semantic naming: tenant/domain/purpose/revision.',
      detailed: 'Semantic naming convention:\n```\n{tenant}/{domain}/{purpose}/{revision}\n```\nExamples:\n- `default/shop-floor/hydraulics/r001`\n- `acme/customer-service/sentiment/r003`\n\nEnsures uniqueness and provides context from name alone.',
    },
    relatedTerms: ['adapter-version', 'adapter-category'],
    aliases: ['adapter id', 'name'],
  },
  {
    id: 'adapter-tier',
    term: 'Adapter Tier',
    category: 'ui-fields',
    content: {
      brief: 'Adapter tier (tier_1, tier_2, tier_3) determines priority for routing and resource allocation.',
      detailed: 'Performance tier hierarchy:\n- **Tier 1**: Critical, highest priority, pinned to GPU\n- **Tier 2**: Important, frequently used, fast loading\n- **Tier 3**: Standard, loaded on-demand\n\nTier determines eviction order (tier 3 evicted first) and routing priority.',
    },
    relatedTerms: ['adapter-state', 'adapter-activation'],
    aliases: ['tier', 'priority tier'],
  },
  {
    id: 'adapter-rank',
    term: 'Adapter Rank',
    category: 'ui-fields',
    content: {
      brief: 'LoRA rank determines model capacity. Higher ranks (16, 32) capture more patterns but use more memory.',
      detailed: 'LoRA rank (dimensionality of adaptation):\n- **4-8**: Small, fast, low memory (simple tasks)\n- **12-16**: Medium, balanced (general purpose)\n- **32-64**: Large, high capacity (complex domains)\n\nHigher rank = more parameters = more memory + training time.',
    },
    relatedTerms: ['trainer-rank', 'adapter-memory'],
    aliases: ['rank', 'lora rank'],
  },
  {
    id: 'adapter-lifecycle',
    term: 'Adapter Lifecycle',
    category: 'ui-fields',
    content: {
      brief: 'Current lifecycle state: active (in use), deprecated (phasing out), or archived (read-only).',
      detailed: 'Lifecycle management state:\n- **Active**: Production use, available for inference\n- **Deprecated**: Phasing out, still usable but warning shown\n- **Archived**: Read-only, no inference, kept for compliance\n\nTransitions controlled by adapter owners/admins.',
    },
    relatedTerms: ['adapter-state', 'adapter-status'],
    aliases: ['lifecycle'],
  },
  {
    id: 'adapter-state',
    term: 'Adapter State',
    category: 'ui-fields',
    content: {
      brief: 'Memory state: Unloaded (not in memory), Cold (disk), Warm (CPU), Hot (GPU), Resident (pinned to GPU).',
      detailed: 'Memory tier state machine:\n- **Unloaded**: Not in memory, registry only\n- **Cold**: On disk, fast mmap loading\n- **Warm**: In CPU RAM, ready for GPU transfer\n- **Hot**: In GPU VRAM, ready for inference\n- **Resident**: Pinned to GPU, never evicted\n\nState transitions managed by lifecycle engine.',
    },
    relatedTerms: ['adapter-tier', 'adapter-memory'],
    aliases: ['state', 'memory state'],
  },
  {
    id: 'adapter-memory',
    term: 'Adapter Memory',
    category: 'ui-fields',
    content: {
      brief: 'Current memory usage of this adapter in bytes. Includes weights and activation buffers.',
      detailed: 'Total memory footprint:\n- LoRA weight matrices\n- Activation buffers\n- Metadata and manifest\n\nTypical sizes:\n- Rank 8: ~20-50 MB\n- Rank 16: ~50-150 MB\n- Rank 32: ~150-300 MB',
    },
    relatedTerms: ['adapter-rank', 'adapter-state', 'memory-usage'],
    aliases: ['memory usage', 'size'],
  },
  {
    id: 'adapter-activation',
    term: 'Adapter Activation',
    category: 'ui-fields',
    content: {
      brief: 'Number of times this adapter has been selected by the router for inference.',
      detailed: 'Activation count (router selection metric):\n- Incremented each time router selects this adapter\n- Used for usage analytics and tier promotion\n- High activation → consider tier 1 promotion\n- Low activation → candidate for eviction/archival',
    },
    relatedTerms: ['adapter-tier', 'adapter-last-used'],
    aliases: ['activations', 'usage count'],
  },
  {
    id: 'adapter-category',
    term: 'Adapter Category',
    category: 'ui-fields',
    content: {
      brief: 'Functional category of the adapter (code, docs, analysis, etc.).',
      detailed: 'Functional categorization:\n- **code**: Code generation, completion\n- **docs**: Documentation, technical writing\n- **analysis**: Data analysis, insights\n- **chat**: Conversational agents\n- **general**: Multi-purpose adapters\n\nHelps in filtering and organizing adapters.',
    },
    relatedTerms: ['adapter-name'],
    aliases: ['category', 'type'],
  },
  {
    id: 'adapter-version',
    term: 'Adapter Version',
    category: 'ui-fields',
    content: {
      brief: 'Semantic version of the adapter weights and configuration.',
      detailed: 'Adapter version (MAJOR.MINOR.PATCH):\n- Incremented on retraining\n- Tracks weights and config changes\n- Used for lineage tracking\n- Required for promotion workflows',
    },
    relatedTerms: ['adapter-name', 'policy-version'],
    aliases: ['version'],
  },
  {
    id: 'adapter-last-used',
    term: 'Adapter Last Used',
    category: 'ui-fields',
    content: {
      brief: 'Timestamp of the last inference request that used this adapter.',
      detailed: 'Last inference timestamp:\n- Updated on each router selection\n- Used for LRU eviction policies\n- Helps identify unused adapters\n- Informs tier demotion decisions',
    },
    relatedTerms: ['adapter-activation', 'adapter-state'],
    aliases: ['last used', 'last accessed'],
  },
  {
    id: 'adapter-actions',
    term: 'Adapter Actions',
    category: 'ui-fields',
    content: {
      brief: 'Available operations: Promote (increase tier), Pin/Unpin (prevent eviction), Evict (free memory), Delete (remove permanently).',
      detailed: 'Adapter management actions:\n- **Promote**: Move to higher tier (better performance)\n- **Demote**: Move to lower tier (free resources)\n- **Pin**: Mark as Resident (never evict)\n- **Unpin**: Allow eviction\n- **Evict**: Unload from memory\n- **Delete**: Permanent removal (requires confirmation)\n\nPermissions vary by role.',
    },
    relatedTerms: ['adapter-tier', 'adapter-state'],
    aliases: ['actions', 'operations'],
  },

  // ===== Training Fields =====
  {
    id: 'training-job-id',
    term: 'Training Job ID',
    category: 'ui-fields',
    content: {
      brief: 'Unique identifier or name for the training job.',
      detailed: 'Training job identifier:\n- Auto-generated UUID or user-provided name\n- Used for tracking job lifecycle\n- Links to resulting adapter\n- Referenced in audit logs',
    },
    relatedTerms: ['training-status', 'training-dataset'],
    aliases: ['job id', 'training id'],
  },
  {
    id: 'training-dataset',
    term: 'Training Dataset',
    category: 'ui-fields',
    content: {
      brief: 'The dataset used for training this adapter.',
      detailed: 'Dataset reference:\n- Links to dataset ID in dataset registry\n- Contains training examples (prompt/completion pairs)\n- Must pass validation before training\n- Immutable once training starts',
    },
    relatedTerms: ['training-job-id', 'trainer-file-upload'],
    aliases: ['dataset'],
  },
  {
    id: 'training-status',
    term: 'Training Status',
    category: 'ui-fields',
    content: {
      brief: 'Current state of the training job: queued, running, completed, failed, or cancelled.',
      detailed: 'Training lifecycle status:\n- **Queued**: Waiting for worker resources\n- **Running**: Active training in progress\n- **Paused**: Temporarily suspended\n- **Completed**: Successfully finished\n- **Failed**: Error occurred (check logs)\n- **Cancelled**: User-initiated stop\n\nStatus transitions logged for audit.',
    },
    relatedTerms: ['training-progress', 'training-actions'],
    aliases: ['status', 'state'],
  },
  {
    id: 'training-progress',
    term: 'Training Progress',
    category: 'ui-fields',
    content: {
      brief: 'Percentage of training epochs completed.',
      detailed: 'Progress metric:\n- Calculated as (completed_epochs / total_epochs) × 100\n- Updates in real-time during training\n- Used for ETA estimation\n- Includes current epoch and batch progress',
    },
    relatedTerms: ['training-status', 'trainer-epochs'],
    aliases: ['progress', 'completion'],
  },
  {
    id: 'training-loss',
    term: 'Training Loss',
    category: 'ui-fields',
    content: {
      brief: 'Current loss value - lower indicates better model fit. Target varies by task.',
      detailed: 'Loss function value:\n- Measures prediction error\n- Should decrease over time\n- Target loss varies by task (typically <1.0 for convergence)\n- Plateau indicates convergence or learning rate issues\n- Divergence (increasing) indicates instability',
    },
    relatedTerms: ['training-progress', 'trainer-learning-rate'],
    aliases: ['loss', 'training error'],
  },
  {
    id: 'training-learning-rate',
    term: 'Training Learning Rate',
    category: 'ui-fields',
    content: {
      brief: 'Step size for gradient descent optimization. Smaller values = slower but more stable training.',
      detailed: 'Optimizer learning rate:\n- Controls gradient descent step size\n- Typical range: 1e-5 to 1e-3\n- Too high: unstable, divergence\n- Too low: slow convergence\n- May use scheduler (warmup, decay)',
    },
    relatedTerms: ['trainer-learning-rate', 'training-loss'],
    aliases: ['learning rate', 'lr'],
  },
  {
    id: 'training-tokens-per-sec',
    term: 'Training Tokens Per Second',
    category: 'ui-fields',
    content: {
      brief: 'Training throughput measured in tokens processed per second.',
      detailed: 'Throughput metric:\n- Tokens/sec during training\n- Higher = faster training\n- GPU: 1000-5000+ tokens/sec\n- CPU: 50-200 tokens/sec\n- Affected by batch size, rank, model size',
    },
    relatedTerms: ['tokens-per-second', 'trainer-batch-size'],
    aliases: ['throughput', 'training speed'],
  },
  {
    id: 'training-created',
    term: 'Training Created',
    category: 'ui-fields',
    content: {
      brief: 'When this training job was created.',
      detailed: 'Job creation timestamp:\n- Recorded when job submitted\n- Used for job age calculations\n- Part of audit trail\n- Helps identify stale jobs',
    },
    relatedTerms: ['training-status', 'training-job-id'],
    aliases: ['created at', 'submitted'],
  },
  {
    id: 'training-actions',
    term: 'Training Actions',
    category: 'ui-fields',
    content: {
      brief: 'Available actions for this job based on its status and your permissions.',
      detailed: 'Training job actions:\n- **Pause**: Suspend running job\n- **Resume**: Continue paused job\n- **Cancel**: Terminate job\n- **Retry**: Rerun failed job\n- **View Logs**: Debug information\n- **Download Adapter**: Get trained weights\n\nActions enabled based on current status.',
    },
    relatedTerms: ['training-status', 'status-running'],
    aliases: ['actions', 'operations'],
  },

  // ===== Training Status-Specific =====
  {
    id: 'status-running',
    term: 'Status: Running',
    category: 'ui-fields',
    content: {
      brief: 'Training is actively in progress. You can pause or stop the job.',
      detailed: 'Active training state. Available actions: Pause, Cancel, View Real-time Metrics. Monitor loss and throughput to ensure convergence.',
    },
    relatedTerms: ['training-status', 'training-actions'],
    aliases: ['running status'],
  },
  {
    id: 'status-completed',
    term: 'Status: Completed',
    category: 'ui-fields',
    content: {
      brief: 'Training finished successfully. The adapter is ready for testing.',
      detailed: 'Training successfully finished. Adapter weights saved and registered. Next steps: test adapter, promote to production, run golden validation.',
    },
    relatedTerms: ['training-status', 'training-actions'],
    aliases: ['completed status'],
  },
  {
    id: 'status-failed',
    term: 'Status: Failed',
    category: 'ui-fields',
    content: {
      brief: 'Training encountered an error. Check logs for details.',
      detailed: 'Training failed due to error. Common causes: OOM, invalid dataset, convergence issues. Check logs for stack trace. Use Retry action after fixing issue.',
    },
    relatedTerms: ['training-status', 'training-actions'],
    aliases: ['failed status', 'error'],
  },
  {
    id: 'status-queued',
    term: 'Status: Queued',
    category: 'ui-fields',
    content: {
      brief: 'Job is waiting in queue. Will start when resources are available.',
      detailed: 'Job waiting for worker resources. Position in queue shown. Cancel if no longer needed. Consider scaling workers if queue time excessive.',
    },
    relatedTerms: ['training-status'],
    aliases: ['queued status', 'pending'],
  },
  {
    id: 'status-paused',
    term: 'Status: Paused',
    category: 'ui-fields',
    content: {
      brief: 'Training is temporarily paused. Resume to continue.',
      detailed: 'Training suspended by user. Checkpoints saved. Resources released. Use Resume to continue from checkpoint. Auto-cancelled if paused >24h.',
    },
    relatedTerms: ['training-status', 'training-actions'],
    aliases: ['paused status'],
  },
  {
    id: 'status-cancelled',
    term: 'Status: Cancelled',
    category: 'ui-fields',
    content: {
      brief: 'Training was cancelled by user.',
      detailed: 'User-initiated cancellation. Partial checkpoints may be available. Not retryable (submit new job instead). Resources freed.',
    },
    relatedTerms: ['training-status'],
    aliases: ['cancelled status', 'stopped'],
  },

  // ===== Audit Fields =====
  {
    id: 'audit-timestamp',
    term: 'Audit Timestamp',
    category: 'ui-fields',
    content: {
      brief: 'When the audit event occurred (local time). All timestamps are recorded in UTC and converted to your local timezone.',
      detailed: 'Event occurrence time:\n- Stored in UTC in database\n- Displayed in browser local timezone\n- Precision: milliseconds\n- Immutable (tamper-evident)',
    },
    relatedTerms: ['audit-event', 'audit-date-range'],
    aliases: ['timestamp', 'event time'],
  },
  {
    id: 'audit-level',
    term: 'Audit Level',
    category: 'ui-fields',
    content: {
      brief: 'Severity level of the audit event: debug (detailed diagnostics), info (general events), warn (attention needed), error (failures), critical (urgent issues).',
      detailed: 'Severity classification:\n- **Debug**: Detailed diagnostics (dev/troubleshooting)\n- **Info**: Normal operations (adapter load, policy apply)\n- **Warn**: Attention needed (deprecated API, quota warning)\n- **Error**: Operation failures (inference error, auth failure)\n- **Critical**: Urgent issues (security violation, data corruption)\n\nFilter logs by level for focused investigation.',
    },
    relatedTerms: ['audit-filter-level', 'audit-event'],
    aliases: ['severity', 'log level'],
  },
  {
    id: 'audit-event',
    term: 'Audit Event',
    category: 'ui-fields',
    content: {
      brief: 'The type of audit event (e.g., adapter.register, policy.apply, user.login). Events follow a hierarchical naming convention.',
      detailed: 'Event type identifier:\n- Hierarchical naming: `category.action`\n- Examples: `adapter.load`, `user.login`, `policy.sign`\n- Indexed for fast filtering\n- Immutable event taxonomy',
    },
    relatedTerms: ['audit-timestamp', 'audit-search'],
    aliases: ['event type', 'event'],
  },
  {
    id: 'audit-user',
    term: 'Audit User',
    category: 'ui-fields',
    content: {
      brief: 'The user who triggered this event. System events are marked as "System" for automated processes.',
      detailed: 'Event actor:\n- User ID/email for user-initiated actions\n- "System" for automated processes\n- Service accounts for API clients\n- Used for accountability and RBAC audits',
    },
    relatedTerms: ['audit-event', 'audit-search'],
    aliases: ['user', 'actor'],
  },
  {
    id: 'audit-details',
    term: 'Audit Details',
    category: 'ui-fields',
    content: {
      brief: 'Additional metadata associated with the event in JSON format. Contains context-specific information about the action performed.',
      detailed: 'Event metadata:\n- Structured JSON with event-specific fields\n- Includes: resource IDs, parameters, results\n- Searchable fields (trace_id, tenant_id, etc.)\n- Retained for compliance period',
    },
    relatedTerms: ['audit-event', 'audit-search'],
    aliases: ['metadata', 'context'],
  },
  {
    id: 'audit-controls',
    term: 'Audit Controls',
    category: 'ui-fields',
    content: {
      brief: 'Configure pagination and manually refresh audit logs. Use filters above for advanced searching.',
      detailed: 'Audit log controls:\n- Pagination: items per page\n- Manual refresh (also auto-refreshes every 30s)\n- Export filtered results\n- Requires audit:view permission',
    },
    relatedTerms: ['audit-items-per-page', 'audit-refresh'],
    aliases: ['controls'],
  },
  {
    id: 'audit-items-per-page',
    term: 'Audit Items Per Page',
    category: 'ui-fields',
    content: {
      brief: 'Number of audit log entries to display per page. Higher values may affect performance.',
      detailed: 'Pagination setting:\n- Options: 25, 50, 100, 200\n- Higher values = more data loaded\n- May affect browser performance on slow connections\n- Saved in user preferences',
    },
    relatedTerms: ['audit-controls', 'audit-pagination-prev'],
    aliases: ['page size', 'items per page'],
  },
  {
    id: 'audit-refresh',
    term: 'Audit Refresh',
    category: 'ui-fields',
    content: {
      brief: 'Manually refresh audit logs from the server. Logs also auto-refresh every 30 seconds.',
      detailed: 'Refresh control:\n- Manual: click refresh button\n- Auto: every 30 seconds\n- Preserves current filters and pagination\n- Shows loading indicator during fetch',
    },
    relatedTerms: ['audit-controls'],
    aliases: ['refresh'],
  },
  {
    id: 'audit-export',
    term: 'Audit Export',
    category: 'ui-fields',
    content: {
      brief: 'Export audit logs as JSON file. Exports filtered results if filters are active, otherwise exports all loaded logs.',
      detailed: 'Export functionality:\n- Format: JSON (newline-delimited)\n- Respects active filters\n- Filename: `audit-logs-{timestamp}.json`\n- Requires audit:view permission\n- Useful for compliance reporting',
    },
    relatedTerms: ['audit-controls', 'audit-events'],
    aliases: ['export', 'download'],
  },
  {
    id: 'audit-events',
    term: 'Audit Events',
    category: 'ui-fields',
    content: {
      brief: 'Immutable audit trail of security and system events. Accessible by Admin, SRE, and Compliance roles only.',
      detailed: 'Audit log properties:\n- Immutable (append-only)\n- Tamper-evident (Merkle tree signatures)\n- Retention policy enforced\n- RBAC-protected (Admin/SRE/Compliance)\n- Supports compliance reporting (SOC2, HIPAA)',
    },
    relatedTerms: ['audit-event', 'audit-timestamp'],
    aliases: ['audit trail', 'audit log'],
  },
  {
    id: 'audit-pagination-prev',
    term: 'Audit Pagination Previous',
    category: 'ui-fields',
    content: {
      brief: 'Navigate to the previous page of audit log results.',
      detailed: 'Previous page navigation. Disabled on first page. Preserves filters and items-per-page setting.',
    },
    relatedTerms: ['audit-pagination-next', 'audit-items-per-page'],
    aliases: ['previous page'],
  },
  {
    id: 'audit-pagination-next',
    term: 'Audit Pagination Next',
    category: 'ui-fields',
    content: {
      brief: 'Navigate to the next page of audit log results.',
      detailed: 'Next page navigation. Disabled on last page. Preserves filters and items-per-page setting.',
    },
    relatedTerms: ['audit-pagination-prev', 'audit-items-per-page'],
    aliases: ['next page'],
  },
  {
    id: 'audit-filter-level',
    term: 'Audit Filter Level',
    category: 'ui-fields',
    content: {
      brief: 'Filter audit logs by severity level. Select multiple levels to show events matching any selected level.',
      detailed: 'Severity filter:\n- Multi-select (OR logic)\n- Options: debug, info, warn, error, critical\n- Useful for focusing on errors or critical events\n- Combined with other filters (AND logic)',
    },
    relatedTerms: ['audit-level', 'audit-search'],
    aliases: ['level filter', 'severity filter'],
  },
  {
    id: 'audit-date-range',
    term: 'Audit Date Range',
    category: 'ui-fields',
    content: {
      brief: 'Filter audit logs by timestamp range. Useful for investigating events within a specific time window.',
      detailed: 'Date range filter:\n- Start/end datetime pickers\n- Timezone-aware (converted to UTC for query)\n- Useful for incident investigation\n- Combined with other filters',
    },
    relatedTerms: ['audit-timestamp', 'audit-filter-level'],
    aliases: ['date filter', 'time range'],
  },
  {
    id: 'audit-search',
    term: 'Audit Search',
    category: 'ui-fields',
    content: {
      brief: 'Search across event type, user ID, tenant ID, component, trace ID, and metadata fields.',
      detailed: 'Full-text search:\n- Searches: event type, user, tenant, component, trace_id\n- Also searches JSON metadata fields\n- Case-insensitive\n- Supports partial matches\n- Combined with filters (AND logic)',
    },
    relatedTerms: ['audit-event', 'audit-user', 'audit-details'],
    aliases: ['search', 'filter'],
  },

  // ===== Node Management Fields =====
  {
    id: 'node-name',
    term: 'Node Name',
    category: 'ui-fields',
    content: {
      brief: 'Unique hostname identifier for this compute node in the cluster.',
      detailed: 'Node identifier:\n- Typically hostname or FQDN\n- Must be unique across cluster\n- Used for routing workloads\n- Shown in metrics and logs',
    },
    relatedTerms: ['node-status', 'compute-nodes'],
    aliases: ['hostname', 'node id'],
  },
  {
    id: 'node-status',
    term: 'Node Status',
    category: 'ui-fields',
    content: {
      brief: 'Current health status: healthy (online and responsive), offline (unreachable), or error (experiencing issues).',
      detailed: 'Node health state:\n- **Healthy**: Passing health checks, ready for workloads\n- **Offline**: Unreachable, no heartbeat received\n- **Error**: Errors reported, degraded performance\n- **Draining**: Gracefully removing workloads before shutdown\n\nBased on heartbeat and health check results.',
    },
    relatedTerms: ['node-last-seen', 'node-actions'],
    aliases: ['status', 'health'],
  },
  {
    id: 'node-cpu',
    term: 'Node CPU',
    category: 'ui-fields',
    content: {
      brief: 'Current CPU utilization percentage across all cores on this node.',
      detailed: 'CPU metrics:\n- Average utilization across all cores\n- Real-time (updated every 5-10s)\n- High utilization (>80%) may indicate overload\n- Used for scheduling decisions',
    },
    relatedTerms: ['cpu-usage', 'node-memory'],
    aliases: ['cpu utilization', 'cpu %'],
  },
  {
    id: 'node-memory',
    term: 'Node Memory',
    category: 'ui-fields',
    content: {
      brief: 'Total available system memory in gigabytes for running workloads.',
      detailed: 'Memory capacity:\n- Total RAM available for adapters and models\n- Excludes OS reserved memory\n- System maintains 15% headroom\n- Used for capacity planning',
    },
    relatedTerms: ['memory-usage', 'adapter-memory'],
    aliases: ['ram', 'memory capacity'],
  },
  {
    id: 'node-gpu',
    term: 'Node GPU',
    category: 'ui-fields',
    content: {
      brief: 'Number of GPU devices available for inference and training acceleration.',
      detailed: 'GPU resources:\n- Count of GPUs on this node\n- Includes discrete GPUs and integrated GPUs\n- Used for inference and training acceleration\n- 0 = CPU-only node',
    },
    relatedTerms: ['node-memory', 'adapter-state'],
    aliases: ['gpu count', 'gpus'],
  },
  {
    id: 'node-adapters',
    term: 'Node Adapters',
    category: 'ui-fields',
    content: {
      brief: 'Count of adapters currently loaded and running on this node.',
      detailed: 'Loaded adapter count:\n- Adapters in Warm/Hot/Resident states\n- Used for load balancing\n- High count may indicate memory pressure\n- View adapter list in node details',
    },
    relatedTerms: ['adapter-count', 'adapter-state'],
    aliases: ['loaded adapters'],
  },
  {
    id: 'node-last-seen',
    term: 'Node Last Seen',
    category: 'ui-fields',
    content: {
      brief: 'Timestamp of the most recent heartbeat received from this node.',
      detailed: 'Heartbeat timestamp:\n- Updated every 10-30 seconds\n- Used to detect offline nodes\n- Stale (>2 minutes) triggers offline status\n- Shows in local timezone',
    },
    relatedTerms: ['node-status'],
    aliases: ['last heartbeat', 'last seen'],
  },
  {
    id: 'node-endpoint',
    term: 'Node Endpoint',
    category: 'ui-fields',
    content: {
      brief: 'Network endpoint URL where the node agent is listening for commands.',
      detailed: 'Node agent endpoint:\n- UDS socket path or HTTP URL\n- Used for control plane communication\n- Typically: `unix:///var/run/aos/node.sock`\n- Egress policy enforced',
    },
    relatedTerms: ['node-name'],
    aliases: ['endpoint', 'agent url'],
  },
  {
    id: 'node-actions',
    term: 'Node Actions',
    category: 'ui-fields',
    content: {
      brief: 'Available operations: view details, test connectivity, mark offline, or evict.',
      detailed: 'Node management actions:\n- **Details**: View full node metrics and logs\n- **Test**: Ping node for connectivity check\n- **Drain**: Gracefully remove workloads\n- **Offline**: Mark as offline (manual)\n- **Evict**: Force remove from cluster\n\nRequires node:manage permission.',
    },
    relatedTerms: ['node-status', 'node-register'],
    aliases: ['actions', 'operations'],
  },
  {
    id: 'node-register',
    term: 'Node Register',
    category: 'ui-fields',
    content: {
      brief: 'Register a new compute node to join the cluster. Requires node:manage permission.',
      detailed: 'Node registration:\n- Add new worker to cluster\n- Provide: hostname, endpoint, labels\n- Requires node:manage permission\n- Node must pass health check before accepting workloads',
    },
    relatedTerms: ['node-name', 'node-labels'],
    aliases: ['register node', 'add node'],
  },
  {
    id: 'node-labels',
    term: 'Node Labels',
    category: 'ui-fields',
    content: {
      brief: 'Key-value metadata tags for organizing and filtering nodes (e.g., region, tier).',
      detailed: 'Node labels:\n- Key-value pairs (e.g., `region=us-west`, `tier=gpu`)\n- Used for workload scheduling affinity\n- Filter nodes by labels in UI\n- Common labels: region, tier, hardware, purpose',
    },
    relatedTerms: ['node-name'],
    aliases: ['labels', 'tags', 'metadata'],
  },

  // ===== Dashboard System Resources =====
  {
    id: 'cpu-usage',
    term: 'CPU Usage',
    category: 'ui-fields',
    content: {
      brief: 'Current CPU utilization percentage across all cores in the system.',
      detailed: 'System-wide CPU metric:\n- Aggregate across all nodes\n- Updated every 5-10 seconds\n- Threshold alerts: >80% warning, >95% critical\n- Includes inference, training, and system overhead',
    },
    relatedTerms: ['node-cpu', 'memory-usage'],
    aliases: ['cpu utilization'],
  },
  {
    id: 'memory-usage',
    term: 'Memory Usage',
    category: 'ui-fields',
    content: {
      brief: 'Current RAM utilization percentage including system and application memory.',
      detailed: 'System-wide memory metric:\n- Aggregate across all nodes\n- Includes: adapters, base models, buffers, OS\n- System maintains 15% headroom\n- Alerts: >85% warning, >95% critical',
    },
    relatedTerms: ['node-memory', 'adapter-memory'],
    aliases: ['ram usage', 'memory utilization'],
  },
  {
    id: 'disk-usage',
    term: 'Disk Usage',
    category: 'ui-fields',
    content: {
      brief: 'Current disk space utilization percentage on the primary storage volume.',
      detailed: 'Storage metric:\n- Primary volume usage (adapters, datasets, models)\n- Includes: registry, checkpoints, telemetry\n- Alerts: >80% warning, >90% critical\n- Cleanup threshold: 85%',
    },
    relatedTerms: ['memory-usage'],
    aliases: ['storage usage', 'disk utilization'],
  },
  {
    id: 'network-bandwidth',
    term: 'Network Bandwidth',
    category: 'ui-fields',
    content: {
      brief: 'Current network throughput in megabytes per second for incoming traffic.',
      detailed: 'Network ingress metric:\n- MB/s incoming traffic\n- Typically low (UDS primary transport)\n- Spikes during: dataset uploads, adapter syncs\n- Egress policy limits outbound',
    },
    relatedTerms: ['disk-usage'],
    aliases: ['network throughput', 'bandwidth'],
  },

  // ===== Dashboard Activity and Actions =====
  {
    id: 'recent-activity',
    term: 'Recent Activity',
    category: 'ui-fields',
    content: {
      brief: 'Real-time feed of system events including adapter operations, policy changes, and telemetry.',
      detailed: 'Activity stream:\n- Last 100 events across system\n- Auto-refreshes every 10 seconds\n- Filter by: event type, user, tenant\n- Links to detailed audit logs',
    },
    relatedTerms: ['audit-events', 'tenant-last-activity'],
    aliases: ['activity feed', 'event stream'],
  },
  {
    id: 'quick-actions',
    term: 'Quick Actions',
    category: 'ui-fields',
    content: {
      brief: 'Frequently used operations for managing tenants, adapters, and system health.',
      detailed: 'Shortcut actions:\n- One-click access to common tasks\n- Respects RBAC permissions\n- Includes: create tenant, deploy adapter, view health\n- Customizable per user role',
    },
    relatedTerms: ['quick-action-health', 'quick-action-create-tenant'],
    aliases: ['shortcuts', 'actions'],
  },
  {
    id: 'export-logs',
    term: 'Export Logs',
    category: 'ui-fields',
    content: {
      brief: 'Download system logs for debugging and audit purposes.',
      detailed: 'Log export:\n- Format: JSON (newline-delimited)\n- Includes: system logs, audit logs, telemetry\n- Date range selection\n- Requires appropriate permissions',
    },
    relatedTerms: ['audit-export', 'telemetry-export'],
    aliases: ['download logs'],
  },

  // ===== Dashboard Quick Action Buttons =====
  {
    id: 'quick-action-health',
    term: 'Quick Action: Health',
    category: 'ui-fields',
    content: {
      brief: 'View detailed system health metrics including CPU, memory, and performance indicators.',
      detailed: 'Health dashboard shortcut:\n- Navigate to system overview page\n- Shows: CPU, memory, disk, network metrics\n- Node health status\n- Performance indicators',
    },
    relatedTerms: ['monitoring-overview', 'quick-actions'],
    aliases: ['health check'],
  },
  {
    id: 'quick-action-create-tenant',
    term: 'Quick Action: Create Tenant',
    category: 'ui-fields',
    content: {
      brief: 'Create a new isolated tenant environment with dedicated adapters and policies.',
      detailed: 'Tenant creation shortcut:\n- Opens create tenant modal\n- Requires tenant:manage permission\n- Prompts for: name, isolation level, UID/GID\n- Auto-initializes tenant registry',
    },
    relatedTerms: ['create-tenant-button', 'tenant-name'],
    aliases: ['new tenant'],
  },
  {
    id: 'quick-action-deploy-adapter',
    term: 'Quick Action: Deploy Adapter',
    category: 'ui-fields',
    content: {
      brief: 'Deploy a code adapter to a specific tenant for inference workloads.',
      detailed: 'Adapter deployment shortcut:\n- Opens deploy adapter modal\n- Requires adapter:register permission\n- Prompts for: adapter selection, target tenant\n- Validates compatibility before deployment',
    },
    relatedTerms: ['adapter-select-field', 'target-tenant-field'],
    aliases: ['deploy adapter'],
  },
  {
    id: 'quick-action-policies',
    term: 'Quick Action: Policies',
    category: 'ui-fields',
    content: {
      brief: 'Review and manage policy packs that govern adapter behavior and compliance.',
      detailed: 'Policy management shortcut:\n- Navigate to policies page\n- View: policy packs, compliance status\n- Actions: apply, sign, compare policies\n- Requires policy:view permission',
    },
    relatedTerms: ['policy-name', 'policy-actions'],
    aliases: ['manage policies'],
  },

  // ===== Dashboard Modal Form Fields =====
  {
    id: 'tenant-name-field',
    term: 'Tenant Name Field',
    category: 'ui-fields',
    content: {
      brief: 'Unique identifier for the tenant. Use lowercase letters, numbers, and hyphens only.',
      detailed: 'Tenant name input:\n- Format: lowercase, alphanumeric, hyphens\n- Must be unique across system\n- Used in URLs and API paths\n- Cannot be changed after creation',
    },
    relatedTerms: ['tenant-name', 'create-tenant-action'],
    aliases: ['tenant identifier'],
  },
  {
    id: 'isolation-level-field',
    term: 'Isolation Level Field',
    category: 'ui-fields',
    content: {
      brief: 'Security isolation level: Standard (shared resources), High (dedicated compute), Maximum (air-gapped).',
      detailed: 'Isolation level selector:\n- **Standard**: Shared compute, namespace isolation\n- **High**: Dedicated worker nodes, strict RBAC\n- **Maximum**: Air-gapped, no network access\n\nHigher isolation = more resources required.',
    },
    relatedTerms: ['tenant-isolation', 'tenant-name-field'],
    aliases: ['isolation selector'],
  },
  {
    id: 'adapter-select-field',
    term: 'Adapter Select Field',
    category: 'ui-fields',
    content: {
      brief: 'Choose an adapter from the registry to deploy to the target tenant.',
      detailed: 'Adapter picker:\n- Lists all registered adapters\n- Filter by: category, tier, name\n- Shows: adapter metadata, size, status\n- Validates compatibility with target tenant',
    },
    relatedTerms: ['adapter-name', 'target-tenant-field'],
    aliases: ['adapter picker'],
  },
  {
    id: 'target-tenant-field',
    term: 'Target Tenant Field',
    category: 'ui-fields',
    content: {
      brief: 'The tenant environment where the adapter will be deployed and made available.',
      detailed: 'Tenant selector:\n- Lists active tenants user has access to\n- Shows: tenant name, isolation level, status\n- Validates user has adapter:register permission\n- Links adapter to tenant registry',
    },
    relatedTerms: ['tenant-name', 'adapter-select-field'],
    aliases: ['tenant selector'],
  },

  // ===== Tenant Management Fields =====
  {
    id: 'tenant-name',
    term: 'Tenant Name',
    category: 'ui-fields',
    content: {
      brief: 'Unique identifier for this tenant. Used for isolation and access control purposes.',
      detailed: 'Tenant identifier:\n- Globally unique across system\n- Used in: URLs, logs, audit trails\n- Format: lowercase alphanumeric + hyphens\n- Cannot be renamed (create new tenant instead)',
    },
    relatedTerms: ['tenant-description', 'tenant-status'],
    aliases: ['tenant id'],
  },
  {
    id: 'tenant-description',
    term: 'Tenant Description',
    category: 'ui-fields',
    content: {
      brief: 'Brief description of the tenant purpose and scope.',
      detailed: 'Tenant description:\n- Free-form text (max 500 chars)\n- Describe: purpose, team, project\n- Shown in tenant list and details\n- Editable after creation',
    },
    relatedTerms: ['tenant-name'],
    aliases: ['description'],
  },
  {
    id: 'tenant-uid',
    term: 'Tenant UID',
    category: 'ui-fields',
    content: {
      brief: 'Unix User ID for filesystem isolation. Each tenant should have a unique UID.',
      detailed: 'Unix UID:\n- Used for filesystem permissions\n- Must be unique per tenant\n- Range: 1000-65535 (avoid system UIDs)\n- Cannot be changed after creation',
    },
    relatedTerms: ['tenant-gid', 'tenant-isolation'],
    aliases: ['uid', 'user id'],
  },
  {
    id: 'tenant-gid',
    term: 'Tenant GID',
    category: 'ui-fields',
    content: {
      brief: 'Unix Group ID for filesystem isolation. Controls group-level access permissions.',
      detailed: 'Unix GID:\n- Used for filesystem group permissions\n- Typically matches UID for single-tenant groups\n- Range: 1000-65535\n- Cannot be changed after creation',
    },
    relatedTerms: ['tenant-uid', 'tenant-isolation'],
    aliases: ['gid', 'group id'],
  },
  {
    id: 'tenant-isolation',
    term: 'Tenant Isolation',
    category: 'ui-fields',
    content: {
      brief: 'Isolation level determines the degree of resource separation (standard, strict, or custom).',
      detailed: 'Isolation levels:\n- **Standard**: Namespace + RBAC isolation\n- **Strict**: Dedicated nodes + network policies\n- **Custom**: Tailored isolation (contact admin)\n\nAffects: compute allocation, network access, storage.',
    },
    relatedTerms: ['tenant-uid', 'tenant-gid', 'isolation-level-field'],
    aliases: ['isolation level'],
  },
  {
    id: 'tenant-status',
    term: 'Tenant Status',
    category: 'ui-fields',
    content: {
      brief: 'Current operational state: Active (running), Paused (temporarily stopped), Suspended (admin action), Maintenance (upgrades), or Archived (decommissioned).',
      detailed: 'Tenant lifecycle status:\n- **Active**: Normal operations, accepting requests\n- **Paused**: User-initiated pause (resumable)\n- **Suspended**: Admin action (policy violation, billing)\n- **Maintenance**: Upgrades in progress (read-only)\n- **Archived**: Decommissioned (read-only, compliance retention)\n\nOnly Active tenants process inference.',
    },
    relatedTerms: ['tenant-actions', 'archive-tenant-action'],
    aliases: ['status', 'state'],
  },
  {
    id: 'tenant-created',
    term: 'Tenant Created',
    category: 'ui-fields',
    content: {
      brief: 'Timestamp when this tenant was first created in the system.',
      detailed: 'Creation timestamp:\n- Immutable record of tenant creation\n- Displayed in local timezone\n- Used for age calculations and retention policies',
    },
    relatedTerms: ['tenant-name', 'tenant-last-activity'],
    aliases: ['created at'],
  },
  {
    id: 'tenant-users',
    term: 'Tenant Users',
    category: 'ui-fields',
    content: {
      brief: 'Number of users assigned to this tenant with active access.',
      detailed: 'User count:\n- Includes: viewers, operators, admins\n- Only active (non-suspended) users\n- Click to view user list\n- Used for license compliance',
    },
    relatedTerms: ['active-sessions', 'tenant-name'],
    aliases: ['user count'],
  },
  {
    id: 'tenant-adapters',
    term: 'Tenant Adapters',
    category: 'ui-fields',
    content: {
      brief: 'Number of LoRA adapters assigned to this tenant for inference.',
      detailed: 'Adapter count:\n- Adapters registered to this tenant\n- Includes: all lifecycle states\n- Click to view adapter list\n- Used for capacity planning',
    },
    relatedTerms: ['adapter-count', 'assign-adapters-action'],
    aliases: ['adapter count'],
  },
  {
    id: 'tenant-policies',
    term: 'Tenant Policies',
    category: 'ui-fields',
    content: {
      brief: 'Number of policy packs applied to this tenant for governance.',
      detailed: 'Policy count:\n- Active policy packs enforced on tenant\n- Includes: egress, determinism, security policies\n- Click to view policy list\n- Used for compliance reporting',
    },
    relatedTerms: ['policy-name', 'assign-policies-action'],
    aliases: ['policy count'],
  },
  {
    id: 'tenant-last-activity',
    term: 'Tenant Last Activity',
    category: 'ui-fields',
    content: {
      brief: 'Most recent activity timestamp for this tenant.',
      detailed: 'Last activity:\n- Updated on: inference, training, policy changes\n- Used to identify inactive tenants\n- Displayed in local timezone\n- Informs archival decisions',
    },
    relatedTerms: ['tenant-created', 'recent-activity'],
    aliases: ['last activity', 'last used'],
  },
  {
    id: 'tenant-actions',
    term: 'Tenant Actions',
    category: 'ui-fields',
    content: {
      brief: 'Available management actions for this tenant.',
      detailed: 'Tenant management actions:\n- **Edit**: Modify description, policies\n- **Pause**: Temporarily suspend operations\n- **Resume**: Restore paused tenant\n- **Archive**: Decommission (requires confirmation)\n- **Assign Policies**: Link policy packs\n- **Assign Adapters**: Link adapters\n\nRequires tenant:manage permission.',
    },
    relatedTerms: ['tenant-status', 'save-tenant-changes'],
    aliases: ['actions', 'operations'],
  },
  {
    id: 'create-tenant-button',
    term: 'Create Tenant Button',
    category: 'ui-fields',
    content: {
      brief: 'Create a new tenant with isolated resources and policies. Requires tenant:manage permission.',
      detailed: 'Tenant creation:\n- Opens create tenant modal\n- Prompts for: name, description, isolation level, UID/GID\n- Validates uniqueness and permissions\n- Initializes tenant registry and namespace',
    },
    relatedTerms: ['create-tenant-action', 'tenant-name-field'],
    aliases: ['new tenant', 'add tenant'],
  },
  {
    id: 'create-tenant-action',
    term: 'Create Tenant Action',
    category: 'ui-fields',
    content: {
      brief: 'Finalize tenant creation with the specified configuration.',
      detailed: 'Tenant creation action:\n- Validates all required fields\n- Creates: namespace, registry, audit log\n- Assigns default policies\n- Redirects to tenant details page',
    },
    relatedTerms: ['create-tenant-button', 'tenant-name'],
    aliases: ['create', 'submit'],
  },
  {
    id: 'save-tenant-changes',
    term: 'Save Tenant Changes',
    category: 'ui-fields',
    content: {
      brief: 'Save modifications to tenant configuration.',
      detailed: 'Save changes:\n- Updates: description, policies, labels\n- Cannot change: name, UID/GID, isolation level\n- Requires tenant:manage permission\n- Creates audit log entry',
    },
    relatedTerms: ['tenant-actions'],
    aliases: ['save', 'update'],
  },
  {
    id: 'archive-tenant-action',
    term: 'Archive Tenant Action',
    category: 'ui-fields',
    content: {
      brief: 'Archive this tenant. Resources will be suspended but can be restored by an administrator.',
      detailed: 'Archive tenant:\n- Sets status to Archived\n- Unloads all adapters\n- Blocks new inference requests\n- Retains data for compliance period\n- Requires confirmation + tenant:manage permission',
    },
    relatedTerms: ['tenant-status', 'tenant-actions'],
    aliases: ['archive', 'decommission'],
  },
  {
    id: 'assign-policies-action',
    term: 'Assign Policies Action',
    category: 'ui-fields',
    content: {
      brief: 'Assign selected policy packs to this tenant for governance enforcement.',
      detailed: 'Policy assignment:\n- Multi-select policy packs\n- Validates compatibility\n- Applies policies immediately\n- Requires policy:apply permission',
    },
    relatedTerms: ['tenant-policies', 'policy-name'],
    aliases: ['assign policies', 'apply policies'],
  },
  {
    id: 'assign-adapters-action',
    term: 'Assign Adapters Action',
    category: 'ui-fields',
    content: {
      brief: 'Assign selected LoRA adapters to this tenant for inference.',
      detailed: 'Adapter assignment:\n- Multi-select adapters from registry\n- Validates tenant compatibility\n- Links adapters to tenant namespace\n- Requires adapter:register permission',
    },
    relatedTerms: ['tenant-adapters', 'adapter-name'],
    aliases: ['assign adapters', 'link adapters'],
  },
  {
    id: 'import-tenants',
    term: 'Import Tenants',
    category: 'ui-fields',
    content: {
      brief: 'Import tenant configurations from JSON or CSV file.',
      detailed: 'Tenant import:\n- Supports: JSON, CSV formats\n- Validates schema before import\n- Bulk create or update tenants\n- Requires tenant:manage permission',
    },
    relatedTerms: ['export-tenants', 'create-tenant-button'],
    aliases: ['import', 'bulk create'],
  },
  {
    id: 'export-tenants',
    term: 'Export Tenants',
    category: 'ui-fields',
    content: {
      brief: 'Export tenant data to JSON or CSV format for backup or migration.',
      detailed: 'Tenant export:\n- Formats: JSON (full metadata), CSV (tabular)\n- Includes: configuration, policies, adapters\n- Respects RBAC (only accessible tenants)\n- Used for: backup, migration, reporting',
    },
    relatedTerms: ['import-tenants', 'export-usage-csv'],
    aliases: ['export', 'download'],
  },
  {
    id: 'export-usage-csv',
    term: 'Export Usage CSV',
    category: 'ui-fields',
    content: {
      brief: 'Download tenant usage metrics as a CSV file.',
      detailed: 'Usage export:\n- CSV format with columns: tenant, inference_count, tokens, cost\n- Date range selection\n- Used for billing and usage analysis\n- Requires tenant:view permission',
    },
    relatedTerms: ['export-tenants', 'tenant-name'],
    aliases: ['usage export', 'billing export'],
  },

  // ===== Promotion and Golden Run Fields =====
  {
    id: 'promotion-cpid',
    term: 'Promotion CPID',
    category: 'ui-fields',
    content: {
      brief: 'Control Plane ID: unique identifier for the promotion candidate. Enter the CPID of the adapter or bundle to promote.',
      detailed: 'Promotion CPID:\n- Identifies adapter/bundle for promotion\n- Links to: training job, golden runs, tests\n- Used for end-to-end promotion tracking\n- Must pass all gates before promotion',
    },
    relatedTerms: ['cpid', 'promotion-gates'],
    aliases: ['promotion id'],
  },
  {
    id: 'promotion-gates',
    term: 'Promotion Gates',
    category: 'ui-fields',
    content: {
      brief: 'Promotion gates are automated checks that must pass before promotion: policy compliance, test coverage, performance benchmarks, and security scans.',
      detailed: 'Promotion gates:\n- **Policy Compliance**: All policies pass validation\n- **Test Coverage**: ≥95% golden run match\n- **Performance**: Latency within SLA\n- **Security**: No CVEs, dependency scan clean\n\nAll gates must pass (AND logic) for promotion.',
    },
    relatedTerms: ['promotion-execute', 'golden-run'],
    aliases: ['gates', 'promotion checks'],
  },
  {
    id: 'promotion-dry-run',
    term: 'Promotion Dry Run',
    category: 'ui-fields',
    content: {
      brief: 'Preview the promotion without making changes. Simulates the entire promotion workflow and reports what would happen.',
      detailed: 'Dry run mode:\n- Simulates full promotion workflow\n- Validates all gates without execution\n- Reports: pass/fail status, warnings, blockers\n- No state changes (safe to run)',
    },
    relatedTerms: ['promotion-execute', 'promotion-gates'],
    aliases: ['dry run', 'simulation'],
  },
  {
    id: 'promotion-history',
    term: 'Promotion History',
    category: 'ui-fields',
    content: {
      brief: 'Chronological record of all promotions and rollbacks. Includes CPID, operator, timestamp, and outcome status.',
      detailed: 'Promotion audit trail:\n- Immutable log of promotions/rollbacks\n- Includes: CPID, operator, timestamp, gates status\n- Used for: compliance, debugging, trend analysis\n- Retained per retention policy',
    },
    relatedTerms: ['promotion-cpid', 'promotion-rollback'],
    aliases: ['history', 'audit trail'],
  },
  {
    id: 'promotion-execute',
    term: 'Promotion Execute',
    category: 'ui-fields',
    content: {
      brief: 'Execute the promotion to move the adapter to a higher tier or environment. Requires all gates to pass.',
      detailed: 'Execute promotion:\n- Validates all gates (blocking)\n- Moves adapter to target tier/environment\n- Updates registry and metadata\n- Creates audit log entry\n- Requires promotion:execute permission',
    },
    relatedTerms: ['promotion-gates', 'promotion-dry-run'],
    aliases: ['execute', 'promote'],
  },
  {
    id: 'promotion-rollback',
    term: 'Promotion Rollback',
    category: 'ui-fields',
    content: {
      brief: 'Revert to the previous promotion state. Use when a promoted adapter causes issues in the target environment.',
      detailed: 'Rollback promotion:\n- Restores previous adapter version\n- Reverts tier/environment change\n- Creates rollback audit entry\n- Requires promotion:execute permission\n- Cannot rollback >1 level (sequential only)',
    },
    relatedTerms: ['promotion-execute', 'promotion-history'],
    aliases: ['rollback', 'revert'],
  },
  {
    id: 'golden-run',
    term: 'Golden Run',
    category: 'ui-fields',
    content: {
      brief: 'A golden run is a reference baseline capturing model outputs under controlled conditions. Used to verify determinism and detect regressions.',
      detailed: 'Golden run:\n- Reference baseline for deterministic validation\n- Captures: inputs, outputs, latency, loss\n- Used for: regression testing, promotion gates\n- Created with fixed seed for reproducibility',
    },
    relatedTerms: ['golden-baseline', 'testing-epsilon'],
    aliases: ['baseline', 'reference run'],
  },
  {
    id: 'golden-baseline',
    term: 'Golden Baseline',
    category: 'ui-fields',
    content: {
      brief: 'The reference golden run to compare against. Select a stable baseline that represents expected behavior.',
      detailed: 'Baseline selection:\n- Choose stable, validated golden run\n- Typically: initial production deployment\n- Used as comparison target\n- Should represent "known good" behavior',
    },
    relatedTerms: ['golden-run', 'golden-comparison'],
    aliases: ['baseline', 'reference'],
  },
  {
    id: 'golden-comparison',
    term: 'Golden Comparison',
    category: 'ui-fields',
    content: {
      brief: 'Side-by-side comparison of two golden runs showing metric differences, epsilon divergence, and output variations.',
      detailed: 'Golden run comparison:\n- Diff view: baseline vs. candidate\n- Metrics: epsilon divergence, latency delta, output diff\n- Visual: side-by-side outputs with highlights\n- Pass/fail: based on epsilon threshold',
    },
    relatedTerms: ['golden-baseline', 'testing-epsilon'],
    aliases: ['comparison', 'diff'],
  },
  {
    id: 'golden-create',
    term: 'Golden Create',
    category: 'ui-fields',
    content: {
      brief: 'Create a new golden baseline from the current model state. Captures outputs for all test inputs.',
      detailed: 'Create golden run:\n- Runs test suite with fixed seed\n- Captures: all outputs, metrics, metadata\n- Stores as new baseline candidate\n- Requires validation before use as baseline',
    },
    relatedTerms: ['golden-run', 'golden-baseline'],
    aliases: ['create baseline', 'new golden run'],
  },

  // ===== Testing Fields =====
  {
    id: 'testing-epsilon',
    term: 'Testing Epsilon',
    category: 'ui-fields',
    content: {
      brief: 'Maximum allowed numerical difference between outputs. Smaller values (1e-8) require stricter determinism, larger values (1e-4) allow more variance.',
      detailed: 'Epsilon threshold:\n- Maximum allowed floating-point difference\n- **1e-8**: Strict determinism (bit-exact)\n- **1e-6**: Standard (production default)\n- **1e-4**: Relaxed (acceptable variance)\n\nUsed for golden run validation and regression testing.',
    },
    relatedTerms: ['golden-comparison', 'testing-pass-rate'],
    aliases: ['epsilon', 'tolerance'],
  },
  {
    id: 'testing-pass-rate',
    term: 'Testing Pass Rate',
    category: 'ui-fields',
    content: {
      brief: 'Percentage of test cases that must pass for overall success. 100% for critical systems, 95%+ for production.',
      detailed: 'Pass rate threshold:\n- Minimum percentage of tests that must pass\n- **100%**: Critical systems (safety, security)\n- **95-99%**: Production systems\n- **90-95%**: Development/staging\n\nUsed for promotion gate decisions.',
    },
    relatedTerms: ['testing-epsilon', 'promotion-gates'],
    aliases: ['pass rate', 'success threshold'],
  },
  {
    id: 'testing-config',
    term: 'Testing Config',
    category: 'ui-fields',
    content: {
      brief: 'Configure test parameters including epsilon threshold, pass rate, and baseline selection before running validation.',
      detailed: 'Test configuration:\n- Set: epsilon, pass rate, baseline\n- Select: test suite, inputs\n- Configure: parallelism, timeout\n- Save as named config for reuse',
    },
    relatedTerms: ['testing-epsilon', 'testing-pass-rate'],
    aliases: ['test config', 'configuration'],
  },
  {
    id: 'testing-run',
    term: 'Testing Run',
    category: 'ui-fields',
    content: {
      brief: 'Execute validation tests comparing adapter outputs against golden baselines. Results determine promotion eligibility.',
      detailed: 'Run tests:\n- Execute test suite against baseline\n- Generate: pass/fail, epsilon deltas, diffs\n- Used for: promotion gates, CI/CD\n- Results stored for audit',
    },
    relatedTerms: ['testing-config', 'golden-comparison'],
    aliases: ['run tests', 'execute'],
  },

  // ===== Base Model Fields =====
  {
    id: 'base-model-name',
    term: 'Base Model Name',
    category: 'ui-fields',
    content: {
      brief: 'The name and identifier of the currently loaded base model used for inference.',
      detailed: 'Base model identifier:\n- Examples: qwen2.5-7b, llama-3.1-8b\n- Includes: model family, size, variant\n- Used for adapter compatibility checks',
    },
    relatedTerms: ['inference-model', 'base-model-status'],
    aliases: ['model name', 'base model'],
  },
  {
    id: 'base-model-status',
    term: 'Base Model Status',
    category: 'ui-fields',
    content: {
      brief: 'Current state of the base model: loaded (ready for inference), loading, unloading, unloaded, or error.',
      detailed: 'Base model state:\n- **Loaded**: Ready for inference\n- **Loading**: Initializing (may take 30s-2min)\n- **Unloading**: Freeing memory\n- **Unloaded**: Not in memory\n- **Error**: Load failed (check logs)\n\nOnly Loaded models can process inference.',
    },
    relatedTerms: ['base-model-name', 'base-model-memory'],
    aliases: ['model status', 'state'],
  },
  {
    id: 'base-model-memory',
    term: 'Base Model Memory',
    category: 'ui-fields',
    content: {
      brief: 'Memory consumption of the base model in GPU VRAM. Larger models require more memory.',
      detailed: 'Base model memory usage:\n- 7B models: ~14-28 GB (FP16/FP32)\n- 8B models: ~16-32 GB\n- 13B models: ~26-52 GB\n\nQuantization reduces memory (INT8: 50%, INT4: 75%).',
    },
    relatedTerms: ['base-model-status', 'adapter-memory'],
    aliases: ['memory usage', 'vram'],
  },

  // ===== Single File Adapter Trainer Fields =====
  {
    id: 'trainer-file-upload',
    term: 'Trainer File Upload',
    category: 'ui-fields',
    content: {
      brief: 'Upload a training file (.txt, .json, .py, .js, .ts, .md). The file content will be used to create training examples for your adapter.',
      detailed: 'Training file upload:\n- Supported: .txt, .json, .py, .js, .ts, .md, .rs\n- Max size: 10 MB\n- Processed into prompt/completion pairs\n- Validated before training',
    },
    relatedTerms: ['training-dataset', 'trainer-adapter-name'],
    aliases: ['file upload', 'upload training file'],
  },
  {
    id: 'trainer-adapter-name',
    term: 'Trainer Adapter Name',
    category: 'ui-fields',
    content: {
      brief: 'Unique name for your trained adapter. Follows semantic naming: tenant/domain/purpose/revision format.',
      detailed: 'Adapter name:\n- Format: {tenant}/{domain}/{purpose}/{revision}\n- Example: default/coding/python-helpers/r001\n- Must be unique in registry\n- Used for identification and routing',
    },
    relatedTerms: ['adapter-name', 'trainer-file-upload'],
    aliases: ['adapter name'],
  },
  {
    id: 'trainer-rank',
    term: 'Trainer Rank',
    category: 'ui-fields',
    content: {
      brief: 'LoRA rank controls adapter capacity. Lower (4-8) = faster training, less memory. Higher (16-64) = more capacity, slower training.',
      detailed: 'LoRA rank selection:\n- **4-8**: Fast, small files (<50MB), simple patterns\n- **12-16**: Balanced, medium files (50-150MB), general purpose\n- **32-64**: High capacity, large files (150-300MB), complex domains\n\nHigher rank = more parameters = longer training time.',
    },
    relatedTerms: ['adapter-rank', 'trainer-alpha'],
    aliases: ['rank'],
  },
  {
    id: 'trainer-alpha',
    term: 'Trainer Alpha',
    category: 'ui-fields',
    content: {
      brief: 'Scaling factor for LoRA weights. Typically set to 2x rank value. Higher alpha = stronger adaptation.',
      detailed: 'LoRA alpha scaling:\n- Typical: alpha = 2 × rank\n- Examples: rank=8 → alpha=16, rank=16 → alpha=32\n- Higher alpha = stronger weight updates\n- Too high = overfitting risk',
    },
    relatedTerms: ['trainer-rank', 'trainer-learning-rate'],
    aliases: ['alpha', 'scaling factor'],
  },
  {
    id: 'trainer-learning-rate',
    term: 'Trainer Learning Rate',
    category: 'ui-fields',
    content: {
      brief: 'Step size for optimization. Smaller (0.0001) = stable but slow. Larger (0.001) = faster but may overshoot.',
      detailed: 'Learning rate:\n- Typical range: 1e-5 to 1e-3\n- Default: 3e-4 (good starting point)\n- Too high: unstable, divergence\n- Too low: slow convergence, underfitting\n- May use warmup + cosine decay',
    },
    relatedTerms: ['training-learning-rate', 'trainer-epochs'],
    aliases: ['learning rate', 'lr'],
  },
  {
    id: 'trainer-epochs',
    term: 'Trainer Epochs',
    category: 'ui-fields',
    content: {
      brief: 'Number of complete passes through training data. More epochs = better fit but risk of overfitting.',
      detailed: 'Training epochs:\n- Typical: 3-10 epochs\n- Small datasets: 5-10 epochs\n- Large datasets: 1-3 epochs\n- Monitor loss: stop if plateaus or diverges\n- Overfitting: use validation set',
    },
    relatedTerms: ['training-progress', 'trainer-batch-size'],
    aliases: ['epochs'],
  },
  {
    id: 'trainer-batch-size',
    term: 'Trainer Batch Size',
    category: 'ui-fields',
    content: {
      brief: 'Samples processed together. Larger = faster, more memory. Smaller = less memory, more gradient noise.',
      detailed: 'Batch size:\n- Typical: 4-32 samples\n- GPU: 16-32 (faster)\n- CPU: 1-8 (memory constrained)\n- Larger batch = smoother gradients, more memory\n- Smaller batch = noisier gradients, less memory',
    },
    relatedTerms: ['training-tokens-per-sec', 'trainer-epochs'],
    aliases: ['batch size'],
  },

  // ===== Management Panel Fields =====
  {
    id: 'management-services',
    term: 'Management Services',
    category: 'ui-fields',
    content: {
      brief: 'Service management: monitor and control core services, monitoring tools, and background processes.',
      detailed: 'Services panel:\n- View: service status, uptime, health\n- Actions: start, stop, restart services\n- Includes: API server, workers, lifecycle engine\n- Requires operator or admin role',
    },
    relatedTerms: ['monitoring-overview', 'management-workers'],
    aliases: ['services'],
  },
  {
    id: 'management-resources',
    term: 'Management Resources',
    category: 'ui-fields',
    content: {
      brief: 'Resource overview: view tenants, adapters, models, and policies with quick navigation links.',
      detailed: 'Resources panel:\n- Quick stats: tenant count, adapter count, policy count\n- Navigation: jump to detail pages\n- Summary: resource utilization\n- Used for: overview, quick access',
    },
    relatedTerms: ['tenant-name', 'adapter-name'],
    aliases: ['resources'],
  },
  {
    id: 'management-workers',
    term: 'Management Workers',
    category: 'ui-fields',
    content: {
      brief: 'Quick actions: common operations for ML pipelines, operations, monitoring, and compliance.',
      detailed: 'Workers panel:\n- Quick actions: train adapter, run inference, view logs\n- Worker status: active jobs, queue depth\n- Actions: pause/resume workers\n- Requires operator role',
    },
    relatedTerms: ['management-services', 'node-status'],
    aliases: ['workers', 'quick actions'],
  },

  // ===== Monitoring Page Fields =====
  {
    id: 'monitoring-overview',
    term: 'Monitoring Overview',
    category: 'ui-fields',
    content: {
      brief: 'System health overview: real-time status of services, nodes, and key performance indicators.',
      detailed: 'Overview panel:\n- Services: status, uptime\n- Nodes: health, resource usage\n- KPIs: latency, throughput, errors\n- Real-time updates (10s refresh)',
    },
    relatedTerms: ['monitoring-resources', 'monitoring-alerts'],
    aliases: ['overview', 'dashboard'],
  },
  {
    id: 'monitoring-resources',
    term: 'Monitoring Resources',
    category: 'ui-fields',
    content: {
      brief: 'Resource utilization: CPU, memory, disk, and GPU usage across compute nodes.',
      detailed: 'Resource monitoring:\n- CPU/Memory/Disk/GPU charts\n- Per-node breakdown\n- Alerts on thresholds\n- Historical trends (last 24h)',
    },
    relatedTerms: ['cpu-usage', 'memory-usage', 'monitoring-overview'],
    aliases: ['resources', 'utilization'],
  },
  {
    id: 'monitoring-alerts',
    term: 'Monitoring Alerts',
    category: 'ui-fields',
    content: {
      brief: 'Active alerts: critical and warning alerts requiring attention, with acknowledgment workflow.',
      detailed: 'Alerts panel:\n- Active alerts: critical, warning\n- Acknowledged: dismissed but tracked\n- Actions: acknowledge, resolve, snooze\n- Notification: email, webhook',
    },
    relatedTerms: ['monitoring-overview'],
    aliases: ['alerts', 'notifications'],
  },
  {
    id: 'monitoring-metrics',
    term: 'Monitoring Metrics',
    category: 'ui-fields',
    content: {
      brief: 'Real-time metrics: live performance charts, throughput, latency, and system telemetry.',
      detailed: 'Metrics panel:\n- Real-time charts: latency, throughput, errors\n- Customizable: time range, metric selection\n- Export: CSV, JSON\n- Used for: debugging, performance analysis',
    },
    relatedTerms: ['latency-p95', 'tokens-per-second'],
    aliases: ['metrics', 'performance'],
  },

  // ===== Telemetry Fields =====
  {
    id: 'telemetry-event',
    term: 'Telemetry Event',
    category: 'ui-fields',
    content: {
      brief: 'Unique identifier for this telemetry bundle. Bundles group related events for efficient storage and transmission.',
      detailed: 'Telemetry bundle:\n- Groups 100-1000 events\n- Unique bundle ID (UUID)\n- Signed with Ed25519\n- Compressed for storage',
    },
    relatedTerms: ['merkle-root', 'telemetry-timestamp'],
    aliases: ['bundle id', 'event bundle'],
  },
  {
    id: 'telemetry-timestamp',
    term: 'Telemetry Timestamp',
    category: 'ui-fields',
    content: {
      brief: 'When this telemetry bundle was created. Bundles are created periodically or when event thresholds are reached.',
      detailed: 'Bundle timestamp:\n- Creation time (UTC)\n- Triggered by: time (5min) or count (1000 events)\n- Used for chronological ordering\n- Displayed in local timezone',
    },
    relatedTerms: ['telemetry-event', 'audit-timestamp'],
    aliases: ['timestamp', 'created at'],
  },
  {
    id: 'telemetry-type',
    term: 'Telemetry Type',
    category: 'ui-fields',
    content: {
      brief: 'Number of telemetry events contained in this bundle. Events include inference requests, policy enforcement, and system metrics.',
      detailed: 'Event count:\n- Typical: 100-1000 events per bundle\n- Event types: inference, training, policy, system\n- Higher count = more efficient storage\n- Click to view event details',
    },
    relatedTerms: ['telemetry-event'],
    aliases: ['event count', 'bundle size'],
  },
  {
    id: 'telemetry-export',
    term: 'Telemetry Export',
    category: 'ui-fields',
    content: {
      brief: 'Export telemetry bundles for offline analysis or archival. Requires audit:view permission. Available in JSON and CSV formats.',
      detailed: 'Export telemetry:\n- Formats: JSON (full), CSV (tabular)\n- Includes: all events, metadata, signatures\n- Date range selection\n- Requires audit:view permission',
    },
    relatedTerms: ['telemetry-event', 'audit-export'],
    aliases: ['export', 'download'],
  },
  {
    id: 'telemetry-filters',
    term: 'Telemetry Filters',
    category: 'ui-fields',
    content: {
      brief: 'Filter telemetry bundles by search terms, CPID, date range, event count, or file size to find specific events.',
      detailed: 'Telemetry filters:\n- Search: bundle ID, CPID, event type\n- Date range: start/end timestamps\n- Event count: min/max events per bundle\n- File size: filter large bundles\n- Combined with AND logic',
    },
    relatedTerms: ['telemetry-event', 'audit-search'],
    aliases: ['filters', 'search'],
  },

  // ===== Replay Panel Fields =====
  {
    id: 'replay-session',
    term: 'Replay Session',
    category: 'ui-fields',
    content: {
      brief: 'Replay session containing a snapshot of execution state at a specific point in time for deterministic replay and verification.',
      detailed: 'Replay session:\n- Snapshot: inputs, model state, config\n- Used for: determinism verification, debugging\n- Includes: manifest, policy, kernel hashes\n- Enables exact reproduction of execution',
    },
    relatedTerms: ['replay-manifest-hash', 'replay-verification'],
    aliases: ['session', 'replay'],
  },
  {
    id: 'replay-manifest-hash',
    term: 'Replay Manifest Hash',
    category: 'ui-fields',
    content: {
      brief: 'BLAKE3 hash of the manifest file that defines the execution context, including model configuration and adapter stack.',
      detailed: 'Manifest hash:\n- BLAKE3 hash of execution manifest\n- Includes: model config, adapter stack, params\n- Used for: integrity verification, matching\n- Mismatch = configuration drift',
    },
    relatedTerms: ['replay-session', 'schema-hash'],
    aliases: ['manifest hash'],
  },
  {
    id: 'replay-policy-hash',
    term: 'Replay Policy Hash',
    category: 'ui-fields',
    content: {
      brief: 'BLAKE3 hash of the policy pack applied during execution. Used to verify policy integrity during replay.',
      detailed: 'Policy hash:\n- BLAKE3 hash of policy pack\n- Ensures: same policies applied during replay\n- Mismatch = policy drift detected\n- Part of determinism verification chain',
    },
    relatedTerms: ['policy-schema-hash', 'replay-session'],
    aliases: ['policy hash'],
  },
  {
    id: 'replay-kernel-hash',
    term: 'Replay Kernel Hash',
    category: 'ui-fields',
    content: {
      brief: 'BLAKE3 hash of the Metal/CoreML kernel used for computation. Ensures deterministic execution across replays.',
      detailed: 'Kernel hash:\n- BLAKE3 hash of compute kernel (Metal/CoreML)\n- Ensures: same kernel version used\n- Mismatch = kernel drift, non-determinism risk\n- Critical for bit-exact reproducibility',
    },
    relatedTerms: ['replay-session', 'replay-verification'],
    aliases: ['kernel hash'],
  },
  {
    id: 'replay-verification',
    term: 'Replay Verification',
    category: 'ui-fields',
    content: {
      brief: 'Cryptographic verification of the replay session. Validates signature chain, hash integrity, and checks for execution divergences.',
      detailed: 'Replay verification:\n- Validates: signatures, hashes, divergences\n- Checks: manifest, policy, kernel integrity\n- Reports: pass/fail, epsilon deltas\n- Used for: compliance, debugging',
    },
    relatedTerms: ['replay-session', 'replay-divergence'],
    aliases: ['verification', 'validation'],
  },
  {
    id: 'replay-divergence',
    term: 'Replay Divergence',
    category: 'ui-fields',
    content: {
      brief: 'Points where replay execution differs from the original. Indicates non-determinism or configuration mismatch.',
      detailed: 'Divergence points:\n- Locations where outputs differ\n- Includes: epsilon delta, timestamp, context\n- Causes: config drift, non-deterministic ops, bugs\n- Investigate: logs, manifests, policies',
    },
    relatedTerms: ['replay-verification', 'testing-epsilon'],
    aliases: ['divergence', 'differences'],
  },

  // ===== Routing Inspector Fields =====
  {
    id: 'routing-k-value',
    term: 'Routing K Value',
    category: 'ui-fields',
    content: {
      brief: 'Number of adapters selected by K-sparse routing. Higher K increases expressiveness but adds compute overhead.',
      detailed: 'K-sparse parameter:\n- K = number of adapters activated per request\n- Typical: K=1-4\n- Higher K = more expressive, slower\n- Lower K = faster, less flexible\n- Budget: 8% routing overhead',
    },
    relatedTerms: ['routing-entropy', 'routing-overhead'],
    aliases: ['k value', 'k-sparse'],
  },
  {
    id: 'routing-entropy',
    term: 'Routing Entropy',
    category: 'ui-fields',
    content: {
      brief: 'Shannon entropy of gate distribution. Higher entropy indicates more uniform adapter selection. Low entropy may indicate collapsed routing.',
      detailed: 'Gate entropy:\n- Shannon entropy of routing probabilities\n- High (>2.0): uniform, diverse routing\n- Low (<0.5): collapsed, few adapters used\n- Used to detect: routing collapse, bias',
    },
    relatedTerms: ['routing-k-value', 'adapter-activation'],
    aliases: ['entropy'],
  },
  {
    id: 'routing-overhead',
    term: 'Routing Overhead',
    category: 'ui-fields',
    content: {
      brief: 'Routing overhead as percentage of inference time. Budget limit is 8%. Values above indicate performance issues.',
      detailed: 'Overhead metric:\n- Percentage: (routing_time / total_time) × 100\n- Target: <8%\n- High overhead: optimize router, reduce K\n- Includes: gate computation, adapter loading',
    },
    relatedTerms: ['routing-latency', 'latency-p95'],
    aliases: ['overhead'],
  },
  {
    id: 'routing-latency',
    term: 'Routing Latency',
    category: 'ui-fields',
    content: {
      brief: 'Router decision latency in microseconds. Lower values indicate faster adapter selection.',
      detailed: 'Router latency:\n- Time to compute gate probabilities + select adapters\n- Target: <1ms (1000μs)\n- Typical: 100-500μs\n- High latency: simplify gates, reduce adapters',
    },
    relatedTerms: ['routing-overhead', 'latency-p95'],
    aliases: ['latency', 'decision time'],
  },
];
