//! Undo/Redo Hook
//!
//! Provides undo/redo functionality for all mutations (create, edit, assign, update, delete, archive, etc.)
//! Following git-like pattern: every action is reversible until committed.
//!
//! Citations:
//! - docs/architecture/Determinism.md - Reversible operations pattern
//! - Plan: Data Operations Productivity Features - Undo/redo for all mutations

import { useState, useCallback, useRef } from 'react';
import { logger, toError } from '../utils/logger';

export interface UndoableAction<T = any> {
  id: string;
  type: string;
  description: string;
  timestamp: number;
  previousState: T;
  reverse: () => Promise<void> | void;
  // Optional: for operations that may need forward state
  forward?: () => Promise<void> | void;
}

interface UndoRedoState {
  history: UndoableAction[];
  currentIndex: number;
  maxHistorySize: number;
}

const MAX_HISTORY_SIZE = 50;

export function useUndoRedo(maxHistorySize: number = MAX_HISTORY_SIZE) {
  const [state, setState] = useState<UndoRedoState>({
    history: [],
    currentIndex: -1,
    maxHistorySize,
  });

  const addAction = useCallback(<T,>(
    action: Omit<UndoableAction<T>, 'id' | 'timestamp'>
  ) => {
    setState((prev) => {
      const newAction: UndoableAction<T> = {
        ...action,
        id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
        timestamp: Date.now(),
      };

      // Remove any actions after currentIndex (when we're in the middle of history)
      const newHistory = prev.history.slice(0, prev.currentIndex + 1);
      
      // Add new action
      newHistory.push(newAction);

      // Trim history if it exceeds max size
      const trimmedHistory = newHistory.slice(-maxHistorySize);

      return {
        history: trimmedHistory,
        currentIndex: trimmedHistory.length - 1,
        maxHistorySize: prev.maxHistorySize,
      };
    });
  }, [maxHistorySize]);

  const undo = useCallback(async () => {
    setState((prev) => {
      if (prev.currentIndex < 0) return prev;

      const action = prev.history[prev.currentIndex];
      if (action.reverse) {
        Promise.resolve(action.reverse()).catch((error) => {
          logger.error('Error reversing action', {
            component: 'useUndoRedo',
            operation: 'undo',
            actionId: action.id,
            actionType: action.type,
          }, toError(error));
        });
      }

      return {
        ...prev,
        currentIndex: prev.currentIndex - 1,
      };
    });
  }, []);

  const redo = useCallback(async () => {
    setState((prev) => {
      if (prev.currentIndex >= prev.history.length - 1) return prev;

      const nextIndex = prev.currentIndex + 1;
      const action = prev.history[nextIndex];
      
      if (action.forward) {
        Promise.resolve(action.forward()).catch((error) => {
          logger.error('Error forwarding action', {
            component: 'useUndoRedo',
            operation: 'redo',
            actionId: action.id,
            actionType: action.type,
          }, toError(error));
        });
      }

      return {
        ...prev,
        currentIndex: nextIndex,
      };
    });
  }, []);

  const canUndo = state.currentIndex >= 0;
  const canRedo = state.currentIndex < state.history.length - 1;
  const lastAction = state.currentIndex >= 0 
    ? state.history[state.currentIndex] 
    : null;

  const clearHistory = useCallback(() => {
    setState({
      history: [],
      currentIndex: -1,
      maxHistorySize,
    });
  }, [maxHistorySize]);

  return {
    addAction,
    undo,
    redo,
    canUndo,
    canRedo,
    lastAction,
    clearHistory,
    historyCount: state.history.length,
  };
}

export default useUndoRedo;

