import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { apiClient } from '../api/client';
import type { Tenant } from '../api/types';
import { logger, toError } from '../utils/logger';
import { BookmarkProvider } from '../contexts/BookmarkContext';

// Tenant Context
interface TenantContextValue {
  selectedTenant: string;
  setSelectedTenant: (tenantId: string) => void;
  tenants: Tenant[];
  isLoading: boolean;
  refreshTenants: () => Promise<void>;
}

const TenantContext = createContext<TenantContextValue | undefined>(undefined);

export function useTenant(): TenantContextValue {
  const context = useContext(TenantContext);
  if (!context) {
    throw new Error('useTenant must be used within FeatureProviders');
  }
  return context;
}

// Tenant Provider Component
function TenantProvider({ children }: { children: ReactNode }) {
  const [selectedTenant, setSelectedTenantState] = useState<string>(() => {
    return localStorage.getItem('selectedTenant') || '';
  });
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refreshTenants = useCallback(async () => {
    try {
      const tenantList = await apiClient.listTenants();
      setTenants(tenantList);
      
      // If no tenant is selected and we have tenants, select the first one
      // Also validate that currently selected tenant still exists
      setSelectedTenantState((current) => {
        if (tenantList.length === 0) {
          // No tenants available
          try {
            localStorage.removeItem('selectedTenant');
          } catch (error) {
            // Ignore localStorage errors
          }
          return '';
        }

        // If current tenant doesn't exist in list, select first available
        if (current && !tenantList.some((t) => t.id === current)) {
          logger.warn('Selected tenant no longer exists, selecting first available', {
            component: 'TenantProvider',
            previousTenant: current,
            newTenant: tenantList[0].id
          });
          const firstTenantId = tenantList[0].id;
          try {
            localStorage.setItem('selectedTenant', firstTenantId);
          } catch (error) {
            logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
          }
          return firstTenantId;
        }

        // If no tenant selected, select first one
        if (!current) {
          const firstTenantId = tenantList[0].id;
          try {
            localStorage.setItem('selectedTenant', firstTenantId);
          } catch (error) {
            logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
          }
          return firstTenantId;
        }

        return current;
      });
    } catch (error) {
      logger.error('Failed to fetch tenants', { component: 'TenantProvider' }, toError(error));
    } finally {
      setIsLoading(false);
    }
  }, []);

  const setSelectedTenant = useCallback((tenantId: string) => {
    // Validate tenant exists in list (unless we're still loading)
    if (!isLoading && tenants.length > 0 && !tenants.some((t) => t.id === tenantId)) {
      logger.warn('Attempted to select non-existent tenant', { 
        component: 'TenantProvider', 
        tenantId,
        availableTenants: tenants.map((t) => t.id)
      });
      return;
    }

    setSelectedTenantState(tenantId);
    try {
      localStorage.setItem('selectedTenant', tenantId);
    } catch (error) {
      logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
    }
  }, [tenants, isLoading]);

  useEffect(() => {
    refreshTenants();
  }, [refreshTenants]);

  const value: TenantContextValue = {
    selectedTenant,
    setSelectedTenant,
    tenants,
    isLoading,
    refreshTenants,
  };

  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

// Feature Providers Component
export function FeatureProviders({ children }: { children: ReactNode }) {
  return (
    <BookmarkProvider>
      <TenantProvider>
        {children}
      </TenantProvider>
    </BookmarkProvider>
  );
}

