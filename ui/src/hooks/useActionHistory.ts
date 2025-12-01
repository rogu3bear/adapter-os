import { useState, useCallback, useRef, useEffect } from 'react';
import { logger, toError } from '@/utils/logger';

export interface ActionHistoryItem<T = unknown> {
  id: string;
  action: string;
  timestamp: number;
  description: string;
  undo: () => Promise<void> | void;
  redo?: () => Promise<void> | void;
  metadata?: T;
}

interface ActionHistoryOptions {
  maxHistorySize?: number;
}

export function useActionHistory(options: ActionHistoryOptions = {}) {
  const { maxHistorySize = 50 } = options;
  
  const [history, setHistory] = useState<ActionHistoryItem[]>([]);
  const [currentIndex, setCurrentIndex] = useState<number>(-1);
  const historyRef = useRef<ActionHistoryItem[]>([]);
  const indexRef = useRef<number>(-1);

  // Keep refs in sync
  useEffect(() => {
    historyRef.current = history;
    indexRef.current = currentIndex;
  }, [history, currentIndex]);

  const addAction = useCallback((action: Omit<ActionHistoryItem, 'id' | 'timestamp'>) => {
    const item: ActionHistoryItem = {
      ...action,
      id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      timestamp: Date.now(),
    };

    setHistory(prev => {
      // Remove any actions after current index (when undoing, then doing a new action)
      const newHistory = prev.slice(0, indexRef.current + 1);
      newHistory.push(item);
      
      // Trim history if too large
      if (newHistory.length > maxHistorySize) {
        return newHistory.slice(newHistory.length - maxHistorySize);
      }
      
      return newHistory;
    });
    
    setCurrentIndex(prev => {
      const newIndex = Math.min(prev + 1, maxHistorySize - 1);
      return newIndex;
    });
  }, [maxHistorySize]);

  const undo = useCallback(async () => {
    if (indexRef.current < 0) {
      return false;
    }

    const action = historyRef.current[indexRef.current];
    if (action) {
      try {
        await action.undo();
        setCurrentIndex(prev => prev - 1);
        return true;
      } catch (error) {
        logger.error('Failed to undo action', {
          component: 'useActionHistory',
          operation: 'undo',
          actionId: action.id,
          actionType: action.action,
        }, toError(error));
        return false;
      }
    }
    return false;
  }, []);

  const redo = useCallback(async () => {
    if (indexRef.current >= historyRef.current.length - 1) {
      return false;
    }

    const nextIndex = indexRef.current + 1;
    const action = historyRef.current[nextIndex];
    if (action) {
      try {
        if (action.redo) {
          await action.redo();
        } else {
          // If no redo function, we can't redo this action
          return false;
        }
        setCurrentIndex(prev => prev + 1);
        return true;
      } catch (error) {
        logger.error('Failed to redo action', {
          component: 'useActionHistory',
          operation: 'redo',
          actionId: action.id,
          actionType: action.action,
        }, toError(error));
        return false;
      }
    }
    return false;
  }, []);

  const canUndo = currentIndex >= 0;
  const canRedo = currentIndex < history.length - 1;

  const clearHistory = useCallback(() => {
    setHistory([]);
    setCurrentIndex(-1);
  }, []);

  const getHistory = useCallback(() => {
    return history;
  }, [history]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Cmd+Z or Ctrl+Z for undo
      if ((event.metaKey || event.ctrlKey) && event.key === 'z' && !event.shiftKey) {
        event.preventDefault();
        if (canUndo) {
          undo();
        }
      }
      // Cmd+Shift+Z or Ctrl+Shift+Z for redo
      else if ((event.metaKey || event.ctrlKey) && event.key === 'z' && event.shiftKey) {
        event.preventDefault();
        if (canRedo) {
          redo();
        }
      }
      // Cmd+Y or Ctrl+Y for redo (alternate)
      else if ((event.metaKey || event.ctrlKey) && event.key === 'y') {
        event.preventDefault();
        if (canRedo) {
          redo();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [canUndo, canRedo, undo, redo]);

  return {
    addAction,
    undo,
    redo,
    canUndo,
    canRedo,
    clearHistory,
    getHistory,
    historyCount: history.length,
    currentAction: history[currentIndex] || null,
  };
}

