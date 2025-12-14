/**
 * UndoSnackbar - 10-second countdown undo bar for detach actions
 *
 * Shows after Detach All with a countdown timer and undo button.
 */

import { useState, useEffect, useCallback } from 'react';
import { X, Undo2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useWorkbench, type UndoAction } from '@/contexts/WorkbenchContext';
import { useActivateAdapterStack } from '@/hooks/admin/useAdmin';
import { useSessionScope } from '@/hooks/chat/useSessionScope';
import { toast } from 'sonner';

interface UndoSnackbarProps {
  /** Session ID for restoring stack selection */
  sessionId: string | null;
  /** Callback to restore adapter overrides */
  onRestoreOverrides?: (overrides: Record<string, number>) => void;
}

export function UndoSnackbar({ sessionId, onRestoreOverrides }: UndoSnackbarProps) {
  const { undoAction, clearUndoAction } = useWorkbench();
  const sessionScope = useSessionScope();
  const activateStack = useActivateAdapterStack();
  const [remainingSeconds, setRemainingSeconds] = useState(10);
  const [isRestoring, setIsRestoring] = useState(false);

  // Update countdown every 100ms
  useEffect(() => {
    if (!undoAction) {
      setRemainingSeconds(10);
      return;
    }

    const interval = setInterval(() => {
      const remaining = Math.max(
        0,
        Math.ceil((undoAction.expiresAt - Date.now()) / 1000)
      );
      setRemainingSeconds(remaining);

      if (remaining <= 0) {
        clearUndoAction();
      }
    }, 100);

    return () => clearInterval(interval);
  }, [undoAction, clearUndoAction]);

  const handleUndo = useCallback(async () => {
    if (!undoAction) return;

    setIsRestoring(true);
    try {
      // Restore the previous stack
      if (undoAction.previousStackId) {
        await activateStack.mutateAsync(undoAction.previousStackId);
      }

      // Restore adapter overrides
      if (
        onRestoreOverrides &&
        Object.keys(undoAction.previousAdapterOverrides).length > 0
      ) {
        onRestoreOverrides(undoAction.previousAdapterOverrides);
      }

      // Restore session stack selection
      if (sessionId && undoAction.previousScope) {
        const { selectedStackId, stackName } = undoAction.previousScope;
        if (selectedStackId) {
          sessionScope.setStackSelection(sessionId, selectedStackId, stackName || undefined);
        }
      }

      toast.success('Stack restored');
      clearUndoAction();
    } catch (error) {
      toast.error('Failed to restore stack');
    } finally {
      setIsRestoring(false);
    }
  }, [undoAction, sessionId, activateStack, onRestoreOverrides, sessionScope, clearUndoAction]);

  const handleDismiss = useCallback(() => {
    clearUndoAction();
  }, [clearUndoAction]);

  if (!undoAction) return null;

  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 animate-in fade-in slide-in-from-bottom-2"
      data-testid="undo-snackbar"
    >
      <div className="flex items-center gap-3 bg-zinc-900 dark:bg-zinc-800 text-white px-4 py-3 rounded-lg shadow-lg">
        <span className="text-sm">Stack detached</span>
        <span className="text-sm text-zinc-400 tabular-nums min-w-[2ch]">
          {remainingSeconds}s
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="text-blue-400 hover:text-blue-300 hover:bg-transparent px-2"
          onClick={handleUndo}
          disabled={isRestoring}
          data-testid="undo-button"
        >
          <Undo2 className="h-4 w-4 mr-1" />
          Undo
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 text-zinc-400 hover:text-zinc-200 hover:bg-transparent"
          onClick={handleDismiss}
          data-testid="undo-dismiss-button"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
