/**
 * Zod validation schemas for AdapterStack API types.
 * These schemas validate runtime data from the API and allow unknown fields.
 */

import { z } from 'zod';
import { ActiveAdapterSchema } from './adapter.zod';

// ============================================================================
// Stack Schemas
// ============================================================================

export const WorkflowTypeSchema = z.enum(['Parallel', 'UpstreamDownstream', 'Sequential']);

/**
 * AdapterStack: Collection of adapters with routing configuration
 */
export const AdapterStackSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().optional(),

  // Adapters can be represented in two ways
  adapters: z.array(ActiveAdapterSchema).optional(),
  adapter_ids: z.array(z.string()).optional(),

  // Metadata
  created_at: z.string(),
  updated_at: z.string(),
  is_default: z.boolean().optional(),
  version: z.number().optional(),

  // Configuration
  workflow_type: WorkflowTypeSchema.optional(),
  lifecycle_state: z.string().optional(),
  determinism_mode: z.string().optional(),
}).passthrough();

export type AdapterStack = z.infer<typeof AdapterStackSchema>;

// ============================================================================
// Stack Response Schemas
// ============================================================================

export const AdapterStackResponseSchema = z.object({
  schema_version: z.string(),
  stack: AdapterStackSchema,
  warnings: z.array(z.string()).optional(),
}).passthrough();

export type AdapterStackResponse = z.infer<typeof AdapterStackResponseSchema>;

export const ListAdapterStacksResponseSchema = z.object({
  schema_version: z.string(),
  stacks: z.array(AdapterStackSchema),
  total: z.number(),
}).passthrough();

export type ListAdapterStacksResponse = z.infer<typeof ListAdapterStacksResponseSchema>;

// ============================================================================
// Stack Request Schemas
// ============================================================================

export const CreateAdapterStackRequestSchema = z.object({
  name: z.string(),
  adapters: z.array(ActiveAdapterSchema),
  description: z.string().optional(),
}).passthrough();

export type CreateAdapterStackRequest = z.infer<typeof CreateAdapterStackRequestSchema>;

export const UpdateAdapterStackRequestSchema = z.object({
  name: z.string().optional(),
  adapters: z.array(ActiveAdapterSchema).optional(),
  description: z.string().optional(),
}).passthrough();

export type UpdateAdapterStackRequest = z.infer<typeof UpdateAdapterStackRequestSchema>;

// ============================================================================
// Stack Operations
// ============================================================================

export const DefaultStackResponseSchema = z.object({
  schema_version: z.string(),
  tenant_id: z.string(),
  stack_id: z.string(),
}).passthrough();

export type DefaultStackResponse = z.infer<typeof DefaultStackResponseSchema>;

export const ValidateStackNameResponseSchema = z.object({
  schema_version: z.string(),
  valid: z.boolean(),
  message: z.string().optional(),
  errors: z.array(z.string()).optional(),
}).passthrough();

export type ValidateStackNameResponse = z.infer<typeof ValidateStackNameResponseSchema>;

// ============================================================================
// Policy Preflight
// ============================================================================

export const PolicyCheckSchema = z.object({
  policy_id: z.string(),
  policy_name: z.string(),
  passed: z.boolean(),
  severity: z.enum(['error', 'warning', 'info']),
  message: z.string(),
  can_override: z.boolean().optional(),
  details: z.string().optional(),
}).passthrough();

export type PolicyCheck = z.infer<typeof PolicyCheckSchema>;

export const PolicyPreflightResponseSchema = z.object({
  checks: z.array(PolicyCheckSchema),
  can_proceed: z.boolean(),
  stack_id: z.string().optional(),
  adapter_ids: z.array(z.string()).optional(),
}).passthrough();

export type PolicyPreflightResponse = z.infer<typeof PolicyPreflightResponseSchema>;
