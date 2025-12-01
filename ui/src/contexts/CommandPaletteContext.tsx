import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import apiClient from '@/api/client';
import type { Adapter, Tenant, Policy, Node, WorkerResponse } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { LucideIcon } from 'lucide-react';
import { useTenant } from '@/providers/FeatureProviders';
import { useAuth } from '@/providers/CoreProviders';

interface ApiError {
  status?: number;
  code?: string;
  name?: string;
  message?: string;
}

export type CommandItemType = 'page' | 'adapter' | 'tenant' | 'policy' | 'node' | 'worker' | 'action';

export interface CommandItem {
  id: string;
  type: CommandItemType;
  title: string;
  description?: string;
  url?: string;
  icon?: React.ComponentType<{ className?: string }>;
  entityId?: string;
  group?: string;
  metadata?: Record<string, unknown>;
  actionId?: string;
  shortcut?: string;
}

interface RecentCommand {
  item: CommandItem;
  timestamp: string;
}

interface CommandPaletteContextValue {
  isOpen: boolean;
  openPalette: () => void;
  closePalette: () => void;
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  searchResults: CommandItem[];
  recentCommands: RecentCommand[];
  routes: CommandItem[];
  entities: {
    adapters: Adapter[];
    tenants: Tenant[];
    policies: Policy[];
    nodes: Node[];
    workers: WorkerResponse[];
  };
  loading: boolean;
  executeCommand: (item: CommandItem) => void;
  refreshEntities: () => Promise<void>;
  refreshError: string | null;
  lastUpdated: string | null;
}

const CommandPaletteContext = createContext<CommandPaletteContextValue | null>(null);

interface CommandPaletteProviderProps {
  children: ReactNode;
  routes: CommandItem[];
}

export function CommandPaletteProvider({ children, routes: providedRoutes }: CommandPaletteProviderProps) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const { user, isLoading: authLoading } = useAuth();
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [recentCommands, setRecentCommands] = useState<RecentCommand[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);

  const [entities, setEntities] = useState<{
    adapters: Adapter[];
    tenants: Tenant[];
    policies: Policy[];
    nodes: Node[];
    workers: WorkerResponse[];
  }>({
    adapters: [],
    tenants: [],
    policies: [],
    nodes: [],
    workers: [],
  });

  const commandActions = useMemo<Record<string, () => Promise<void> | void>>(() => {
    const dispatch = (eventName: string, detail?: Record<string, unknown>) => {
      if (typeof window === 'undefined') {
        return;
      }
      window.dispatchEvent(new CustomEvent(eventName, { detail }));
    };

    return {
      'open-notifications': () => dispatch('aos:open-notifications'),
      'open-help': () => dispatch('aos:open-help'),
      'open-adapter-export': () => dispatch('aos:open-adapter-export', { scope: 'selected' }),
    };
  }, []);

  // Load recent commands from localStorage
  useEffect(() => {
    try {
      const saved = localStorage.getItem('aos_recent_commands');
      if (saved) {
        setRecentCommands(JSON.parse(saved));
      }
    } catch (err) {
      logger.error('Failed to load recent commands', { component: 'CommandPalette' }, toError(err));
    }
  }, []);

  // Save recent commands to localStorage
  useEffect(() => {
    try {
      const savedValue = JSON.stringify(recentCommands.slice(0, 10));
      localStorage.setItem('aos_recent_commands', savedValue);
      // Note: Real storage events are automatically dispatched to other tabs
      // when localStorage changes. We don't need to manually dispatch.
    } catch (err) {
      logger.error('Failed to save recent commands', { component: 'CommandPalette' }, toError(err));
    }
  }, [recentCommands]);

  // Listen for storage events from other tabs
  // Note: The 'storage' event only fires for changes from OTHER windows/tabs,
  // not for changes made in the current tab.
  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      // Only react to real cross-tab events (storageArea is set for real events)
      if (e.key === 'aos_recent_commands' && e.newValue && e.storageArea === localStorage) {
        try {
          const parsed = JSON.parse(e.newValue);
          setRecentCommands(Array.isArray(parsed) ? parsed : []);
        } catch (err) {
          logger.error('Failed to parse recent commands from storage event', {
            component: 'CommandPalette',
          }, toError(err));
        }
      }
    };

    window.addEventListener('storage', handleStorageChange);
    return () => window.removeEventListener('storage', handleStorageChange);
  }, []);

  // Helper function for retrying with exponential backoff
  const retryWithBackoff = useCallback(async function <T>(
    fn: () => Promise<T>,
    maxRetries: number = 3,
    baseDelay: number = 1000
  ): Promise<T> {
    let lastError: Error;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        return await fn();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        const apiError = error as ApiError;

        // Only retry on rate limiting (429) or network errors
        const shouldRetry = apiError?.status === 429 ||
                           apiError?.code === 'NETWORK_ERROR' ||
                           apiError?.name === 'NetworkError';

        if (!shouldRetry || attempt === maxRetries) {
          throw error;
        }

        // Exponential backoff: 1s, 2s, 4s...
        const delay = baseDelay * Math.pow(2, attempt);
        await new Promise(resolve => setTimeout(resolve, delay));
      }
    }

    throw lastError!;
  }, []);

  const refreshEntities = useCallback(async () => {
    setLoading(true);
    setRefreshError(null);
    try {
      // Use selected tenant for workers if available (and not 'default')
      const tenantIdForWorkers = selectedTenant && selectedTenant !== 'default' ? selectedTenant : undefined;

      const [adapters, tenants, policies, nodes, workers] = await Promise.allSettled([
        retryWithBackoff(() => apiClient.listAdapters()),
        retryWithBackoff(() => apiClient.listTenants()),
        retryWithBackoff(() => apiClient.listPolicies()),
        retryWithBackoff(() => apiClient.listNodes()),
        retryWithBackoff(() => apiClient.listWorkers(tenantIdForWorkers)),
      ]);

      const errors: string[] = [];

      setEntities(prev => ({
        adapters: adapters.status === 'fulfilled' ? adapters.value : prev.adapters,
        tenants: tenants.status === 'fulfilled' ? tenants.value : prev.tenants,
        policies: policies.status === 'fulfilled' ? policies.value : prev.policies,
        nodes: nodes.status === 'fulfilled' ? nodes.value : prev.nodes,
        workers: workers.status === 'fulfilled' ? workers.value : prev.workers,
      }));

      if (adapters.status === 'rejected') {
        const adapterError = adapters.reason as ApiError;
        const errorObj = toError(adapters.reason);
        const isPermissionError = adapterError?.status === 403 || adapterError?.status === 401;
        const isRateLimitError = adapterError?.status === 429;

        if (isPermissionError) {
          logger.debug('Adapters not accessible for CommandPalette (permission denied)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: adapterError?.status,
          });
        } else if (isRateLimitError) {
          logger.warn('Adapters temporarily unavailable due to rate limiting (retrying)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: adapterError?.status,
          });
        } else {
          errors.push('adapters');
          logger.error('Failed to load adapters for CommandPalette', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: adapterError?.status,
          }, errorObj);
        }
      }
      if (tenants.status === 'rejected') {
        const tenantError = tenants.reason as ApiError;
        const errorObj = toError(tenants.reason);
        const isPermissionError = tenantError?.status === 403 || tenantError?.status === 401;
        const isRateLimitError = tenantError?.status === 429;

        if (isPermissionError) {
          // Log but don't show in UI - tenants might not be accessible to all users
          logger.debug('Tenants not accessible for CommandPalette (permission denied)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: tenantError?.status,
          });
        } else if (isRateLimitError) {
          logger.warn('Tenants temporarily unavailable due to rate limiting (retrying)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: tenantError?.status,
          });
        } else {
          // Log and show in UI for other errors (network, database, etc.)
          errors.push('tenants');
          logger.error('Failed to load tenants for CommandPalette', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: tenantError?.status,
          }, errorObj);
        }
      }
      if (policies.status === 'rejected') {
        const policyError = policies.reason as ApiError;
        const errorObj = toError(policies.reason);
        const isPermissionError = policyError?.status === 403 || policyError?.status === 401;
        const isRateLimitError = policyError?.status === 429;

        if (isPermissionError) {
          logger.debug('Policies not accessible for CommandPalette (permission denied)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: policyError?.status,
          });
        } else if (isRateLimitError) {
          logger.warn('Policies temporarily unavailable due to rate limiting (retrying)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: policyError?.status,
          });
        } else {
          errors.push('policies');
          logger.error('Failed to load policies for CommandPalette', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: policyError?.status,
          }, errorObj);
        }
      }
      if (nodes.status === 'rejected') {
        const nodeError = nodes.reason as ApiError;
        const errorObj = toError(nodes.reason);
        const isPermissionError = nodeError?.status === 403 || nodeError?.status === 401;
        const isRateLimitError = nodeError?.status === 429;

        if (isPermissionError) {
          logger.debug('Nodes not accessible for CommandPalette (permission denied)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: nodeError?.status,
          });
        } else if (isRateLimitError) {
          // Rate limiting is handled by retry logic, don't show as error to user
          logger.warn('Nodes temporarily unavailable due to rate limiting (retrying)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: nodeError?.status,
          });
        } else {
          errors.push('nodes');
          logger.error('Failed to load nodes for CommandPalette', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: nodeError?.status,
          }, errorObj);
        }
      }
      if (workers.status === 'rejected') {
        const workerError = workers.reason as ApiError;
        const errorObj = toError(workers.reason);
        const isPermissionError = workerError?.status === 403 || workerError?.status === 401;
        const isRateLimitError = workerError?.status === 429;

        if (isPermissionError) {
          // Log but don't show in UI - workers might not be accessible to all users
          logger.debug('Workers not accessible for CommandPalette (permission denied)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: workerError?.status,
          });
        } else if (isRateLimitError) {
          logger.warn('Workers temporarily unavailable due to rate limiting (retrying)', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: workerError?.status,
          });
        } else {
          // Log warning but don't show in UI - workers loading failure is non-critical
          logger.warn('Failed to load workers for CommandPalette', {
            component: 'CommandPaletteContext',
            operation: 'refreshEntities',
            error: errorObj.message,
            status: workerError?.status,
          });
        }
      }

      // Only show critical errors in UI (non-critical errors like workers are logged but not displayed)
      if (errors.length > 0) {
        setRefreshError(`Failed to load ${errors.join(', ')}. Showing last known data.`);
      } else {
        setRefreshError(null);
      }

      setLastUpdated(new Date().toISOString());
    } catch (err) {
      logger.error('Failed to refresh entities', { component: 'CommandPalette' }, toError(err));
      setRefreshError(err instanceof Error ? err.message : 'Unexpected error while refreshing entities');
    } finally {
      setLoading(false);
    }
  }, [selectedTenant, retryWithBackoff]);

  // Initial load and periodic refresh - only after authentication
  useEffect(() => {
    // Only start refreshing if user is authenticated and auth is not loading
    if (!authLoading && user) {
      refreshEntities();
      // Refresh every 3 minutes to reduce rate limiting pressure
      const interval = setInterval(refreshEntities, 180000);
      return () => clearInterval(interval);
    }
  }, [refreshEntities, user, authLoading]);

  // Simple string matching search
  const searchItems = useCallback((query: string): CommandItem[] => {
    if (!query.trim()) {
      return [];
    }

    const lowerQuery = query.toLowerCase();
    const results: CommandItem[] = [];

    // Search routes
    for (const route of providedRoutes) {
      const routeUrl = route.url?.toLowerCase() ?? '';
      const actionId = route.actionId?.toLowerCase() ?? '';
      if (
        route.title.toLowerCase().includes(lowerQuery) ||
        route.description?.toLowerCase().includes(lowerQuery) ||
        routeUrl.includes(lowerQuery) ||
        actionId.includes(lowerQuery)
      ) {
        results.push(route);
      }
    }

    // Search adapters
    for (const adapter of entities.adapters) {
      if (
        adapter.name?.toLowerCase().includes(lowerQuery) ||
        adapter.adapter_id?.toLowerCase().includes(lowerQuery) ||
        adapter.framework?.toLowerCase().includes(lowerQuery) ||
        adapter.category?.toLowerCase().includes(lowerQuery)
      ) {
        results.push({
          id: `adapter-${adapter.adapter_id}`,
          type: 'adapter',
          title: adapter.name || adapter.adapter_id,
          description: `${adapter.framework || 'Unknown'} • ${adapter.category || 'Unknown category'}`,
          url: `/adapters?adapter=${encodeURIComponent(adapter.adapter_id)}`,
          entityId: adapter.adapter_id,
          group: 'Adapters',
          metadata: {
            entity: adapter,
            state: adapter.current_state,
            actions: [
              {
                id: 'navigate',
                kind: 'navigate',
                label: 'Open adapter details',
                url: `/adapters?adapter=${encodeURIComponent(adapter.adapter_id)}`,
              },
              {
                id: 'export',
                kind: 'export',
                label: 'Download manifest',
                url: `/v1/adapters/${adapter.adapter_id}/manifest`,
              },
            ],
          },
        });
      }
    }

    // Search tenants
    for (const tenant of entities.tenants) {
      if (
        tenant.name?.toLowerCase().includes(lowerQuery) ||
        tenant.id?.toLowerCase().includes(lowerQuery)
      ) {
        results.push({
          id: `tenant-${tenant.id}`,
          type: 'tenant',
          title: tenant.name,
          description: `Organization • ${tenant.isolation_level || 'Unknown isolation'}`,
          url: `/tenants?tenant=${encodeURIComponent(tenant.id)}`,
          entityId: tenant.id,
          group: 'Organizations',
          metadata: {
            entity: tenant,
            actions: [
              {
                id: 'navigate',
                kind: 'navigate',
                label: 'Open organization overview',
                url: `/tenants?tenant=${encodeURIComponent(tenant.id)}`,
              },
              {
                id: 'export',
                kind: 'export',
                label: 'Export organization manifest',
                url: `/v1/tenants/${tenant.id}/export`,
              },
            ],
          },
        });
      }
    }

    // Search policies
    for (const policy of entities.policies) {
      if (policy.cpid?.toLowerCase().includes(lowerQuery)) {
        results.push({
          id: `policy-${policy.cpid}`,
          type: 'policy',
          title: policy.cpid,
          description: `Policy • ${policy.schema_hash?.slice(0, 8) || 'No hash'}`,
          url: `/policies?policy=${encodeURIComponent(policy.cpid)}`,
          entityId: policy.cpid,
          group: 'Policies',
          metadata: {
            entity: policy,
            actions: [
              {
                id: 'navigate',
                kind: 'navigate',
                label: 'Open policy',
                url: `/policies?policy=${encodeURIComponent(policy.cpid)}`,
              },
              {
                id: 'export',
                kind: 'export',
                label: 'Download policy',
                url: `/v1/policies/${policy.cpid}`,
              },
            ],
          },
        });
      }
    }

    // Search nodes
    for (const node of entities.nodes) {
      if (
        node.hostname?.toLowerCase().includes(lowerQuery) ||
        node.id?.toLowerCase().includes(lowerQuery)
      ) {
        results.push({
          id: `node-${node.id}`,
          type: 'node',
          title: node.hostname || node.id,
          description: `Node • ${node.status || 'Unknown status'}`,
          url: `/admin?node=${encodeURIComponent(node.id)}`,
          entityId: node.id,
          group: 'Nodes',
          metadata: {
            entity: node,
            actions: [
              {
                id: 'navigate',
                kind: 'navigate',
                label: 'Open node diagnostics',
                url: `/admin?node=${encodeURIComponent(node.id)}`,
              },
              {
                id: 'api',
                kind: 'api',
                label: 'API endpoint',
                url: `/v1/nodes/${node.id}`,
              },
            ],
          },
        });
      }
    }

    // Search workers
    for (const worker of entities.workers) {
      if (
        worker.id?.toLowerCase().includes(lowerQuery) ||
        worker.tenant_id?.toLowerCase().includes(lowerQuery)
      ) {
        results.push({
          id: `worker-${worker.id}`,
          type: 'worker',
          title: `Worker ${worker.id.slice(0, 8)}`,
          description: `Worker • Tenant: ${worker.tenant_id || 'Unknown'}`,
          url: `/admin?worker=${encodeURIComponent(worker.id)}`,
          entityId: worker.id,
          group: 'Workers',
          metadata: {
            entity: worker,
            tenantId: worker.tenant_id,
            actions: [
              {
                id: 'navigate',
                kind: 'navigate',
                label: 'Open worker diagnostics',
                url: `/admin?worker=${encodeURIComponent(worker.id)}`,
              },
              {
                id: 'api',
                kind: 'api',
                label: 'API endpoint',
                url: `/v1/workers/${worker.id}`,
              },
            ],
          },
        });
      }
    }

    // Remove duplicates and sort by relevance (exact matches first, then partial)
    const uniqueResults = Array.from(
      new Map(results.map(item => [item.id, item])).values()
    );

    return uniqueResults.sort((a, b) => {
      const aExact = a.title.toLowerCase() === lowerQuery;
      const bExact = b.title.toLowerCase() === lowerQuery;
      if (aExact && !bExact) return -1;
      if (!aExact && bExact) return 1;
      
      const aStarts = a.title.toLowerCase().startsWith(lowerQuery);
      const bStarts = b.title.toLowerCase().startsWith(lowerQuery);
      if (aStarts && !bStarts) return -1;
      if (!aStarts && bStarts) return 1;

      return a.title.localeCompare(b.title);
    });
  }, [providedRoutes, entities]);

  const searchResults = searchQuery ? searchItems(searchQuery) : [];

  const executeCommand = useCallback((item: CommandItem) => {
    // Add to recent commands
    setRecentCommands(prev => {
      const filtered = prev.filter(cmd => cmd.item.id !== item.id);
      return [
        { item, timestamp: new Date().toISOString() },
        ...filtered,
      ].slice(0, 10);
    });

    const run = async () => {
      try {
        if (item.actionId) {
          const action = commandActions[item.actionId];
          if (action) {
            await Promise.resolve(action());
          } else {
            logger.warn('Unknown command palette action', {
              component: 'CommandPalette',
              operation: 'executeCommand',
              actionId: item.actionId,
            });
          }
        } else if (item.url) {
          navigate(item.url);
        }
      } catch (err) {
        logger.error('Command palette action failed', {
          component: 'CommandPalette',
          operation: 'executeCommand',
          actionId: item.actionId ?? null,
          url: item.url ?? null,
        }, toError(err));
      } finally {
        setIsOpen(false);
        setSearchQuery('');
      }
    };

    void run();
  }, [commandActions, navigate]);

  const openPalette = useCallback(() => {
    setIsOpen(true);
    // Refresh entities when opening, but only if authenticated
    if (user && !authLoading) {
      refreshEntities();
    }
  }, [refreshEntities, user, authLoading]);

  const closePalette = useCallback(() => {
    setIsOpen(false);
    setSearchQuery('');
  }, []);

  return (
    <CommandPaletteContext.Provider
      value={{
        isOpen,
        openPalette,
        closePalette,
        searchQuery,
        setSearchQuery,
        searchResults,
        recentCommands,
        routes: providedRoutes,
        entities,
        loading,
        executeCommand,
        refreshEntities,
        refreshError,
        lastUpdated,
      }}
    >
      {children}
    </CommandPaletteContext.Provider>
  );
}

export function useCommandPalette(): CommandPaletteContextValue {
  const context = useContext(CommandPaletteContext);
  if (!context) {
    throw new Error('useCommandPalette must be used within CommandPaletteProvider');
  }
  return context;
}
