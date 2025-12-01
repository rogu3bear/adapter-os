/**
 * Validation Schema Index
 *
 * Central export point for all Zod validation schemas.
 * These schemas provide type-safe validation for frontend forms and API requests,
 * matching backend validation rules from Rust types.
 */

// ===== Form Schemas (UI-specific) =====
export {
  TrainingConfigSchema,
  type TrainingConfigFormData,
  DatasetConfigSchema,
  type DatasetConfigFormData,
  InferenceRequestSchema,
  type InferenceRequestFormData,
  PromotionRequestSchema,
  type PromotionRequestFormData,
  BatchPromptSchema,
  type BatchPromptFormData,
} from './forms';

// ===== Backend-Mapped Training Schemas =====
export {
  TrainingConfigSchema as BackendTrainingConfigSchema,
  StartTrainingRequestSchema,
  TrainingJobStatusSchema,
  UploadDatasetRequestSchema,
  ValidateDatasetRequestSchema,
  TrainingTemplates,
  type TrainingConfig as BackendTrainingConfig,
  type StartTrainingRequest,
  type TrainingJobStatus,
  type UploadDatasetRequest,
  type ValidateDatasetRequest,
} from './training.schema';

// ===== Backend-Mapped Adapter Schemas =====
export {
  adapterNameSchema,
  registerAdapterRequestSchema,
  adapterNameValidationSchema,
  adapterLifecycleStateSchema,
  adapterTierSchema,
  stackNameSchema,
  createAdapterStackRequestSchema,
  pinAdapterRequestSchema,
  AdapterTTLSchema,
  AdapterNameUtils,
  SupportedLanguages,
  ReservedTenants,
  ReservedDomains,
  type RegisterAdapterRequest,
  type AdapterNameValidation,
  type AdapterLifecycleState,
  type CreateAdapterStackRequest,
  type PinAdapterRequest,
} from './adapter.schema';

// ===== Backend-Mapped Inference Schemas =====
export {
  InferRequestSchema as BackendInferRequestSchema,
  StreamingInferenceRequestSchema,
  FinishReasonSchema,
  RouterCandidateSchema,
  RouterDecisionSchema,
  EvidenceSpanSchema,
  InferenceTraceSchema,
  InferencePresets,
  InferenceUtils,
  type InferRequest as BackendInferRequest,
  type StreamingInferenceRequest,
  type FinishReason,
  type RouterCandidate,
  type RouterDecision,
  type EvidenceSpan,
  type InferenceTrace,
} from './inference.schema';

// ===== Common Schemas =====
export {
  tenantIdSchema,
  RepositoryIdSchema,
  CommitShaSchema,
  blake3HashSchema,
  descriptionSchema,
  FilePathSchema,
  paginationSchema,
  timestampSchema,
  EmailSchema,
  UuidSchema,
  UrlSchema,
  PercentageSchema,
  ChunkSizeSchema,
  FileSizeSchema,
  BatchSizeSchema,
  LanguageSchema,
  validationStatusSchema,
  errorResponseSchema,
  ValidationUtils,
  type TenantId,
  type RepositoryId,
  type CommitSha,
  type Blake3Hash,
  type Description,
  type FilePath,
  type Pagination,
  type Timestamp,
  type Email,
  type Uuid,
  type Url,
  type Percentage,
  type ChunkSize,
  type FileSize,
  type BatchSize,
  type Language,
  type ValidationStatus,
  type ErrorResponse,
} from './common.schema';

// ===== Admin Schemas =====
export {
  StackFormSchema,
  type StackFormData,
  TenantFormSchema,
  type TenantFormData,
  UserFormSchema,
  type UserFormData,
} from './admin.schema';

// ===== Validation Utilities =====
export {
  formatValidationError,
  parseValidationErrors,
  validateField,
  formatFieldError,
} from './utils';

/**
 * Re-export zod for convenience
 */
export { z } from 'zod';
