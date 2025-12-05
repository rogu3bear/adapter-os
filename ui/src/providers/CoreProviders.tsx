import React, { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react';
import { apiClient } from '@/api/client';
import type { User } from '@/api/types';
import type { LoginRequest, LoginResponse } from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';

const SELECTED_TENANT_KEY = 'selectedTenant';
const TENANT_BOOTSTRAP_KEY = 'aos-tenant-bootstrap';
const AUTH_SESSION_KEY = 'aos-auth-active';
export const SESSION_EXPIRED_FLAG_KEY = 'aos-session-expired';
export const TENANT_SELECTION_REQUIRED_KEY = 'aos-tenant-selection-required';

// Auth Context
interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  authError: Error | null;
  login: (credentials: LoginRequest) => Promise<LoginResponse>;
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

// Theme Context
type Theme = 'light' | 'dark' | 'system';

interface ThemeContextValue {
  theme: Theme;
  toggleTheme: () => void;
  setTheme: (theme: Theme) => void;
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within CoreProviders');
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
      });
      setAuthError(null);
      try {
        sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
      } catch {
        // best-effort session bookkeeping
      }
    } catch (error) {
      setUser(null);
      const err = toError(error);
      setAuthError(err);
      logger.error('Failed to fetch user', { component: 'AuthProvider' }, err);
      try {
        const hadSession = sessionStorage.getItem(AUTH_SESSION_KEY) === 'true';
        if (hadSession) {
          sessionStorage.setItem(SESSION_EXPIRED_FLAG_KEY, 'Session expired — sign in again.');
          sessionStorage.removeItem(AUTH_SESSION_KEY);
        }
      } catch {
        // ignore storage errors
      }
    } finally {
      isRefreshingRef.current = false;
    }
  }, []);

  const login = useCallback(async (credentials: LoginRequest): Promise<LoginResponse> => {
    setAuthError(null);
    try {
      logger.info('Initiating login', {
        component: 'AuthProvider',
        operation: 'login',
        username: credentials.username,
      });
      const response = await apiClient.login(credentials);
      logger.info('Login successful', {
        component: 'AuthProvider',
        operation: 'login',
        user_id: response.user_id,
        tenant_id: response.tenant_id,
      });

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
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
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
        email: credentials.email ?? response.user_id,
        display_name: credentials.email ?? response.user_id,
        role: response.role as User['role'],
        tenant_id: resolvedTenantId,
        permissions: [], // refreshed below
      });

      // Best-effort hydration from /auth/me
      refreshUser().catch(err => {
        logger.warn('Post-login user refresh failed; using optimistic user state', { component: 'AuthProvider' }, toError(err));
      });
      return { ...response, tenant_id: resolvedTenantId, tenants: resolvedTenants };
    } catch (error) {
      const err = toError(error);
      setAuthError(err);
      logger.error('Login failed', { component: 'AuthProvider' }, err);
      throw error; // Re-throw so caller can handle
    }
  }, [refreshUser]);

  const logout = useCallback(async () => {
    try {
      await apiClient.logout();
    } catch (error) {
      logger.error('Logout error', { component: 'AuthProvider' }, toError(error));
    } finally {
      setUser(null);
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
      try {
        sessionStorage.setItem(AUTH_SESSION_KEY, 'true');
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
      } catch {
        // ignore storage errors
      }
    } catch (error) {
      const err = toError(error);
      setAuthError(err);
      logger.error('Session refresh error', { component: 'AuthProvider' }, err);
      setUser(null);
      try {
        sessionStorage.removeItem(AUTH_SESSION_KEY);
        sessionStorage.setItem(SESSION_EXPIRED_FLAG_KEY, 'Session expired — sign in again.');
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
    // Only attempt to refresh user if we have an auth token (cookie-based auth)
    // Check if we might be authenticated by looking for existing session indicators
    // This prevents 401 errors on initial page load before login
    const checkAuth = async () => {
      try {
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
    login,
    logout,
    refreshUser,
    refreshSession,
    logoutAllSessions,
    updateProfile,
    clearAuthError,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

// Theme Provider Component
function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    try {
      const stored = localStorage.getItem('theme');
      if (stored === 'light' || stored === 'dark' || stored === 'system') {
        return stored;
      }
    } catch (error) {
      logger.warn('Failed to read theme from localStorage', { component: 'ThemeProvider' });
    }
    return 'system';
  });

  // Apply theme immediately and listen to system preference changes
  useEffect(() => {
    const applyTheme = () => {
      const root = document.documentElement;
      const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)')?.matches);
      
      if (isDark) {
        root.classList.add('dark');
      } else {
        root.classList.remove('dark');
      }
    };

    applyTheme();

    // Listen to system preference changes when theme is 'system'
    if (theme === 'system' && typeof window !== 'undefined' && window.matchMedia) {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      if (!mediaQuery) return;

      const handleChange = () => applyTheme();

      // Modern browsers
      if (mediaQuery.addEventListener) {
        mediaQuery.addEventListener('change', handleChange);
        return () => mediaQuery.removeEventListener('change', handleChange);
      }
      // Fallback for older browsers
      else if (mediaQuery.addListener) {
        mediaQuery.addListener(handleChange);
        return () => mediaQuery.removeListener(handleChange);
      }
    }
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setThemeState((prev) => {
      const next = prev === 'light' ? 'dark' : prev === 'dark' ? 'system' : 'light';
      try {
        localStorage.setItem('theme', next);
      } catch (error) {
        logger.warn('Failed to save theme to localStorage', { component: 'ThemeProvider' });
      }
      return next;
    });
  }, []);

  const setTheme = useCallback((newTheme: Theme) => {
    if (newTheme !== 'light' && newTheme !== 'dark' && newTheme !== 'system') {
      logger.warn('Invalid theme value', { component: 'ThemeProvider', theme: newTheme });
      return;
    }
    setThemeState(newTheme);
    try {
      localStorage.setItem('theme', newTheme);
    } catch (error) {
      logger.warn('Failed to save theme to localStorage', { component: 'ThemeProvider' });
    }
  }, []);

  const value: ThemeContextValue = {
    theme,
    toggleTheme,
    setTheme,
  };

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
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
      <ThemeProvider>
        <ResizeProvider>
          {children}
        </ResizeProvider>
      </ThemeProvider>
    </AuthProvider>
  );
}
