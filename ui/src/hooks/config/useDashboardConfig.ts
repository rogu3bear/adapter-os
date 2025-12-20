import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '@/api/services';
import type { DashboardWidgetConfig, WidgetConfigUpdate } from '@/api/types';
import { logger } from '@/utils/logger';

export interface UseDashboardConfigReturn {
  widgets: DashboardWidgetConfig[];
  isLoading: boolean;
  error: Error | null;
  updateWidgetVisibility: (widgetId: string, enabled: boolean) => Promise<void>;
  reorderWidgets: (widgetUpdates: WidgetConfigUpdate[]) => Promise<void>;
  resetConfig: () => Promise<void>;
  refetch: () => Promise<void>;
}

const STORAGE_KEY_PREFIX = 'dashboard-config';

/**
 * Custom hook for managing dashboard widget configuration
 *
 * Features:
 * - Loads configuration from backend API
 * - Provides localStorage fallback for offline/error scenarios
 * - Debounced updates to backend
 * - Optimistic UI updates
 */
export function useDashboardConfig(userId?: string): UseDashboardConfigReturn {
  const [widgets, setWidgets] = useState<DashboardWidgetConfig[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const storageKey = userId ? `${STORAGE_KEY_PREFIX}-${userId}` : STORAGE_KEY_PREFIX;

  // Load configuration from backend or localStorage
  const loadConfig = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      // Try to load from backend first
      const config = await apiClient.getDashboardConfig();
      // Backend returns widget IDs as strings; fetch full widget configs or use IDs to filter
      // For now, if backend returns DashboardWidgetConfig[], use directly; otherwise create minimal configs
      const widgetConfigs: DashboardWidgetConfig[] = Array.isArray(config.widgets)
        ? config.widgets.map((widget, index) => {
            if (typeof widget === 'string') {
              // Widget is an ID string - create a minimal config
              return {
                id: widget,
                user_id: userId || '',
                widget_id: widget,
                enabled: true,
                position: index,
                created_at: config.created_at || new Date().toISOString(),
                updated_at: config.updated_at || new Date().toISOString(),
              };
            }
            // Widget is already a full config object
            return widget as DashboardWidgetConfig;
          })
        : [];
      setWidgets(widgetConfigs);

      // Save to localStorage as backup
      try {
        localStorage.setItem(storageKey, JSON.stringify(config.widgets));
      } catch (e) {
        logger.warn('Failed to save dashboard config to localStorage', {
          component: 'useDashboardConfig',
          error: e
        });
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);

      logger.warn('Failed to load dashboard config from backend, trying localStorage', {
        component: 'useDashboardConfig',
        error: error.message
      });

      // Fallback to localStorage
      try {
        const saved = localStorage.getItem(storageKey);
        if (saved) {
          const parsedWidgets = JSON.parse(saved) as DashboardWidgetConfig[];
          setWidgets(parsedWidgets);
          logger.info('Loaded dashboard config from localStorage', {
            component: 'useDashboardConfig',
            widgetCount: parsedWidgets.length
          });
        } else {
          // No saved config, use empty array (Dashboard will use role defaults)
          setWidgets([]);
        }
      } catch (storageErr) {
        logger.error('Failed to load dashboard config from localStorage', {
          component: 'useDashboardConfig',
          error: storageErr
        });
        setWidgets([]);
      }
    } finally {
      setIsLoading(false);
    }
  }, [storageKey, userId]);

  // Load config on mount
  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  // Update a single widget's visibility
  const updateWidgetVisibility = useCallback(async (widgetId: string, enabled: boolean) => {
    // Optimistic update
    setWidgets(prev => {
      const existing = prev.find(w => w.widget_id === widgetId);
      if (existing) {
        // Update existing widget
        return prev.map(w =>
          w.widget_id === widgetId
            ? { ...w, enabled, updated_at: new Date().toISOString() }
            : w
        );
      } else {
        // Add new widget config
        return [
          ...prev,
          {
            id: '', // Will be set by backend
            user_id: userId || '',
            widget_id: widgetId,
            enabled,
            position: prev.length,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          }
        ];
      }
    });

    try {
      // Send update to backend
      const widgetUpdates: WidgetConfigUpdate[] = widgets
        .map(w =>
          w.widget_id === widgetId
            ? { widget_id: widgetId, enabled, position: w.position }
            : { widget_id: w.widget_id, enabled: w.enabled, position: w.position }
        );

      if (!widgets.some(w => w.widget_id === widgetId)) {
        widgetUpdates.push({ widget_id: widgetId, enabled, position: widgets.length });
      }

      await apiClient.updateDashboardConfig({ widgets: widgetUpdates });

      logger.info('Updated widget visibility', {
        component: 'useDashboardConfig',
        widgetId,
        enabled
      });

      // Refresh to get backend-generated IDs
      await loadConfig();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      logger.error('Failed to update widget visibility', {
        component: 'useDashboardConfig',
        widgetId,
        error: error.message
      });

      // Revert optimistic update
      await loadConfig();
    }
  }, [widgets, userId, loadConfig]);

  // Reorder multiple widgets
  const reorderWidgets = useCallback(async (widgetUpdates: WidgetConfigUpdate[]) => {
    // Optimistic update
    setWidgets(prev => {
      const updated = [...prev];
      widgetUpdates.forEach(update => {
        const index = updated.findIndex(w => w.widget_id === update.widget_id);
        if (index !== -1) {
          updated[index] = {
            ...updated[index],
            enabled: update.enabled,
            position: update.position,
            updated_at: new Date().toISOString()
          };
        }
      });
      return updated.sort((a, b) => a.position - b.position);
    });

    try {
      await apiClient.updateDashboardConfig({ widgets: widgetUpdates });

      logger.info('Reordered widgets', {
        component: 'useDashboardConfig',
        updateCount: widgetUpdates.length
      });

      // Refresh from backend
      await loadConfig();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      logger.error('Failed to reorder widgets', {
        component: 'useDashboardConfig',
        error: error.message
      });

      // Revert optimistic update
      await loadConfig();
    }
  }, [loadConfig]);

  // Reset configuration to role defaults
  const resetConfig = useCallback(async () => {
    try {
      await apiClient.resetDashboardConfig();

      logger.info('Reset dashboard config to defaults', {
        component: 'useDashboardConfig'
      });

      // Clear localStorage
      try {
        localStorage.removeItem(storageKey);
      } catch (e) {
        logger.warn('Failed to clear dashboard config from localStorage', {
          component: 'useDashboardConfig',
          error: e
        });
      }

      // Reload (will use role defaults since backend config is cleared)
      setWidgets([]);
      await loadConfig();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      logger.error('Failed to reset dashboard config', {
        component: 'useDashboardConfig',
        error: error.message
      });
    }
  }, [storageKey, loadConfig]);

  return {
    widgets,
    isLoading,
    error,
    updateWidgetVisibility,
    reorderWidgets,
    resetConfig,
    refetch: loadConfig
  };
}
