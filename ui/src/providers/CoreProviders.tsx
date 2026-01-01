import React, { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react';
import { apiClient } from '@/api/services';
import type { User } from '@/api/types';
import type { LoginRequest, LoginResponse, SessionMode } from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';
import { ThemeProvider as AosThemeProvider } from '@/theme/ThemeProvider';
import {
  isDevBypassEnabled,
  tryDevBypassLogin,
  markDevBypassActivated,
  clearDevBypassTimestamp,
  isDevBypassExpired,
  getDevBypassRemainingMs,
} from '@/auth/authBootstrap';
import { clearSessionExpiredFlag, markSessionExpired } from '@/auth/session';
import { logAuthEvent } from '@/lib/logUIError';
import { AUTH_STORAGE_KEYS } from '@/auth/constants';
import { workspaceIdFromTenantId, type WorkspaceId } from '@/types/workspace';

// Re-export for backward compatibility
export const SESSION_EXPIRED_FLAG_KEY = AUTH_STORAGE_KEYS.SESSION_EXPIRED;
export const TENANT_SELECTION_REQUIRED_KEY = AUTH_STORAGE_KEYS.TENANT_SELECTION_REQUIRED;

// Session-scoped tenant selection with user validation
interface TenantSelection {
  tenantId: WorkspaceId;
  userId: string;
}

function ensureDeviceId(): string {
  try {
    const existing = localStorage.getItem(AUTH_STORAGE_KEYS.DEVICE_ID);
    if (existing) return existing;
    const generated =
      typeof crypto !== 'undefined' && crypto.randomUUID
        ? crypto.randomUUID()
        : `device-${Date.now()}`;
    localStorage.setItem(AUTH_STORAGE_KEYS.DEVICE_ID, generated);
    return generated;
  } catch {
    return 'device-unknown';
  }
}

// Auth Context
interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  authError: Error | null;
  authTimeout: boolean;
  accessToken: string | null;
  sessionMode: SessionMode;
  /** Whether dev bypass mode is currently active (session mode is dev_bypass and not expired) */
  isDevBypassActive: boolean;
  /** Remaining time in ms before dev bypass expires (0 if not active) */
  devBypassRemainingMs: number;
  login: (credentials: LoginRequest) => Promise<LoginResponse>;
  devBypassLogin: () => Promise<LoginResponse>;
  logout: () => Promise<void>;
  refreshUser: () => Promise<void>;
  refreshSession: () => Promise<void>;
  logoutAllSessions: () => Promise<void>;
  updateProfile: (updates: { display_name?: string; avatar_url?: string }) => Promise<void>;
  clearAuthError: () => void;
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within CoreProviders');
  }
  return context;
}

// Resize Context
interface ResizeContextValue {
  getLayout: (storageKey: string) => number[] | null;
  setLayout: (storageKey: string, sizes: number[]) => void;
}

const ResizeContext = createContext<ResizeContextValue | undefined>(undefined);

export function useResize(): ResizeContextValue {
  const context = useContext(ResizeContext);
  if (!context) {
    throw new Error('useResize must be used within CoreProviders');
  }
  return context;
}

// RequireAuth Component
interface RequireAuthProps {
  children: ReactNode;
  fallback?: ReactNode;
}

export function RequireAuth({ children, fallback }: RequireAuthProps) {
  const { user, isLoading } = useAuth();

  if (isLoading) {
    return fallback || <div>Loading...</div>;
  }

  if (!user) {
    return fallback || <div>Authentication required</div>;
  }

  return <>{children}</>;
}

// Auth Provider Component
function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [authError, setAuthError] = useState<Error | null>(null);
  const [authTimeout, setAuthTimeout] = useState(false);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [sessionMode, setSessionMode] = useState<SessionMode>('normal');
  const [devBypassRemainingMs, setDevBypassRemainingMs] = useState(0);
  const isRefreshingRef = useRef(false);
  const devBypassTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const clearAuthError = useCallback(() => {
    setAuthError(null);
  }, []);

  // Start the dev bypass countdown timer
  const startDevBypassTimer = useCallback(() => {
    // Clear any existing timer
    if (devBypassTimerRef.current) {
      clearInterval(devBypassTimerRef.current);
      devBypassTimerRef.current = null;
    }

    // Update immediately
    setDevBypassRemainingMs(getDevBypassRemainingMs());

    // Update every second
    devBypassTimerRef.current = setInterval(() => {
      const remaining = getDevBypassRemainingMs();
      setDevBypassRemainingMs(remaining);

      // If expired, trigger logout
      if (remaining <= 0) {
        logger.info('Dev bypass session expired after 1 hour timeout', { component: 'AuthProvider' });
        if (devBypassTimerRef.current) {
          clearInterval(devBypassTimerRef.current);
          devBypassTimerRef.current = null;
        }
        // Clear the user state to force re-authentication
        setUser(null);
        setSessionMode('normal');
        setAccessToken(null);
        clearDevBypassTimestamp();
        markSessionExpired();
        try {
          sessionStorage.removeItem(AUTH_STORAGE_KEYS.AUTH_SESSION);
        } catch {
          // ignore storage errors
        }
      }
    }, 1000);
  }, []);

  // Stop the dev bypass timer
  const stopDevBypassTimer = useCallback(() => {
    if (devBypassTimerRef.current) {
      clearInterval(devBypassTimerRef.current);
      devBypassTimerRef.current = null;
    }
    setDevBypassRemainingMs(0);
  }, []);

  const refreshUser = useCallback(async () => {
    // Prevent concurrent refresh calls
    if (isRefreshingRef.current) {
      return;
    }

    isRefreshingRef.current = true;
    try {
      const userInfo = await apiClient.getCurrentUser();
      setUser({
        id: userInfo.user_id,
        email: userInfo.email,
        display_name: userInfo.display_name || userInfo.email,
        role: userInfo.role as User['role'],
        tenant_id: workspaceIdFromTenantId(userInfo.tenant_id),
        permissions: userInfo.permissions || [],
        last_login_at: userInfo.last_login_at ?? undefined,
        mfa_enabled: userInfo.mfa_enabled ?? undefined,
        token_last_rotated_at: userInfo.token_last_rotated_at ?? undefined,
        admin_tenants: userInfo.admin_tenants,
      });
      setAuthError(null);
      try {
        sessionStorage.setItem(AUTH_STORAGE_KEYS.AUTH_SESSION, 'true');
        clearSessionExpiredFlag();
      } catch {
        // best-effort session bookkeeping
      }
    } catch (error) {
      setUser(null);
      setSessionMode('normal');
      const err = toError(error);
      setAuthError(err);
      logger.error('Failed to fetch user', { component: 'AuthProvider' }, err);
      try {
        const hadSession = sessionStorage.getItem(AUTH_STORAGE_KEYS.AUTH_SESSION) === 'true';
        if (hadSession) {
          markSessionExpired();
          sessionStorage.removeItem(AUTH_STORAGE_KEYS.AUTH_SESSION);
        }
      } catch {
        // ignore storage errors
      }
    } finally {
      isRefreshingRef.current = false;
    }
  }, []);

  const applyLoginResponse = useCallback(
    async (response: LoginResponse, options: { emailHint?: string; sessionMode: SessionMode }): Promise<LoginResponse> => {
      const mode = options.sessionMode ?? 'normal';
      setSessionMode(mode);

      apiClient.setToken(response.token);
      setAccessToken(response.token);

      // Prefer previously selected tenant when still available to avoid landing in wrong tenant
      let resolvedTenantId = workspaceIdFromTenantId(response.tenant_id);
      let resolvedTenants = response.tenants;
      try {
        const cachedSelectionJson = sessionStorage.getItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
        if (cachedSelectionJson) {
          const cachedSelection: TenantSelection = JSON.parse(cachedSelectionJson);
          // Validate cached tenant belongs to current user
          if (
            cachedSelection.userId === response.user_id &&
            cachedSelection.tenantId &&
            response.tenants?.some((t) => t.id === cachedSelection.tenantId) &&
            cachedSelection.tenantId !== resolvedTenantId
          ) {
            const switched = await apiClient.switchTenant(cachedSelection.tenantId);
            resolvedTenantId = workspaceIdFromTenantId(switched.tenant_id ?? cachedSelection.tenantId);
            resolvedTenants = switched.tenants ?? resolvedTenants;
          } else if (cachedSelection.userId !== response.user_id) {
            // Clear stale cache from previous user
            sessionStorage.removeItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
          }
        }
      } catch {
        // ignore storage errors or JSON parse errors
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
      }

      const latestToken = apiClient.getToken ? apiClient.getToken() : response.token;
      if (latestToken) {
        apiClient.setToken(latestToken);
        setAccessToken(latestToken);
      }

      // Cache tenant context immediately to avoid blank state during initial load
      try {
        const tenantSelection: TenantSelection = {
          tenantId: resolvedTenantId,
          userId: response.user_id,
        };
        sessionStorage.setItem(AUTH_STORAGE_KEYS.SELECTED_TENANT, JSON.stringify(tenantSelection));
      } catch {
        // ignore storage errors
      }
      if (resolvedTenants) {
        try {
          sessionStorage.setItem(AUTH_STORAGE_KEYS.TENANT_BOOTSTRAP, JSON.stringify(resolvedTenants));
        } catch {
          // ignore storage errors
        }
      }

      try {
        sessionStorage.setItem(AUTH_STORAGE_KEYS.AUTH_SESSION, 'true');
        clearSessionExpiredFlag();
        if (resolvedTenants && resolvedTenants.length > 1) {
          sessionStorage.setItem(AUTH_STORAGE_KEYS.TENANT_SELECTION_REQUIRED, '1');
        } else {
          sessionStorage.removeItem(AUTH_STORAGE_KEYS.TENANT_SELECTION_REQUIRED);
        }
      } catch {
        // ignore storage errors
      }

      // Optimistically set user from login response to avoid auth/me flakiness
      setUser({
        id: response.user_id,
        email: options.emailHint ?? response.user_id,
        display_name: options.emailHint ?? response.user_id,
        role: response.role as User['role'],
        tenant_id: resolvedTenantId,
        permissions: [], // refreshed below
        admin_tenants: undefined, // refreshed from /auth/me below
      });

      // Best-effort hydration from /auth/me
      refreshUser().catch(err => {
        logger.warn('Post-login user refresh failed; using optimistic user state', { component: 'AuthProvider' }, toError(err));
      });

      return { ...response, tenant_id: resolvedTenantId, tenants: resolvedTenants };
    },
    [refreshUser],
  );

  const login = useCallback(async (credentials: LoginRequest): Promise<LoginResponse> => {
    setAuthError(null);
    try {
      logger.info('Initiating login', {
        component: 'AuthProvider',
        operation: 'login',
        username: credentials.username,
      });
      const response = await apiClient.login({
        ...credentials,
        device_id: ensureDeviceId(),
      });
      logger.info('Login successful', {
        component: 'AuthProvider',
        operation: 'login',
        user_id: response.user_id,
        tenant_id: response.tenant_id,
      });
      const normalizedMode: SessionMode = (response.session_mode as SessionMode) ?? 'normal';
      return applyLoginResponse(response, {
        emailHint: credentials.email ?? response.user_id,
        sessionMode: normalizedMode,
      });
    } catch (error) {
      const err = toError(error);
      setAuthError(err);
      logger.error('Login failed', { component: 'AuthProvider' }, err);
      throw error; // Re-throw so caller can handle
    }
  }, [applyLoginResponse]);

  const devBypassLogin = useCallback(async (): Promise<LoginResponse> => {
    setAuthError(null);
    try {
      logger.info('Initiating dev bypass login', {
        component: 'AuthProvider',
        operation: 'devBypassLogin',
      });
      const response = await apiClient.devBypass();
      const normalizedMode: SessionMode = (response.session_mode as SessionMode) ?? 'dev_bypass';
      const result = await applyLoginResponse(response, {
        emailHint: response.user_id,
        sessionMode: normalizedMode,
      });

      // Mark dev bypass activation and start timer for 1-hour timeout
      markDevBypassActivated();
      startDevBypassTimer();

      logAuthEvent('UI auth session established', {
        component: 'AuthProvider',
        operation: 'devBypassLogin',
        mode: normalizedMode,
        user_id: response.user_id,
        tenant_id: result.tenant_id,
      });
      return result;
    } catch (error) {
      const err = toError(error);
      setAuthError(err);
      logger.error('Dev bypass login failed', { component: 'AuthProvider' }, err);
      throw error;
    }
  }, [applyLoginResponse, startDevBypassTimer]);

  const logout = useCallback(async () => {
    try {
      await apiClient.logout();
    } catch (error) {
      logger.error('Logout error', { component: 'AuthProvider' }, toError(error));
    } finally {
      // Clear auth state atomically
      setUser(null);
      setAccessToken(null);
      setSessionMode('normal');
      setAuthError(null);

      // Stop dev bypass timer and clear timestamp
      stopDevBypassTimer();
      clearDevBypassTimestamp();

      // Atomic session state clear - all or nothing
      try {
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.TENANT_BOOTSTRAP);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.AUTH_SESSION);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.TENANT_SELECTION_REQUIRED);
      } catch {
        // Ignore storage errors during logout
      }
    }
  }, [stopDevBypassTimer]);

  const refreshSession = useCallback(async () => {
    try {
      await apiClient.refreshSession();
      await refreshUser();
      setAccessToken(apiClient.getToken ? apiClient.getToken() ?? null : null);
      try {
        sessionStorage.setItem(AUTH_STORAGE_KEYS.AUTH_SESSION, 'true');
        clearSessionExpiredFlag();
      } catch {
        // ignore storage errors
      }
    } catch (error) {
      const err = toError(error);
      setAuthError(err);
      logger.error('Session refresh error', { component: 'AuthProvider' }, err);
      setUser(null);
      setSessionMode('normal');
      try {
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.AUTH_SESSION);
        markSessionExpired();
      } catch {
        // ignore storage errors
      }
    }
  }, [refreshUser]);

  const logoutAllSessions = useCallback(async () => {
    try {
      await apiClient.logoutAllSessions();
    } catch (error) {
      logger.error('Logout all sessions error', { component: 'AuthProvider' }, toError(error));
    } finally {
      // Clear auth state atomically
      setUser(null);
      setAccessToken(null);
      setSessionMode('normal');
      setAuthError(null);

      // Stop dev bypass timer and clear timestamp
      stopDevBypassTimer();
      clearDevBypassTimestamp();

      // Atomic session state clear - all or nothing
      try {
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.TENANT_BOOTSTRAP);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.AUTH_SESSION);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED);
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.TENANT_SELECTION_REQUIRED);
      } catch {
        // Ignore storage errors during logout
      }
    }
  }, [stopDevBypassTimer]);

  const updateProfile = useCallback(async (updates: { display_name?: string; avatar_url?: string }) => {
    try {
      await apiClient.updateUserProfile(updates);
      // Refresh user to get latest data
      await refreshUser();
    } catch (error) {
      logger.error('Failed to update profile', { component: 'AuthProvider' }, toError(error));
      throw error;
    }
  }, [refreshUser]);

  useEffect(() => {
    // Bootstrap auth on first load:
    // 1) If server is in dev bypass, /auth/me returns admin + wildcard tenants.
    // 2) Otherwise, fall back to normal refreshUser flow (may 401).
    const checkAuth = async () => {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => {
        controller.abort();
        setAuthTimeout(true);
        setIsLoading(false);
      }, 30000); // 30 second timeout

      try {
        const devBypassEnvEnabled = isDevBypassEnabled(); // Dev bypass environment policy documented in docs/AUTHENTICATION.md
        if (devBypassEnvEnabled) {
          // Always try dev bypass when enabled - the server will validate
          // Clear any stale timestamp if expired, but still attempt bypass
          if (isDevBypassExpired()) {
            logger.debug('Dev bypass timestamp expired or not set; will attempt fresh bypass', {
              component: 'AuthProvider',
            });
            clearDevBypassTimestamp();
          }

          // Always attempt dev bypass login when env is enabled
          const devClaims = await tryDevBypassLogin();
          if (devClaims) {
            logger.debug('AuthProvider: using dev-bypass claims from /auth/me', {
              component: 'AuthProvider',
            });
            const normalizedRole = typeof devClaims.role === 'string' ? devClaims.role.toLowerCase() : undefined;
            const allowedRoles = ['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer'] as const;
            const resolvedRole = (allowedRoles as readonly string[]).includes(normalizedRole ?? '')
              ? (normalizedRole as User['role'])
              : 'viewer';

            setSessionMode('dev_bypass');
            setUser({
              id: devClaims.user_id,
              email: devClaims.email,
              display_name: devClaims.display_name || devClaims.email,
              role: resolvedRole,
              tenant_id: workspaceIdFromTenantId(devClaims.tenant_id),
              permissions: devClaims.permissions || [],
              last_login_at: devClaims.last_login_at ?? undefined,
              mfa_enabled: devClaims.mfa_enabled ?? undefined,
              token_last_rotated_at: devClaims.token_last_rotated_at ?? undefined,
              admin_tenants: devClaims.admin_tenants,
            });
            setAuthError(null);

            // Mark activation timestamp for session timeout tracking
            markDevBypassActivated();
            startDevBypassTimer();

            try {
              sessionStorage.setItem(AUTH_STORAGE_KEYS.AUTH_SESSION, 'true');
              sessionStorage.removeItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED);
            } catch {
              // ignore storage errors
            }
            clearTimeout(timeoutId);
            return;
          }
        } else {
          logger.debug('Dev bypass disabled by env; skipping bootstrap', { component: 'AuthProvider' });
        }

        // Attempt to get current user - if 401, we're not authenticated
        await refreshUser();
        clearTimeout(timeoutId);
      } catch (error) {
        clearTimeout(timeoutId);
        if (controller.signal.aborted) {
          // Timeout already handled above
          return;
        }
        // Not authenticated - this is expected on initial load
        setUser(null);
      } finally {
        setIsLoading(false);
      }
    };
    checkAuth();
  }, [refreshUser, startDevBypassTimer]);

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (devBypassTimerRef.current) {
        clearInterval(devBypassTimerRef.current);
        devBypassTimerRef.current = null;
      }
    };
  }, []);

  // Compute isDevBypassActive from sessionMode and remaining time
  const isDevBypassActive = sessionMode === 'dev_bypass' && devBypassRemainingMs > 0;

  const value: AuthContextValue = {
    user,
    isLoading,
    authError,
    authTimeout,
    accessToken,
    sessionMode,
    isDevBypassActive,
    devBypassRemainingMs,
    login,
    devBypassLogin,
    logout,
    refreshUser,
    refreshSession,
    logoutAllSessions,
    updateProfile,
    clearAuthError,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

// Resize Provider Component
function ResizeProvider({ children }: { children: ReactNode }) {
  const getLayout = useCallback((storageKey: string): number[] | null => {
    try {
      const stored = localStorage.getItem(`resize:layout:${storageKey}`);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed) && parsed.every((n) => typeof n === 'number')) {
          return parsed;
        }
      }
    } catch (error) {
      logger.warn('Failed to load layout', { component: 'ResizeProvider', storageKey });
    }
    return null;
  }, []);

  const setLayout = useCallback((storageKey: string, sizes: number[]) => {
    // Validate sizes array
    if (!Array.isArray(sizes) || sizes.length === 0 || !sizes.every((n) => typeof n === 'number' && n >= 0 && n <= 100)) {
      logger.warn('Invalid layout sizes', { component: 'ResizeProvider', storageKey, sizes });
      return;
    }

    try {
      localStorage.setItem(`resize:layout:${storageKey}`, JSON.stringify(sizes));
    } catch (error) {
      // Handle quota exceeded or other storage errors
      if (error instanceof DOMException && error.name === 'QuotaExceededError') {
        logger.warn('Storage quota exceeded, clearing old layouts', { component: 'ResizeProvider' });
        // Clear old layout entries (keep last 10)
        try {
          const keys = Object.keys(localStorage).filter((k) => k.startsWith('resize:layout:'));
          if (keys.length > 10) {
            keys.slice(0, keys.length - 10).forEach((k) => localStorage.removeItem(k));
            // Retry saving
            localStorage.setItem(`resize:layout:${storageKey}`, JSON.stringify(sizes));
          }
        } catch (retryError) {
          logger.warn('Failed to save layout after cleanup', { component: 'ResizeProvider', storageKey });
        }
      } else {
        logger.warn('Failed to save layout', { component: 'ResizeProvider', storageKey });
      }
    }
  }, []);

  const value: ResizeContextValue = {
    getLayout,
    setLayout,
  };

  return <ResizeContext.Provider value={value}>{children}</ResizeContext.Provider>;
}

// Core Providers Component
export function CoreProviders({ children }: { children: ReactNode }) {
  return (
    <AuthProvider>
      <AosThemeProvider>
        <ResizeProvider>
          {children}
        </ResizeProvider>
      </AosThemeProvider>
    </AuthProvider>
  );
}

export { useTheme } from '@/theme/ThemeProvider';
