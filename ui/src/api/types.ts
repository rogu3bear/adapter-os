/**
 * API Types Aggregate
 *
 * This is the canonical import point for all API types in the AdapterOS UI.
 * 【2025-11-19†types†modularization】
 *
 * # Re-export Strategy
 *
 * This file re-exports all types from modular files to provide:
 *
 * 1. **Single entry point**: Consumers import from `@/api/types` only
 * 2. **Internal organization**: Types are logically organized in sub-modules:
 *    - `auth-types.ts`: Authentication, user, session, workspace types
 *    - `adapter-types.ts`: Adapter lifecycle, stacks, manifests, policies
 *    - `training-types.ts`: Training jobs, datasets, configurations
 *    - `api-types.ts`: Request/response wrappers, error types
 * 3. **Maintenance**: Developers can modify sub-modules without updating imports
 *
 * # Import Guidelines
 *
 * Always import from this file:
 * ```tsx
 * import type { Adapter, User, TrainingJob } from '@/api/types';
 * ```
 *
 * Never import from sub-modules directly:
 * ```tsx
 * // ❌ DO NOT DO THIS:
 * import type { Adapter } from '@/api/adapter-types';
 * import type { User } from '@/api/auth-types';
 * ```
 *
 * # Sub-module Coupling
 *
 * This creates multiple paths to the same types:
 * - `User` (and 30+ other types from `auth-types`)
 * - `Adapter` (and 60+ other types from `adapter-types`)
 * - `TrainingJob` (and 10+ other types from `training-types`)
 * - Error types and wrappers from `api-types`
 *
 * All re-exported types are publicly available at this namespace to avoid
 * confusion from mixed import sources.
 */

// Re-export all types from modular files
export * from '@/api/auth-types';
export * from '@/api/adapter-types';
export * from '@/api/training-types';
export * from '@/api/api-types';
export * from '@/api/federation-types';
export * from '@/api/plugin-types';
export * from '@/api/streaming-types';
export * from '@/api/owner-types';
export * from '@/api/lineage-types';
