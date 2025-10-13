// Policy Schema Constants
// Defines the structure and validation rules for all 20 policy packs

export interface PolicyFieldDefinition {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'array' | 'object' | 'enum';
  label: string;
  description: string;
  required: boolean;
  default?: any;
  options?: string[];
  min?: number;
  max?: number;
  validation?: (value: any) => string | null;
}

export interface PolicyPackDefinition {
  id: string;
  name: string;
  description: string;
  fields: PolicyFieldDefinition[];
}

export const POLICY_PACKS: PolicyPackDefinition[] = [
  {
    id: 'egress',
    name: 'Egress Ruleset',
    description: 'Zero network enforcement during serving',
    fields: [
      {
        name: 'mode',
        type: 'enum',
        label: 'Mode',
        description: 'Network access mode',
        required: true,
        default: 'deny_all',
        options: ['deny_all', 'allow_list'],
      },
      {
        name: 'serve_requires_pf',
        type: 'boolean',
        label: 'Serve Requires PF',
        description: 'Require packet filter rules to be active',
        required: true,
        default: true,
      },
      {
        name: 'allow_tcp',
        type: 'boolean',
        label: 'Allow TCP',
        description: 'Allow TCP connections',
        required: true,
        default: false,
      },
      {
        name: 'allow_udp',
        type: 'boolean',
        label: 'Allow UDP',
        description: 'Allow UDP connections',
        required: true,
        default: false,
      },
      {
        name: 'uds_paths',
        type: 'array',
        label: 'UDS Paths',
        description: 'Allowed Unix domain socket paths',
        required: true,
        default: ['/var/run/aos/<tenant>/*.sock'],
      },
    ],
  },
  {
    id: 'determinism',
    name: 'Determinism Ruleset',
    description: 'Replay guarantees and reproducibility',
    fields: [
      {
        name: 'require_metallib_embed',
        type: 'boolean',
        label: 'Require Metallib Embed',
        description: 'Require precompiled Metal kernels',
        required: true,
        default: true,
      },
      {
        name: 'require_kernel_hash_match',
        type: 'boolean',
        label: 'Require Kernel Hash Match',
        description: 'Verify kernel hashes match plan',
        required: true,
        default: true,
      },
      {
        name: 'rng',
        type: 'enum',
        label: 'RNG',
        description: 'Random number generation strategy',
        required: true,
        default: 'hkdf_seeded',
        options: ['hkdf_seeded', 'deterministic'],
      },
      {
        name: 'retrieval_tie_break',
        type: 'array',
        label: 'Retrieval Tie Break',
        description: 'Tie-breaking order for retrieval',
        required: true,
        default: ['score_desc', 'doc_id_asc'],
      },
    ],
  },
  {
    id: 'router',
    name: 'Router Ruleset',
    description: 'K-sparse gating configuration',
    fields: [
      {
        name: 'k_sparse',
        type: 'number',
        label: 'K Sparse',
        description: 'Number of adapters to select',
        required: true,
        default: 3,
        min: 1,
        max: 16,
      },
      {
        name: 'gate_quant',
        type: 'enum',
        label: 'Gate Quantization',
        description: 'Gate value quantization',
        required: true,
        default: 'q15',
        options: ['q15', 'q8', 'f16'],
      },
      {
        name: 'entropy_floor',
        type: 'number',
        label: 'Entropy Floor',
        description: 'Minimum entropy to prevent collapse',
        required: true,
        default: 0.02,
        min: 0,
        max: 1,
      },
      {
        name: 'sample_tokens_full',
        type: 'number',
        label: 'Sample Tokens Full',
        description: 'Number of tokens to fully log',
        required: true,
        default: 128,
        min: 0,
      },
    ],
  },
  {
    id: 'evidence',
    name: 'Evidence Ruleset',
    description: 'Open-book grounding requirements',
    fields: [
      {
        name: 'require_open_book',
        type: 'boolean',
        label: 'Require Open Book',
        description: 'Require evidence for regulated domains',
        required: true,
        default: true,
      },
      {
        name: 'min_spans',
        type: 'number',
        label: 'Minimum Spans',
        description: 'Minimum evidence spans required',
        required: true,
        default: 1,
        min: 0,
      },
      {
        name: 'prefer_latest_revision',
        type: 'boolean',
        label: 'Prefer Latest Revision',
        description: 'Prefer latest document revision',
        required: true,
        default: true,
      },
      {
        name: 'warn_on_superseded',
        type: 'boolean',
        label: 'Warn on Superseded',
        description: 'Warn when using superseded documents',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'refusal',
    name: 'Refusal Ruleset',
    description: 'Abstain thresholds and behavior',
    fields: [
      {
        name: 'abstain_threshold',
        type: 'number',
        label: 'Abstain Threshold',
        description: 'Confidence threshold for abstaining',
        required: true,
        default: 0.55,
        min: 0,
        max: 1,
      },
      {
        name: 'missing_fields_templates',
        type: 'object',
        label: 'Missing Fields Templates',
        description: 'Templates for missing field messages',
        required: false,
        default: {},
      },
    ],
  },
  {
    id: 'numeric',
    name: 'Numeric & Units Ruleset',
    description: 'Unit normalization and validation',
    fields: [
      {
        name: 'canonical_units',
        type: 'object',
        label: 'Canonical Units',
        description: 'Canonical unit mappings',
        required: true,
        default: { torque: 'in_lbf', pressure: 'psi' },
      },
      {
        name: 'max_rounding_error',
        type: 'number',
        label: 'Max Rounding Error',
        description: 'Maximum rounding error allowed',
        required: true,
        default: 0.5,
        min: 0,
      },
      {
        name: 'require_units_in_trace',
        type: 'boolean',
        label: 'Require Units in Trace',
        description: 'Include units in trace output',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'rag',
    name: 'RAG Index Ruleset',
    description: 'Tenant isolation and retrieval configuration',
    fields: [
      {
        name: 'index_scope',
        type: 'enum',
        label: 'Index Scope',
        description: 'Index isolation level',
        required: true,
        default: 'per_tenant',
        options: ['per_tenant', 'shared'],
      },
      {
        name: 'doc_tags_required',
        type: 'array',
        label: 'Document Tags Required',
        description: 'Required document metadata tags',
        required: true,
        default: ['doc_id', 'rev', 'effectivity', 'source_type'],
      },
      {
        name: 'embedding_model_hash',
        type: 'string',
        label: 'Embedding Model Hash',
        description: 'BLAKE3 hash of embedding model',
        required: true,
        default: 'b3:...',
      },
      {
        name: 'topk',
        type: 'number',
        label: 'Top K',
        description: 'Number of documents to retrieve',
        required: true,
        default: 5,
        min: 1,
        max: 100,
      },
      {
        name: 'order',
        type: 'array',
        label: 'Order',
        description: 'Result ordering strategy',
        required: true,
        default: ['score_desc', 'doc_id_asc'],
      },
    ],
  },
  {
    id: 'isolation',
    name: 'Isolation Ruleset',
    description: 'Multi-tenant process boundaries',
    fields: [
      {
        name: 'process_model',
        type: 'enum',
        label: 'Process Model',
        description: 'Process isolation strategy',
        required: true,
        default: 'per_tenant',
        options: ['per_tenant', 'shared'],
      },
      {
        name: 'uds_root',
        type: 'string',
        label: 'UDS Root',
        description: 'Unix domain socket root path',
        required: true,
        default: '/var/run/aos/<tenant>',
      },
      {
        name: 'forbid_shm',
        type: 'boolean',
        label: 'Forbid Shared Memory',
        description: 'Forbid shared memory across tenants',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'telemetry',
    name: 'Telemetry Ruleset',
    description: 'Event sampling and logging',
    fields: [
      {
        name: 'schema_hash',
        type: 'string',
        label: 'Schema Hash',
        description: 'Telemetry schema hash',
        required: true,
        default: 'b3:...',
      },
      {
        name: 'router_full_tokens',
        type: 'number',
        label: 'Router Full Tokens',
        description: 'Tokens to log at 100% sampling',
        required: true,
        default: 128,
        min: 0,
      },
    ],
  },
  {
    id: 'retention',
    name: 'Retention Ruleset',
    description: 'Bundle retention policies',
    fields: [
      {
        name: 'keep_bundles_per_cpid',
        type: 'number',
        label: 'Keep Bundles Per CPID',
        description: 'Number of bundles to retain per CPID',
        required: true,
        default: 12,
        min: 1,
      },
      {
        name: 'keep_incident_bundles',
        type: 'boolean',
        label: 'Keep Incident Bundles',
        description: 'Retain bundles referenced by incidents',
        required: true,
        default: true,
      },
      {
        name: 'keep_promotion_bundles',
        type: 'boolean',
        label: 'Keep Promotion Bundles',
        description: 'Retain promotion bundles',
        required: true,
        default: true,
      },
      {
        name: 'evict_strategy',
        type: 'enum',
        label: 'Eviction Strategy',
        description: 'Bundle eviction strategy',
        required: true,
        default: 'oldest_first_safe',
        options: ['oldest_first_safe', 'lru', 'fifo'],
      },
    ],
  },
  {
    id: 'performance',
    name: 'Performance Ruleset',
    description: 'Latency budgets and thresholds',
    fields: [
      {
        name: 'latency_p95_ms',
        type: 'number',
        label: 'Latency P95 (ms)',
        description: 'P95 latency budget',
        required: true,
        default: 24,
        min: 1,
      },
      {
        name: 'router_overhead_pct_max',
        type: 'number',
        label: 'Router Overhead Max (%)',
        description: 'Maximum router overhead percentage',
        required: true,
        default: 8,
        min: 0,
        max: 100,
      },
      {
        name: 'throughput_tokens_per_s_min',
        type: 'number',
        label: 'Throughput Min (tokens/s)',
        description: 'Minimum throughput requirement',
        required: true,
        default: 40,
        min: 1,
      },
    ],
  },
  {
    id: 'memory',
    name: 'Memory Ruleset',
    description: 'Eviction and K-reduction policies',
    fields: [
      {
        name: 'min_headroom_pct',
        type: 'number',
        label: 'Min Headroom (%)',
        description: 'Minimum memory headroom percentage',
        required: true,
        default: 15,
        min: 0,
        max: 100,
      },
      {
        name: 'evict_order',
        type: 'array',
        label: 'Eviction Order',
        description: 'Adapter eviction priority order',
        required: true,
        default: ['ephemeral_ttl', 'cold_lru', 'warm_lru'],
      },
      {
        name: 'k_reduce_before_evict',
        type: 'boolean',
        label: 'K Reduce Before Evict',
        description: 'Reduce K before evicting hot adapters',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'artifacts',
    name: 'Artifacts Ruleset',
    description: 'Signing, SBOM, and CAS requirements',
    fields: [
      {
        name: 'require_signature',
        type: 'boolean',
        label: 'Require Signature',
        description: 'Require Ed25519 signatures',
        required: true,
        default: true,
      },
      {
        name: 'require_sbom',
        type: 'boolean',
        label: 'Require SBOM',
        description: 'Require Software Bill of Materials',
        required: true,
        default: true,
      },
      {
        name: 'cas_only',
        type: 'boolean',
        label: 'CAS Only',
        description: 'Content-addressed storage only',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'secrets',
    name: 'Secrets Ruleset',
    description: 'Key management and rotation',
    fields: [
      {
        name: 'env_allowed',
        type: 'array',
        label: 'Allowed Environment Variables',
        description: 'Allowed environment variable names',
        required: true,
        default: [],
      },
      {
        name: 'keystore',
        type: 'enum',
        label: 'Keystore',
        description: 'Key storage backend',
        required: true,
        default: 'secure_enclave',
        options: ['secure_enclave', 'file'],
      },
      {
        name: 'rotate_on_promotion',
        type: 'boolean',
        label: 'Rotate on Promotion',
        description: 'Rotate keys on CP promotion',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'build_release',
    name: 'Build & Release Ruleset',
    description: 'Promotion gates and requirements',
    fields: [
      {
        name: 'require_replay_zero_diff',
        type: 'boolean',
        label: 'Require Replay Zero Diff',
        description: 'Require determinism verification',
        required: true,
        default: true,
      },
      {
        name: 'require_signed_plan',
        type: 'boolean',
        label: 'Require Signed Plan',
        description: 'Require signed plan manifests',
        required: true,
        default: true,
      },
      {
        name: 'require_rollback_plan',
        type: 'boolean',
        label: 'Require Rollback Plan',
        description: 'Require rollback plan availability',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'compliance',
    name: 'Compliance Ruleset',
    description: 'Control matrix and evidence mapping',
    fields: [
      {
        name: 'control_matrix_hash',
        type: 'string',
        label: 'Control Matrix Hash',
        description: 'BLAKE3 hash of control matrix',
        required: true,
        default: 'b3:...',
      },
      {
        name: 'require_evidence_links',
        type: 'boolean',
        label: 'Require Evidence Links',
        description: 'Require evidence pointers for controls',
        required: true,
        default: true,
      },
      {
        name: 'require_itar_suite_green',
        type: 'boolean',
        label: 'Require ITAR Suite Green',
        description: 'Require ITAR isolation tests to pass',
        required: true,
        default: true,
      },
    ],
  },
  {
    id: 'incident',
    name: 'Incident Ruleset',
    description: 'Runbook procedures for failures',
    fields: [
      {
        name: 'memory',
        type: 'array',
        label: 'Memory Incident Actions',
        description: 'Actions for memory pressure',
        required: true,
        default: ['drop_ephemeral', 'reduce_k', 'evict_cold', 'deny_new_sessions'],
      },
      {
        name: 'router_skew',
        type: 'array',
        label: 'Router Skew Actions',
        description: 'Actions for router skew',
        required: true,
        default: ['entropy_floor_on', 'cap_activation', 'recalibrate', 'rebuild_plan'],
      },
      {
        name: 'determinism',
        type: 'array',
        label: 'Determinism Failure Actions',
        description: 'Actions for determinism failures',
        required: true,
        default: ['freeze_plan', 'export_bundle', 'diff_hashes', 'rollback'],
      },
      {
        name: 'violation',
        type: 'array',
        label: 'Policy Violation Actions',
        description: 'Actions for policy violations',
        required: true,
        default: ['isolate', 'export_bundle', 'rotate_keys', 'open_ticket'],
      },
    ],
  },
  {
    id: 'output',
    name: 'LLM Output Ruleset',
    description: 'Format requirements and safety filters',
    fields: [
      {
        name: 'format',
        type: 'enum',
        label: 'Output Format',
        description: 'Required output format',
        required: true,
        default: 'json',
        options: ['json', 'text'],
      },
      {
        name: 'require_trace',
        type: 'boolean',
        label: 'Require Trace',
        description: 'Include trace information',
        required: true,
        default: true,
      },
      {
        name: 'forbidden_topics',
        type: 'array',
        label: 'Forbidden Topics',
        description: 'Forbidden topic classes',
        required: true,
        default: ['tenant_crossing', 'export_control_bypass'],
      },
    ],
  },
  {
    id: 'adapters',
    name: 'Adapter Lifecycle Ruleset',
    description: 'Activation thresholds and quality requirements',
    fields: [
      {
        name: 'min_activation_pct',
        type: 'number',
        label: 'Min Activation (%)',
        description: 'Minimum activation percentage',
        required: true,
        default: 2.0,
        min: 0,
        max: 100,
      },
      {
        name: 'min_quality_delta',
        type: 'number',
        label: 'Min Quality Delta',
        description: 'Minimum quality improvement',
        required: true,
        default: 0.5,
        min: 0,
      },
      {
        name: 'require_registry_admit',
        type: 'boolean',
        label: 'Require Registry Admission',
        description: 'Require registry admission',
        required: true,
        default: true,
      },
    ],
  },
];

export function getPolicyPack(id: string): PolicyPackDefinition | undefined {
  return POLICY_PACKS.find((pack) => pack.id === id);
}

export function getDefaultPolicyConfig(): Record<string, any> {
  const config: Record<string, any> = {
    schema: 'adapteros.policy.v1',
    packs: {},
  };

  for (const pack of POLICY_PACKS) {
    const packConfig: Record<string, any> = {};
    for (const field of pack.fields) {
      if (field.default !== undefined) {
        packConfig[field.name] = field.default;
      }
    }
    config.packs[pack.id] = packConfig;
  }

  return config;
}
