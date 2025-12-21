//! Undo/Redo Context Provider
//!
//! Global undo/redo state management with keyboard shortcuts.
//!
//! Citations:
//! - ui/src/hooks/useUndoRedo.ts - Undo/redo hook implementation
//! - Plan: Data Operations Productivity Features - Global undo/redo with keyboard shortcuts

import React, { createContext, useContext, useEffect, useCallback } from 'react';
import { useUndoRedo, UndoableAction } from '@/hooks/ui/useUndoRedo';

interface UndoRedoContextType {
  addAction: <T = unknown>(action: Omit<UndoableAction<T>, 'id' | 'timestamp'>) => void;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  canUndo: boolean;
  canRedo: boolean;
  lastAction: UndoableAction | null;
  clearHistory: () => void;
  historyCount: number;
}

const UndoRedoContext = createContext<UndoRedoContextType | undefined>(undefined);

export function UndoRedoProvider({ children }: { children: React.ReactNode }) {
  const {
    addAction,
    undo,
    redo,
    canUndo,
    canRedo,
    lastAction,
    clearHistory,
    historyCount,
  } = useUndoRedo();

  // Keyboard shortcuts: Cmd/Ctrl+Z for undo, Cmd/Ctrl+Shift+Z for redo
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const modifier = isMac ? event.metaKey : event.ctrlKey;
      
      // Undo: Cmd/Ctrl+Z (but not Cmd/Ctrl+Shift+Z)
      if (modifier && event.key === 'z' && !event.shiftKey) {
        event.preventDefault();
        if (canUndo) {
          undo();
        }
      }
      
      // Redo: Cmd/Ctrl+Shift+Z or Cmd/Ctrl+Y
      if (modifier && (event.shiftKey && event.key === 'Z') || event.key === 'y') {
        event.preventDefault();
        if (canRedo) {
          redo();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [canUndo, canRedo, undo, redo]);

  const contextValue: UndoRedoContextType = {
    addAction,
    undo,
    redo,
    canUndo,
    canRedo,
    lastAction,
    clearHistory,
    historyCount,
  };

  return (
    <UndoRedoContext.Provider value={contextValue}>
      {children}
    </UndoRedoContext.Provider>
  );
}

export function useUndoRedoContext() {
  const context = useContext(UndoRedoContext);
  if (context === undefined) {
    throw new Error('useUndoRedoContext must be used within an UndoRedoProvider');
  }
  return context;
}

export default UndoRedoContext;

