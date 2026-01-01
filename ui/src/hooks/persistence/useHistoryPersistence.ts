//! History Persistence Hook
//!
//! Provides utilities for persisting and managing action history in localStorage/IndexedDB.

import { useCallback, useEffect, useRef } from 'react';
import { logger } from '@/utils/logger';
import { ActionHistoryItem } from '@/types/history';
import { getStorageStatus, evictOldestEntries } from '@/utils/storage';

const STORAGE_KEY = 'aos_action_history_v2';
const BACKUP_KEY = 'aos_action_history_backup';
const MAX_STORED_SIZE = 1000;
const STORAGE_SIZE_LIMIT = 5 * 1024 * 1024; // 5MB

interface StorageQuota {
  used: number;
  total: number;
  available: number;
}

interface PersistenceConfig {
  useIndexedDB?: boolean;
  useLocalStorage?: boolean;
  autoBackup?: boolean;
  backupInterval?: number; // ms
}

export function useHistoryPersistence(config: PersistenceConfig = {}) {
  const {
    useIndexedDB = true,
    useLocalStorage = true,
    autoBackup = true,
    backupInterval = 3600000, // 1 hour
  } = config;

  const backupTimerRef = useRef<NodeJS.Timeout>();

  // Save history to localStorage
  const saveToLocalStorage = useCallback((actions: ActionHistoryItem[]): boolean => {
    try {
      if (!useLocalStorage || typeof window === 'undefined') return false;

      const trimmed = actions.slice(-MAX_STORED_SIZE);
      const data = JSON.stringify({
        version: 2,
        timestamp: Date.now(),
        actions: trimmed,
      });

      // Check size
      const sizeInBytes = new Blob([data]).size;
      if (sizeInBytes > STORAGE_SIZE_LIMIT) {
        logger.warn('History data exceeds size limit', {
          component: 'useHistoryPersistence',
          sizeInBytes,
          limit: STORAGE_SIZE_LIMIT,
        });
        return false;
      }

      localStorage.setItem(STORAGE_KEY, data);
      return true;
    } catch (error) {
      logger.warn('Failed to save history to localStorage', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }, [useLocalStorage]);

  // Load history from localStorage
  const loadFromLocalStorage = useCallback((): ActionHistoryItem[] => {
    try {
      if (!useLocalStorage || typeof window === 'undefined') return [];

      const data = localStorage.getItem(STORAGE_KEY);
      if (!data) return [];

      const parsed = JSON.parse(data);
      if (parsed.version === 2 && Array.isArray(parsed.actions)) {
        logger.info('Loaded history from localStorage', {
          component: 'useHistoryPersistence',
          count: parsed.actions.length,
        });
        return parsed.actions;
      }

      return [];
    } catch (error) {
      logger.warn('Failed to load history from localStorage', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return [];
    }
  }, [useLocalStorage]);

  // Save to IndexedDB
  const saveToIndexedDB = useCallback(async (actions: ActionHistoryItem[]): Promise<boolean> => {
    try {
      if (!useIndexedDB || typeof window === 'undefined') return false;

      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open('aos_history', 1);
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
        request.onupgradeneeded = (event) => {
          const db = (event.target as IDBOpenDBRequest).result;
          if (!db.objectStoreNames.contains('history')) {
            db.createObjectStore('history', { keyPath: 'id' });
          }
        };
      });

      const transaction = db.transaction('history', 'readwrite');
      const store = transaction.objectStore('history');

      // Clear old data
      await new Promise<void>((resolve, reject) => {
        const clearRequest = store.clear();
        clearRequest.onsuccess = () => resolve();
        clearRequest.onerror = () => reject(clearRequest.error);
      });

      // Add new data
      const trimmed = actions.slice(-MAX_STORED_SIZE);
      for (const action of trimmed) {
        await new Promise<void>((resolve, reject) => {
          const addRequest = store.add(action);
          addRequest.onsuccess = () => resolve();
          addRequest.onerror = () => reject(addRequest.error);
        });
      }

      db.close();

      logger.info('Saved history to IndexedDB', {
        component: 'useHistoryPersistence',
        count: trimmed.length,
      });

      return true;
    } catch (error) {
      logger.warn('Failed to save to IndexedDB', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }, [useIndexedDB]);

  // Load from IndexedDB
  const loadFromIndexedDB = useCallback(async (): Promise<ActionHistoryItem[]> => {
    try {
      if (!useIndexedDB || typeof window === 'undefined') return [];

      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open('aos_history', 1);
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
      });

      const transaction = db.transaction('history', 'readonly');
      const store = transaction.objectStore('history');

      const actions = await new Promise<ActionHistoryItem[]>((resolve, reject) => {
        const getAllRequest = store.getAll();
        getAllRequest.onsuccess = () => resolve(getAllRequest.result);
        getAllRequest.onerror = () => reject(getAllRequest.error);
      });

      db.close();

      logger.info('Loaded history from IndexedDB', {
        component: 'useHistoryPersistence',
        count: actions.length,
      });

      return actions;
    } catch (error) {
      logger.warn('Failed to load from IndexedDB', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return [];
    }
  }, [useIndexedDB]);

  // Create backup
  const createBackup = useCallback((actions: ActionHistoryItem[]): string => {
    const backup = {
      version: 2,
      timestamp: Date.now(),
      count: actions.length,
      actions,
    };

    const json = JSON.stringify(backup, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);

    logger.info('Created history backup', {
      component: 'useHistoryPersistence',
      count: actions.length,
      size: blob.size,
    });

    return url;
  }, []);

  // Download backup
  const downloadBackup = useCallback((actions: ActionHistoryItem[], filename?: string): void => {
    const url = createBackup(actions);
    const element = document.createElement('a');
    element.setAttribute('href', url);
    element.setAttribute('download', filename || `history-backup-${Date.now()}.json`);
    element.style.display = 'none';
    document.body.appendChild(element);
    element.click();
    document.body.removeChild(element);
    URL.revokeObjectURL(url);
  }, [createBackup]);

  // Import history
  const importHistory = useCallback(async (file: File): Promise<ActionHistoryItem[] | null> => {
    try {
      const text = await file.text();
      const backup = JSON.parse(text);

      if (backup.version !== 2 || !Array.isArray(backup.actions)) {
        logger.error('Invalid backup format', {
          component: 'useHistoryPersistence',
          version: backup.version,
        });
        return null;
      }

      logger.info('Imported history', {
        component: 'useHistoryPersistence',
        count: backup.actions.length,
      });

      return backup.actions;
    } catch (error) {
      logger.warn('Failed to import history', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }, []);

  // Get storage quota
  const getStorageQuota = useCallback(async (): Promise<StorageQuota | null> => {
    try {
      if (!navigator.storage || !navigator.storage.estimate) {
        return null;
      }

      const estimate = await navigator.storage.estimate();
      return {
        used: estimate.usage || 0,
        total: estimate.quota || 0,
        available: (estimate.quota || 0) - (estimate.usage || 0),
      };
    } catch (error) {
      logger.warn('Failed to get storage quota', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }, []);

  // Clear all storage
  const clearAllStorage = useCallback((): boolean => {
    try {
      if (useLocalStorage && typeof window !== 'undefined') {
        localStorage.removeItem(STORAGE_KEY);
        localStorage.removeItem(BACKUP_KEY);
      }

      logger.info('Cleared all history storage', {
        component: 'useHistoryPersistence',
      });

      return true;
    } catch (error) {
      logger.warn('Failed to clear storage', {
        component: 'useHistoryPersistence',
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }, [useLocalStorage]);

  // Save with quota check - warns at 80%, evicts at 90%
  const saveWithQuotaCheck = useCallback(async (actions: ActionHistoryItem[]): Promise<boolean> => {
    const status = await getStorageStatus();

    if (status?.shouldEvict) {
      logger.warn('Storage quota at 90%+, evicting oldest entries', {
        component: 'useHistoryPersistence',
        percent: Math.round(status.percent * 100),
      });
      evictOldestEntries(STORAGE_KEY, Math.floor(MAX_STORED_SIZE * 0.5));
    } else if (status?.shouldWarn) {
      logger.warn('Storage quota at 80%+', {
        component: 'useHistoryPersistence',
        percent: Math.round(status.percent * 100),
        used: status.used,
        total: status.total,
      });
    }

    return saveToLocalStorage(actions);
  }, [saveToLocalStorage]);

  // Setup auto backup
  useEffect(() => {
    if (!autoBackup) return;

    const setupAutoBackup = async () => {
      const actions = await loadFromIndexedDB();
      if (actions.length > 0) {
        const backupData = {
          version: 2,
          timestamp: Date.now(),
          actions,
        };
        if (typeof window !== 'undefined') {
          try {
            localStorage.setItem(BACKUP_KEY, JSON.stringify(backupData));
          } catch (error) {
            logger.warn('Failed to create auto backup', {
              component: 'useHistoryPersistence',
              error: error instanceof Error ? error.message : String(error),
            });
          }
        }
      }
    };

    backupTimerRef.current = setInterval(setupAutoBackup, backupInterval);
    setupAutoBackup();

    return () => {
      if (backupTimerRef.current) {
        clearInterval(backupTimerRef.current);
      }
    };
  }, [autoBackup, backupInterval, loadFromIndexedDB]);

  return {
    // LocalStorage
    saveToLocalStorage,
    saveWithQuotaCheck,
    loadFromLocalStorage,

    // IndexedDB
    saveToIndexedDB,
    loadFromIndexedDB,

    // Backup/Restore
    createBackup,
    downloadBackup,
    importHistory,

    // Storage management
    getStorageQuota,
    clearAllStorage,
  };
}

export default useHistoryPersistence;
