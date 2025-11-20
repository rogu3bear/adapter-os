// Authentication and user-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†auth_types】

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user_id: string;
  role: string;
  email?: string;
  display_name?: string;
  tenant_id?: string;
  expires_at?: string;
}

export interface UserInfoResponse {
  user_id: string;
  email: string;
  role: string;
  display_name?: string;
  tenant_id?: string;
  created_at?: string;
  last_login_at?: string;
}

export interface User {
  user_id: string;
  email: string;
  role: UserRole;
  display_name?: string;
  tenant_id?: string;
  created_at: string;
  updated_at: string;
  last_login_at?: string;
  is_active: boolean;
}

export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'viewer';

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
  token_id: string;
  user_id: string;
  issued_at: string;
  expires_at: string;
  last_used_at?: string;
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
  session_timeout_minutes: number;
  max_login_attempts: number;
  password_min_length: number;
  mfa_required: boolean;
  allowed_domains?: string[];
}
