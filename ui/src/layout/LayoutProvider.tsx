import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import apiClient from '@/api/client';
import type { Tenant, User, UserRole } from '@/api/types';

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
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const verify = async () => {
      try {
        const token = apiClient.getToken();
        if (token) {
          const currentUser = await apiClient.getCurrentUser();
          setUser({
            id: currentUser.user_id,
            email: currentUser.email,
            display_name: currentUser.email.split('@')[0],
            role: currentUser.role.charAt(0).toUpperCase() + currentUser.role.slice(1) as UserRole,
            tenant_id: 'default',
            permissions: [],
          });
        }
      } catch {
        apiClient.setToken(null);
        setUser(null);
      } finally {
        setIsLoading(false);
      }
    };
    void verify();
  }, []);

  const login = useCallback(async (credentials: { email: string; password: string }) => {
    const response = await apiClient.login(credentials);
    setUser({
      id: response.user_id,
      email: credentials.email,
      display_name: credentials.email.split('@')[0],
      role: response.role.charAt(0).toUpperCase() + response.role.slice(1) as UserRole,
      tenant_id: 'default',
      permissions: [],
    });
  }, []);

  const logout = useCallback(async () => {
    try { await apiClient.logout(); } catch {}
    setUser(null);
  }, []);

  const value = useMemo(() => ({ user, isLoading, login, logout }), [user, isLoading, login, logout]);
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}

// Tenant
interface TenantContextValue {
  selectedTenant: string;
  setSelectedTenant: (tenantId: string) => void;
  tenants: Tenant[];
}

const TenantContext = createContext<TenantContextValue | undefined>(undefined);

function TenantProvider({ children }: { children: React.ReactNode }) {
  const { user } = useAuth();
  const [selectedTenant, setSelectedTenantState] = useState<string>('default');
  const [tenants, setTenants] = useState<Tenant[]>([]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem('aos_selected_tenant');
      if (saved) setSelectedTenantState(saved);
    } catch {}
  }, []);

  const setSelectedTenant = useCallback((tenantId: string) => {
    setSelectedTenantState(tenantId);
    try { localStorage.setItem('aos_selected_tenant', tenantId); } catch {}
  }, []);

  useEffect(() => {
    const loadTenants = async () => {
      if (!user) { setTenants([]); return; }
      try {
        const list = await apiClient.listTenants();
        setTenants(list);
      } catch {
        setTenants([]);
      }
    };
    void loadTenants();
  }, [user]);

  const value = useMemo(() => ({ selectedTenant, setSelectedTenant, tenants }), [selectedTenant, setSelectedTenant, tenants]);
  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

export function useTenant() {
  const ctx = useContext(TenantContext);
  if (!ctx) throw new Error('useTenant must be used within TenantProvider');
  return ctx;
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

// Combined LayoutProvider
export function LayoutProvider({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider>
      <AuthProvider>
        <TenantProvider>
          <ResizeProvider>
            {children}
          </ResizeProvider>
        </TenantProvider>
      </AuthProvider>
    </ThemeProvider>
  );
}


