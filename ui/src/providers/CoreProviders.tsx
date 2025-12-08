import React, { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react';
import { apiClient } from '@/api/client';
import type { User } from '@/api/types';
import type { LoginRequest, LoginResponse, SessionMode } from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';
import { ThemeProvider as AosThemeProvider } from '@/theme/ThemeProvider';
import { isDevBypassEnabled, tryDevBypassLogin } from '@/auth/authBootstrap';
import { clearSessionExpiredFlag, markSessionExpired, SESSION_EXPIRED_FLAG_KEY } from '@/auth/session';
import { logAuthEvent } from '@/lib/logUIError';

const SELECTED_TENANT_KEY = 'selectedTenant';
const TENANT_BOOTSTRAP_KEY = 'aos-tenant-bootstrap';
const AUTH_SESSION_KEY = 'aos-auth-active';
export { SESSION_EXPIRED_FLAG_KEY };
export const TENANT_SELECTION_REQUIRED_KEY = 'aos-tenant-selection-required';
const DEVICE_ID_KEY = 'aos-device-id';

function ensureDeviceId(): string {
  try {
    const existing = localStorage.getItem(DEVICE_ID_KEY);
    if (existing) return existing;
    const generated =
      typeof crypto !== 'undefined' && crypto.randomUUID
        ? crypto.randomUUID()
        : `device-${Date.now()}`;
    localStorage.setItem(DEVICE_ID_KEY, generated);
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
  accessToken: string | null;
  sessionMode: SessionMode;
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
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [sessionMode, setSessionMode] = useState<SessionMode>('normal');
  const isRefreshingRef = useRef(false);

  const clearAuthError = useCallback(() => {
    setAuthError(null);
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
        tenant_id: userInfo.tenant_id || '',
        permissions: userInfo.permissions || [],
        last_login_at: userInfo.last_login_at,
        mfa_enabled: userInfo.mfa_enabled,
        token_last_rotated_at: userInfo.token_last_rotated_at,
        admin_tenants: userInfo.admin_tenants,
      });
      setAuthError(null);
      try {
        sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
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
        const hadSession = sessionStorage.getItem(AUTH_SESSION_KEY) === 'true';
        if (hadSession) {
          markSessionExpired();
          sessionStorage.removeItem(AUTH_SESSION_KEY);
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
      const mode = options.sessionMode ?? response.session_mode ?? 'normal';
      setSessionMode(mode);

      apiClient.setToken(response.token);
      setAccessToken(response.token);

      // Prefer previously selected tenant when still available to avoid landing in wrong tenant
      let resolvedTenantId = response.tenant_id || '';
      let resolvedTenants = response.tenants;
      try {
        const cachedTenant = localStorage.getItem(SELECTED_TENANT_KEY);
        if (
          cachedTenant &&
          response.tenants?.some((t) => t.id === cachedTenant) &&
          cachedTenant !== resolvedTenantId
        ) {
          const switched = await apiClient.switchTenant(cachedTenant);
          resolvedTenantId = switched.tenant_id || cachedTenant;
          resolvedTenants = switched.tenants ?? resolvedTenants;
        }
      } catch {
        // ignore storage errors
      }

      const latestToken = apiClient.getToken ? apiClient.getToken() : response.token;
      if (latestToken) {
        apiClient.setToken(latestToken);
        setAccessToken(latestToken);
      }

      // Cache tenant context immediately to avoid blank state during initial load
      try {
        localStorage.setItem(SELECTED_TENANT_KEY, resolvedTenantId);
      } catch {
        // ignore storage errors
      }
      if (resolvedTenants) {
        try {
          sessionStorage.setItem(TENANT_BOOTSTRAP_KEY, JSON.stringify(resolvedTenants));
        } catch {
          // ignore storage errors
        }
      }

      try {
        sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
        clearSessionExpiredFlag();
        if (resolvedTenants && resolvedTenants.length > 1) {
          sessionStorage.setItem(TENANT_SELECTION_REQUIRED_KEY, '1');
        } else {
          sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
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
        admin_tenants: response.admin_tenants,
      });

      // Best-effort hydration from /auth/me
      refreshUser().catch(err => {
        logger.warn('Post-login user refresh failed; using optimistic user state', { component: 'AuthProvider' }, toError(err));
      });

      return { ...response, tenant_id: resolvedTenantId, tenants: resolvedTenants, session_mode: mode };
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
      const normalizedMode = response.session_mode ?? 'normal';
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
      const normalizedMode: SessionMode = response.session_mode ?? 'dev_bypass';
      const result = await applyLoginResponse(response, {
        emailHint: response.user_id,
        sessionMode: normalizedMode,
      });
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
  }, [applyLoginResponse]);

  const logout = useCallback(async () => {
    try {
      await apiClient.logout();
    } catch (error) {
      logger.error('Logout error', { component: 'AuthProvider' }, toError(error));
    } finally {
      setUser(null);
      setAccessToken(null);
      setSessionMode('normal');
      setAuthError(null); // Clear auth error on logout
      try {
        localStorage.removeItem(SELECTED_TENANT_KEY);
        sessionStorage.removeItem(TENANT_BOOTSTRAP_KEY);
        sessionStorage.removeItem(AUTH_SESSION_KEY);
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
        sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
      } catch {
        // Ignore storage errors during logout
      }
    }
  }, []);

  const refreshSession = useCallback(async () => {
    try {
      await apiClient.refreshSession();
      await refreshUser();
      setAccessToken(apiClient.getToken ? apiClient.getToken() ?? null : null);
      try {
        sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
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
        sessionStorage.removeItem(AUTH_SESSION_KEY);
        markSessionExpired();
      } catch {
        // ignore storage errors
      }
    }
  }, [refreshUser]);

  const logoutAllSessions = useCallback(async () => {
    try {
      await apiClient.logoutAllSessions();
      setUser(null);
      setAuthError(null); // Clear auth error on logout
      setAccessToken(null);
      setSessionMode('normal');
    } catch (error) {
      logger.error('Logout all sessions error', { component: 'AuthProvider' }, toError(error));
    } finally {
      try {
        localStorage.removeItem(SELECTED_TENANT_KEY);
        sessionStorage.removeItem(TENANT_BOOTSTRAP_KEY);
        sessionStorage.removeItem(AUTH_SESSION_KEY);
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
        sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
      } catch {
        // Ignore storage errors during logout
      }
    }
  }, []);

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
      try {
        const devBypassEnvEnabled = isDevBypassEnabled(); // Dev bypass environment policy documented in docs/AUTHENTICATION.md
        if (devBypassEnvEnabled) {
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
              tenant_id: devClaims.tenant_id || '',
              permissions: devClaims.permissions || [],
              last_login_at: devClaims.last_login_at,
              mfa_enabled: devClaims.mfa_enabled,
              token_last_rotated_at: devClaims.token_last_rotated_at,
              admin_tenants: devClaims.admin_tenants,
            });
            setAuthError(null);
            try {
              sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
              sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
            } catch {
              // ignore storage errors
            }
            return;
          }
        } else {
          logger.debug('Dev bypass disabled by env; skipping bootstrap', { component: 'AuthProvider' });
        }

        // Attempt to get current user - if 401, we're not authenticated
        await refreshUser();
      } catch {
        // Not authenticated - this is expected on initial load
        setUser(null);
      } finally {
        setIsLoading(false);
      }
    };
    checkAuth();
  }, [refreshUser]);

  const value: AuthContextValue = {
    user,
    isLoading,
    authError,
    accessToken,
    sessionMode,
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
