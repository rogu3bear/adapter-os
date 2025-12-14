/**
 * Zod validation schemas for Adapter API types.
 * These schemas validate runtime data from the API and allow unknown fields.
 */

import { z } from 'zod';

// ============================================================================
// Enums and Constants
// ============================================================================

export const AdapterCategorySchema = z.enum(['code', 'framework', 'codebase', 'ephemeral']);
export const AdapterStateSchema = z.enum(['unloaded', 'loading', 'cold', 'warm', 'hot', 'resident', 'error']);
export const AdapterScopeSchema = z.enum(['global', 'tenant', 'repo', 'commit', 'project']);
export const LifecycleStateSchema = z.enum(['draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed']);
export const EvictionPrioritySchema = z.enum(['never', 'low', 'normal', 'high', 'critical']);
export const LoraTierSchema = z.enum(['micro', 'standard', 'max']);
export const AdapterHealthFlagSchema = z.enum(['healthy', 'degraded', 'unsafe', 'corrupt', 'unknown']);
export const AttachModeSchema = z.enum(['free', 'requires_dataset']);

// ============================================================================
// Core Adapter Schemas
// ============================================================================

/**
 * AdapterSummary: Minimal adapter representation (typically from list endpoints)
 */
export const AdapterSummarySchema = z.object({
  adapter_id: z.string(),
  name: z.string(),
  category: AdapterCategorySchema.optional(),
  current_state: AdapterStateSchema.optional(),
  lifecycle_state: z.string().optional(),
  memory_bytes: z.number().optional(),
  activation_count: z.number().optional(),
  last_activated: z.string().optional(),
  tenant_id: z.string().optional(),
  description: z.string().optional(),
  pinned: z.boolean().optional(),
}).passthrough();

export type AdapterSummary = z.infer<typeof AdapterSummarySchema>;

/**
 * Full Adapter schema with all possible fields
 */
export const AdapterSchema = z.object({
  // Core required fields
  id: z.string(),
  adapter_id: z.string(),
  name: z.string(),
  hash_b3: z.string(),
  rank: z.number(),
  tier: z.string(),
  created_at: z.string(),

  // Optional metadata
  tenant_id: z.string().optional(),
  description: z.string().optional(),
  category: AdapterCategorySchema.optional(),
  scope: AdapterScopeSchema.optional(),

  // Semantic naming
  adapter_name: z.string().optional(),
  tenant_namespace: z.string().optional(),
  domain: z.string().optional(),
  purpose: z.string().optional(),
  revision: z.string().optional(),
  version: z.string().optional(),

  // Lineage
  parent_id: z.string().optional(),
  fork_type: z.enum(['independent', 'extension']).optional(),
  fork_reason: z.string().optional(),

  // Code intelligence
  languages: z.array(z.string()).optional(),
  languages_json: z.string().optional(),
  framework: z.string().optional(),
  framework_id: z.string().optional(),
  framework_version: z.string().optional(),
  repo_id: z.string().optional(),
  commit_sha: z.string().optional(),
  intent: z.string().optional(),
  base_model_id: z.string().optional(),

  // LoRA configuration
  lora_tier: LoraTierSchema.optional(),
  lora_strength: z.number().optional(),
  lora_scope: z.string().optional(),

  // State management
  current_state: AdapterStateSchema.optional(),
  lifecycle_state: z.union([LifecycleStateSchema, AdapterStateSchema, z.string()]).optional(),
  runtime_state: z.string().optional(),
  state: AdapterStateSchema.optional(),
  status: z.enum(['active', 'inactive', 'loading', 'error']).optional(),

  // Trust and security
  adapter_trust_state: z.enum(['allowed', 'warn', 'blocked', 'unknown', 'blocked_regressed']).optional(),
  dataset_version_ids: z.array(z.string()).optional(),
  dataset_version_trust: z.array(z.any()).optional(),

  // Memory and performance
  pinned: z.boolean().optional(),
  memory_bytes: z.number().optional(),
  last_activated: z.string().optional(),
  activation_count: z.number().optional(),
  last_inference: z.string().optional(),
  error_count: z.number().optional(),

  // Storage consistency
  kv_consistent: z.boolean().optional(),
  kv_message: z.string().optional(),

  // Drift/determinism
  drift_reference_backend: z.string().optional(),
  drift_baseline_backend: z.string().optional(),
  drift_test_backend: z.string().optional(),
  drift_tier: z.enum(['low', 'standard', 'high']).optional(),
  drift_metric: z.number().optional(),
  drift_loss_metric: z.number().optional(),
  drift_slice_size: z.number().optional(),
  drift_slice_offset: z.number().optional(),
  assurance_tier: z.enum(['low', 'standard', 'high']).optional(),

  // Manifest and verification
  manifest_schema_version: z.string().optional(),
  content_hash_b3: z.string().optional(),
  signature_valid: z.boolean().optional(),

  // CoreML export
  coreml_export_available: z.boolean().optional(),
  coreml_export_status: z.string().optional(),
  coreml_export_verified: z.boolean().optional(),
  coreml_verification_status: z.string().optional(),
  coreml_export_last_verified_at: z.string().optional(),
  coreml_export_last_exported_at: z.string().optional(),

  // Attach mode (publish)
  attach_mode: AttachModeSchema.optional(),
  required_scope_dataset_version_id: z.string().optional(),
  is_archived: z.boolean().optional(),
  published_at: z.string().optional(),
  short_description: z.string().optional(),

  // Timestamps
  updated_at: z.string().optional(),
  active: z.boolean().optional(),
}).passthrough();

export type Adapter = z.infer<typeof AdapterSchema>;

/**
 * ActiveAdapter: Adapter with gate value (used in stacks)
 */
export const ActiveAdapterSchema = z.object({
  adapter_id: z.string(),
  gate: z.number(),
  priority: EvictionPrioritySchema.optional(),

  // Optional enriched fields
  id: z.string().optional(),
  name: z.string().optional(),
  lifecycle_state: z.string().optional(),
}).passthrough();

export type ActiveAdapter = z.infer<typeof ActiveAdapterSchema>;

// ============================================================================
// Adapter Response Schemas
// ============================================================================

export const AdapterResponseSchema = z.object({
  schema_version: z.string(),
  adapter: AdapterSchema,
}).passthrough();

export type AdapterResponse = z.infer<typeof AdapterResponseSchema>;

export const ListAdaptersResponseSchema = z.object({
  schema_version: z.string(),
  adapters: z.array(AdapterSchema),
  total: z.number(),
  page: z.number(),
  page_size: z.number(),
}).passthrough();

export type ListAdaptersResponse = z.infer<typeof ListAdaptersResponseSchema>;

// ============================================================================
// Adapter Manifest and Metrics
// ============================================================================

export const AdapterManifestSchema = z.object({
  version: z.string(),
  name: z.string(),
  description: z.string().optional(),
  base_model: z.string(),
  rank: z.number(),
  alpha: z.number(),
  target_modules: z.array(z.string()),
  created_at: z.string(),
  hash: z.string(),
  quantization: z.string().optional(),
  dtype: z.string().optional(),
  lora_tier: LoraTierSchema.optional(),
  scope: z.string().optional(),
}).passthrough();

export type AdapterManifest = z.infer<typeof AdapterManifestSchema>;

export const AdapterMetricsSchema = z.object({
  adapter_id: z.string().optional(),
  inference_count: z.number(),
  total_tokens: z.number(),
  avg_latency_ms: z.number(),
  error_count: z.number(),
  last_used: z.string().optional(),
  performance: z.record(z.string(), z.number()).optional(),
}).passthrough();

export type AdapterMetrics = z.infer<typeof AdapterMetricsSchema>;

// ============================================================================
// Adapter Health
// ============================================================================

export const AdapterHealthDomainSchema = z.enum(['drift', 'trust', 'storage', 'other']);

export const AdapterHealthSubcodeSchema = z.object({
  domain: AdapterHealthDomainSchema,
  code: z.string(),
  message: z.string().optional(),
  data: z.record(z.string(), z.unknown()).optional(),
}).passthrough();

export const AdapterDriftSummarySchema = z.object({
  current: z.number(),
  hard_threshold: z.number().optional(),
  tier: z.string().optional(),
}).passthrough();

export const AdapterDatasetHealthSchema = z.object({
  dataset_version_id: z.string(),
  trust_state: z.string(),
  overall_trust_status: z.string().optional(),
}).passthrough();

export const AdapterStorageHealthSchema = z.object({
  reconciler_status: z.string(),
  last_checked_at: z.string().optional(),
  issues: z.array(AdapterHealthSubcodeSchema).optional(),
}).passthrough();

export const AdapterBackendHealthSchema = z.object({
  backend: z.string().optional(),
  coreml_device_type: z.string().optional(),
  coreml_used: z.boolean().optional(),
}).passthrough();

export const AdapterActivationEventSchema = z.object({
  adapter_id: z.string(),
  event_type: z.enum(['activated', 'deactivated', 'promoted', 'demoted']),
  timestamp: z.string(),
  reason: z.string().optional(),
}).passthrough();

export const AdapterHealthResponseSchema = z.object({
  schema_version: z.string(),
  adapter_id: z.string(),
  health: AdapterHealthFlagSchema,
  primary_subcode: AdapterHealthSubcodeSchema.optional(),
  subcodes: z.array(AdapterHealthSubcodeSchema),
  drift_summary: AdapterDriftSummarySchema.optional(),
  datasets: z.array(AdapterDatasetHealthSchema),
  storage: AdapterStorageHealthSchema.optional(),
  backend: AdapterBackendHealthSchema.optional(),
  recent_activations: z.array(AdapterActivationEventSchema),
  total_activations: z.number(),
  selected_count: z.number(),
  avg_gate_value: z.number(),
  memory_usage_mb: z.number(),
  policy_violations: z.array(z.string()),
}).passthrough();

export type AdapterHealthResponse = z.infer<typeof AdapterHealthResponseSchema>;

// ============================================================================
// Adapter State and Lifecycle
// ============================================================================

export const AdapterStateResponseSchema = z.object({
  schema_version: z.string(),
  adapter_id: z.string(),
  current_state: AdapterStateSchema,
  previous_state: AdapterStateSchema.optional(),
  old_state: AdapterStateSchema.optional(),
  new_state: AdapterStateSchema.optional(),
  transition_time: z.string().optional(),
  reason: z.string().optional(),
}).passthrough();

export type AdapterStateResponse = z.infer<typeof AdapterStateResponseSchema>;

// ============================================================================
// Publish/Archive
// ============================================================================

export const PublishAdapterRequestSchema = z.object({
  name: z.string().optional(),
  short_description: z.string().optional(),
  attach_mode: AttachModeSchema,
  required_scope_dataset_version_id: z.string().optional(),
}).passthrough();

export type PublishAdapterRequest = z.infer<typeof PublishAdapterRequestSchema>;

export const PublishAdapterResponseSchema = z.object({
  schema_version: z.string(),
  version_id: z.string(),
  repo_id: z.string(),
  attach_mode: AttachModeSchema,
  required_scope_dataset_version_id: z.string().optional(),
  published_at: z.string(),
  short_description: z.string().optional(),
}).passthrough();

export type PublishAdapterResponse = z.infer<typeof PublishAdapterResponseSchema>;
