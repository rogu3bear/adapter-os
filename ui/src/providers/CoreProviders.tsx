import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import apiClient from '@/api/client';
import type { User, UserRole } from '@/api/types';

// Theme
type Theme = 'light' | 'dark';

interface ThemeContextValue {
  theme: Theme;
  setTheme: (t: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>('light');

  useEffect(() => {
    try {
      const saved = localStorage.getItem('aos_theme');
      if (saved === 'light' || saved === 'dark') {
        setThemeState(saved);
      }
    } catch {}
  }, []);

  useEffect(() => {
    // Apply theme only on client to avoid SSR/layout thrash
    const root = document.documentElement;
    if (theme === 'dark') root.classList.add('dark');
    else root.classList.remove('dark');
    try {
      localStorage.setItem('aos_theme', theme);
    } catch {}
  }, [theme]);

  const setTheme = useCallback((t: Theme) => setThemeState(t), []);
  const toggleTheme = useCallback(() => setThemeState((prev) => (prev === 'dark' ? 'light' : 'dark')), []);

  const value = useMemo(() => ({ theme, setTheme, toggleTheme }), [theme, setTheme, toggleTheme]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider');
  return ctx;
}

// Auth
interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  login: (credentials: { email: string; password: string }) => Promise<void>;
  logout: () => Promise<void>;
  refreshUser: () => Promise<void>;
  refreshSession: () => Promise<void>;
  logoutAllSessions: () => Promise<void>;
  updateProfile: (updates: { displayName?: string }) => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

// Valid user roles - centralized definition
const VALID_USER_ROLES: UserRole[] = ['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer'];

function validateAndNormalizeUserRole(role: string): UserRole {
  // Check if the role is valid
  if (VALID_USER_ROLES.includes(role as UserRole)) {
    return role as UserRole;
  }

  // Log invalid role for debugging
  console.warn('Invalid user role received from server, defaulting to viewer', {
    receivedRole: role,
    validRoles: VALID_USER_ROLES,
    component: 'AuthProvider'
  });

  // Default to viewer role for security (most restrictive)
  return 'viewer';
}

function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const buildUser = useCallback((data: {
    user_id: string;
    email?: string;
    display_name?: string;
    role: string;
    tenant_id?: string;
    permissions?: string[];
    last_login_at?: string;
    mfa_enabled?: boolean;
    token_last_rotated_at?: string;
  }, fallbackEmail?: string): User => {
    const email = data.email || fallbackEmail || '';
    const derivedDisplayName = email ? email.split('@')[0] : 'User';

    return {
      id: data.user_id,
      email,
      display_name: data.display_name || derivedDisplayName,
      role: validateAndNormalizeUserRole(data.role),
      tenant_id: data.tenant_id || 'default',
      permissions: data.permissions ?? [],
      last_login_at: data.last_login_at,
      mfa_enabled: data.mfa_enabled,
      token_last_rotated_at: data.token_last_rotated_at,
    };
  }, []);

  const verifyAuth = useCallback(async () => {
    try {
      const currentUser = await apiClient.getCurrentUser();
      setUser(buildUser(currentUser));
    } catch (error: any) {
      // If 401, try refresh if available
      if (error.message?.includes('401')) {
        try {
          // Attempt refresh - assuming server supports /v1/auth/refresh
          const refreshedUser = await apiClient.refreshSession();
          setUser(buildUser(refreshedUser));
        } catch {
          setUser(null);
        }
      } else {
        setUser(null);
      }
    }
  }, [buildUser]);

  useEffect(() => {
    void verifyAuth().finally(() => setIsLoading(false));
  }, [verifyAuth]);

  const login = useCallback(async (credentials: { email: string; password: string }) => {
    const response = await apiClient.login(credentials);
    setUser(buildUser({
      user_id: response.user_id,
      email: response.email ?? credentials.email,
      display_name: response.display_name,
      role: response.role,
      tenant_id: response.tenant_id,
      permissions: response.permissions,
      last_login_at: response.last_login_at,
      token_last_rotated_at: response.token_last_rotated_at,
    }, credentials.email));
  }, [buildUser]);

  const logout = useCallback(async () => {
    try { await apiClient.logout(); } catch {}
    setUser(null);
  }, []);

  const refreshUser = useCallback(async () => {
    try {
      const currentUser = await apiClient.getCurrentUser();
      setUser(buildUser(currentUser));
    } catch (error) {
      setUser(null);
      throw error;
    }
  }, [buildUser]);

  const refreshSession = useCallback(async () => {
    try {
      const refreshed = await apiClient.refreshSession();
      setUser(buildUser(refreshed));
    } catch (error) {
      setUser(null);
      throw error;
    }
  }, [buildUser]);

  const logoutAllSessions = useCallback(async () => {
    await apiClient.logoutAllSessions();
    try {
      const currentUser = await apiClient.getCurrentUser();
      setUser(buildUser(currentUser));
    } catch {
      // If current session invalidated, ensure state cleared
      setUser(null);
    }
  }, [buildUser]);

  const updateProfile = useCallback(async (updates: { displayName?: string }) => {
    const payload: { display_name?: string } = {};
    if (updates.displayName !== undefined) {
      payload.display_name = updates.displayName;
    }
    const updated = await apiClient.updateUserProfile(payload);
    setUser(buildUser(updated));
  }, [buildUser]);

  const value = useMemo(
    () => ({
      user,
      isLoading,
      login,
      logout,
      refreshUser,
      refreshSession,
      logoutAllSessions,
      updateProfile,
    }),
    [user, isLoading, login, logout, refreshUser, refreshSession, logoutAllSessions, updateProfile]
  );
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}

// Route guard component
export function RequireAuth({ children }: { children: React.ReactNode }) {
  const { user, isLoading } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!isLoading && !user) {
      navigate('/login', { replace: true });
    }
  }, [user, isLoading, navigate]);

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
      </div>
    );
  }

  if (!user) {
    return null;
  }

  return <>{children}</>;
}

// Resize persistence
interface ResizeContextValue {
  getLayout: (key: string) => number[] | undefined;
  setLayout: (key: string, layout: number[]) => void;
}

const ResizeContext = createContext<ResizeContextValue | undefined>(undefined);

function ResizeProvider({ children }: { children: React.ReactNode }) {
  const getLayout = useCallback((key: string) => {
    try {
      const raw = localStorage.getItem(`aos_layout_${key}`);
      return raw ? (JSON.parse(raw) as number[]) : undefined;
    } catch {
      return undefined;
    }
  }, []);

  const setLayout = useCallback((key: string, layout: number[]) => {
    try { localStorage.setItem(`aos_layout_${key}`, JSON.stringify(layout)); } catch {}
  }, []);

  const value = useMemo(() => ({ getLayout, setLayout }), [getLayout, setLayout]);
  return <ResizeContext.Provider value={value}>{children}</ResizeContext.Provider>;
}

export function useResize() {
  const ctx = useContext(ResizeContext);
  if (!ctx) throw new Error('useResize must be used within ResizeProvider');
  return ctx;
}

/**
 * CoreProviders - Fundamental providers with no dependencies
 * Groups: Theme, Auth, Resize
 */
export function CoreProviders({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider>
      <AuthProvider>
        <ResizeProvider>
          {children}
        </ResizeProvider>
      </AuthProvider>
    </ThemeProvider>
  );
}
