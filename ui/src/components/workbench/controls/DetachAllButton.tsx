/**
 * DetachAllButton - Escape hatch to detach all adapters
 *
 * Deactivates the current stack, clears session adapter overrides, and clears
 * the session stack selection. Shows an undo snackbar for 10 seconds to restore
 * the previous state (including session scope).
 */

import { useState, useCallback } from 'react';
import { Unlink, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { useDeactivateAdapterStack } from '@/hooks/admin/useAdmin';
import { useSessionScope } from '@/hooks/chat/useSessionScope';

interface DetachAllButtonProps {
  /** Currently active stack ID (null if no stack active) */
  activeStackId: string | null;
  /** Session ID for clearing stack selection */
  sessionId: string | null;
  /** Session adapter strength overrides to capture for undo */
  adapterOverrides?: Record<string, number>;
  /** Callback when detach completes (to clear session overrides) */
  onDetach?: () => void;
  /** Callback to clear stack selection (sets to "base model only" mode) */
  onClearStack?: () => void;
}

export function DetachAllButton({
  activeStackId,
  sessionId,
  adapterOverrides = {},
  onDetach,
  onClearStack,
}: DetachAllButtonProps) {
  const { setUndoAction } = useWorkbench();
  const sessionScope = useSessionScope();
  const deactivateStack = useDeactivateAdapterStack();
  const [isDetaching, setIsDetaching] = useState(false);

  const handleDetach = useCallback(async () => {
    // Capture previous state for undo
    const previousStackId = activeStackId;
    const previousOverrides = { ...adapterOverrides };

    // Capture previous session scope for undo
    let previousScope = null;
    if (sessionId) {
      const currentScope = sessionScope.getSessionScope(sessionId);
      previousScope = {
        selectedStackId: currentScope.selectedStackId,
        stackName: currentScope.stackName,
      };
    }

    setIsDetaching(true);
    try {
      // Deactivate the stack via API
      await deactivateStack.mutateAsync();

      // Clear session adapter overrides
      onDetach?.();

      // Clear stack selection to "base model only" mode
      onClearStack?.();

      // Clear session stack selection
      if (sessionId) {
        sessionScope.clearStackSelection(sessionId);
      }

      // Set undo action with 10 second expiry
      setUndoAction({
        type: 'detach_all',
        previousStackId,
        previousAdapterOverrides: previousOverrides,
        previousScope,
        expiresAt: Date.now() + 10000,
      });

      // Toast is suppressed here since UndoSnackbar will show
    } catch (error) {
      // Error toast is shown by the mutation hook
    } finally {
      setIsDetaching(false);
    }
  }, [activeStackId, sessionId, adapterOverrides, deactivateStack, onClearStack, onDetach, sessionScope, setUndoAction]);

  const isDisabled = !activeStackId || isDetaching;

  return (
    <Button
      variant="destructive"
      size="sm"
      className="w-full justify-start"
      disabled={isDisabled}
      onClick={handleDetach}
      data-testid="detach-all-button"
    >
      {isDetaching ? (
        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
      ) : (
        <Unlink className="h-4 w-4 mr-2" />
      )}
      Detach All
    </Button>
  );
}
