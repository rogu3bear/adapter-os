/**
 * Inference Form Types
 *
 * Form state and validation types for inference operations.
 * Maps to schemas in ui/src/schemas/forms.ts
 *
 * Citations:
 * - ui/src/schemas/forms.ts - InferenceRequestSchema, BatchPromptSchema
 * - ui/src/components/InferencePlayground.tsx - Inference form implementation
 */

/**
 * Inference request form data
 *
 * Derived from InferenceRequestSchema (ui/src/schemas/forms.ts)
 */
export interface InferenceRequestFormData {
  prompt: string;
  max_tokens: number;
  temperature: number;
  top_k: number;
  top_p: number;
  backend?: 'auto' | 'coreml' | 'mlx' | 'metal';
  model?: string;
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

/**
 * Batch prompt form data
 *
 * Derived from BatchPromptSchema (ui/src/schemas/forms.ts)
 */
export interface BatchPromptFormData {
  prompt: string;
  max_tokens?: number;
  temperature?: number;
}

/**
 * Inference form validation state
 */
export interface InferenceFormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
}

/**
 * Complete inference form state
 */
export interface InferenceFormState {
  formData: InferenceRequestFormData;
  validation: InferenceFormValidationState;
  isStreaming: boolean;
  canSubmit: boolean;
}

/**
 * Batch inference form state
 */
export interface BatchInferenceFormState {
  prompts: BatchPromptFormData[];
  commonSettings: {
    max_tokens?: number;
    temperature?: number;
    backend?: string;
    model?: string;
  };
  validation: InferenceFormValidationState;
  progress?: {
    current: number;
    total: number;
  };
}

/**
 * Sampling parameters form data (subset of inference params)
 */
export interface SamplingParametersFormData {
  temperature: number;
  top_k: number;
  top_p: number;
  max_tokens: number;
  seed?: number;
}

/**
 * Chat message form data
 */
export interface ChatMessageFormData {
  message: string;
  attachments?: Array<{
    type: 'file' | 'image' | 'code';
    content: string;
    name?: string;
  }>;
}

/**
 * Chat form state
 */
export interface ChatFormState {
  formData: ChatMessageFormData;
  validation: InferenceFormValidationState;
  isWaitingForResponse: boolean;
  canSend: boolean;
}
