/**
 * Adapter validation schemas
 *
 * Maps to backend types:
 * - adapteros-api-types/src/adapters.rs::RegisterAdapterRequest
 * - adapteros-policy/src/packs/naming_policy.rs::NamingPolicy
 * - adapteros-core/src/naming.rs::AdapterName
 */

import { z } from 'zod';

/**
 * Supported languages
 *
 * From: adapteros-server-api/src/validation.rs::validate_languages()
 */
export const SupportedLanguages = [
  'python',
  'rust',
  'typescript',
  'javascript',
  'go',
  'java',
  'c',
  'cpp',
  'csharp',
] as const;

/**
 * Reserved tenant names (cannot be used)
 *
 * From: adapteros-policy/src/packs/naming_policy.rs
 */
export const ReservedTenants = ['system', 'admin', 'root', 'default', 'test'] as const;

/**
 * Reserved domain names (cannot be used)
 *
 * From: adapteros-policy/src/packs/naming_policy.rs
 */
export const ReservedDomains = ['core', 'internal', 'deprecated'] as const;

/**
 * Adapter tier schema
 */
export const adapterTierSchema = z.number()
  .int('Tier must be an integer')
  .min(1, 'Tier must be at least 1')
  .max(3, 'Tier must not exceed 3')
  .describe('Adapter tier (1=high priority, 3=low priority)');

/**
 * Adapter name component validators
 */
const tenantValidator = z.string()
  .min(1, 'Tenant cannot be empty')
  .max(50, 'Tenant must not exceed 50 characters')
  .regex(
    /^[a-z0-9_-]+$/,
    'Tenant must contain only lowercase letters, numbers, underscores, and hyphens'
  )
  .refine(
    (val) => !ReservedTenants.includes(val as any),
    'This tenant name is reserved'
  );

const domainValidator = z.string()
  .min(1, 'Domain cannot be empty')
  .max(50, 'Domain must not exceed 50 characters')
  .regex(
    /^[a-z0-9_-]+$/,
    'Domain must contain only lowercase letters, numbers, underscores, and hyphens'
  )
  .refine(
    (val) => !ReservedDomains.includes(val as any),
    'This domain name is reserved'
  );

const purposeValidator = z.string()
  .min(1, 'Purpose cannot be empty')
  .max(50, 'Purpose must not exceed 50 characters')
  .regex(
    /^[a-z0-9_-]+$/,
    'Purpose must contain only lowercase letters, numbers, underscores, and hyphens'
  );

const revisionValidator = z.string()
  .regex(
    /^r\d{3,}$/,
    'Revision must be in format rXXX (e.g., r001, r042)'
  );

/**
 * Semantic adapter name schema
 *
 * Format: {tenant}/{domain}/{purpose}/{revision}
 * Example: tenant-a/engineering/code-review/r001
 *
 * Validation from: adapteros-policy/src/packs/naming_policy.rs
 */
export const adapterNameSchema = z.string()
  .min(3, 'Adapter name is too short')
  .max(200, 'Adapter name must not exceed 200 characters')
  .regex(
    /^[a-z0-9_-]+\/[a-z0-9_-]+\/[a-z0-9_-]+\/r\d{3,}$/,
    'Use format: tenant/domain/purpose/rXXX (e.g., tenant-a/engineering/code-review/r001)'
  )
  .refine(
    (val) => {
      const parts = val.split('/');
      if (parts.length !== 4) return false;

      const [tenant, domain, purpose, revision] = parts;

      // Validate each component
      try {
        tenantValidator.parse(tenant);
        domainValidator.parse(domain);
        purposeValidator.parse(purpose);
        revisionValidator.parse(revision);
        return true;
      } catch {
        return false;
      }
    },
    'Invalid adapter name components'
  )
  .describe('Semantic adapter name');

/**
 * Register adapter request schema
 *
 * Maps to: adapteros-api-types/src/adapters.rs::RegisterAdapterRequest
 */
export const registerAdapterRequestSchema = z.object({
  // Adapter ID (unique identifier)
  // From: adapteros-server-api/src/validation/mod.rs::validate_adapter_id()
  adapter_id: z.string()
    .min(1, 'Adapter ID is required')
    .max(128, 'Adapter ID must not exceed 128 characters')
    .describe('Unique adapter identifier'),

  // Semantic name
  name: adapterNameSchema,

  // BLAKE3 hash
  hash_b3: z.string()
    .regex(
      /^b3:[a-f0-9]{64}$/,
      'Hash must be in format: b3:{64 hex characters}'
    )
    .describe('BLAKE3 hash of adapter weights'),

  // LoRA rank
  rank: z.number()
    .int('Rank must be an integer')
    .min(1, 'Rank must be at least 1')
    .max(256, 'Rank must not exceed 256')
    .describe('LoRA rank dimension'),

  // Adapter tier
  tier: adapterTierSchema,

  // Supported languages
  languages: z.array(
    z.enum(SupportedLanguages)
  )
    .min(1, 'At least one language must be specified')
    .max(10, 'Too many languages')
    .describe('Supported programming languages'),

  // Optional: Framework
  framework: z.string()
    .min(1, 'Framework cannot be empty if provided')
    .max(50, 'Framework must not exceed 50 characters')
    .optional()
    .describe('Framework or library'),
});

export type RegisterAdapterRequest = z.infer<typeof registerAdapterRequestSchema>;

/**
 * Adapter name validation request schema
 *
 * For validating names before registration
 */
export const adapterNameValidationSchema = z.object({
  // Name to validate
  name: adapterNameSchema,

  // Tenant requesting validation
  tenant_id: z.string()
    .min(1, 'Tenant ID is required')
    .max(50, 'Tenant ID too long')
    .describe('Requesting tenant'),

  // Optional: Parent adapter name (if forking)
  parent_name: adapterNameSchema.optional()
    .describe('Parent adapter for hierarchy validation'),

  // Optional: Latest revision in lineage
  latest_revision: z.number()
    .int('Latest revision must be an integer')
    .min(1, 'Latest revision must be at least 1')
    .optional()
    .describe('Latest revision number in lineage'),
});

export type AdapterNameValidation = z.infer<typeof adapterNameValidationSchema>;

/**
 * Adapter lifecycle states
 *
 * From: adapteros-lora-lifecycle
 */
export const adapterLifecycleStateSchema = z.enum([
  'unloaded',
  'cold',
  'warm',
  'hot',
  'resident',
]);

export type AdapterLifecycleState = z.infer<typeof adapterLifecycleStateSchema>;

/**
 * Stack name schema
 *
 * Format: stack.{namespace} or stack.{namespace}-{identifier}
 * Example: stack.production-env, stack.my-namespace
 */
export const stackNameSchema = z.string()
  .min(6, 'Stack name is too short')
  .max(100, 'Stack name must not exceed 100 characters')
  .regex(
    /^stack\.[a-z0-9_-]+$/,
    'Use format: stack.{namespace} (e.g., stack.production-env)'
  )
  .describe('Adapter stack name');

/**
 * Create adapter stack request schema
 */
export const createAdapterStackRequestSchema = z.object({
  // Stack name
  name: stackNameSchema,

  // Description
  // From: adapteros-server-api/src/validation/mod.rs::validate_description()
  description: z.string()
    .min(1, 'Description is required')
    .max(10000, 'Description must not exceed 10000 characters')
    .describe('Stack description'),

  // Adapter IDs in the stack
  adapter_ids: z.array(
    z.string().min(1, 'Adapter ID cannot be empty')
  )
    .min(1, 'At least one adapter required')
    .max(8, 'Stack cannot exceed 8 adapters (MAX_K=8)')
    .describe('Adapter IDs in stack'),

  // Workflow type
  workflow_type: z.string()
    .min(1, 'Workflow type is required')
    .max(50, 'Workflow type too long')
    .optional()
    .describe('Workflow type'),
});

export type CreateAdapterStackRequest = z.infer<typeof createAdapterStackRequestSchema>;

/**
 * Adapter pinning request schema
 *
 * From: adapteros-db/src/pinned_adapters.rs
 */
export const pinAdapterRequestSchema = z.object({
  // Adapter ID to pin
  adapter_id: z.string()
    .min(1, 'Adapter ID is required')
    .describe('Adapter to pin'),

  // Optional: Pin until timestamp (RFC3339 format)
  pinned_until: z.string()
    .regex(
      /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}/,
      'Use format: YYYY-MM-DD HH:MM:SS or ISO 8601'
    )
    .optional()
    .describe('Pin expiration (optional)'),

  // Reason for pinning
  reason: z.string()
    .min(1, 'Reason is required')
    .max(500, 'Reason must not exceed 500 characters')
    .describe('Reason for pinning'),

  // User pinning the adapter
  pinned_by: z.string()
    .min(1, 'User is required')
    .max(100, 'User identifier too long')
    .describe('User pinning the adapter'),
});

export type PinAdapterRequest = z.infer<typeof pinAdapterRequestSchema>;

/**
 * Adapter TTL (Time-To-Live) schema
 *
 * For ephemeral/temporary adapters
 */
export const AdapterTTLSchema = z.string()
  .regex(
    /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}/,
    'Use format: YYYY-MM-DD HH:MM:SS or ISO 8601'
  )
  .optional()
  .describe('Adapter expiration timestamp');

/**
 * Helper functions for adapter naming
 */
export const AdapterNameUtils = {
  /**
   * Parse adapter name into components
   */
  parse(name: string) {
    const parts = name.split('/');
    if (parts.length !== 4) {
      throw new Error('Invalid adapter name format');
    }

    const [tenant, domain, purpose, revision] = parts;
    const revisionMatch = revision.match(/^r(\d+)$/);

    if (!revisionMatch) {
      throw new Error('Invalid revision format');
    }

    return {
      tenant,
      domain,
      purpose,
      revision,
      revisionNumber: parseInt(revisionMatch[1], 10),
      lineage: `${tenant}/${domain}/${purpose}`,
    };
  },

  /**
   * Check if two adapters are in the same lineage
   */
  isSameLineage(name1: string, name2: string): boolean {
    try {
      const parsed1 = this.parse(name1);
      const parsed2 = this.parse(name2);
      return parsed1.lineage === parsed2.lineage;
    } catch {
      return false;
    }
  },

  /**
   * Get next revision in lineage
   */
  nextRevision(name: string): string {
    try {
      const parsed = this.parse(name);
      const nextRev = parsed.revisionNumber + 1;
      return `${parsed.lineage}/r${nextRev.toString().padStart(3, '0')}`;
    } catch {
      throw new Error('Invalid adapter name');
    }
  },

  /**
   * Validate revision gap (max 5)
   */
  validateRevisionGap(currentRevision: number, newRevision: number): boolean {
    const gap = newRevision - currentRevision;
    return gap > 0 && gap <= 5;
  },
};
