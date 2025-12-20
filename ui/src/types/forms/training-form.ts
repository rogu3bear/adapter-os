/**
 * Training Form Types
 *
 * Form state and validation types for training workflows.
 * Maps to Zod schemas in ui/src/schemas/forms.ts
 *
 * Citations:
 * - ui/src/schemas/forms.ts - TrainingConfigSchema
 * - ui/src/components/SingleFileAdapterTrainer.tsx - TrainerConfigSchema
 * - ui/src/components/TrainingWizard.tsx - Multi-step training form
 */

/**
 * Training configuration form data
 *
 * Derived from TrainingConfigSchema (ui/src/schemas/forms.ts)
 */
export interface TrainingConfigFormData {
  // Basic info
  name: string;
  description?: string;
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';

  // Data source
  dataSourceType: 'repository' | 'template' | 'custom' | 'directory';
  templateId?: string;
  repositoryId?: string;
  customData?: string;
  datasetPath?: string;
  directoryRoot?: string;
  directoryPath?: string;

  // Category-specific config
  language?: string;
  symbolTargets?: string[];
  frameworkId?: string;
  frameworkVersion?: string;
  apiPatterns?: string[];
  repoScope?: string;
  filePatterns?: string[];
  excludePatterns?: string[];
  ttlSeconds?: number;
  contextWindow?: number;

  // Training parameters
  rank: number;
  alpha: number;
  epochs: number;
  learningRate: number;
  batchSize: number;
  targets: string[];
  warmupSteps?: number;
  maxSeqLength?: number;

  // Packaging & registration
  packageAfter?: boolean;
  registerAfter?: boolean;
  adaptersRoot?: string;
  adapterId?: string;
  tier?: number;
}

/**
 * Semantic naming training form (SingleFileAdapterTrainer)
 *
 * Uses semantic naming format: {tenant}/{domain}/{purpose}/{revision}
 */
export interface SemanticTrainingFormData {
  // Semantic naming components
  tenant: string;
  domain: string;
  purpose: string;
  revision: string;

  // Training parameters
  rank: number;
  alpha: number;
  epochs: number;
  batchSize: number;
  learningRate: number;
}

/**
 * Training form validation state
 */
export interface TrainingFormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
}

/**
 * Complete training form state
 */
export interface TrainingFormState {
  config: TrainingConfigFormData;
  validation: TrainingFormValidationState;
  step?: number; // For multi-step wizard
  totalSteps?: number;
}

/**
 * Training wizard step state
 */
export interface TrainingWizardStep {
  id: string;
  label: string;
  description?: string;
  isComplete: boolean;
  isValid: boolean;
}

/**
 * Training wizard form state
 */
export interface TrainingWizardFormState {
  currentStep: number;
  steps: TrainingWizardStep[];
  formData: Partial<TrainingConfigFormData>;
  canGoNext: boolean;
  canGoPrevious: boolean;
  canSubmit: boolean;
}

/**
 * Dataset configuration form data
 *
 * Derived from DatasetConfigSchema (ui/src/schemas/forms.ts)
 */
export interface DatasetConfigFormData {
  name: string;
  description?: string;
  strategy: 'identity' | 'question_answer' | 'masked_lm';
  maxSequenceLength: number;
  validationSplit: number;
  tokenizer?: string;
}

/**
 * Dataset form state
 */
export interface DatasetFormState {
  config: DatasetConfigFormData;
  validation: TrainingFormValidationState;
}
