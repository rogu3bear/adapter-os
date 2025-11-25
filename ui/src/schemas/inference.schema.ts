/**
 * Inference request validation schemas
 *
 * Maps to backend types:
 * - adapteros-api-types/src/inference.rs::InferRequest
 * - adapteros-api/src/streaming.rs::StreamingInferenceRequest
 */

import { z } from 'zod';

/**
 * Default values from backend
 *
 * From: adapteros-api/src/streaming.rs
 * - default_max_tokens() -> 512
 * - default_temperature() -> 0.7
 * - default_stream() -> true
 */
const DEFAULT_MAX_TOKENS = 512;
const DEFAULT_TEMPERATURE = 0.7;
const DEFAULT_TOP_K = 50;
const DEFAULT_TOP_P = 0.9;

/**
 * Inference request schema
 *
 * Maps to: adapteros-api-types/src/inference.rs::InferRequest
 */
export const InferRequestSchema = z.object({
  // Input prompt
  prompt: z.string()
    .min(1, 'Prompt is required')
    .max(8192, 'Prompt must not exceed 8192 characters')
    .describe('Input prompt or messages'),

 // Maximum tokens to generate
  max_tokens: z.number()
    .int('Max tokens must be an integer')
    .min(1, 'Max tokens must be at least 1')
    .max(4096, 'Max tokens must not exceed 4096')
    .default(DEFAULT_MAX_TOKENS)
    .describe('Maximum tokens to generate'),

  // Temperature for sampling
  temperature: z.number()
    .min(0, 'Temperature must be non-negative')
    .max(2, 'Temperature must not exceed 2.0')
    .default(DEFAULT_TEMPERATURE)
    .describe('Temperature for sampling'),

  // Top-k sampling parameter
  top_k: z.number()
    .int('Top-k must be an integer')
    .min(1, 'Top-k must be at least 1')
    .max(100, 'Top-k must not exceed 100')
    .optional()
    .describe('Top-k sampling parameter'),

  // Top-p sampling parameter
  top_p: z.number()
    .min(0, 'Top-p must be non-negative')
    .max(1, 'Top-p must not exceed 1.0')
    .optional()
    .describe('Top-p (nucleus) sampling parameter'),

  // Random seed (for deterministic generation)
  seed: z.number()
    .int('Seed must be an integer')
    .min(0, 'Seed must be non-negative')
    .optional()
    .describe('Random seed for deterministic generation'),

  // Backend selection
  backend: z.enum(['auto', 'mlx', 'coreml', 'metal'])
    .optional()
    .default('auto')
    .describe('Backend to use for inference (auto|mlx|coreml|metal)'),

  // Require evidence (for audit trail)
  require_evidence: z.boolean()
    .optional()
    .describe('Require evidence for audit trail'),
});

export type InferRequest = z.infer<typeof InferRequestSchema>;

/**
 * Streaming inference request schema
 *
 * Maps to: adapteros-api/src/streaming.rs::StreamingInferenceRequest
 */
export const StreamingInferenceRequestSchema = z.object({
  // Input prompt
  prompt: z.string()
    .min(1, 'Prompt is required')
    .max(8192, 'Prompt must not exceed 8192 characters')
    .describe('Input prompt or messages'),

  // Model identifier
  model: z.string()
    .min(1, 'Model identifier cannot be empty if provided')
    .max(100, 'Model identifier too long')
    .optional()
    .describe('Model identifier'),

  // Maximum tokens to generate
  max_tokens: z.number()
    .int('Max tokens must be an integer')
    .min(1, 'Max tokens must be at least 1')
    .max(4096, 'Max tokens must not exceed 4096')
    .default(DEFAULT_MAX_TOKENS)
    .describe('Maximum tokens to generate'),

  // Temperature for sampling
  temperature: z.number()
    .min(0, 'Temperature must be non-negative')
    .max(2, 'Temperature must not exceed 2.0')
    .default(DEFAULT_TEMPERATURE)
    .describe('Temperature for sampling'),

  // Top-p sampling parameter
  top_p: z.number()
    .min(0, 'Top-p must be non-negative')
    .max(1, 'Top-p must not exceed 1.0')
    .optional()
    .describe('Top-p (nucleus) sampling parameter'),

  // Stop sequences
  stop: z.array(z.string())
    .max(10, 'Too many stop sequences')
    .optional()
    .default([])
    .describe('Stop sequences'),

  // Whether to stream the response
  stream: z.boolean()
    .default(true)
    .describe('Enable streaming response'),

  // Backend selection
  backend: z.enum(['auto', 'mlx', 'coreml', 'metal'])
    .optional()
    .default('auto')
    .describe('Backend to use for inference (auto|mlx|coreml|metal)'),

  // Active adapter stack name
  adapter_stack: z.string()
    .min(1, 'Adapter stack name cannot be empty if provided')
    .max(100, 'Adapter stack name too long')
    .optional()
    .describe('Active adapter stack'),

  // Stack ID for telemetry correlation
  stack_id: z.string()
    .min(1, 'Stack ID cannot be empty if provided')
    .optional()
    .describe('Stack ID for telemetry'),

  // Stack version for telemetry correlation
  stack_version: z.number()
    .int('Stack version must be an integer')
    .optional()
    .describe('Stack version for telemetry'),
});

export type StreamingInferenceRequest = z.infer<typeof StreamingInferenceRequestSchema>;

/**
 * Finish reason enum
 */
export const FinishReasonSchema = z.enum([
  'stop',        // Natural completion
  'length',      // Max tokens reached
  'content_filter', // Content policy violation
  'cancelled',   // User cancelled
]);

export type FinishReason = z.infer<typeof FinishReasonSchema>;

/**
 * Router candidate schema (for observability)
 *
 * Maps to: adapteros-api-types/src/inference.rs::RouterCandidate
 */
export const RouterCandidateSchema = z.object({
  adapter_idx: z.number()
    .int('Adapter index must be an integer')
    .min(0, 'Adapter index must be non-negative')
    .describe('Adapter index'),

  raw_score: z.number()
    .describe('Raw score from router'),

  gate_q15: z.number()
    .int('Gate value must be an integer')
    .describe('Q15 quantized gate value'),
});

export type RouterCandidate = z.infer<typeof RouterCandidateSchema>;

/**
 * Router decision schema (for observability)
 *
 * Maps to: adapteros-api-types/src/inference.rs::RouterDecision
 */
export const RouterDecisionSchema = z.object({
  step: z.number()
    .int('Step must be an integer')
    .min(0, 'Step must be non-negative')
    .describe('Generation step'),

  token_idx: z.number()
    .int('Token index must be an integer')
    .min(0, 'Token index must be non-negative')
    .optional()
    .describe('Token index in sequence'),

  input_token_id: z.number()
    .int('Token ID must be an integer')
    .optional()
    .describe('Input token ID'),

  candidate_adapters: z.array(RouterCandidateSchema)
    .describe('Candidate adapters'),

  gates: z.array(z.number())
    .optional()
    .describe('Gate values for each adapter'),

  entropy: z.number()
    .min(0, 'Entropy must be non-negative')
    .describe('Router entropy'),

  tau: z.number()
    .positive('Tau must be positive')
    .describe('Temperature parameter'),

  entropy_floor: z.number()
    .min(0, 'Entropy floor must be non-negative')
    .describe('Minimum entropy'),

  stack_hash: z.string()
    .optional()
    .describe('Stack hash for verification'),
});

export type RouterDecision = z.infer<typeof RouterDecisionSchema>;

/**
 * Evidence span schema (for audit trail)
 *
 * Maps to: adapteros-api-types/src/inference.rs::EvidenceSpan
 */
export const EvidenceSpanSchema = z.object({
  doc_id: z.string()
    .min(1, 'Document ID is required')
    .describe('Source document ID'),

  span_hash: z.string()
    .min(1, 'Span hash is required')
    .describe('BLAKE3 hash of the span content'),

  relevance_score: z.number()
    .min(0, 'Relevance score must be non-negative')
    .max(1, 'Relevance score must not exceed 1.0')
    .optional()
    .describe('Relevance score (0-1)'),

  confidence: z.number()
    .min(0, 'Confidence must be non-negative')
    .max(1, 'Confidence must not exceed 1.0')
    .optional()
    .describe('Confidence score (0-1)'),
});

export type EvidenceSpan = z.infer<typeof EvidenceSpanSchema>;

/**
 * Inference trace schema (for observability)
 *
 * Maps to: adapteros-api-types/src/inference.rs::InferenceTrace
 */
export const InferenceTraceSchema = z.object({
  adapters_used: z.array(z.string())
    .max(8, 'Too many adapters (MAX_K=8)')
    .describe('Adapters used in inference'),

  router_decisions: z.array(RouterDecisionSchema)
    .describe('Router decisions at each step'),

  evidence_spans: z.array(EvidenceSpanSchema)
    .optional()
    .describe('Evidence spans for audit trail'),

  latency_ms: z.number()
    .int('Latency must be an integer')
    .min(0, 'Latency must be non-negative')
    .describe('Total inference latency in milliseconds'),
});

export type InferenceTrace = z.infer<typeof InferenceTraceSchema>;

/**
 * Inference preset configurations
 */
export const InferencePresets = {
  creative: {
    name: 'Creative',
    description: 'High temperature for creative writing',
    config: {
      temperature: 1.2,
      top_p: 0.95,
      max_tokens: 1024,
    },
  },
  balanced: {
    name: 'Balanced',
    description: 'Balanced settings for general use',
    config: {
      temperature: 0.7,
      top_p: 0.9,
      max_tokens: 512,
    },
  },
  precise: {
    name: 'Precise',
    description: 'Low temperature for factual responses',
    config: {
      temperature: 0.2,
      top_p: 0.8,
      max_tokens: 512,
    },
  },
  deterministic: {
    name: 'Deterministic',
    description: 'Zero temperature with seed for reproducibility',
    config: {
      temperature: 0.0,
      seed: 42,
      max_tokens: 512,
    },
  },
} as const;

/**
 * Helper functions for inference validation
 */
export const InferenceUtils = {
  /**
   * Calculate estimated tokens from text
   * Rough estimate: 1 token ≈ 4 characters
   */
  estimateTokenCount(text: string): number {
    return Math.ceil(text.length / 4);
  },

  /**
   * Validate prompt length against max tokens
   */
  validatePromptLength(prompt: string, maxTokens: number): boolean {
    const estimatedTokens = this.estimateTokenCount(prompt);
    // Leave room for generation
    return estimatedTokens + maxTokens <= 4096;
  },

  /**
   * Get recommended max_tokens for prompt
   */
  getRecommendedMaxTokens(prompt: string): number {
    const estimatedTokens = this.estimateTokenCount(prompt);
    const remaining = 4096 - estimatedTokens;
    return Math.min(remaining, DEFAULT_MAX_TOKENS);
  },

  /**
   * Validate temperature range for preset
   */
  validateTemperature(temperature: number): {
    valid: boolean;
    suggestion?: string;
  } {
    if (temperature < 0) {
      return {
        valid: false,
        suggestion: 'Temperature cannot be negative. Use 0.0 for deterministic output.',
      };
    }
    if (temperature > 2) {
      return {
        valid: false,
        suggestion: 'Temperature above 2.0 may produce incoherent output. Try 0.7-1.2.',
      };
    }
    if (temperature === 0) {
      return {
        valid: true,
        suggestion: 'Temperature 0.0 produces deterministic output (with seed).',
      };
    }
    return { valid: true };
  },
};
