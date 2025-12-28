/**
 * Centralized Auth Constants
 *
 * Single source of truth for all auth-related storage keys, events, and error codes.
 * Import from this file instead of defining inline constants.
 */

/**
 * Storage keys for auth-related data.
 * Session storage keys are cleared when tab closes.
 * Local storage keys persist across sessions.
 */
export const AUTH_STORAGE_KEYS = {
  // Session storage (cleared on tab close)
  /** Cached tenant selection with user validation: { tenantId: string, userId: string } */
  SELECTED_TENANT: 'selectedTenant',
  /** Cached list of available tenants after login */
  TENANT_BOOTSTRAP: 'aos-tenant-bootstrap',
  /** Marks an active auth session exists */
  AUTH_SESSION: 'aos-auth-active',
  /** Flags that session expired (consumed on next login page load) */
  SESSION_EXPIRED: 'aos-session-expired',
  /** Indicates multi-tenant choice is needed */
  TENANT_SELECTION_REQUIRED: 'aos-tenant-selection-required',
  /** Deep link to restore after login */
  POST_LOGIN_REDIRECT: 'postLoginRedirect',

  // Local storage (persists across sessions)
  /** Device UUID for session tracking */
  DEVICE_ID: 'aos-device-id',
  /** Marks first admin login completed (skips onboarding) */
  FIRST_RUN: 'aos-first-login-completed',
  /** Timestamp when dev bypass was activated (for timeout checking) */
  DEV_BYPASS_ACTIVATED_AT: 'aos-dev-bypass-activated-at',
} as const;

/** Type for storage key values */
export type AuthStorageKey = (typeof AUTH_STORAGE_KEYS)[keyof typeof AUTH_STORAGE_KEYS];

/**
 * Custom events dispatched for auth state changes.
 * Listen with window.addEventListener(AUTH_EVENTS.SESSION_EXPIRED, handler).
 */
export const AUTH_EVENTS = {
  /** Dispatched when session expires (triggers redirect to login) */
  SESSION_EXPIRED: 'aos:session-expired',
} as const;

/** Type for event name values */
export type AuthEvent = (typeof AUTH_EVENTS)[keyof typeof AUTH_EVENTS];

/**
 * API error codes returned by auth endpoints.
 * Use for error handling and user-friendly message mapping.
 */
export const AUTH_ERROR_CODES = {
  /** Account locked due to too many failed attempts */
  ACCOUNT_LOCKED: 'ACCOUNT_LOCKED',
  /** Account disabled by administrator */
  ACCOUNT_DISABLED: 'ACCOUNT_DISABLED',
  /** Wrong email or password */
  INVALID_CREDENTIALS: 'INVALID_CREDENTIALS',
  /** MFA code required to complete login */
  MFA_REQUIRED: 'MFA_REQUIRED',
  /** Session expired, re-authentication needed */
  SESSION_EXPIRED: 'SESSION_EXPIRED',
  /** User has no access to requested tenant */
  TENANT_ACCESS_DENIED: 'TENANT_ACCESS_DENIED',
  /** Tenant isolation violation */
  TENANT_ISOLATION_ERROR: 'TENANT_ISOLATION_ERROR',
  /** User authenticated but has no tenant access */
  NO_TENANT_ACCESS: 'NO_TENANT_ACCESS',
} as const;

/** Type for error code values */
export type AuthErrorCode = (typeof AUTH_ERROR_CODES)[keyof typeof AUTH_ERROR_CODES];

/**
 * Map error codes to user-friendly messages.
 */
export const AUTH_ERROR_MESSAGES: Record<AuthErrorCode, string> = {
  [AUTH_ERROR_CODES.ACCOUNT_LOCKED]:
    'Your account is locked. Try again later or contact an administrator.',
  [AUTH_ERROR_CODES.ACCOUNT_DISABLED]:
    'Your account is disabled. Contact an administrator.',
  [AUTH_ERROR_CODES.INVALID_CREDENTIALS]: 'Invalid email or password.',
  [AUTH_ERROR_CODES.MFA_REQUIRED]:
    'Multi-factor authentication required. Enter your TOTP code.',
  [AUTH_ERROR_CODES.SESSION_EXPIRED]: 'Session expired. Please log in again.',
  [AUTH_ERROR_CODES.TENANT_ACCESS_DENIED]:
    'You have no role in this workspace. Request access from an admin.',
  [AUTH_ERROR_CODES.TENANT_ISOLATION_ERROR]:
    'You have no role in this workspace. Request access from an admin.',
  [AUTH_ERROR_CODES.NO_TENANT_ACCESS]:
    "You're signed in but have no workspace access. Ask an admin to grant access.",
};

/**
 * Get user-friendly error message for an auth error code.
 * Falls back to provided fallback or generic message.
 */
export function getAuthErrorMessage(
  code: string | undefined,
  fallback = 'Login failed'
): string {
  if (!code) return fallback;
  return AUTH_ERROR_MESSAGES[code as AuthErrorCode] ?? fallback;
}

/**
 * Default auth configuration values.
 */
export const AUTH_DEFAULTS = {
  /** Maximum login attempts before lockout */
  MAX_LOGIN_ATTEMPTS: 5,
  /** Health check polling interval when healthy (ms) */
  HEALTH_POLL_INTERVAL_READY: 10000,
  /** Health check polling interval when degraded (ms) */
  HEALTH_POLL_INTERVAL_DEGRADED: 2500,
  /** Health check request timeout (ms) */
  HEALTH_CHECK_TIMEOUT: 10000,
  /** Auth bootstrap timeout (ms) */
  AUTH_BOOTSTRAP_TIMEOUT: 30000,
  /** Dev bypass session timeout (1 hour in milliseconds) */
  DEV_BYPASS_TIMEOUT_MS: 60 * 60 * 1000,
} as const;
