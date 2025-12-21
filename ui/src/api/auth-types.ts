// Authentication and user-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-12-19†security-critical†auth_types_migration】
//
// SECURITY REVIEW CHECKLIST:
// ✓ Password fields are only in LoginRequest (never logged or stored in frontend state)
// ✓ Token fields are security-sensitive (JWT access tokens, handle with care)
// ✓ admin_tenants wildcard ("*") ONLY works in debug builds with AOS_DEV_NO_AUTH=1
// ✓ UserRole enum matches backend exactly (7 roles)
// ✓ LoginRequest username field is optional (email is primary)
// ✓ LoginResponse does NOT include admin_tenants (only in UserInfoResponse and Claims)

import type { components } from './generated';

// =============================================================================
// MIGRATED FROM GENERATED (Core Auth Types)
// =============================================================================

/**
 * Login request
 * SECURITY: Password field is sensitive - never log or store unencrypted
 */
export type LoginRequest = components['schemas']['LoginRequest'];

/**
 * Login response with JWT token
 * SECURITY: Token field contains JWT access token - handle with care
 * NOTE: admin_tenants is NOT included here (only in UserInfoResponse)
 */
export type LoginResponse = components['schemas']['LoginResponse'] & {
  // UI-specific session fields
  session_mode?: string;
};

/**
 * User information response
 * SECURITY: admin_tenants field contains tenant access list
 * - Empty array = user can only access their own tenant
 * - ["*"] = wildcard admin access (DEBUG BUILDS ONLY with AOS_DEV_NO_AUTH=1)
 * - ["tenant-id-1", "tenant-id-2"] = specific tenant access for admins
 */
export type UserInfoResponse = components['schemas']['UserInfoResponse'];

/**
 * Minimal tenant summary for tenant picker
 */
export type TenantSummary = components['schemas']['TenantSummary'];

/**
 * Tenant list response (for /v1/auth/me endpoint)
 */
export interface TenantListResponse {
  schema_version: string;
  tenants: TenantSummary[];
}

/**
 * Switch tenant request
 * NOTE: Backend endpoint not yet implemented (type defined for future use)
 */
export interface SwitchTenantRequest {
  tenant_id: string;
}

/**
 * Switch tenant response (reuses LoginResponse shape)
 * NOTE: Backend endpoint not yet implemented (type defined for future use)
 */
export type SwitchTenantResponse = LoginResponse;

/**
 * Logout request (empty for now, extensible)
 * Backend does not accept request body for logout (POST /v1/auth/logout)
 */
export interface LogoutRequest {
  token?: string; // Frontend-only field for compatibility
}

/**
 * Token refresh response
 */
export interface RefreshResponse {
  token: string;
  expires_at: number;
}

// =============================================================================
// MANUAL TYPES (Frontend-Specific, Security, or UI-Only)
// =============================================================================

/**
 * Session mode (frontend state for dev bypass)
 * SECURITY: 'dev_bypass' mode ONLY works in debug builds with AOS_DEV_NO_AUTH=1
 * Production builds ignore this and always use 'normal' mode
 */
export type SessionMode = 'normal' | 'dev_bypass';

/**
 * Frontend wrapper for auth token (UI state management)
 */
export interface AuthToken {
  token: string;
  token_type: string;
  expires_in: number;
  user: User;
}

/**
 * Cursor IDE integration config response (UI-only)
 */
export interface CursorConfigResponse {
  cursor_id: string;
  config: Record<string, unknown>;
  created_at: string;
  is_ready?: boolean;
  model_name?: string;
  api_endpoint?: string;
  setup_instructions?: string;
}

/**
 * Cursor model info (UI-only)
 */
export interface CursorModelInfo {
  id: string;
  name: string;
  provider?: string;
  context_length?: number;
  capabilities?: string[];
}

/**
 * User role enum
 * SECURITY: Must match backend exactly (crates/adapteros-api-types/src/auth.rs)
 * Backend roles: Admin, Developer, Operator, Sre, Compliance, Auditor, Viewer
 */
export type UserRole = 'admin' | 'developer' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';

/**
 * User entity (UI compatibility wrapper with alias fields)
 */
export interface User {
  user_id?: string;
  id?: string;  // Alias for user_id for compatibility
  email: string;
  display_name?: string;
  name?: string;  // Alias for display_name (UI compatibility)
  role: UserRole;
  tenant_id?: string;
  created_at?: string;
  last_login?: string;
  last_login_at?: string;  // Alias for last_login
  mfa_enabled?: boolean;
  permissions?: string[];
  token_last_rotated_at?: string;
  admin_tenants?: string[]; // Tenant IDs the admin can manage; "*" wildcard is dev-only (debug / dev-bypass)
}

/**
 * Register user request (admin functionality)
 */
export interface RegisterUserRequest {
  email: string;
  password: string;
  display_name?: string;
  role?: UserRole;
  tenant_id?: string;
}

/**
 * Update user request
 */
export interface UpdateUserRequest {
  display_name?: string;
  role?: UserRole;
  is_active?: boolean;
}

/**
 * Change password request
 * SECURITY: Both passwords are sensitive - never log
 */
export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

/**
 * Reset password request
 */
export interface ResetPasswordRequest {
  email: string;
}

/**
 * User response wrapper
 */
export interface UserResponse {
  user: User;
}

/**
 * List users response (paginated)
 */
export interface ListUsersResponse {
  users: User[];
  total: number;
  page: number;
  page_size: number;
}

/**
 * Refresh token request
 */
export interface RefreshTokenRequest {
  refresh_token: string;
}

/**
 * Rotate token response
 */
export interface RotateTokenResponse {
  token: string;
  token_type: string;
  expires_in: number;
  created_at?: string;
  expires_at?: string;
  last_rotated_at?: string;
}

/**
 * Session information (for audit/management)
 */
export interface SessionInfo {
  id: string;
  device?: string;
  ip_address?: string;
  user_agent?: string;
  location?: string;
  created_at: string;
  last_seen_at: string;
  is_current: boolean;
}

/**
 * Token metadata (audit/tracking)
 */
export interface TokenMetadata {
  token_id?: string;
  user_id?: string;
  issued_at?: string;
  created_at?: string;
  expires_at?: string;
  last_used_at?: string;
  last_rotated_at?: string;
  device_info?: string;
  ip_address?: string;
}

/**
 * Update profile request
 */
export interface UpdateProfileRequest {
  display_name?: string;
  email?: string;
}

/**
 * Profile response
 */
export interface ProfileResponse {
  user_id: string;
  email: string;
  display_name?: string;
  role: UserRole;
  tenant_id?: string;
  created_at: string;
  updated_at: string;
  last_login_at?: string;
}

/**
 * Auth configuration response (public subset)
 */
export interface AuthConfigResponse {
  allow_registration: boolean;
  require_email_verification: boolean;
  access_token_ttl_minutes?: number;
  session_timeout_minutes: number;
  max_login_attempts: number;
  password_min_length: number;
  mfa_required: boolean;
  allowed_domains?: string[];
  production_mode?: boolean;
  dev_token_enabled?: boolean;
  /** Whether dev bypass is actually allowed (computed from config) */
  dev_bypass_allowed?: boolean;
  jwt_mode?: string;
  token_expiry_hours?: number;
}

// =============================================================================
// Contact/Message types (for support/feedback)
// =============================================================================

export interface Contact {
  id: string;
  name: string;
  email: string;
  type: 'support' | 'sales' | 'technical';
  category?: string;
  role?: string;
  last_interaction?: string;
  interaction_count?: number;
  discovered_at?: string;
}

export interface Message {
  id: string;
  from: string;
  to: string;
  subject: string;
  body: string;
  timestamp: string;
  created_at: string;
  read: boolean;
  thread_id?: string;
  attachments?: string[];
  read_at?: string;
  content?: string;
  from_user_id?: string;
  from_user_display_name?: string;
  edited_at?: string;
}

// =============================================================================
// Workspace types
// =============================================================================

export interface Workspace {
  id: string;
  name: string;
  description?: string;
  owner_id: string;
  members: WorkspaceMember[];
  created_at: string;
  settings?: Record<string, unknown>;
  is_default?: boolean;
  member_count?: number;
}

export interface WorkspaceMember {
  user_id: string;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  joined_at: string;
  user_display_name?: string;
  user_email?: string;
}

// =============================================================================
// Session types
// =============================================================================

export interface Session {
  session_id: string;
  user_id: string;
  created_at: string;
  expires_at: string;
  ip_address?: string;
  user_agent?: string;
  is_active?: boolean;
  device_info?: string;
}

// =============================================================================
// Activity tracking types
// =============================================================================

export interface ActivityEvent {
  id: string;
  type?: string;
  actor?: string;
  action?: string;
  target?: string;
  metadata?: Record<string, unknown>;
  timestamp: string;
  created_at?: string;
  workspace_id?: string;
  event_type?: string;
}

export interface RecentActivityEvent extends ActivityEvent {
  user_name?: string;
  resource_name?: string;
  user_id?: string;
  tenant_id?: string;
  message?: string;
  level?: string;
  component?: string;
}

// =============================================================================
// Workspace resource types
// =============================================================================

export interface WorkspaceResource {
  id: string;
  workspace_id: string;
  resource_type: string;
  resource_id: string;
  permissions: string[];
  resource_name?: string;
  shared_at?: string;
  shared_by?: string;
}

// =============================================================================
// Request types (Create/Add operations)
// =============================================================================

export interface CreateMessageRequest {
  to?: string;
  subject?: string;
  body?: string;
  thread_id?: string;
  content?: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  description?: string;
  settings?: Record<string, unknown>;
  tenant_id?: string;
}

export interface AddWorkspaceMemberRequest {
  workspace_id: string;
  user_id: string;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  tenant_id?: string;
}

export interface CreateActivityEventRequest {
  type?: string;
  actor?: string;
  action?: string;
  target?: string;
  target_id?: string;
  target_type?: string;
  metadata?: Record<string, unknown>;
  metadata_json?: string;
  event_type?: string;
}
