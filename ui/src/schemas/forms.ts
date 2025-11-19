/**
 * Zod schemas for form validation
 * Organized by feature area (training, datasets, inference, promotion)
 */

import { z } from 'zod';

/**
 * Training Configuration Schema
 * Validates all training parameters for the TrainingWizard
 */
export const TrainingConfigSchema = z.object({
  // Basic info
  name: z
    .string()
    .min(1, 'Adapter name is required')
    .min(3, 'Name must be at least 3 characters')
    .max(100, 'Name must be at most 100 characters')
    .regex(/^[a-zA-Z0-9_-]+$/, 'Name can only contain alphanumeric characters, hyphens, and underscores'),

  description: z
    .string()
    .max(500, 'Description must be at most 500 characters')
    .optional()
    .default(''),

  category: z
    .enum(['code', 'framework', 'codebase', 'ephemeral'])
    .refine(
      (val) => ['code', 'framework', 'codebase', 'ephemeral'].includes(val),
      'Select a valid adapter category'
    ),

  scope: z
    .enum(['global', 'tenant', 'repo', 'commit'])
    .refine(
      (val) => ['global', 'tenant', 'repo', 'commit'].includes(val),
      'Select a valid scope'
    ),

  // Data source
  dataSourceType: z
    .enum(['repository', 'template', 'custom', 'directory'])
    .refine(
      (val) => ['repository', 'template', 'custom', 'directory'].includes(val),
      'Select a valid data source type'
    ),

  templateId: z
    .string()
    .optional(),

  repositoryId: z
    .string()
    .optional(),

  customData: z
    .string()
    .optional(),

  datasetPath: z
    .string()
    .optional(),

  directoryRoot: z
    .string()
    .optional(),

  directoryPath: z
    .string()
    .optional(),

  // Category-specific config
  language: z
    .string()
    .optional(),

  symbolTargets: z
    .array(z.string())
    .optional()
    .default([]),

  frameworkId: z
    .string()
    .optional(),

  frameworkVersion: z
    .string()
    .optional(),

  apiPatterns: z
    .array(z.string())
    .optional()
    .default([]),

  repoScope: z
    .string()
    .optional(),

  filePatterns: z
    .array(z.string())
    .optional()
    .default([]),

  excludePatterns: z
    .array(z.string())
    .optional()
    .default([]),

  ttlSeconds: z
    .number()
    .int('TTL must be an integer')
    .positive('TTL must be positive')
    .optional(),

  contextWindow: z
    .number()
    .int('Context window must be an integer')
    .positive('Context window must be positive')
    .optional(),

  // Training parameters
  rank: z
    .number()
    .int('Rank must be an integer')
    .min(2, 'Rank must be at least 2')
    .max(256, 'Rank must be at most 256'),

  alpha: z
    .number()
    .int('Alpha must be an integer')
    .positive('Alpha must be positive'),

  epochs: z
    .number()
    .int('Epochs must be an integer')
    .min(1, 'Epochs must be at least 1')
    .max(1000, 'Epochs must be at most 1000'),

  learningRate: z
    .number()
    .positive('Learning rate must be positive')
    .max(1, 'Learning rate must be at most 1'),

  batchSize: z
    .number()
    .int('Batch size must be an integer')
    .min(1, 'Batch size must be at least 1')
    .max(512, 'Batch size must be at most 512'),

  targets: z
    .array(z.string())
    .min(1, 'Select at least one LoRA target module')
    .default(['q_proj', 'v_proj']),

  warmupSteps: z
    .number()
    .int('Warmup steps must be an integer')
    .positive('Warmup steps must be positive')
    .optional(),

  maxSeqLength: z
    .number()
    .int('Max sequence length must be an integer')
    .positive('Max sequence length must be positive')
    .optional(),

  // Packaging & registration
  packageAfter: z
    .boolean()
    .optional()
    .default(true),

  registerAfter: z
    .boolean()
    .optional()
    .default(true),

  adaptersRoot: z
    .string()
    .optional(),

  adapterId: z
    .string()
    .optional(),

  tier: z
    .number()
    .int('Tier must be an integer')
    .positive('Tier must be positive')
    .optional(),
}).refine(
  (data) => {
    // If dataSourceType is template, require templateId
    if (data.dataSourceType === 'template' && !data.templateId) {
      return false;
    }
    return true;
  },
  {
    message: 'Template ID is required when using template data source',
    path: ['templateId'],
  }
).refine(
  (data) => {
    // If dataSourceType is repository, require repositoryId
    if (data.dataSourceType === 'repository' && !data.repositoryId) {
      return false;
    }
    return true;
  },
  {
    message: 'Repository ID is required when using repository data source',
    path: ['repositoryId'],
  }
).refine(
  (data) => {
    // If dataSourceType is directory, require directoryRoot
    if (data.dataSourceType === 'directory' && !data.directoryRoot) {
      return false;
    }
    return true;
  },
  {
    message: 'Directory root is required when using directory data source',
    path: ['directoryRoot'],
  }
).refine(
  (data) => {
    // If dataSourceType is custom, require datasetPath
    if (data.dataSourceType === 'custom' && !data.datasetPath) {
      return false;
    }
    return true;
  },
  {
    message: 'Dataset path is required when using custom data source',
    path: ['datasetPath'],
  }
).refine(
  (data) => {
    // If category is code, require language
    if (data.category === 'code' && !data.language) {
      return false;
    }
    return true;
  },
  {
    message: 'Language is required for code category',
    path: ['language'],
  }
);

export type TrainingConfigFormData = z.infer<typeof TrainingConfigSchema>;

/**
 * Dataset Configuration Schema
 * Validates dataset configuration for DatasetBuilder
 */
export const DatasetConfigSchema = z.object({
  name: z
    .string()
    .min(1, 'Dataset name is required')
    .min(3, 'Name must be at least 3 characters')
    .max(100, 'Name must be at most 100 characters'),

  description: z
    .string()
    .max(500, 'Description must be at most 500 characters')
    .optional()
    .default(''),

  strategy: z
    .enum(['identity', 'question_answer', 'masked_lm'])
    .default('identity'),

  maxSequenceLength: z
    .number()
    .int('Max sequence length must be an integer')
    .min(128, 'Max sequence length must be at least 128')
    .max(8192, 'Max sequence length must be at most 8192')
    .default(2048),

  validationSplit: z
    .number()
    .min(0, 'Validation split must be at least 0')
    .max(0.5, 'Validation split must be at most 0.5')
    .default(0.1),

  tokenizer: z
    .string()
    .optional(),
});

export type DatasetConfigFormData = z.infer<typeof DatasetConfigSchema>;

/**
 * Inference Request Schema
 * Validates inference parameters for InferencePlayground
 */
export const InferenceRequestSchema = z.object({
  prompt: z
    .string()
    .min(1, 'Prompt cannot be empty')
    .min(3, 'Prompt must be at least 3 characters')
    .max(50000, 'Prompt must be at most 50,000 characters')
    .refine(
      (val) => {
        // Check for invisible Unicode characters
        const hasInvisible = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D]/.test(val);
        return !hasInvisible;
      },
      'Prompt contains unsupported control or invisible characters'
    ),

  max_tokens: z
    .number()
    .int('Max tokens must be an integer')
    .min(10, 'Max tokens must be at least 10')
    .max(2000, 'Max tokens must be at most 2000')
    .default(100),

  temperature: z
    .number()
    .min(0, 'Temperature must be at least 0')
    .max(2, 'Temperature must be at most 2')
    .default(0.7),

  top_k: z
    .number()
    .int('Top K must be an integer')
    .min(1, 'Top K must be at least 1')
    .max(100, 'Top K must be at most 100')
    .default(50),

  top_p: z
    .number()
    .min(0, 'Top P must be at least 0')
    .max(1, 'Top P must be at most 1')
    .default(0.9),

  seed: z
    .number()
    .int('Seed must be an integer')
    .optional(),

  require_evidence: z
    .boolean()
    .optional()
    .default(false),

  adapters: z
    .array(z.string())
    .optional(),
});

export type InferenceRequestFormData = z.infer<typeof InferenceRequestSchema>;

/**
 * Promotion Request Schema
 * Validates promotion workflow parameters
 */
export const PromotionRequestSchema = z.object({
  stage_id: z
    .string()
    .min(1, 'Stage ID is required'),

  justification: z
    .string()
    .min(10, 'Justification must be at least 10 characters')
    .max(2000, 'Justification must be at most 2000 characters'),

  target_environment: z
    .enum(['staging', 'production']),

  // Optional rollback plan
  rollbackPlan: z
    .object({
      trigger_conditions: z
        .array(z.string())
        .optional()
        .default([]),

      rollback_steps: z
        .array(z.string())
        .optional()
        .default([]),

      notification_contacts: z
        .array(z.string().email('Invalid email address'))
        .optional()
        .default([]),
    })
    .optional(),

  approver: z
    .string()
    .optional(),

  approved_at: z
    .string()
    .optional(),

  notes: z
    .string()
    .max(2000, 'Notes must be at most 2000 characters')
    .optional(),
});

export type PromotionRequestFormData = z.infer<typeof PromotionRequestSchema>;

/**
 * Batch Inference Request Schema
 * Validates a single prompt in a batch operation
 */
export const BatchPromptSchema = z.object({
  prompt: z
    .string()
    .min(1, 'Prompt cannot be empty')
    .min(3, 'Prompt must be at least 3 characters'),

  max_tokens: z
    .number()
    .int('Max tokens must be an integer')
    .min(10, 'Max tokens must be at least 10')
    .max(2000, 'Max tokens must be at most 2000')
    .optional(),

  temperature: z
    .number()
    .min(0, 'Temperature must be at least 0')
    .max(2, 'Temperature must be at most 2')
    .optional(),
});

export type BatchPromptFormData = z.infer<typeof BatchPromptSchema>;
