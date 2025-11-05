import React from 'react';
import { Button } from './button';
import { Undo2, Redo2, X } from 'lucide-react';
import { cn } from './utils';

export interface UndoRedoBarProps {
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onDismiss?: () => void;
  className?: string;
  currentActionDescription?: string;
}

export function UndoRedoBar({
  canUndo,
  canRedo,
  onUndo,
  onRedo,
  onDismiss,
  className,
  currentActionDescription
}: UndoRedoBarProps) {
  if (!canUndo && !canRedo) {
    return null;
  }

  return (
    <div
      className={cn(
        'fixed bottom-4 right-4 bg-primary text-primary-foreground px-4 py-3 rounded-lg',
        'shadow-lg border border-border z-50 flex items-center gap-3',
        'min-w-[250px] max-w-[400px]',
        className
      )}
    >
      <div className="flex items-center gap-2 flex-1">
        {currentActionDescription && (
          <span className="text-sm font-medium truncate">
            {currentActionDescription}
          </span>
        )}
      </div>

      <div className="flex items-center gap-1 border-l border-primary-foreground/20 pl-3">
        <Button
          variant="ghost"
          size="sm"
          onClick={onUndo}
          disabled={!canUndo}
          className="h-7 px-2 hover:bg-primary-foreground/10 text-primary-foreground disabled:opacity-50"
          aria-label="Undo"
          title="Undo (Cmd/Ctrl+Z)"
        >
          <Undo2 className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={onRedo}
          disabled={!canRedo}
          className="h-7 px-2 hover:bg-primary-foreground/10 text-primary-foreground disabled:opacity-50"
          aria-label="Redo"
          title="Redo (Cmd/Ctrl+Shift+Z)"
        >
          <Redo2 className="h-3.5 w-3.5" />
        </Button>
      </div>

      {onDismiss && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onDismiss}
          className="h-7 w-7 p-0 hover:bg-primary-foreground/10 text-primary-foreground"
          aria-label="Dismiss"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      )}
    </div>
  );
}

