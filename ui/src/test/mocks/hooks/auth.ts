/**
 * Auth Hook Mock Factories
 *
 * Factory functions for creating mock useAuth, useAuthFlow, and useHealthPolling return values.
 */

import { vi, type Mock } from 'vitest';
import type { User, SessionMode, LoginRequest, LoginResponse } from '@/api/types';
import type { HealthResponse, ComponentHealth } from '@/api/api-types';
import type { AuthFlowState, UseAuthFlowReturn, LoginCredentials } from '@/hooks/auth/useAuthFlow';
import type { UseHealthPollingReturn } from '@/hooks/auth/useHealthPolling';
import { createMockUser } from '@/test/mocks/data/auth';

/**
 * Return type for useAuth hook mock
 */
export interface UseAuthMockReturn {
  user: User | null;
  isLoading: boolean;
  authError: Error | null;
  accessToken: string | null;
  sessionMode: SessionMode;
  login: Mock<(req: LoginRequest) => Promise<LoginResponse>>;
  devBypassLogin: Mock<() => Promise<LoginResponse>>;
  logout: Mock<() => Promise<void>>;
  refreshUser: Mock<() => Promise<void>>;
  refreshSession: Mock<() => Promise<void>>;
  logoutAllSessions: Mock<() => Promise<void>>;
  updateProfile: Mock<(data: { display_name?: string; avatar_url?: string }) => Promise<void>>;
  clearAuthError: Mock<() => void>;
}

/**
 * Options for createUseAuthMock factory
 */
export interface UseAuthMockOptions {
  /** User object or null for unauthenticated state. Pass Partial<User> to override specific fields. */
  user?: Partial<User> | null;
  /** Loading state (default: false) */
  isLoading?: boolean;
  /** Auth error (default: null) */
  authError?: Error | null;
  /** Access token (default: 'mock-token') */
  accessToken?: string | null;
  /** Session mode (default: 'normal') */
  sessionMode?: SessionMode;
}

/**
 * Create a mock return value for useAuth hook
 *
 * @example
 * ```typescript
 * // Default authenticated admin user
 * const auth = createUseAuthMock();
 *
 * // Unauthenticated state
 * const unauth = createUseAuthMock({ user: null, accessToken: null });
 *
 * // Loading state
 * const loading = createUseAuthMock({ isLoading: true, user: null });
 *
 * // Viewer role
 * const viewer = createUseAuthMock({ user: { role: 'viewer' } });
 *
 * // Custom tenant
 * const tenant = createUseAuthMock({ user: { tenant_id: 'my-tenant' } });
 * ```
 */
export function createUseAuthMock(options: UseAuthMockOptions = {}): UseAuthMockReturn {
  const user = options.user === null ? null : createMockUser(options.user ?? {});

  const mockLoginResponse: LoginResponse = {
    schema_version: '1.0',
    token: 'mock-token',
    user_id: user?.id ?? 'user-1',
    tenant_id: user?.tenant_id ?? 'test-tenant',
    role: user?.role ?? 'admin',
    expires_in: 3600,
  };

  return {
    user,
    isLoading: options.isLoading ?? false,
    authError: options.authError ?? null,
    accessToken: options.accessToken ?? (user ? 'mock-token' : null),
    sessionMode: options.sessionMode ?? 'normal',
    login: vi.fn().mockResolvedValue(mockLoginResponse),
    devBypassLogin: vi.fn().mockResolvedValue(mockLoginResponse),
    logout: vi.fn().mockResolvedValue(undefined),
    refreshUser: vi.fn().mockResolvedValue(undefined),
    refreshSession: vi.fn().mockResolvedValue(undefined),
    logoutAllSessions: vi.fn().mockResolvedValue(undefined),
    updateProfile: vi.fn().mockResolvedValue(undefined),
    clearAuthError: vi.fn(),
  };
}

/**
 * Options for createUseHealthPollingMock factory
 */
export interface UseHealthPollingMockOptions {
  /** Backend status (default: 'ready') - matches BackendStatus type */
  backendStatus?: 'checking' | 'ready' | 'issue';
  /** Whether system is ready for login (default: true) */
  isReady?: boolean;
  /** Health error message (default: null) */
  healthError?: string | null;
  /** Health data (default: healthy status) - matches HealthResponse type */
  health?: HealthResponse | null;
  /** All components (default: empty object) - matches ComponentHealth type */
  allComponents?: Record<string, ComponentHealth>;
  /** Issue components (default: empty array) */
  issueComponents?: Array<{ name: string; status: 'healthy' | 'degraded' | 'unhealthy'; message?: string }>;
  /** Last updated timestamp (default: 'just now') */
  lastUpdated?: string | null;
}

/**
 * Create a mock return value for useHealthPolling hook
 *
 * @example
 * ```typescript
 * // Default healthy state
 * const health = createUseHealthPollingMock();
 *
 * // Issue state (backend has problems)
 * const issue = createUseHealthPollingMock({
 *   backendStatus: 'issue',
 *   isReady: false,
 *   healthError: 'Connection failed',
 * });
 *
 * // Degraded health with issues
 * const degraded = createUseHealthPollingMock({
 *   backendStatus: 'issue',
 *   issueComponents: [{ name: 'worker', status: 'degraded', message: 'High load' }],
 * });
 * ```
 */
export function createUseHealthPollingMock(
  options: UseHealthPollingMockOptions = {}
): UseHealthPollingReturn {
  const defaultHealth: HealthResponse = {
    status: 'healthy',
    version: '1.0.0',
    schema_version: '1.0',
  };

  const health = options.health ?? defaultHealth;

  const systemHealth = {
    schema_version: '1.0',
    status: (health?.status ?? 'healthy') as 'healthy' | 'degraded' | 'unhealthy',
    version: '1.0.0',
    uptime_seconds: 3600,
    timestamp: new Date().toISOString(),
    components: options.allComponents ?? {},
  };

  return {
    backendStatus: options.backendStatus ?? 'ready',
    isReady: options.isReady ?? true,
    healthError: options.healthError ?? null,
    health,
    systemHealth,
    allComponents: options.allComponents ?? {},
    issueComponents: options.issueComponents ?? [],
    lastUpdated: options.lastUpdated ?? 'just now',
    refresh: vi.fn().mockResolvedValue(undefined),
  };
}

/**
 * Options for createUseAuthFlowMock factory
 */
export interface UseAuthFlowMockOptions {
  /** Auth flow state (default: ready) */
  state?: AuthFlowState;
  /** Health polling state (uses createUseHealthPollingMock) */
  healthOptions?: UseHealthPollingMockOptions;
  /** Whether MFA field should be shown (default: false) */
  showMfaField?: boolean;
  /** Whether form can be submitted (default: true) */
  canSubmit?: boolean;
  /** Whether dev bypass is available (default: false) */
  devBypassAllowed?: boolean;
  /** Number of failed login attempts (default: 0) */
  failedAttempts?: number;
  /** Max allowed login attempts (default: 5) */
  maxAttempts?: number;
}

/**
 * Create a mock return value for useAuthFlow hook
 *
 * @example
 * ```typescript
 * // Default ready state
 * const auth = createUseAuthFlowMock();
 *
 * // Authenticating state
 * const loading = createUseAuthFlowMock({
 *   state: { status: 'authenticating', config: mockConfig },
 *   canSubmit: false,
 * });
 *
 * // Error state
 * const error = createUseAuthFlowMock({
 *   state: { status: 'error', config: mockConfig, error: { message: 'Invalid credentials' } },
 *   failedAttempts: 1,
 * });
 *
 * // MFA required
 * const mfa = createUseAuthFlowMock({
 *   state: { status: 'mfa_required', config: mockConfig, email: 'test@example.com' },
 *   showMfaField: true,
 * });
 * ```
 */
export function createUseAuthFlowMock(options: UseAuthFlowMockOptions = {}): UseAuthFlowReturn {
  const mockConfig = {
    allow_registration: false,
    require_email_verification: false,
    mfa_required: false,
    max_login_attempts: 5,
    password_min_length: 8,
    access_token_ttl_minutes: 60,
    session_timeout_minutes: 1440,
    dev_bypass_allowed: true,
  };

  const defaultState: AuthFlowState = { status: 'ready', config: mockConfig };

  return {
    state: options.state ?? defaultState,
    health: createUseHealthPollingMock(options.healthOptions),
    login: vi.fn().mockResolvedValue(undefined),
    devBypass: vi.fn().mockResolvedValue(undefined),
    retryConfig: vi.fn().mockResolvedValue(undefined),
    clearError: vi.fn(),
    showMfaField: options.showMfaField ?? false,
    canSubmit: options.canSubmit ?? true,
    devBypassAllowed: options.devBypassAllowed ?? false,
    failedAttempts: options.failedAttempts ?? 0,
    maxAttempts: options.maxAttempts ?? 5,
  };
}
