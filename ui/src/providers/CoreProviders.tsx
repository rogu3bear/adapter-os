import React, { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react';
import { apiClient } from '../api/client';
import type { User, LoginRequest } from '../api/types';
import { logger, toError } from '../utils/logger';

// Auth Context
interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  login: (credentials: LoginRequest) => Promise<void>;
  logout: () => Promise<void>;
  refreshUser: () => Promise<void>;
  refreshSession: () => Promise<void>;
  logoutAllSessions: () => Promise<void>;
  updateProfile: (updates: { display_name?: string; avatar_url?: string }) => Promise<void>;
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
  const isRefreshingRef = useRef(false);

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
    } catch (error) {
      setUser(null);
      logger.error('Failed to fetch user', { component: 'AuthProvider' }, toError(error));
    } finally {
      isRefreshingRef.current = false;
    }
  }, []);

  const login = useCallback(async (credentials: LoginRequest) => {
    try {
      await apiClient.login(credentials);
      await refreshUser();
    } catch (error) {
      logger.error('Login failed', { component: 'AuthProvider' }, toError(error));
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
    }
  }, []);

  const refreshSession = useCallback(async () => {
    try {
      await apiClient.refreshSession();
      await refreshUser();
    } catch (error) {
      logger.error('Session refresh error', { component: 'AuthProvider' }, toError(error));
      setUser(null);
    }
  }, [refreshUser]);

  const logoutAllSessions = useCallback(async () => {
    try {
      await apiClient.logoutAllSessions();
      setUser(null);
    } catch (error) {
      logger.error('Logout all sessions error', { component: 'AuthProvider' }, toError(error));
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
    refreshUser().finally(() => setIsLoading(false));
  }, [refreshUser]);

  const value: AuthContextValue = {
    user,
    isLoading,
    login,
    logout,
    refreshUser,
    refreshSession,
    logoutAllSessions,
    updateProfile,
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
      const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
      
      if (isDark) {
        root.classList.add('dark');
      } else {
        root.classList.remove('dark');
      }
    };

    applyTheme();

    // Listen to system preference changes when theme is 'system'
    if (theme === 'system') {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
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

