/**
 * Zod validation schemas for Inference API types.
 * These schemas validate runtime data from the API and allow unknown fields.
 */

import { z } from 'zod';

// ============================================================================
// Enums and Constants
// ============================================================================

export const BackendNameSchema = z.enum(['mlx', 'metal', 'coreml', 'cpu']);
export const CoreMLModeSchema = z.enum(['auto', 'cpu_only', 'cpu_and_gpu', 'cpu_and_ne', 'all']);
export const FusionIntervalSchema = z.enum(['never', 'always', 'auto']);
export const StopReasonCodeSchema = z.enum([
  'end_of_sequence',
  'max_tokens',
  'stop_sequence',
  'safety',
  'error',
  'user_abort',
  'budget',
  'repetition',
]);

// ============================================================================
// Inference Request
// ============================================================================

export const StopPolicySpecSchema = z.object({
  max_tokens: z.number().optional(),
  stop_sequences: z.array(z.string()).optional(),
}).passthrough();

export type StopPolicySpec = z.infer<typeof StopPolicySpecSchema>;

export const InferRequestSchema = z.object({
  // Core required field
  prompt: z.string(),

  // Model and backend selection
  model: z.string().optional(),
  backend: BackendNameSchema.optional(),

  // Routing configuration
  routing_determinism_mode: z.string().optional(),
  adapter_stack: z.array(z.string()).optional(),
  stack_id: z.string().optional(),
  adapters: z.array(z.string()).optional(),
  effective_adapter_ids: z.array(z.string()).optional(),
  domain: z.string().optional(),

  // Generation parameters
  max_tokens: z.number().optional(),
  temperature: z.number().optional(),
  top_p: z.number().optional(),
  top_k: z.number().optional(),
  seed: z.number().optional(),
  stream: z.boolean().optional(),
  stop_policy: StopPolicySpecSchema.optional(),

  // CoreML and fusion
  coreml_mode: CoreMLModeSchema.optional(),
  fusion_interval: FusionIntervalSchema.optional(),

  // Evidence and RAG
  require_evidence: z.boolean().optional(),
  rag_enabled: z.boolean().optional(),
  collection_id: z.string().optional(),

  // Session and tenant
  session_id: z.string().optional(),
  tenant_id: z.string().optional(),
}).passthrough();

export type InferRequest = z.infer<typeof InferRequestSchema>;

// ============================================================================
// Run Receipt
// ============================================================================

export const RunReceiptSchema = z.object({
  // Core receipt fields
  trace_id: z.string(),
  run_head_hash: z.string(),
  output_digest: z.string(),
  receipt_digest: z.string(),
  signature: z.string().optional(),
  attestation: z.string().optional(),

  // Token accounting
  logical_prompt_tokens: z.number(),
  prefix_cached_token_count: z.number(),
  billed_input_tokens: z.number(),
  logical_output_tokens: z.number(),
  billed_output_tokens: z.number(),

  // Stop controller fields
  stop_reason_code: StopReasonCodeSchema.optional(),
  stop_reason_token_index: z.number().optional(),
  stop_policy_digest_b3: z.string().optional(),

  // KV quota/residency fields
  tenant_kv_quota_bytes: z.number().optional(),
  tenant_kv_bytes_used: z.number().optional(),
  kv_evictions: z.number().optional(),
  kv_residency_policy_id: z.string().optional(),
  kv_quota_enforced: z.boolean().optional(),

  // Prefix KV cache fields
  prefix_kv_key_b3: z.string().optional(),
  prefix_cache_hit: z.boolean().optional(),
  prefix_kv_bytes: z.number().optional(),

  // Model cache identity
  model_cache_identity_v2_digest_b3: z.string().optional(),
}).passthrough();

export type RunReceipt = z.infer<typeof RunReceiptSchema>;

// ============================================================================
// Citation and Evidence
// ============================================================================

export const CharRangeSchema = z.object({
  start: z.number(),
  end: z.number(),
}).passthrough();

export const BoundingBoxSchema = z.object({
  x: z.number(),
  y: z.number(),
  width: z.number(),
  height: z.number(),
}).passthrough();

export const CitationSchema = z.object({
  adapter_id: z.string(),
  file_path: z.string(),
  chunk_id: z.string(),
  offset_start: z.number(),
  offset_end: z.number(),
  preview: z.string(),
  citation_id: z.string().optional(),
  page_number: z.number().optional(),
  char_range: CharRangeSchema.optional(),
  bbox: BoundingBoxSchema.optional(),
  relevance_score: z.number().optional(),
  rank: z.number().optional(),
}).passthrough();

export type Citation = z.infer<typeof CitationSchema>;

// ============================================================================
// Inference Response
// ============================================================================

export const InferResponseTraceSchema = z.object({
  latency_ms: z.number(),
  steps: z.array(z.object({
    adapter: z.string(),
    latency_ms: z.number(),
    tokens: z.number(),
  }).passthrough()).optional(),
  router_decisions: z.array(z.object({
    adapter: z.string(),
    score: z.number(),
  }).passthrough()).optional(),
  evidence_spans: z.array(z.object({
    text: z.string(),
    relevance: z.number(),
  }).passthrough()).optional(),
}).passthrough();

export const InferResponseSchema = z.object({
  // Core required fields
  schema_version: z.string(),
  id: z.string(),
  text: z.string(),
  tokens_generated: z.number(),
  latency_ms: z.number(),
  adapters_used: z.array(z.string()),

  // Receipt and citations
  run_receipt: RunReceiptSchema.optional(),
  citations: z.array(CitationSchema).optional(),

  // Generation metadata
  finish_reason: z.enum(['stop', 'length', 'error', 'budget', 'repetition']).optional(),
  stop_reason_code: StopReasonCodeSchema.optional(),
  tokens: z.array(z.number()).optional(),
  token_count: z.number().optional(),
  prompt_tokens: z.number().optional(),

  // Backend information
  backend: BackendNameSchema.optional(),
  backend_used: z.union([BackendNameSchema, z.string()]).optional(),
  coreml_compute_preference: z.string().optional(),
  coreml_compute_units: z.string().optional(),
  coreml_gpu_used: z.boolean().nullable().optional(),

  // Fallback tracking
  fallback_backend: z.union([BackendNameSchema, z.string()]).optional(),
  fallback_triggered: z.boolean().optional(),

  // Determinism
  determinism_mode_applied: z.string().optional(),
  replay_guarantee: z.string().nullable().optional(),

  // Pinned adapter fallback
  unavailable_pinned_adapters: z.array(z.string()).optional(),
  pinned_routing_fallback: z.enum(['stack_only', 'partial']).nullable().optional(),

  // Trace information
  trace: InferResponseTraceSchema.optional(),

  // Alternative field names
  model: z.string().optional(),
  response: z.string().optional(),
  error: z.string().optional(),
}).passthrough();

export type InferResponse = z.infer<typeof InferResponseSchema>;

// ============================================================================
// Batch Inference
// ============================================================================

export const BatchInferRequestSchema = z.object({
  prompts: z.array(z.string()).optional(),
  requests: z.array(InferRequestSchema).optional(),
  model: z.string().optional(),
  backend: BackendNameSchema.optional(),
  max_tokens: z.number().optional(),
  temperature: z.number().optional(),
  adapter_stack: z.array(z.string()).optional(),
}).passthrough();

export type BatchInferRequest = z.infer<typeof BatchInferRequestSchema>;

export const BatchInferResponseSchema = z.object({
  schema_version: z.string(),
  results: z.array(InferResponseSchema),
  responses: z.array(InferResponseSchema), // alias for results
  total_tokens: z.number(),
  total_latency_ms: z.number(),
}).passthrough();

export type BatchInferResponse = z.infer<typeof BatchInferResponseSchema>;
