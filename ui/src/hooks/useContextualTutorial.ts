import { useState, useEffect, useCallback, useRef } from 'react';
import type { TutorialConfig } from '@/components/ContextualTutorial';
import { Tutorial } from '@/api/types';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';

const STORAGE_KEY = 'aos_tutorials';

// Map numeric position to string position
function mapPositionToString(position: number | undefined): 'top' | 'bottom' | 'left' | 'right' | 'center' | undefined {
  if (position === undefined) return undefined;
  const positionMap: Record<number, 'top' | 'bottom' | 'left' | 'right' | 'center'> = {
    0: 'top',
    1: 'right',
    2: 'bottom',
    3: 'left',
    4: 'center',
  };
  return positionMap[position] ?? 'center';
}

// Convert API Tutorial to TutorialConfig
function tutorialToConfig(tutorial: Tutorial): TutorialConfig {
  return {
    id: tutorial.id,
    title: tutorial.title,
    description: tutorial.description,
    steps: tutorial.steps.map(step => ({
      id: step.id || crypto.randomUUID(),
      title: step.title,
      content: step.content,
      targetSelector: step.target_selector || undefined,
      position: mapPositionToString(step.position),
    })),
    trigger: (tutorial.trigger || 'manual') as TutorialConfig['trigger'],
    dismissible: tutorial.dismissible,
  };
}

// Storage event payload type
interface TutorialStoragePayload {
  [tutorialId: string]: {
    completed: boolean;
    dismissed: boolean;
    completed_at?: string;
    dismissed_at?: string;
    ts: string;
  };
}

export function useContextualTutorial(pagePath: string) {
  const [activeTutorial, setActiveTutorial] = useState<TutorialConfig | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [tutorials, setTutorials] = useState<Tutorial[]>([]);
  const [statusCache, setStatusCache] = useState<Record<string, { completed: boolean; dismissed: boolean }>>({});
  const [loading, setLoading] = useState(true);
  const storageListenerRef = useRef<((e: StorageEvent) => void) | null>(null);

  // Fetch tutorials from API
  const fetchTutorials = useCallback(async () => {
    try {
      setLoading(true);
      const fetchedTutorials = await apiClient.listTutorials();
      setTutorials(fetchedTutorials);

      // Build status cache
      const cache: Record<string, { completed: boolean; dismissed: boolean }> = {};
      for (const tutorial of fetchedTutorials) {
        cache[tutorial.id] = {
          completed: tutorial.completed,
          dismissed: tutorial.dismissed,
        };
      }
      setStatusCache(cache);

      // Sync to localStorage for cross-tab
      const payload: TutorialStoragePayload = {};
      for (const tutorial of fetchedTutorials) {
        payload[tutorial.id] = {
          completed: tutorial.completed,
          dismissed: tutorial.dismissed,
          completed_at: tutorial.completed_at,
          dismissed_at: tutorial.dismissed_at,
          ts: new Date().toISOString(),
        };
      }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
        // Dispatch storage event for immediate cross-tab sync
        window.dispatchEvent(new StorageEvent('storage', {
          key: STORAGE_KEY,
          newValue: JSON.stringify(payload),
        }));
      } catch (err) {
        logger.error('Failed to save tutorials to localStorage', {
          component: 'useContextualTutorial',
          operation: 'fetchTutorials',
        }, toError(err));
      }
    } catch (err) {
      logger.error('Failed to fetch tutorials', {
        component: 'useContextualTutorial',
        operation: 'fetchTutorials',
      }, toError(err));

      // Fallback: try to load from localStorage
      try {
        const saved = localStorage.getItem(STORAGE_KEY);
        if (saved) {
          const payload: TutorialStoragePayload = JSON.parse(saved);
          const cache: Record<string, { completed: boolean; dismissed: boolean }> = {};
          for (const [id, status] of Object.entries(payload)) {
            cache[id] = {
              completed: status.completed,
              dismissed: status.dismissed,
            };
          }
          setStatusCache(cache);
        }
      } catch (fallbackErr) {
        logger.error('Failed to load tutorials from localStorage fallback', {
          component: 'useContextualTutorial',
          operation: 'fetchTutorials',
        }, toError(fallbackErr));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  // Load tutorials on mount
  useEffect(() => {
    fetchTutorials();
  }, [fetchTutorials]);

  // Listen for storage events from other tabs
  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY && e.newValue) {
        try {
          const payload: TutorialStoragePayload = JSON.parse(e.newValue);
          const cache: Record<string, { completed: boolean; dismissed: boolean }> = {};
          for (const [id, status] of Object.entries(payload)) {
            cache[id] = {
              completed: status.completed,
              dismissed: status.dismissed,
            };
          }
          setStatusCache(cache);
          // Optionally refresh from API to ensure consistency
          fetchTutorials();
        } catch (err) {
          logger.error('Failed to parse tutorials from storage event', {
            component: 'useContextualTutorial',
            operation: 'storage_listener',
          }, toError(err));
        }
      }
    };

    storageListenerRef.current = handleStorageChange;
    window.addEventListener('storage', handleStorageChange);
    return () => {
      if (storageListenerRef.current) {
        window.removeEventListener('storage', storageListenerRef.current);
      }
    };
  }, [fetchTutorials]);

  // Get available tutorials for current page
  const getAvailableTutorials = useCallback((): TutorialConfig[] => {
    const pageTutorials: Record<string, string[]> = {
      '/training': ['training-tutorial'],
      '/adapters': ['adapter-management-tutorial'],
      '/security/policies': ['policy-management-tutorial'],
      '/dashboard': ['dashboard-tutorial'],
    };

    const tutorialIds = pageTutorials[pagePath] || [];
    return tutorials
      .filter(t => tutorialIds.includes(t.id))
      .map(tutorialToConfig);
  }, [tutorials, pagePath]);

  const availableTutorials = getAvailableTutorials();

  // Check if tutorial is completed (from cache or API)
  const isTutorialCompleted = useCallback((tutorialId: string): boolean => {
    return statusCache[tutorialId]?.completed ?? false;
  }, [statusCache]);

  // Check if tutorial is dismissed (from cache or API)
  const isTutorialDismissed = useCallback((tutorialId: string): boolean => {
    return statusCache[tutorialId]?.dismissed ?? false;
  }, [statusCache]);

  // Update status cache and emit storage event
  const updateStatusCache = useCallback((tutorialId: string, updates: { completed?: boolean; dismissed?: boolean }) => {
    setStatusCache(prev => {
      const updated = { ...prev };
      if (!updated[tutorialId]) {
        updated[tutorialId] = { completed: false, dismissed: false };
      }
      if (updates.completed !== undefined) {
        updated[tutorialId].completed = updates.completed;
      }
      if (updates.dismissed !== undefined) {
        updated[tutorialId].dismissed = updates.dismissed;
      }

      // Emit storage event for cross-tab sync
      const payload: TutorialStoragePayload = {};
      for (const [id, status] of Object.entries(updated)) {
        payload[id] = {
          ...status,
          ts: new Date().toISOString(),
        };
      }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
        window.dispatchEvent(new StorageEvent('storage', {
          key: STORAGE_KEY,
          newValue: JSON.stringify(payload),
        }));
      } catch (err) {
        logger.error('Failed to emit tutorial storage event', {
          component: 'useContextualTutorial',
          operation: 'updateStatusCache',
          tutorialId,
        }, toError(err));
      }

      return updated;
    });
  }, []);

  // Start a tutorial
  const startTutorial = useCallback((tutorialId?: string) => {
    let tutorial: TutorialConfig | null = null;

    if (tutorialId) {
      tutorial = availableTutorials.find(t => t.id === tutorialId) || null;
    } else {
      tutorial = availableTutorials.find(
        t => !isTutorialCompleted(t.id) && !isTutorialDismissed(t.id)
      ) || availableTutorials[0] || null;
    }

    if (tutorial) {
      setActiveTutorial(tutorial);
      setIsOpen(true);
    }
  }, [availableTutorials, isTutorialCompleted, isTutorialDismissed]);

  // Close tutorial
  const closeTutorial = useCallback(async () => {
    setIsOpen(false);
    if (activeTutorial) {
      // Mark as dismissed if dismissible
      if (activeTutorial.dismissible) {
        try {
          await apiClient.markTutorialDismissed(activeTutorial.id);
          updateStatusCache(activeTutorial.id, { dismissed: true });
          
          // Refresh from API to ensure consistency
          await fetchTutorials();
        } catch (err) {
          logger.error('Failed to mark tutorial as dismissed', {
            component: 'useContextualTutorial',
            operation: 'closeTutorial',
            tutorialId: activeTutorial.id,
          }, toError(err));
        }
      }
    }
  }, [activeTutorial, updateStatusCache, fetchTutorials]);

  // Complete tutorial
  const completeTutorial = useCallback(async () => {
    if (activeTutorial) {
      try {
        await apiClient.markTutorialCompleted(activeTutorial.id);
        updateStatusCache(activeTutorial.id, { completed: true });
        
        // Refresh from API to ensure consistency
        await fetchTutorials();
      } catch (err) {
        logger.error('Failed to mark tutorial as completed', {
          component: 'useContextualTutorial',
          operation: 'completeTutorial',
          tutorialId: activeTutorial.id,
        }, toError(err));
        // Still close tutorial even on error
      }
    }
    setIsOpen(false);
    setActiveTutorial(null);
  }, [activeTutorial, updateStatusCache, fetchTutorials]);

  // Auto-start tutorial if configured
  useEffect(() => {
    const autoTutorial = availableTutorials.find(
      t => t.trigger === 'auto' && !isTutorialCompleted(t.id) && !isTutorialDismissed(t.id)
    );

    if (autoTutorial) {
      const timer = setTimeout(() => {
        startTutorial(autoTutorial.id);
      }, 1000);
      return () => clearTimeout(timer);
    }
  }, [pagePath, availableTutorials, isTutorialCompleted, isTutorialDismissed, startTutorial]);

  return {
    activeTutorial,
    isOpen,
    availableTutorials,
    startTutorial,
    closeTutorial,
    completeTutorial,
    isTutorialCompleted,
    isTutorialDismissed,
    loading,
  };
}
