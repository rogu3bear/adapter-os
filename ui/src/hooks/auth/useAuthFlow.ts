/**
 * Auth Flow State Machine Hook
 *
 * Manages the complete login flow with explicit state transitions.
 * Eliminates scattered state by centralizing all auth flow logic.
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { useAuth } from '@/providers/CoreProviders';
import { apiClient } from '@/api/services';
import type { AuthConfigResponse } from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';
import { useHealthPolling, type UseHealthPollingReturn } from './useHealthPolling';
import {
  AUTH_STORAGE_KEYS,
  AUTH_ERROR_CODES,
  AUTH_DEFAULTS,
  getAuthErrorMessage,
} from '@/auth/constants';
import { isDevBypassEnabled } from '@/auth/authBootstrap';
import { consumeSessionExpiredFlag } from '@/auth/session';
import { getDemoEntryPath, isDemoMvpMode } from '@/config/demo';

/** Login credentials from the form */
export interface LoginCredentials {
  email: string;
  password: string;
  totp?: string;
}

/** Error information for auth flow */
export interface AuthFlowError {
  message: string;
  code?: string;
  isLockout?: boolean;
}

/** Extended error type for API errors */
interface ApiError extends Error {
  code?: string;
  status?: number;
  details?: Record<string, unknown>;
}

/** Type guard for API errors */
function isApiError(err: unknown): err is ApiError {
  return err instanceof Error && ('code' in err || 'status' in err);
}

/**
 * Auth flow states - discriminated union for type safety.
 * Each state contains only the data relevant to that state.
 */
export type AuthFlowState =
  | { status: 'checking_health' }
  | { status: 'health_error'; error: string }
  | { status: 'loading_config' }
  | { status: 'config_error'; error: string }
  | { status: 'ready'; config: AuthConfigResponse }
  | { status: 'authenticating'; config: AuthConfigResponse }
  | { status: 'mfa_required'; config: AuthConfigResponse; email: string }
  | { status: 'locked_out'; config: AuthConfigResponse; message: string }
  | { status: 'error'; config: AuthConfigResponse; error: AuthFlowError }
  | { status: 'success'; redirectPath: string };

export interface UseAuthFlowReturn {
  /** Current state of the auth flow */
  state: AuthFlowState;
  /** Health polling state and controls */
  health: UseHealthPollingReturn;
  /** Submit login credentials */
  login: (credentials: LoginCredentials) => Promise<void>;
  /** Trigger dev bypass login */
  devBypass: () => Promise<void>;
  /** Retry loading config after error */
  retryConfig: () => Promise<void>;
  /** Clear error and return to ready state */
  clearError: () => void;
  /** Whether MFA field should be shown */
  showMfaField: boolean;
  /** Whether form can be submitted */
  canSubmit: boolean;
  /** Whether dev bypass is available */
  devBypassAllowed: boolean;
  /** Number of failed login attempts */
  failedAttempts: number;
  /** Max allowed login attempts */
  maxAttempts: number;
}

/**
 * Hook that manages the complete auth flow as a state machine.
 *
 * State transitions:
 * - checking_health -> (health ready) -> loading_config
 * - loading_config -> ready | config_error
 * - ready -> authenticating (on login)
 * - authenticating -> success | mfa_required | locked_out | error
 * - mfa_required -> authenticating (on retry with TOTP)
 * - error -> ready (on clearError) | authenticating (on retry)
 */
export function useAuthFlow(): UseAuthFlowReturn {
  const { user, login: authLogin, devBypassLogin, sessionMode } = useAuth();
  const health = useHealthPolling();

  // Initialize with any session expired message
  const initialError = consumeSessionExpiredFlag();

  const [state, setState] = useState<AuthFlowState>({ status: 'checking_health' });
  const [failedAttempts, setFailedAttempts] = useState(0);
  const [showMfaField, setShowMfaField] = useState(false);
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);

  const configAbortRef = useRef<AbortController | null>(null);
  const isMountedRef = useRef(true);
  const devBypassFlagEnabled = isDevBypassEnabled();

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      isMountedRef.current = false;
      configAbortRef.current?.abort();
    };
  }, []);

  // Load auth config
  const loadConfig = useCallback(async () => {
    setState({ status: 'loading_config' });

    configAbortRef.current?.abort();
    const controller = new AbortController();
    configAbortRef.current = controller;

    try {
      const config = await apiClient.getAuthConfig(controller.signal);

      if (controller.signal.aborted || !isMountedRef.current) {
        return;
      }

      // Check if MFA is globally required
      if (config.mfa_required) {
        setShowMfaField(true);
      }

      logger.debug('Auth config loaded', {
        component: 'useAuthFlow',
        access_token_ttl_minutes: config.access_token_ttl_minutes,
        session_timeout_minutes: config.session_timeout_minutes,
        dev_bypass_allowed: config.dev_bypass_allowed,
      });

      // If there was a session expired error, show it
      if (initialError) {
        setState({
          status: 'error',
          config,
          error: { message: initialError, code: AUTH_ERROR_CODES.SESSION_EXPIRED },
        });
      } else {
        setState({ status: 'ready', config });
      }
    } catch (err) {
      if (configAbortRef.current?.signal.aborted || !isMountedRef.current) {
        return;
      }

      logger.warn('Auth config load failed', {
        component: 'useAuthFlow',
        operation: 'loadConfig',
      });

      setState({
        status: 'config_error',
        error: 'Unable to load sign-in settings. You can still try to sign in.',
      });
    } finally {
      if (configAbortRef.current === controller) {
        configAbortRef.current = null;
      }
    }
  }, [initialError]);

  // Transition: health ready -> load config
  useEffect(() => {
    if (health.isReady && state.status === 'checking_health') {
      loadConfig();
    } else if (!health.isReady && health.healthError && state.status === 'checking_health') {
      setState({ status: 'health_error', error: health.healthError });
    }
  }, [health.isReady, health.healthError, state.status, loadConfig]);

  // Update health error state if it changes
  useEffect(() => {
    if (state.status === 'health_error' && health.isReady) {
      loadConfig();
    }
  }, [health.isReady, state.status, loadConfig]);

  // Handle login
  const login = useCallback(
    async (credentials: LoginCredentials) => {
      // Get current config from state
      const currentConfig =
        state.status === 'ready' ||
        state.status === 'error' ||
        state.status === 'mfa_required' ||
        state.status === 'locked_out'
          ? state.config
          : null;

      if (!currentConfig) {
        logger.warn('Login attempted without config', { component: 'useAuthFlow' });
        return;
      }

      const maxAttempts = currentConfig.max_login_attempts ?? AUTH_DEFAULTS.MAX_LOGIN_ATTEMPTS;

      // Check lockout
      if (failedAttempts >= maxAttempts) {
        setState({
          status: 'locked_out',
          config: currentConfig,
          message:
            'Too many failed attempts. Account temporarily locked—please try again later or contact an administrator.',
        });
        return;
      }

      setState({ status: 'authenticating', config: currentConfig });
      setPendingEmail(credentials.email);

      try {
        logger.info('Initiating login', {
          component: 'useAuthFlow',
          operation: 'login',
          email: credentials.email,
        });

        const result = await authLogin({
          username: credentials.email, // Backend expects username field
          email: credentials.email,
          password: credentials.password,
          totp_code: credentials.totp,
        });

        if (!isMountedRef.current) return;

        // Reset failed attempts on success
        setFailedAttempts(0);
        setShowMfaField(false);

        // Determine redirect path
        const demoMode = isDemoMvpMode(sessionMode);
        let redirectPath = demoMode ? getDemoEntryPath() : '/dashboard';

        // Check for stored redirect
        try {
          const stored = sessionStorage.getItem(AUTH_STORAGE_KEYS.POST_LOGIN_REDIRECT);
          if (stored) {
            sessionStorage.removeItem(AUTH_STORAGE_KEYS.POST_LOGIN_REDIRECT);
            redirectPath = stored;
          }
        } catch {
          // Ignore storage errors
        }

        // Check for first-run admin redirect
        if (!demoMode && result.role?.toLowerCase() === 'admin') {
          try {
            const hasCompletedFirstRun = localStorage.getItem(AUTH_STORAGE_KEYS.FIRST_RUN);
            if (!hasCompletedFirstRun) {
              localStorage.setItem(AUTH_STORAGE_KEYS.FIRST_RUN, 'true');
              redirectPath = '/dashboard';
            }
          } catch {
            // Ignore storage errors
          }
        }

        logger.info('Login successful', {
          component: 'useAuthFlow',
          operation: 'login',
          user_id: result.user_id,
          tenant_id: result.tenant_id,
        });

        setState({ status: 'success', redirectPath });
      } catch (err) {
        if (!isMountedRef.current) return;

        const apiErr = isApiError(err) ? err : undefined;
        const errorCode = apiErr?.code;

        // Check for MFA required
        const mfaNeeded =
          errorCode === AUTH_ERROR_CODES.MFA_REQUIRED ||
          (apiErr?.details &&
            typeof apiErr.details === 'object' &&
            (apiErr.details as Record<string, unknown>).requires_mfa === true);

        if (mfaNeeded) {
          setShowMfaField(true);
          setState({
            status: 'mfa_required',
            config: currentConfig,
            email: credentials.email,
          });
          return;
        }

        // Handle lockout from server
        if (errorCode === AUTH_ERROR_CODES.ACCOUNT_LOCKED) {
          setFailedAttempts(maxAttempts);
          setState({
            status: 'locked_out',
            config: currentConfig,
            message: getAuthErrorMessage(errorCode),
          });
          return;
        }

        // Increment failed attempts
        const newAttempts = failedAttempts + 1;
        setFailedAttempts(newAttempts);

        // Check if now locked out
        if (newAttempts >= maxAttempts) {
          setState({
            status: 'locked_out',
            config: currentConfig,
            message:
              'Too many failed attempts. Account temporarily locked—please try again later or contact an administrator.',
          });
          return;
        }

        // Regular error
        const errorMessage = getAuthErrorMessage(
          errorCode,
          err instanceof Error ? err.message : 'Login failed'
        );

        logger.error('Login failed', {
          component: 'useAuthFlow',
          operation: 'login',
          errorCode,
          status: apiErr?.status,
        }, toError(err));

        setState({
          status: 'error',
          config: currentConfig,
          error: { message: errorMessage, code: errorCode },
        });
      }
    },
    [state, authLogin, failedAttempts, sessionMode]
  );

  // Handle dev bypass
  const devBypass = useCallback(async () => {
    const currentConfig =
      state.status === 'ready' || state.status === 'error' ? state.config : null;

    if (!currentConfig) {
      logger.warn('Dev bypass attempted without config', { component: 'useAuthFlow' });
      return;
    }

    setState({ status: 'authenticating', config: currentConfig });

    try {
      logger.info('Initiating dev bypass login', {
        component: 'useAuthFlow',
        operation: 'devBypass',
      });

      await devBypassLogin();

      if (!isMountedRef.current) return;

      const demoMode = isDemoMvpMode(sessionMode);
      const redirectPath = demoMode ? getDemoEntryPath() : '/dashboard';

      setState({ status: 'success', redirectPath });
    } catch (err) {
      if (!isMountedRef.current) return;

      const apiErr = isApiError(err) ? err : undefined;
      const errorMessage = apiErr?.message || (err instanceof Error ? err.message : 'Dev bypass failed');

      logger.error('Dev bypass failed', {
        component: 'useAuthFlow',
        operation: 'devBypass',
        errorCode: apiErr?.code,
        status: apiErr?.status,
      }, toError(err));

      setState({
        status: 'error',
        config: currentConfig,
        error: { message: errorMessage, code: apiErr?.code },
      });
    }
  }, [state, devBypassLogin, sessionMode]);

  // Retry loading config
  const retryConfig = useCallback(async () => {
    await loadConfig();
  }, [loadConfig]);

  // Clear error and return to ready
  const clearError = useCallback(() => {
    if (state.status === 'error' || state.status === 'locked_out') {
      setState({ status: 'ready', config: state.config });
    }
  }, [state]);

  // Compute derived values
  const currentConfig =
    state.status === 'ready' ||
    state.status === 'authenticating' ||
    state.status === 'mfa_required' ||
    state.status === 'locked_out' ||
    state.status === 'error'
      ? state.config
      : null;

  const maxAttempts = currentConfig?.max_login_attempts ?? AUTH_DEFAULTS.MAX_LOGIN_ATTEMPTS;

  const devBypassAllowed =
    devBypassFlagEnabled && (currentConfig?.dev_bypass_allowed ?? false);

  const canSubmit =
    (state.status === 'ready' ||
      state.status === 'error' ||
      state.status === 'mfa_required') &&
    failedAttempts < maxAttempts;

  // Handle already authenticated user
  useEffect(() => {
    if (user && state.status !== 'success' && state.status !== 'authenticating') {
      const demoMode = isDemoMvpMode(sessionMode);
      setState({
        status: 'success',
        redirectPath: demoMode ? getDemoEntryPath() : '/dashboard',
      });
    }
  }, [user, state.status, sessionMode]);

  return {
    state,
    health,
    login,
    devBypass,
    retryConfig,
    clearError,
    showMfaField,
    canSubmit,
    devBypassAllowed,
    failedAttempts,
    maxAttempts,
  };
}
