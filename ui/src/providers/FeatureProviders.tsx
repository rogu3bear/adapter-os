import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { apiClient } from '@/api/client';
import type { Tenant } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { BookmarkProvider } from '@/contexts/BookmarkContext';
import { ModalProvider } from '@/contexts/ModalContext';
import { HistoryProvider } from '@/contexts/HistoryContext';
import { BreadcrumbProvider } from '@/contexts/BreadcrumbContext';
import { UndoRedoProvider } from '@/contexts/UndoRedoContext';
import { useAuth } from './CoreProviders';

// Tenant Context
interface TenantContextValue {
  selectedTenant: string;
  /** Returns true if tenant was successfully selected, false if tenant doesn't exist */
  setSelectedTenant: (tenantId: string) => boolean;
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
  const { user } = useAuth();
  const [selectedTenant, setSelectedTenantState] = useState<string>(() => {
    return localStorage.getItem('selectedTenant') || '';
  });
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refreshTenants = useCallback(async () => {
    try {
      const tenantList = await apiClient.listTenants();
      setTenants(tenantList);

      // Prefer the authenticated user's tenant when available to avoid stale selections
      setSelectedTenantState((current) => {
        if (!tenantList || tenantList.length === 0) {
          try {
            localStorage.removeItem('selectedTenant');
          } catch (error) {
            // Ignore localStorage errors
          }
          return '';
        }

        const userTenantId = user?.tenant_id;
        const hasUserTenant = Boolean(userTenantId && tenantList.some((t) => t.id === userTenantId));
        const hasCurrent = Boolean(current && tenantList.some((t) => t.id === current));

        if (hasUserTenant && current !== userTenantId) {
          try {
            localStorage.setItem('selectedTenant', userTenantId!);
          } catch (error) {
            logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
          }
          return userTenantId!;
        }

        if (hasCurrent) {
          return current;
        }

        const firstTenantId = tenantList[0].id;
        try {
          localStorage.setItem('selectedTenant', firstTenantId);
        } catch (error) {
          logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
        }
        return firstTenantId;
      });
    } catch (error) {
      logger.error('Failed to fetch tenants', { component: 'TenantProvider' }, toError(error));
    } finally {
      setIsLoading(false);
    }
  }, [user?.tenant_id]);

  const setSelectedTenant = useCallback((tenantId: string): boolean => {
    // Validate tenant exists in list (unless we're still loading)
    if (!isLoading && tenants.length > 0 && !tenants.some((t) => t.id === tenantId)) {
      logger.warn('Attempted to select non-existent tenant', {
        component: 'TenantProvider',
        tenantId,
        availableTenants: tenants.map((t) => t.id)
      });
      return false;
    }

    setSelectedTenantState(tenantId);
    try {
      localStorage.setItem('selectedTenant', tenantId);
    } catch (error) {
      logger.warn('Failed to save selected tenant to localStorage', { component: 'TenantProvider' });
    }
    return true;
  }, [tenants, isLoading]);

  // Only fetch tenants when user is authenticated
  useEffect(() => {
    if (user) {
      refreshTenants();
    } else {
      // Reset tenants state when not authenticated
      setTenants([]);
      setIsLoading(false);
    }
  }, [refreshTenants, user]);

  // Align initial tenant selection with claims when available
  useEffect(() => {
    if (user?.tenant_id && !selectedTenant) {
      setSelectedTenantState(user.tenant_id);
      try {
        localStorage.setItem('selectedTenant', user.tenant_id);
      } catch {
        // Ignore storage errors
      }
    }
  }, [user?.tenant_id, selectedTenant]);

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
      <ModalProvider>
        <HistoryProvider>
          <BreadcrumbProvider>
            <TenantProvider>
              <UndoRedoProvider>
                {children}
              </UndoRedoProvider>
            </TenantProvider>
          </BreadcrumbProvider>
        </HistoryProvider>
      </ModalProvider>
    </BookmarkProvider>
  );
}
