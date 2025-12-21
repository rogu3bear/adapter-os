/**
 * Frontend Domain Types (camelCase)
 *
 * These types are derived from generated backend types with camelCase keys.
 * Use these in UI components and hooks instead of raw backend types.
 *
 * The transformation is type-safe at compile time via CamelCaseKeys<T>.
 * For runtime transformation, use toCamelCase() from ./transformers.ts
 *
 * @example
 * ```typescript
 * import type { InferResponse, Adapter } from '@/api/domain-types';
 * import { toCamelCase } from '@/api/transformers';
 *
 * const backendData = await fetch('/api/infer').then(r => r.json());
 * const frontendData: InferResponse = toCamelCase(backendData);
 * ```
 */

import type { components } from './generated';

// ============================================================================
// Type-level Case Transformation
// ============================================================================

/**
 * Converts a snake_case string literal type to camelCase
 */
type SnakeToCamel<S extends string> = S extends `${infer T}_${infer U}`
  ? `${T}${Capitalize<SnakeToCamel<U>>}`
  : S;

/**
 * Recursively transforms all keys in an object type from snake_case to camelCase
 *
 * @template T - The backend type with snake_case keys
 */
export type CamelCaseKeys<T> = T extends Array<infer U>
  ? Array<CamelCaseKeys<U>>
  : T extends object
  ? {
      [K in keyof T as SnakeToCamel<K & string>]: CamelCaseKeys<T[K]>;
    }
  : T;

// ============================================================================
// Inference Types
// ============================================================================

export type InferRequest = CamelCaseKeys<components['schemas']['InferRequest']>;
export type InferResponse = CamelCaseKeys<components['schemas']['InferResponse']>;
export type InferenceTrace = CamelCaseKeys<components['schemas']['InferenceTrace']>;
export type RunReceipt = CamelCaseKeys<components['schemas']['RunReceipt']>;
export type DeterministicReceipt = CamelCaseKeys<components['schemas']['DeterministicReceipt']>;
export type Citation = CamelCaseKeys<components['schemas']['Citation']>;
export type StopReasonCode = CamelCaseKeys<components['schemas']['StopReasonCode']>;
export type StopPolicySpec = CamelCaseKeys<components['schemas']['StopPolicySpec']>;
export type ReplayGuarantee = CamelCaseKeys<components['schemas']['ReplayGuarantee']>;

// Batch inference
export type BatchInferRequest = CamelCaseKeys<components['schemas']['BatchInferRequest']>;
export type BatchInferResponse = CamelCaseKeys<components['schemas']['BatchInferResponse']>;
export type BatchInferItemRequest = CamelCaseKeys<components['schemas']['BatchInferItemRequest']>;
export type BatchInferItemResponse = CamelCaseKeys<components['schemas']['BatchInferItemResponse']>;

// Streaming
export type StreamingInferRequest = CamelCaseKeys<components['schemas']['StreamingInferRequest']>;
export type StreamingChunk = CamelCaseKeys<components['schemas']['StreamingChunk']>;
export type StreamingChoice = CamelCaseKeys<components['schemas']['StreamingChoice']>;

// ============================================================================
// Adapter Types
// ============================================================================

export type Adapter = CamelCaseKeys<components['schemas']['AdapterResponse']>;
export type AdapterLifecycleState = CamelCaseKeys<components['schemas']['AdapterLifecycleState']>;
export type AdapterStateResponse = CamelCaseKeys<components['schemas']['AdapterStateResponse']>;
export type AdapterProvenance = CamelCaseKeys<components['schemas']['AdapterProvenance']>;
export type AdapterVersionResponse = CamelCaseKeys<components['schemas']['AdapterVersionResponse']>;
export type AdapterSummary = CamelCaseKeys<components['schemas']['AdapterSummary']>;

// Adapter stacks
export type StackResponse = CamelCaseKeys<components['schemas']['StackResponse']>;
export type CreateStackRequest = CamelCaseKeys<components['schemas']['CreateStackRequest']>;
export type DefaultStackResponse = CamelCaseKeys<components['schemas']['DefaultStackResponse']>;
export type SetDefaultStackRequest = CamelCaseKeys<components['schemas']['SetDefaultStackRequest']>;
export type StackProvenance = CamelCaseKeys<components['schemas']['StackProvenance']>;
export type StackSummary = CamelCaseKeys<components['schemas']['StackSummary']>;

// Adapter performance & memory
export type AdapterMetricsResponse = CamelCaseKeys<components['schemas']['AdapterMetricsResponse']>;
export type AdapterPerformance = CamelCaseKeys<components['schemas']['AdapterPerformance']>;
export type AdapterMemoryUsageResponse = CamelCaseKeys<components['schemas']['AdapterMemoryUsageResponse']>;
export type AdapterMemoryInfo = CamelCaseKeys<components['schemas']['AdapterMemoryInfo']>;
export type AdapterMemorySummary = CamelCaseKeys<components['schemas']['AdapterMemorySummary']>;
export type MemoryLocation = CamelCaseKeys<components['schemas']['MemoryLocation']>;
export type AdapterUsageResponse = CamelCaseKeys<components['schemas']['AdapterUsageResponse']>;
export type AdapterStatsResponse = CamelCaseKeys<components['schemas']['AdapterStatsResponse']>;
export type AdapterStats = CamelCaseKeys<components['schemas']['AdapterStats']>;

// Adapter activation & routing
export type AdapterActivationResponse = CamelCaseKeys<components['schemas']['AdapterActivationResponse']>;
export type AdapterFired = CamelCaseKeys<components['schemas']['AdapterFired']>;
export type AdapterScore = CamelCaseKeys<components['schemas']['AdapterScore']>;
export type RouterCandidateResponse = CamelCaseKeys<components['schemas']['RouterCandidateResponse']>;
export type RouterCandidate = CamelCaseKeys<components['schemas']['RouterCandidate']>;

// Adapter repository
export type AdapterRepositoryResponse = CamelCaseKeys<components['schemas']['AdapterRepositoryResponse']>;
export type AdapterRepositoryPolicyResponse = CamelCaseKeys<components['schemas']['AdapterRepositoryPolicyResponse']>;
export type RepoTier = CamelCaseKeys<components['schemas']['RepoTier']>;

// Adapter swap/hotswap
export type AdapterSwapRequest = CamelCaseKeys<components['schemas']['AdapterSwapRequest']>;
export type AdapterSwapResponse = CamelCaseKeys<components['schemas']['AdapterSwapResponse']>;

// ============================================================================
// Training Types
// ============================================================================

export type TrainingJobResponse = CamelCaseKeys<components['schemas']['TrainingJobResponse']>;
export type TrainingJobListResponse = CamelCaseKeys<components['schemas']['TrainingJobListResponse']>;
export type TrainingJobProvenance = CamelCaseKeys<components['schemas']['TrainingJobProvenance']>;
export type StartTrainingRequest = CamelCaseKeys<components['schemas']['StartTrainingRequest']>;
export type TrainingConfigRequest = CamelCaseKeys<components['schemas']['TrainingConfigRequest']>;
export type TrainingMetricsResponse = CamelCaseKeys<components['schemas']['TrainingMetricsResponse']>;
export type TrainingTemplateResponse = CamelCaseKeys<components['schemas']['TrainingTemplateResponse']>;

// Datasets
export type DatasetResponse = CamelCaseKeys<components['schemas']['DatasetResponse']>;
export type DatasetFileResponse = CamelCaseKeys<components['schemas']['DatasetFileResponse']>;
export type DatasetVersionsResponse = CamelCaseKeys<components['schemas']['DatasetVersionsResponse']>;
export type DatasetVersionSummary = CamelCaseKeys<components['schemas']['DatasetVersionSummary']>;
export type DatasetStatisticsResponse = CamelCaseKeys<components['schemas']['DatasetStatisticsResponse']>;
export type CreateDatasetVersionRequest = CamelCaseKeys<components['schemas']['CreateDatasetVersionRequest']>;
export type CreateDatasetVersionResponse = CamelCaseKeys<components['schemas']['CreateDatasetVersionResponse']>;
export type UploadDatasetResponse = CamelCaseKeys<components['schemas']['UploadDatasetResponse']>;
export type ValidateDatasetRequest = CamelCaseKeys<components['schemas']['ValidateDatasetRequest']>;
export type ValidateDatasetResponse = CamelCaseKeys<components['schemas']['ValidateDatasetResponse']>;
export type DatasetValidationStatus = CamelCaseKeys<components['schemas']['DatasetValidationStatus']>;
export type DatasetProvenance = CamelCaseKeys<components['schemas']['DatasetProvenance']>;
export type DatasetVersionSelection = CamelCaseKeys<components['schemas']['DatasetVersionSelection']>;
export type DatasetVersionTrustSnapshot = CamelCaseKeys<components['schemas']['DatasetVersionTrustSnapshot']>;
export type DatasetTrustOverrideRequest = CamelCaseKeys<components['schemas']['DatasetTrustOverrideRequest']>;
export type UpdateDatasetSafetyRequest = CamelCaseKeys<components['schemas']['UpdateDatasetSafetyRequest']>;
export type UpdateDatasetSafetyResponse = CamelCaseKeys<components['schemas']['UpdateDatasetSafetyResponse']>;

// ============================================================================
// Chat Types
// ============================================================================

export type ChatSession = CamelCaseKeys<components['schemas']['ChatSession']>;
export type ChatMessage = CamelCaseKeys<components['schemas']['ChatMessage']>;
export type ChatMessageResponse = CamelCaseKeys<components['schemas']['ChatMessageResponse']>;
export type CreateChatSessionRequest = CamelCaseKeys<components['schemas']['CreateChatSessionRequest']>;
export type CreateChatSessionResponse = CamelCaseKeys<components['schemas']['CreateChatSessionResponse']>;
export type AddChatMessageRequest = CamelCaseKeys<components['schemas']['AddChatMessageRequest']>;
export type SessionsResponse = CamelCaseKeys<components['schemas']['SessionsResponse']>;
export type SessionSummary = CamelCaseKeys<components['schemas']['SessionSummary']>;
export type SessionInfo = CamelCaseKeys<components['schemas']['SessionInfo']>;
export type ChatBootstrapResponse = CamelCaseKeys<components['schemas']['ChatBootstrapResponse']>;
export type ChatProvenanceResponse = CamelCaseKeys<components['schemas']['ChatProvenanceResponse']>;
export type ChatProvenanceEntry = CamelCaseKeys<components['schemas']['ChatProvenanceEntry']>;
export type CreateChatFromJobRequest = CamelCaseKeys<components['schemas']['CreateChatFromJobRequest']>;
export type CreateChatFromJobResponse = CamelCaseKeys<components['schemas']['CreateChatFromJobResponse']>;

// Session routing
export type SessionRouterViewResponse = CamelCaseKeys<components['schemas']['SessionRouterViewResponse']>;
export type SessionStep = CamelCaseKeys<components['schemas']['SessionStep']>;
export type SessionAction = CamelCaseKeys<components['schemas']['SessionAction']>;

// ============================================================================
// Document & Evidence Types
// ============================================================================

export type DocumentResponse = CamelCaseKeys<components['schemas']['DocumentResponse']>;
export type AddDocumentRequest = CamelCaseKeys<components['schemas']['AddDocumentRequest']>;
export type CollectionResponse = CamelCaseKeys<components['schemas']['CollectionResponse']>;
export type CollectionDetailResponse = CamelCaseKeys<components['schemas']['CollectionDetailResponse']>;
export type CreateCollectionRequest = CamelCaseKeys<components['schemas']['CreateCollectionRequest']>;
export type UpdateCollectionRequest = CamelCaseKeys<components['schemas']['UpdateCollectionRequest']>;
export type CollectionDocumentInfo = CamelCaseKeys<components['schemas']['CollectionDocumentInfo']>;

// Evidence
export type EvidenceResponse = CamelCaseKeys<components['schemas']['EvidenceResponse']>;
export type CreateEvidenceRequest = CamelCaseKeys<components['schemas']['CreateEvidenceRequest']>;

// Chunking
export type ChunkResponse = CamelCaseKeys<components['schemas']['ChunkResponse']>;
export type InitiateChunkedUploadRequest = CamelCaseKeys<components['schemas']['InitiateChunkedUploadRequest']>;
export type InitiateChunkedUploadResponse = CamelCaseKeys<components['schemas']['InitiateChunkedUploadResponse']>;
export type CompleteChunkedUploadRequest = CamelCaseKeys<components['schemas']['CompleteChunkedUploadRequest']>;
export type CompleteChunkedUploadResponse = CamelCaseKeys<components['schemas']['CompleteChunkedUploadResponse']>;
export type UploadChunkResponse = CamelCaseKeys<components['schemas']['UploadChunkResponse']>;
export type UploadSessionStatusResponse = CamelCaseKeys<components['schemas']['UploadSessionStatusResponse']>;

// ============================================================================
// System & Health Types
// ============================================================================

export type HealthResponse = CamelCaseKeys<components['schemas']['HealthResponse']>;
export type SystemHealthResponse = CamelCaseKeys<components['schemas']['SystemHealthResponse']>;
export type ComponentHealth = CamelCaseKeys<components['schemas']['ComponentHealth']>;
export type ComponentStatus = CamelCaseKeys<components['schemas']['ComponentStatus']>;
export type ErrorResponse = CamelCaseKeys<components['schemas']['ErrorResponse']>;
export type ApiErrorBody = CamelCaseKeys<components['schemas']['ApiErrorBody']>;
export type FailureCode = CamelCaseKeys<components['schemas']['FailureCode']>;

// System state
export type SystemStateResponse = CamelCaseKeys<components['schemas']['SystemStateResponse']>;
export type SystemOverviewResponse = CamelCaseKeys<components['schemas']['SystemOverviewResponse']>;
export type SystemMetricsResponse = CamelCaseKeys<components['schemas']['SystemMetricsResponse']>;
export type SystemReadySection = CamelCaseKeys<components['schemas']['SystemReadySection']>;
export type TelemetrySection = CamelCaseKeys<components['schemas']['TelemetrySection']>;
export type DrainSection = CamelCaseKeys<components['schemas']['DrainSection']>;

// Capacity
export type CapacityResponse = CamelCaseKeys<components['schemas']['CapacityResponse']>;
export type CapacityLimits = CamelCaseKeys<components['schemas']['CapacityLimits']>;
export type CapacityUsage = CamelCaseKeys<components['schemas']['CapacityUsage']>;

// Load metrics
export type LoadAverageResponse = CamelCaseKeys<components['schemas']['LoadAverageResponse']>;

// ============================================================================
// Worker Types
// ============================================================================

export type WorkerDetailResponse = CamelCaseKeys<components['schemas']['WorkerDetailResponse']>;
export type WorkerStopResponse = CamelCaseKeys<components['schemas']['WorkerStopResponse']>;
export type WorkerResourceUsage = CamelCaseKeys<components['schemas']['WorkerResourceUsage']>;
export type WorkerTask = CamelCaseKeys<components['schemas']['WorkerTask']>;
export type WorkerType = CamelCaseKeys<components['schemas']['WorkerType']>;

// Service control
export type ServiceControlResponse = CamelCaseKeys<components['schemas']['ServiceControlResponse']>;
export type ServiceStatus = CamelCaseKeys<components['schemas']['ServiceStatus']>;
export type ServiceState = CamelCaseKeys<components['schemas']['ServiceState']>;
export type ServiceHealthStatus = CamelCaseKeys<components['schemas']['ServiceHealthStatus']>;
export type ShutdownMode = CamelCaseKeys<components['schemas']['ShutdownMode']>;

// ============================================================================
// Model Types
// ============================================================================

export type ModelListResponse = CamelCaseKeys<components['schemas']['ModelListResponse']>;
export type ModelStatusResponse = CamelCaseKeys<components['schemas']['ModelStatusResponse']>;
export type ModelRuntimeHealthResponse = CamelCaseKeys<components['schemas']['ModelRuntimeHealthResponse']>;
export type ModelValidationResponse = CamelCaseKeys<components['schemas']['ModelValidationResponse']>;
export type BaseModelInfo = CamelCaseKeys<components['schemas']['BaseModelInfo']>;
export type BaseModelStatusResponse = CamelCaseKeys<components['schemas']['BaseModelStatusResponse']>;
export type ImportModelRequest = CamelCaseKeys<components['schemas']['ImportModelRequest']>;
export type ImportModelResponse = CamelCaseKeys<components['schemas']['ImportModelResponse']>;

// Backend types
export type BackendKind = CamelCaseKeys<components['schemas']['BackendKind']>;
export type CoreMLMode = CamelCaseKeys<components['schemas']['CoreMLMode']>;

// ============================================================================
// Policy Types
// ============================================================================

export type PolicyAssignmentResponse = CamelCaseKeys<components['schemas']['PolicyAssignmentResponse']>;
export type PolicyHistoryResponse = CamelCaseKeys<components['schemas']['PolicyHistoryResponse']>;
export type PolicyViolationResponse = CamelCaseKeys<components['schemas']['PolicyViolationResponse']>;
export type CreateExecutionPolicyRequest = CamelCaseKeys<components['schemas']['CreateExecutionPolicyRequest']>;
export type CreatePolicyResponse = CamelCaseKeys<components['schemas']['CreatePolicyResponse']>;
export type AssignPolicyRequest = CamelCaseKeys<components['schemas']['AssignPolicyRequest']>;
export type TogglePolicyRequest = CamelCaseKeys<components['schemas']['TogglePolicyRequest']>;
export type CategoryPolicyRequest = CamelCaseKeys<components['schemas']['CategoryPolicyRequest']>;
export type CategoryPolicyResponse = CamelCaseKeys<components['schemas']['CategoryPolicyResponse']>;
export type CategoryPoliciesResponse = CamelCaseKeys<components['schemas']['CategoryPoliciesResponse']>;
export type TenantExecutionPolicy = CamelCaseKeys<components['schemas']['TenantExecutionPolicy']>;
export type TenantPolicyBindingResponse = CamelCaseKeys<components['schemas']['TenantPolicyBindingResponse']>;
export type DeterminismPolicy = CamelCaseKeys<components['schemas']['DeterminismPolicy']>;
export type RoutingPolicy = CamelCaseKeys<components['schemas']['RoutingPolicy']>;
export type ApprovalRecord = CamelCaseKeys<components['schemas']['ApprovalRecord']>;
export type ApproveRequest = CamelCaseKeys<components['schemas']['ApproveRequest']>;
export type ApproveResponse = CamelCaseKeys<components['schemas']['ApproveResponse']>;

// ============================================================================
// Auth Types
// ============================================================================

export type LoginRequest = CamelCaseKeys<components['schemas']['LoginRequest']>;
export type LoginResponse = CamelCaseKeys<components['schemas']['LoginResponse']>;
export type LogoutResponse = CamelCaseKeys<components['schemas']['LogoutResponse']>;
export type UserInfoResponse = CamelCaseKeys<components['schemas']['UserInfoResponse']>;
export type AuthConfigResponse = CamelCaseKeys<components['schemas']['AuthConfigResponse']>;
export type TokenRevocationResponse = CamelCaseKeys<components['schemas']['TokenRevocationResponse']>;

// MFA
export type MfaStatusResponse = CamelCaseKeys<components['schemas']['MfaStatusResponse']>;
export type MfaEnrollStartResponse = CamelCaseKeys<components['schemas']['MfaEnrollStartResponse']>;
export type MfaEnrollVerifyRequest = CamelCaseKeys<components['schemas']['MfaEnrollVerifyRequest']>;
export type MfaEnrollVerifyResponse = CamelCaseKeys<components['schemas']['MfaEnrollVerifyResponse']>;
export type MfaDisableRequest = CamelCaseKeys<components['schemas']['MfaDisableRequest']>;

// ============================================================================
// Tenant & Workspace Types
// ============================================================================

export type TenantResponse = CamelCaseKeys<components['schemas']['TenantResponse']>;
export type TenantSummary = CamelCaseKeys<components['schemas']['TenantSummary']>;
export type TenantState = CamelCaseKeys<components['schemas']['TenantState']>;
export type CreateTenantRequest = CamelCaseKeys<components['schemas']['CreateTenantRequest']>;
export type TenantHydrationResponse = CamelCaseKeys<components['schemas']['TenantHydrationResponse']>;
export type HydrateTenantRequest = CamelCaseKeys<components['schemas']['HydrateTenantRequest']>;

// Workspaces
export type WorkspaceResponse = CamelCaseKeys<components['schemas']['WorkspaceResponse']>;
export type CreateWorkspaceRequest = CamelCaseKeys<components['schemas']['CreateWorkspaceRequest']>;
export type UpdateWorkspaceRequest = CamelCaseKeys<components['schemas']['UpdateWorkspaceRequest']>;
export type AddWorkspaceMemberRequest = CamelCaseKeys<components['schemas']['AddWorkspaceMemberRequest']>;
export type UpdateWorkspaceMemberRequest = CamelCaseKeys<components['schemas']['UpdateWorkspaceMemberRequest']>;

// ============================================================================
// Git & Repository Types
// ============================================================================

export type GitStatusResponse = CamelCaseKeys<components['schemas']['GitStatusResponse']>;
export type StartGitSessionRequest = CamelCaseKeys<components['schemas']['StartGitSessionRequest']>;
export type StartGitSessionResponse = CamelCaseKeys<components['schemas']['StartGitSessionResponse']>;
export type EndGitSessionRequest = CamelCaseKeys<components['schemas']['EndGitSessionRequest']>;
export type EndGitSessionResponse = CamelCaseKeys<components['schemas']['EndGitSessionResponse']>;
export type CommitResponse = CamelCaseKeys<components['schemas']['CommitResponse']>;
export type CommitDeltaRequest = CamelCaseKeys<components['schemas']['CommitDeltaRequest']>;
export type CommitDeltaResponse = CamelCaseKeys<components['schemas']['CommitDeltaResponse']>;
export type CommitDiffResponse = CamelCaseKeys<components['schemas']['CommitDiffResponse']>;
export type DiffStats = CamelCaseKeys<components['schemas']['DiffStats']>;
export type Delta = CamelCaseKeys<components['schemas']['Delta']>;
export type FileChangeEvent = CamelCaseKeys<components['schemas']['FileChangeEvent']>;

// Repository scan
export type ScanRepositoryRequest = CamelCaseKeys<components['schemas']['ScanRepositoryRequest']>;
export type ScanStatusResponse = CamelCaseKeys<components['schemas']['ScanStatusResponse']>;
export type ScanJobResponse = CamelCaseKeys<components['schemas']['ScanJobResponse']>;
export type ScanJobStatusResponse = CamelCaseKeys<components['schemas']['ScanJobStatusResponse']>;
export type ScanJobResult = CamelCaseKeys<components['schemas']['ScanJobResult']>;
export type ScanJobProgress = CamelCaseKeys<components['schemas']['ScanJobProgress']>;
export type TriggerScanRequest = CamelCaseKeys<components['schemas']['TriggerScanRequest']>;

// Repository management
export type CreateRepoRequest = CamelCaseKeys<components['schemas']['CreateRepoRequest']>;
export type CreateRepoResponse = CamelCaseKeys<components['schemas']['CreateRepoResponse']>;
export type UpdateRepoRequest = CamelCaseKeys<components['schemas']['UpdateRepoRequest']>;
export type RepositoryListResponse = CamelCaseKeys<components['schemas']['RepositoryListResponse']>;
export type RepositoryDetailResponse = CamelCaseKeys<components['schemas']['RepositoryDetailResponse']>;
export type RepositoryResponse = CamelCaseKeys<components['schemas']['RepositoryResponse']>;
export type RepoDetailResponse = CamelCaseKeys<components['schemas']['RepoDetailResponse']>;
export type RepoSummaryResponse = CamelCaseKeys<components['schemas']['RepoSummaryResponse']>;
export type RepoTimelineEventResponse = CamelCaseKeys<components['schemas']['RepoTimelineEventResponse']>;
export type RepoTrainingJobLinkResponse = CamelCaseKeys<components['schemas']['RepoTrainingJobLinkResponse']>;
export type RegisterRepositoryResponse = CamelCaseKeys<components['schemas']['RegisterRepositoryResponse']>;

// Branch classification
export type BranchSummary = CamelCaseKeys<components['schemas']['BranchSummary']>;
export type BranchClassification = CamelCaseKeys<components['schemas']['BranchClassification']>;

// ============================================================================
// Audit & Activity Types
// ============================================================================

export type AuditLogResponse = CamelCaseKeys<components['schemas']['AuditLogResponse']>;
export type AuditLogsResponse = CamelCaseKeys<components['schemas']['AuditLogsResponse']>;
export type AuditsResponse = CamelCaseKeys<components['schemas']['AuditsResponse']>;
export type AuditExtended = CamelCaseKeys<components['schemas']['AuditExtended']>;
export type ActivityEventResponse = CamelCaseKeys<components['schemas']['ActivityEventResponse']>;
export type CreateActivityEventRequest = CamelCaseKeys<components['schemas']['CreateActivityEventRequest']>;

// ============================================================================
// Batch Job Types
// ============================================================================

export type BatchJobResponse = CamelCaseKeys<components['schemas']['BatchJobResponse']>;
export type CreateBatchJobRequest = CamelCaseKeys<components['schemas']['CreateBatchJobRequest']>;
export type BatchStatusResponse = CamelCaseKeys<components['schemas']['BatchStatusResponse']>;
export type BatchItemsResponse = CamelCaseKeys<components['schemas']['BatchItemsResponse']>;
export type BatchItemResultResponse = CamelCaseKeys<components['schemas']['BatchItemResultResponse']>;

// ============================================================================
// Replay & Determinism Types
// ============================================================================

export type DeterminismStatusResponse = CamelCaseKeys<components['schemas']['DeterminismStatusResponse']>;
export type GoldenCompareRequest = CamelCaseKeys<components['schemas']['GoldenCompareRequest']>;

// ============================================================================
// Federation Types
// ============================================================================

export type FederationStatusResponse = CamelCaseKeys<components['schemas']['FederationStatusResponse']>;
export type FederationSyncStatusResponse = CamelCaseKeys<components['schemas']['FederationSyncStatusResponse']>;
export type NodeDetailResponse = CamelCaseKeys<components['schemas']['NodeDetailResponse']>;

// ============================================================================
// Domain Adapter Types
// ============================================================================

export type DomainAdapterResponse = CamelCaseKeys<components['schemas']['DomainAdapterResponse']>;
export type DomainAdapterManifestResponse = CamelCaseKeys<components['schemas']['DomainAdapterManifestResponse']>;
export type DomainAdapterExecutionResponse = CamelCaseKeys<components['schemas']['DomainAdapterExecutionResponse']>;
export type CreateDomainAdapterRequest = CamelCaseKeys<components['schemas']['CreateDomainAdapterRequest']>;
export type LoadDomainAdapterRequest = CamelCaseKeys<components['schemas']['LoadDomainAdapterRequest']>;
export type TestDomainAdapterRequest = CamelCaseKeys<components['schemas']['TestDomainAdapterRequest']>;
export type TestDomainAdapterResponse = CamelCaseKeys<components['schemas']['TestDomainAdapterResponse']>;

// ============================================================================
// Memory & Resource Types
// ============================================================================

export type UmaMemoryResponse = CamelCaseKeys<components['schemas']['UmaMemoryResponse']>;
export type UmaMemoryBreakdownResponse = CamelCaseKeys<components['schemas']['UmaMemoryBreakdownResponse']>;
export type AneMemoryState = CamelCaseKeys<components['schemas']['AneMemoryState']>;
export type EvictionCandidate = CamelCaseKeys<components['schemas']['EvictionCandidate']>;
export type EvictionConfig = CamelCaseKeys<components['schemas']['EvictionConfig']>;

// ============================================================================
// Drift & Validation Types
// ============================================================================

export type DriftSummaryResponse = CamelCaseKeys<components['schemas']['DriftSummaryResponse']>;
export type DriftFieldResponse = CamelCaseKeys<components['schemas']['DriftFieldResponse']>;
export type ValidationIssue = CamelCaseKeys<components['schemas']['ValidationIssue']>;

// ============================================================================
// Settings Types
// ============================================================================

export type ServerSettings = CamelCaseKeys<components['schemas']['ServerSettings']>;
export type SystemSettings = CamelCaseKeys<components['schemas']['SystemSettings']>;
export type SecuritySettings = CamelCaseKeys<components['schemas']['SecuritySettings']>;
export type UpdateSettingsRequest = CamelCaseKeys<components['schemas']['UpdateSettingsRequest']>;
export type SettingsUpdateResponse = CamelCaseKeys<components['schemas']['SettingsUpdateResponse']>;

// ============================================================================
// Dashboard & UI Types
// ============================================================================

export type GetDashboardConfigResponse = CamelCaseKeys<components['schemas']['GetDashboardConfigResponse']>;
export type UpdateDashboardConfigRequest = CamelCaseKeys<components['schemas']['UpdateDashboardConfigRequest']>;
export type UpdateDashboardConfigResponse = CamelCaseKeys<components['schemas']['UpdateDashboardConfigResponse']>;
export type DashboardWidgetConfig = CamelCaseKeys<components['schemas']['DashboardWidgetConfig']>;
export type WidgetConfigUpdate = CamelCaseKeys<components['schemas']['WidgetConfigUpdate']>;

// Journey system
export type JourneyResponse = CamelCaseKeys<components['schemas']['JourneyResponse']>;

// Tutorials
export type TutorialResponse = CamelCaseKeys<components['schemas']['TutorialResponse']>;
export type TutorialStatusResponse = CamelCaseKeys<components['schemas']['TutorialStatusResponse']>;
export type TutorialStep = CamelCaseKeys<components['schemas']['TutorialStep']>;

// ============================================================================
// Storage Types
// ============================================================================

export type StorageStatsResponse = CamelCaseKeys<components['schemas']['StorageStatsResponse']>;
export type StorageModeResponse = CamelCaseKeys<components['schemas']['StorageModeResponse']>;

// ============================================================================
// Lifecycle Types
// ============================================================================

export type LifecycleStatusResponse = CamelCaseKeys<components['schemas']['LifecycleStatusResponse']>;
export type LifecycleHistoryResponse = CamelCaseKeys<components['schemas']['LifecycleHistoryResponse']>;
export type RuntimeSessionResponse = CamelCaseKeys<components['schemas']['RuntimeSessionResponse']>;
export type RuntimePathsResponse = CamelCaseKeys<components['schemas']['RuntimePathsResponse']>;

// ============================================================================
// Contact Types
// ============================================================================

export type ContactResponse = CamelCaseKeys<components['schemas']['ContactResponse']>;
export type ContactsResponse = CamelCaseKeys<components['schemas']['ContactsResponse']>;
export type CreateContactRequest = CamelCaseKeys<components['schemas']['CreateContactRequest']>;
export type Contact = CamelCaseKeys<components['schemas']['Contact']>;
export type ContactUpsertParams = CamelCaseKeys<components['schemas']['ContactUpsertParams']>;
export type ContactInteraction = CamelCaseKeys<components['schemas']['ContactInteraction']>;
export type ContactInteractionResponse = CamelCaseKeys<components['schemas']['ContactInteractionResponse']>;
export type ContactInteractionsResponse = CamelCaseKeys<components['schemas']['ContactInteractionsResponse']>;

// ============================================================================
// Metrics Types
// ============================================================================

export type MetricsTimeSeriesResponse = CamelCaseKeys<components['schemas']['MetricsTimeSeriesResponse']>;
export type MetaResponse = CamelCaseKeys<components['schemas']['MetaResponse']>;
export type EpsilonStatsResponse = CamelCaseKeys<components['schemas']['EpsilonStatsResponse']>;

// ============================================================================
// Bootstrap & CLI Types
// ============================================================================

export type BootstrapRequest = CamelCaseKeys<components['schemas']['BootstrapRequest']>;
export type BootstrapResponse = CamelCaseKeys<components['schemas']['BootstrapResponse']>;
export type CliRunRequest = CamelCaseKeys<components['schemas']['CliRunRequest']>;
export type CliRunResponse = CamelCaseKeys<components['schemas']['CliRunResponse']>;

// ============================================================================
// Miscellaneous Types
// ============================================================================

export type ShareResourceRequest = CamelCaseKeys<components['schemas']['ShareResourceRequest']>;
export type TrustOverrideRequest = CamelCaseKeys<components['schemas']['TrustOverrideRequest']>;
export type TrustOverrideResponse = CamelCaseKeys<components['schemas']['TrustOverrideResponse']>;
export type ChainVerificationResult = CamelCaseKeys<components['schemas']['ChainVerificationResult']>;
export type ChainVerificationSchema = CamelCaseKeys<components['schemas']['ChainVerificationSchema']>;
export type TimeRange = CamelCaseKeys<components['schemas']['TimeRange']>;
export type TableCounts = CamelCaseKeys<components['schemas']['TableCounts']>;
export type BrokenLink = CamelCaseKeys<components['schemas']['BrokenLink']>;
export type CharRange = CamelCaseKeys<components['schemas']['CharRange']>;
export type BoundingBox = CamelCaseKeys<components['schemas']['BoundingBox']>;
export type FeatureVector = CamelCaseKeys<components['schemas']['FeatureVector']>;
export type StateOrigin = CamelCaseKeys<components['schemas']['StateOrigin']>;
export type DataLineageMode = CamelCaseKeys<components['schemas']['DataLineageMode']>;
export type WorkflowType = CamelCaseKeys<components['schemas']['WorkflowType']>;

// KV isolation
export type KvIsolationHealthResponse = CamelCaseKeys<components['schemas']['KvIsolationHealthResponse']>;
export type KvIsolationScanRequest = CamelCaseKeys<components['schemas']['KvIsolationScanRequest']>;

// Discovery stream
export type DiscoveryStreamQuery = CamelCaseKeys<components['schemas']['DiscoveryStreamQuery']>;
export type StreamQuery = CamelCaseKeys<components['schemas']['StreamQuery']>;

// ============================================================================
// Re-export toCamelCase transformer for convenience
// ============================================================================

export { toCamelCase, toSnakeCase } from './transformers';
