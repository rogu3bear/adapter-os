//! Undo/Redo Hook
//!
//! Provides undo/redo functionality for all mutations (create, edit, assign, update, delete, archive, etc.)
//! Following git-like pattern: every action is reversible until committed.
//!
//! Citations:
//! - docs/architecture/Determinism.md - Reversible operations pattern
//! - Plan: Data Operations Productivity Features - Undo/redo for all mutations

import { useState, useCallback, useRef } from 'react';
import { logger, toError } from '@/utils/logger';

export interface UndoableAction<T = unknown> {
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
    const currentState = await new Promise<typeof state>((resolve) => {
      setState((prev) => {
        resolve(prev);
        return prev;
      });
    });

    if (currentState.currentIndex < 0) return;

    const action = currentState.history[currentState.currentIndex];

    if (action.reverse) {
      try {
        await Promise.resolve(action.reverse());
      } catch (error) {
        logger.error('Error reversing action', {
          component: 'useUndoRedo',
          operation: 'undo',
          actionId: action.id,
          actionType: action.type,
        }, toError(error));
        return; // Don't update state if reverse failed
      }
    }

    setState((prev) => ({
      ...prev,
      currentIndex: prev.currentIndex - 1,
    }));
  }, []);

  const redo = useCallback(async () => {
    const currentState = await new Promise<typeof state>((resolve) => {
      setState((prev) => {
        resolve(prev);
        return prev;
      });
    });

    if (currentState.currentIndex >= currentState.history.length - 1) return;

    const nextIndex = currentState.currentIndex + 1;
    const action = currentState.history[nextIndex];

    if (action.forward) {
      try {
        await Promise.resolve(action.forward());
      } catch (error) {
        logger.error('Error forwarding action', {
          component: 'useUndoRedo',
          operation: 'redo',
          actionId: action.id,
          actionType: action.type,
        }, toError(error));
        return; // Don't update state if forward failed
      }
    }

    setState((prev) => ({
      ...prev,
      currentIndex: nextIndex,
    }));
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

