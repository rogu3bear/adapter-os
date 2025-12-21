/**
 * Training configuration validation schemas
 *
 * Maps to backend types:
 * - adapteros-api-types/src/training.rs::TrainingConfigRequest
 * - adapteros-types/src/training/mod.rs::TrainingConfig
 * - adapteros-api-types/src/training.rs::StartTrainingRequest
 */

import { z } from 'zod';
import { adapterNameSchema } from './adapter.schema';

/**
 * Training configuration schema
 *
 * Validation rules extracted from:
 * - TrainingConfig::default_for_adapter() (rank: 16, alpha: 32, epochs: 3)
 * - TrainingConfig::quick_training() (rank: 8, epochs: 1)
 * - TrainingConfig::deep_training() (rank: 32, epochs: 5)
 */
export const TrainingConfigSchema = z.object({
  // LoRA rank dimension (typically 4, 8, 16, 32)
  rank: z.number()
    .int('Rank must be an integer')
    .min(1, 'Rank must be at least 1')
    .max(256, 'Rank must not exceed 256')
    .describe('LoRA rank dimension'),

  // LoRA alpha scaling factor (typically 2x rank)
  alpha: z.number()
    .int('Alpha must be an integer')
    .min(1, 'Alpha must be at least 1')
    .max(512, 'Alpha must not exceed 512')
    .describe('LoRA alpha scaling factor'),

  // Target linear layer names to apply LoRA
  targets: z.array(z.string().min(1, 'Target name cannot be empty'))
    .min(1, 'At least one target layer must be specified')
    .max(20, 'Too many target layers')
    .describe('Target linear layer names'),

  // Number of training epochs
  epochs: z.number()
    .int('Epochs must be an integer')
    .min(1, 'At least 1 epoch required')
    .max(1000, 'Epochs must not exceed 1000')
    .describe('Number of training epochs'),

  // Learning rate for optimizer
  learning_rate: z.number()
    .positive('Learning rate must be positive')
    .max(1, 'Learning rate must not exceed 1.0')
    .describe('Learning rate for optimizer'),

  // Batch size for training
  batch_size: z.number()
    .int('Batch size must be an integer')
    .min(1, 'Batch size must be at least 1')
    .max(512, 'Batch size must not exceed 512')
    .describe('Batch size for training'),

  // Optional: Warmup steps for learning rate schedule
  warmup_steps: z.number()
    .int('Warmup steps must be an integer')
    .min(0, 'Warmup steps must be non-negative')
    .max(10000, 'Warmup steps must not exceed 10000')
    .optional()
    .describe('Warmup steps for learning rate schedule'),

  // Optional: Maximum sequence length (default 2048)
  max_seq_length: z.number()
    .int('Max sequence length must be an integer')
    .min(128, 'Max sequence length must be at least 128')
    .max(8192, 'Max sequence length must not exceed 8192')
    .optional()
    .describe('Maximum sequence length'),

  // Optional: Gradient accumulation steps
  gradient_accumulation_steps: z.number()
    .int('Gradient accumulation steps must be an integer')
    .min(1, 'Gradient accumulation steps must be at least 1')
    .max(64, 'Gradient accumulation steps must not exceed 64')
    .optional()
    .describe('Gradient accumulation steps for larger effective batch size'),
});

export type TrainingConfig = z.infer<typeof TrainingConfigSchema>;

// Post-training actions schema (aligned with adapteros-api-types::PostActionsRequest)
const PostActionsSchema = z.object({
  package: z.boolean().optional(),
  register: z.boolean().optional(),
  create_stack: z.boolean().optional(),
  activate_stack: z.boolean().optional(),
  tier: z.string().optional(),
  adapters_root: z.string().optional(),
});

/**
 * Start training request schema
 *
 * Maps to: adapteros-api-types/src/training.rs::StartTrainingRequest
 */
export const StartTrainingRequestSchema = z.object({
  // Adapter name (semantic naming format)
  // Uses canonical schema from adapter.schema.ts for consistency
  adapter_name: adapterNameSchema,

  // Training configuration
  config: TrainingConfigSchema,

  // Optional: Template ID
  template_id: z.string()
    .min(1, 'Template ID cannot be empty')
    .max(100, 'Template ID too long')
    .optional()
    .describe('Training template to use'),

  // Optional: Repository ID
  // From: adapteros-server-api/src/validation/mod.rs::validate_repo_id()
  repo_id: z.string()
    .regex(
      /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_-]+$/,
      'Repository ID must be in format: owner/repo'
    )
    .max(256, 'Repository ID must not exceed 256 characters')
    .optional()
    .describe('Source repository'),

  // Optional: Dataset ID
  dataset_id: z.string()
    .min(1, 'Dataset ID cannot be empty')
    .max(100, 'Dataset ID too long')
    .optional()
    .describe('Training dataset'),
  dataset_version_ids: z.array(
    z.object({
      dataset_version_id: z.string().min(1, 'Dataset version ID is required'),
      weight: z.number().positive('Weight must be positive').optional(),
    })
  )
    .min(1, 'At least one dataset version is required')
    .optional(),
  synthetic_mode: z.boolean().optional().default(false),
  data_lineage_mode: z.enum(['versioned', 'dataset_only', 'synthetic', 'legacy_unpinned']).optional(),
  branch_classification: z.enum(['protected', 'high', 'sandbox']).optional(),
  post_actions: PostActionsSchema.optional(),
});

export type StartTrainingRequest = z.infer<typeof StartTrainingRequestSchema>;

/**
 * Training job status enum
 *
 * Maps to: adapteros-types/src/training/mod.rs::TrainingJobStatus
 */
export const TrainingJobStatusSchema = z.enum([
  'pending',
  'running',
  'completed',
  'failed',
  'cancelled',
]);

export type TrainingJobStatus = z.infer<typeof TrainingJobStatusSchema>;

/**
 * Upload dataset request schema
 *
 * Maps to: adapteros-api-types/src/training.rs::UploadDatasetRequest
 */
export const UploadDatasetRequestSchema = z.object({
  // Dataset name
  name: z.string()
    .min(1, 'Dataset name is required')
    .max(200, 'Dataset name must not exceed 200 characters')
    .regex(
      /^[a-zA-Z0-9_-]+$/,
      'Dataset name must contain only letters, numbers, underscores, and hyphens'
    )
    .describe('Dataset name'),

  // Optional: Description
  // From: adapteros-server-api/src/validation/mod.rs::validate_description()
  description: z.string()
    .min(1, 'Description cannot be empty if provided')
    .max(10000, 'Description must not exceed 10000 characters')
    .optional()
    .describe('Dataset description'),

  // Format: 'patches', 'jsonl', 'txt', 'custom'
  format: z.enum(['patches', 'jsonl', 'txt', 'custom'])
    .describe('Dataset format'),
});

export type UploadDatasetRequest = z.infer<typeof UploadDatasetRequestSchema>;

/**
 * Dataset validation request schema
 *
 * Maps to: adapteros-api-types/src/training.rs::ValidateDatasetRequest
 */
export const ValidateDatasetRequestSchema = z.object({
  dataset_id: z.string()
    .min(1, 'Dataset ID is required')
    .describe('Dataset to validate'),
});

export type ValidateDatasetRequest = z.infer<typeof ValidateDatasetRequestSchema>;

// Predefined training templates
export const TrainingTemplates = {
  quick: {
    id: 'quick-training',
    name: 'Quick Training',
    description: 'Fast training with minimal parameters',
    config: {
      rank: 8,
      alpha: 16,
      targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj'],
      epochs: 1,
      learning_rate: 0.002,
      batch_size: 16,
    },
  },
  standard: {
    id: 'standard-training',
    name: 'Standard Training',
    description: 'Balanced training configuration',
    config: {
      rank: 16,
      alpha: 32,
      targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'],
      epochs: 3,
      learning_rate: 0.001,
      batch_size: 32,
      warmup_steps: 100,
      max_seq_length: 2048,
      gradient_accumulation_steps: 4,
    },
  },
  deep: {
    id: 'deep-training',
    name: 'Deep Training',
    description: 'Comprehensive training with high rank',
    config: {
      rank: 32,
      alpha: 64,
      targets: [
        'q_proj',
        'k_proj',
        'v_proj',
        'o_proj',
        'gate_proj',
        'up_proj',
        'down_proj',
        'mlp.dense_h_to_4h',
        'mlp.dense_4h_to_h',
      ],
      epochs: 5,
      learning_rate: 0.0005,
      batch_size: 64,
      warmup_steps: 500,
      max_seq_length: 4096,
      gradient_accumulation_steps: 8,
    },
  },
} as const;
