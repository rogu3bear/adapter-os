/**
 * API Types Barrel Export
 *
 * This file re-exports all API types for convenient importing.
 * For Zod validation schemas, import from '@/api/schemas' directly.
 * Prefer importing from specific files for better tree-shaking.
 */

// Generated types (auto-generated, DO NOT EDIT generated.ts directly)
export type { components, paths, operations } from './generated';

// Transformers
export { toCamelCase, toSnakeCase } from './transformers';
export type { CamelCaseKeys, SnakeCaseKeys, SnakeToCamel, CamelToSnake } from './transformers';

// ============================================================================
// Type files - ordered to handle conflicts (later exports win)
// ============================================================================

// Foundation types (no conflicts)
export * from './federation-types';
export * from './plugin-types';
export * from './lineage-types';
export * from './replay-types';
export * from './pilot-status-types';
export * from './activityEventTypes';

// Document types (foundation for evidence types)
export * from './document-types';

// Training types (re-exports EvidenceType from document-types)
export * from './training-types';

// Chat types
export * from './chat-types';

// Auth types
export * from './auth-types';

// Streaming types (unique event types)
export * from './streaming-types';

// Owner types (has ServiceHealthStatus which may conflict)
export * from './owner-types';

// System state types - export explicitly to avoid conflicts
// AdapterSummary, AdapterLifecycleState, etc. also in api-types/adapter-types
export type {
  StateOrigin,
  ServiceState,
  MemoryPressureLevel,
  AneMemoryState,
  MemoryState,
  SystemStateResponse,
  SystemStateQuery,
} from './system-state-types';

// Policy types - export explicitly to avoid PolicyCheck/PolicyPreflightResponse conflicts
export type {
  PolicyStatus,
  PolicyCategory,
  PolicySeverity,
  PolicyCheckDetails,
  PolicyCheckRequest,
  PolicyCheckResponse,
  PolicyCheckSummary,
  PolicyOverrideRequest,
  PolicyOverrideResponse,
  DryRunPromotionWithPoliciesRequest,
  DryRunPromotionWithPoliciesResponse,
  PromotionGateResult,
  PolicyPreflightRequest,
} from './policyTypes';

// Repo types - export explicitly to avoid CoreMLMode conflict with api-types
export type {
  RepoStatus,
  RepoBranchSummary,
  RepoSummary,
  RepoDetail,
  ReleaseState,
  RepoVersionSummary,
  RepoVersionDetail,
  RepoTimelineEventType,
  RepoTimelineEvent,
  RepoTrainingJobLink,
  CreateRepoRequest,
  UpdateRepoRequest,
  PromoteVersionRequest,
  RollbackVersionRequest,
  TagVersionRequest,
  StartTrainingFromVersionRequest,
  RepoAssuranceTier,
  AdapterRepositoryPolicy,
  UpdateAdapterRepositoryPolicyRequest,
} from './repo-types';

// API types (canonical source for inference, tenant, node types)
export * from './api-types';

// Adapter types (canonical source for adapter types)
// These take precedence over policyTypes for PolicyCheck, PolicyPreflightResponse
export * from './adapter-types';

// ============================================================================
// API utilities
// ============================================================================
export * from './helpers';
export * from './status';
export * from './queryInvalidation';
export * from './queryOptions';

// ============================================================================
// API services (domain-organized clients)
// ============================================================================
export * from './services';

// ============================================================================
// Note: Zod validation schemas are NOT re-exported here.
// Import from '@/api/schemas' directly if needed.
// This avoids type conflicts between Zod-inferred types and hand-written types.
// ============================================================================
