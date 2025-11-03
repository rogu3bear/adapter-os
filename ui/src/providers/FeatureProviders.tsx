import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import type { Tenant } from '@/api/types';
import { useAuth } from './CoreProviders';

// Tenant
interface TenantContextValue {
  selectedTenant: string;
  setSelectedTenant: (tenantId: string) => void;
  tenants: Tenant[];
}

const TenantContext = createContext<TenantContextValue | undefined>(undefined);

function TenantProvider({ children }: { children: React.ReactNode }) {
  // Optional auth dependency - gracefully handle if auth is not available
  // TenantProvider will work without auth (just won't load tenants)
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
    try {
      const name = tenants.find(t => t.id === tenantId)?.name || tenantId;
      toast.success(`Switched to tenant: ${name}`);
    } catch {}
  }, [tenants]);

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

/**
 * FeatureProviders - Feature-specific providers
 * Groups: Tenant (optional auth dependency)
 */
export function FeatureProviders({ children }: { children: React.ReactNode }) {
  return (
    <TenantProvider>
      {children}
    </TenantProvider>
  );
}

