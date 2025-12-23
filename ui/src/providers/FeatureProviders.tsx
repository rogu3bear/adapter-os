import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { TenantSummary } from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { BookmarkProvider } from '@/contexts/BookmarkContext';
import { HistoryProvider } from '@/contexts/HistoryContext';
import { BreadcrumbProvider } from '@/contexts/BreadcrumbContext';
import { UndoRedoProvider } from '@/contexts/UndoRedoContext';
import { TENANT_SELECTION_REQUIRED_KEY, useAuth } from './CoreProviders';
import { streamingService } from '@/services/StreamingService';
import { TENANT_SWITCH_EVENT } from '@/utils/tenant';

const TENANT_BOOTSTRAP_KEY = 'aos-tenant-bootstrap';
const SELECTED_TENANT_KEY = 'selectedTenant';

// Session-scoped tenant selection with user validation
interface TenantSelection {
  tenantId: string;
  userId: string;
}

// Tenant loading timeout (10 seconds)
const TENANT_LOADING_TIMEOUT_MS = 10000;

// Tenant Context
interface TenantContextValue {
  selectedTenant: string;
  /** Returns true if tenant was successfully selected, false if tenant doesn't exist */
  setSelectedTenant: (tenantId: string) => Promise<boolean>;
  tenants: TenantSummary[];
  isLoading: boolean;
  /** Error that occurred during tenant loading, if any */
  loadError: Error | null;
  /** Whether tenant loading timed out */
  loadTimedOut: boolean;
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
  const { user, refreshUser } = useAuth();
  const queryClient = useQueryClient();
  const [selectedTenant, setSelectedTenantState] = useState<string>(() => {
    try {
      const cachedJson = sessionStorage.getItem(SELECTED_TENANT_KEY);
      if (cachedJson) {
        const cached: TenantSelection = JSON.parse(cachedJson);
        return cached.tenantId || '';
      }
    } catch {
      // Ignore parse errors
    }
    return '';
  });
  const selectedTenantRef = useRef(selectedTenant);
  const userRef = useRef(user);
  const [tenants, setTenants] = useState<TenantSummary[]>(() => {
    try {
      const cached = sessionStorage.getItem(TENANT_BOOTSTRAP_KEY);
      if (cached) {
        return JSON.parse(cached) as TenantSummary[];
      }
    } catch {
      // ignore parse errors
    }
    return [];
  });
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [loadTimedOut, setLoadTimedOut] = useState(false);
  const loadingTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    selectedTenantRef.current = selectedTenant;
  }, [selectedTenant]);

  useEffect(() => {
    userRef.current = user;
  }, [user]);

  const clearTenantSelectionRequirement = useCallback(() => {
    try {
      sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
    } catch {
      // ignore storage errors
    }
  }, []);

  const resetTenantCaches = useCallback(async (tenantId: string) => {
    try {
      await queryClient.cancelQueries();
    } catch {
      // ignore cancellation errors
    }

    // Selectively clear tenant-specific queries instead of wiping everything.
    // This prevents flash of empty states for global data (auth, system health, meta).
    const GLOBAL_QUERY_PREFIXES = [
      'auth',
      'user',
      'system-health',
      'system-overview',
      'meta',
      'feature-flags',
      'tenants', // Tenant list itself is global
      'user-tenants',
    ];

    queryClient.removeQueries({
      predicate: (query) => {
        const key = query.queryKey;
        // Keep queries that start with global prefixes
        if (Array.isArray(key) && key.length > 0) {
          const firstKey = String(key[0]).toLowerCase();
          return !GLOBAL_QUERY_PREFIXES.some(prefix => firstKey.startsWith(prefix));
        }
        // Remove unknown query key formats
        return true;
      },
    });

    // Invalidate remaining queries to trigger refetch with new tenant context
    queryClient.invalidateQueries();
    streamingService.unsubscribeAll();
    window.dispatchEvent(new CustomEvent(TENANT_SWITCH_EVENT, { detail: { tenantId } }));
  }, [queryClient]);

  // Cross-tab tenant sync: when another tab switches tenants, keep this tab's in-memory
  // state (and React Query caches) aligned with the backend's active tenant.
  useEffect(() => {
    if (typeof window === 'undefined') return;

    const handleStorage = (event: StorageEvent) => {
      if (event.storageArea !== window.sessionStorage) return;
      if (event.key !== SELECTED_TENANT_KEY) return;

      try {
        const nextValue = event.newValue;
        if (!nextValue) return;

        const nextSelection: TenantSelection = JSON.parse(nextValue);
        const nextTenantId = nextSelection.tenantId;

        // Validate user matches current user
        if (!userRef.current || nextSelection.userId !== userRef.current.id) {
          return;
        }

        if (nextTenantId === selectedTenantRef.current) return;

        setSelectedTenantState(nextTenantId);
        clearTenantSelectionRequirement();

        void resetTenantCaches(nextTenantId).then(() => {
          if (!userRef.current) return;
          (async () => {
            try {
              await refreshUser();
            } catch (err) {
              logger.warn(
                'Failed to refresh user after cross-tab tenant change',
                { component: 'TenantProvider' },
                toError(err)
              );
            }
          })();
        });
      } catch {
        // Ignore parse errors
      }
    };

    window.addEventListener('storage', handleStorage);
    return () => window.removeEventListener('storage', handleStorage);
  }, [clearTenantSelectionRequirement, refreshUser, resetTenantCaches]);

  const refreshTenants = useCallback(async () => {
    // Clear any previous error/timeout state
    setLoadError(null);
    setLoadTimedOut(false);

    // Set loading timeout
    if (loadingTimeoutRef.current) {
      clearTimeout(loadingTimeoutRef.current);
    }
    loadingTimeoutRef.current = setTimeout(() => {
      if (isLoading) {
        setLoadTimedOut(true);
        setIsLoading(false);
        logger.error('Tenant loading timed out', {
          component: 'TenantProvider',
          timeoutMs: TENANT_LOADING_TIMEOUT_MS,
        });
      }
    }, TENANT_LOADING_TIMEOUT_MS);

    try {
      const tenantList = await apiClient.listUserTenants();

      // Clear timeout on success
      if (loadingTimeoutRef.current) {
        clearTimeout(loadingTimeoutRef.current);
        loadingTimeoutRef.current = null;
      }

      setTenants(tenantList);
      try {
        sessionStorage.removeItem(TENANT_BOOTSTRAP_KEY);
      } catch {
        // ignore storage errors
      }
      if (tenantList.length > 1) {
        toast.info('Multiple tenants available. Use the tenant switcher to pick one.');
      }
      try {
        if (tenantList.length > 1) {
          sessionStorage.setItem(TENANT_SELECTION_REQUIRED_KEY, '1');
        } else {
          sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
        }
      } catch {
        // ignore storage errors
      }

      // Prefer the authenticated user's tenant when available to avoid stale selections
      setSelectedTenantState((current) => {
        if (!tenantList || tenantList.length === 0) {
          try {
            sessionStorage.removeItem(SELECTED_TENANT_KEY);
          } catch (error) {
            // Ignore storage errors
          }
          return '';
        }

        const userId = user?.id;
        if (!userId) return current || '';

        const userTenantId = user?.tenant_id;
        const hasUserTenant = Boolean(userTenantId && tenantList.some((t) => t.id === userTenantId));
        const hasCurrent = Boolean(current && tenantList.some((t) => t.id === current));

        if (hasUserTenant && current !== userTenantId) {
          try {
            const selection: TenantSelection = { tenantId: userTenantId!, userId };
            sessionStorage.setItem(SELECTED_TENANT_KEY, JSON.stringify(selection));
          } catch (error) {
            logger.warn('Failed to save selected tenant to sessionStorage', { component: 'TenantProvider' });
          }
          return userTenantId!;
        }

        if (hasCurrent) {
          return current;
        }

        const firstTenantId = tenantList[0].id;
        try {
          const selection: TenantSelection = { tenantId: firstTenantId, userId };
          sessionStorage.setItem(SELECTED_TENANT_KEY, JSON.stringify(selection));
        } catch (error) {
          logger.warn('Failed to save selected tenant to sessionStorage', { component: 'TenantProvider' });
        }
        return firstTenantId;
      });
    } catch (error) {
      // Clear timeout on error
      if (loadingTimeoutRef.current) {
        clearTimeout(loadingTimeoutRef.current);
        loadingTimeoutRef.current = null;
      }
      const err = toError(error);
      setLoadError(err);
      logger.error('Failed to fetch tenants', { component: 'TenantProvider' }, err);
    } finally {
      setIsLoading(false);
    }
  }, [user?.tenant_id, isLoading]);

  const setSelectedTenant = useCallback(async (tenantId: string): Promise<boolean> => {
    // Validate tenant exists in list (unless we're still loading)
    if (!isLoading && tenants.length > 0 && !tenants.some((t) => t.id === tenantId)) {
      logger.warn('Attempted to select non-existent tenant', {
        component: 'TenantProvider',
        tenantId,
        availableTenants: tenants.map((t) => t.id)
      });
      return false;
    }

    const userId = user?.id;
    if (!userId) {
      logger.warn('Cannot select tenant without authenticated user', { component: 'TenantProvider' });
      return false;
    }

    const alreadyActive =
      tenantId === selectedTenant ||
      (!!user?.tenant_id && tenantId === user.tenant_id);
    if (alreadyActive) {
      setSelectedTenantState(tenantId);
      try {
        const selection: TenantSelection = { tenantId, userId };
        sessionStorage.setItem(SELECTED_TENANT_KEY, JSON.stringify(selection));
      } catch (error) {
        logger.warn('Failed to save selected tenant to sessionStorage', { component: 'TenantProvider' });
      }
      clearTenantSelectionRequirement();
      return true;
    }

    try {
      const resp = await apiClient.switchTenant(tenantId);
      setSelectedTenantState(tenantId);
      try {
        const selection: TenantSelection = { tenantId, userId };
        sessionStorage.setItem(SELECTED_TENANT_KEY, JSON.stringify(selection));
      } catch (error) {
        logger.warn('Failed to save selected tenant to sessionStorage', { component: 'TenantProvider' });
      }
      if (resp?.tenants) {
        setTenants(resp.tenants);
        try {
          sessionStorage.setItem(TENANT_BOOTSTRAP_KEY, JSON.stringify(resp.tenants));
        } catch {
          // ignore storage errors
        }
      }
      clearTenantSelectionRequirement();
      await resetTenantCaches(tenantId);
      try {
        await refreshUser();
      } catch (err) {
        logger.warn('Failed to refresh user after tenant switch', { component: 'TenantProvider' }, toError(err));
      }
      return true;
    } catch (error) {
      const err = toError(error) as Error & { status?: number; code?: string; failure_code?: string };
      if (err?.code === 'PARSE_ERROR') {
        logger.warn('Tenant switch returned unparsable payload; assuming success', {
          component: 'TenantProvider',
          tenantId,
        }, err);
        setSelectedTenantState(tenantId);
        clearTenantSelectionRequirement();
        await resetTenantCaches(tenantId);
        return true;
      }

      logger.error('Failed to switch tenant', { component: 'TenantProvider', tenantId }, err);

      // Let the global session-expiry handler drive UX for 401s
      if (err?.status === 401 || err?.code === 'SESSION_EXPIRED') {
        return false;
      }

      if (err?.status === 403 || err?.code === 'TENANT_ACCESS_DENIED' || err?.failure_code === 'TENANT_ACCESS_DENIED') {
        toast.error('You do not have access to this tenant.');
        return false;
      }

      logger.warn('Proceeding with local tenant selection despite switch error', {
        component: 'TenantProvider',
        tenantId,
        errorCode: err?.code,
        status: err?.status,
      });
      setSelectedTenantState(tenantId);
      clearTenantSelectionRequirement();
      await resetTenantCaches(tenantId);
      return true;
    }
  }, [tenants, isLoading, selectedTenant, user?.tenant_id, clearTenantSelectionRequirement, refreshUser, resetTenantCaches]);

  // Only fetch tenants when user is authenticated
  useEffect(() => {
    if (user) {
      refreshTenants();
    } else {
      // Reset tenants state when not authenticated
      setTenants([]);
      setIsLoading(false);
      try {
        sessionStorage.removeItem(TENANT_BOOTSTRAP_KEY);
      } catch {
        // ignore storage errors
      }
    }
  }, [refreshTenants, user]);

  // Align initial tenant selection with claims when available
  useEffect(() => {
    if (user?.tenant_id && user?.id && !selectedTenant) {
      setSelectedTenantState(user.tenant_id);
      try {
        const selection: TenantSelection = { tenantId: user.tenant_id, userId: user.id };
        sessionStorage.setItem(SELECTED_TENANT_KEY, JSON.stringify(selection));
      } catch {
        // Ignore storage errors
      }
    }
  }, [user?.tenant_id, user?.id, selectedTenant]);

  useEffect(() => {
    if (!selectedTenant) return;
    if (tenants.some((t) => t.id === selectedTenant)) {
      clearTenantSelectionRequirement();
    }
  }, [selectedTenant, tenants, clearTenantSelectionRequirement]);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (loadingTimeoutRef.current) {
        clearTimeout(loadingTimeoutRef.current);
      }
    };
  }, []);

  const value: TenantContextValue = {
    selectedTenant,
    setSelectedTenant,
    tenants,
    isLoading,
    loadError,
    loadTimedOut,
    refreshTenants,
  };

  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

// Feature Providers Component
export function FeatureProviders({ children }: { children: ReactNode }) {
  return (
    <BookmarkProvider>
      <HistoryProvider>
        <BreadcrumbProvider>
          <TenantProvider>
            <UndoRedoProvider>
              {children}
            </UndoRedoProvider>
          </TenantProvider>
        </BreadcrumbProvider>
      </HistoryProvider>
    </BookmarkProvider>
  );
}
