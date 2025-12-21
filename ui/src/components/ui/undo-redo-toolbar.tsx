//! Undo/Redo Toolbar Component
//!
//! Fixed position toolbar with undo/redo buttons and keyboard shortcut indicators.
//!
//! Citations:
//! - ui/src/contexts/UndoRedoContext.tsx - Undo/redo context provider
//! - Plan: Data Operations Productivity Features - Undo/redo toolbar UI

import React from 'react';
import { Button } from './button';
import { Undo2, Redo2 } from 'lucide-react';
import { useUndoRedoContext } from '@/contexts/UndoRedoContext';

interface UndoRedoToolbarProps {
  className?: string;
}

export function UndoRedoToolbar({ className = '' }: UndoRedoToolbarProps) {
  const { undo, redo, canUndo, canRedo, lastAction } = useUndoRedoContext();
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;

  if (!lastAction) return null;

  return (
    <div className={`fixed bottom-4 right-4 z-50 ${className}`}>
      <div className="bg-background border border-border rounded-lg shadow-lg px-3 py-2 flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => undo()}
          disabled={!canUndo}
          title={`Undo ${lastAction.description} (${isMac ? 'Cmd' : 'Ctrl'}+Z)`}
          className="h-8"
        >
          <Undo2 className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => redo()}
          disabled={!canRedo}
          title={`Redo (${isMac ? 'Cmd' : 'Ctrl'}+Shift+Z or ${isMac ? 'Cmd' : 'Ctrl'}+Y)`}
          className="h-8"
        >
          <Redo2 className="h-4 w-4" />
        </Button>
        {lastAction && (
          <span className="text-xs text-muted-foreground px-2 max-w-[200px] truncate">
            Last: {lastAction.description}
          </span>
        )}
      </div>
    </div>
  );
}

export default UndoRedoToolbar;

