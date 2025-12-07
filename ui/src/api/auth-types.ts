// Authentication and user-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†auth_types】

export interface LoginRequest {
  username: string;  // Required by backend
  email: string;
  password: string;
  totp_code?: string;
  device_id?: string;
}

export interface LoginResponse {
  schema_version: string;  // Required by backend API
  token: string;
  user_id: string;
  tenant_id: string;  // Required by backend (not optional)
  role: string;
  expires_in: number;  // Changed from expires_at to expires_in (seconds)
  tenants?: TenantSummary[];
  mfa_level?: string;
  admin_tenants?: string[]; // Tenant IDs the admin can manage; "*" wildcard is dev-only (debug / dev-bypass)
}

export interface RefreshResponse {
  token: string;
  expires_at: number;
}

export interface TenantSummary {
  schema_version: string;
  id: string;
  name: string;
  status?: string | null;
  created_at?: string | null;
}

export interface TenantListResponse {
  schema_version: string;
  tenants: TenantSummary[];
}

export interface SwitchTenantRequest {
  tenant_id: string;
}

export type SwitchTenantResponse = LoginResponse;

export interface UserInfoResponse {
  schema_version: string;  // Required by backend API
  user_id: string;
  email: string;
  role: string;
  created_at: string;  // Required by backend (not optional)
  display_name?: string;
  tenant_id?: string;
  last_login_at?: string;
  mfa_enabled?: boolean;
  permissions?: string[];
  token_last_rotated_at?: string;
  admin_tenants?: string[]; // Tenant IDs the admin can manage; "*" wildcard is dev-only (debug / dev-bypass)
}

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

export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';

export interface RegisterUserRequest {
  email: string;
  password: string;
  display_name?: string;
  role?: UserRole;
  tenant_id?: string;
}

export interface UpdateUserRequest {
  display_name?: string;
  role?: UserRole;
  is_active?: boolean;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

export interface ResetPasswordRequest {
  email: string;
}

export interface UserResponse {
  user: User;
}

export interface ListUsersResponse {
  users: User[];
  total: number;
  page: number;
  page_size: number;
}

export interface AuthToken {
  token: string;
  token_type: string;
  expires_in: number;
  user: User;
}

export interface RefreshTokenRequest {
  refresh_token: string;
}

export interface RotateTokenResponse {
  token: string;
  token_type: string;
  expires_in: number;
  created_at?: string;
  expires_at?: string;
  last_rotated_at?: string;
}

export interface LogoutRequest {
  token?: string;
}

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

export interface UpdateProfileRequest {
  display_name?: string;
  email?: string;
}

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

// Cursor/Config types
export interface CursorConfigResponse {
  cursor_id: string;
  config: Record<string, unknown>;
  created_at: string;
  is_ready?: boolean;
  model_name?: string;
  api_endpoint?: string;
  setup_instructions?: string;
}

export interface CursorModelInfo {
  id: string;
  name: string;
  provider?: string;
  context_length?: number;
  capabilities?: string[];
}

// Contact/Message types (for support/feedback)
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

// Workspace types
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

// Session types
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

// Activity tracking types
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

// Workspace resource types
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

export interface RecentActivityEvent extends ActivityEvent {
  user_name?: string;
  resource_name?: string;
  user_id?: string;
  tenant_id?: string;
  message?: string;
  level?: string;
  component?: string;
}

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
