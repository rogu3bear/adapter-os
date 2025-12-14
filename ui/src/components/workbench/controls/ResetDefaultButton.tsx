/**
 * ResetDefaultButton - Reset to the tenant's default adapter stack
 *
 * Activates the default stack, providing a quick way to return
 * to a known-good configuration.
 */

import { useState, useCallback } from 'react';
import { RotateCcw, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useActivateAdapterStack } from '@/hooks/admin/useAdmin';
import { toast } from 'sonner';

interface ResetDefaultButtonProps {
  /** Default stack ID (null if no default configured) */
  defaultStackId: string | null;
  /** Currently active stack ID */
  activeStackId: string | null;
  /** Callback when reset completes */
  onReset?: () => void;
}

export function ResetDefaultButton({
  defaultStackId,
  activeStackId,
  onReset,
}: ResetDefaultButtonProps) {
  const activateStack = useActivateAdapterStack();
  const [isResetting, setIsResetting] = useState(false);

  const handleReset = useCallback(async () => {
    if (!defaultStackId) {
      toast.error('No default stack configured');
      return;
    }

    setIsResetting(true);
    try {
      await activateStack.mutateAsync(defaultStackId);
      onReset?.();
      // Success toast is shown by the mutation hook
    } catch (error) {
      // Error toast is shown by the mutation hook
    } finally {
      setIsResetting(false);
    }
  }, [defaultStackId, activateStack, onReset]);

  // Disabled if no default stack or already on default stack
  const isDisabled =
    !defaultStackId ||
    defaultStackId === activeStackId ||
    isResetting;

  return (
    <Button
      variant="secondary"
      size="sm"
      className="w-full justify-start"
      disabled={isDisabled}
      onClick={handleReset}
      data-testid="reset-default-button"
    >
      {isResetting ? (
        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
      ) : (
        <RotateCcw className="h-4 w-4 mr-2" />
      )}
      Reset Default
    </Button>
  );
}
