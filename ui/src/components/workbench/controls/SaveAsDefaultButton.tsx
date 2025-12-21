/**
 * SaveAsDefaultButton - Save current stack as the tenant's default
 *
 * Explicitly sets the currently active stack as the default for the tenant.
 * This is separate from Detach All - users must intentionally save their default.
 */

import { useCallback } from 'react';
import { Star, Loader2, Check } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useSetDefaultStack } from '@/hooks/admin/useAdmin';
import { useTenant } from '@/providers/FeatureProviders';
import { toast } from 'sonner';

interface SaveAsDefaultButtonProps {
  /** Currently active stack ID (null if no stack active) */
  activeStackId: string | null;
  /** Current default stack ID for comparison */
  currentDefaultStackId: string | null;
  /** Callback when save completes */
  onSaved?: () => void;
}

export function SaveAsDefaultButton({
  activeStackId,
  currentDefaultStackId,
  onSaved,
}: SaveAsDefaultButtonProps) {
  const { selectedTenant } = useTenant();
  const setDefaultStack = useSetDefaultStack(selectedTenant);

  const handleSave = useCallback(async () => {
    if (!activeStackId || !selectedTenant) {
      return;
    }

    try {
      await setDefaultStack.mutateAsync(activeStackId);

      toast.success('Stack saved as default');
      onSaved?.();
    } catch {
      // Error toast is shown by the mutation hook
    }
  }, [activeStackId, selectedTenant, setDefaultStack, onSaved]);

  // Disable if:
  // - No active stack
  // - Current stack is already the default
  // - Mutation is pending
  const isAlreadyDefault = activeStackId === currentDefaultStackId;
  const isDisabled = !activeStackId || isAlreadyDefault || setDefaultStack.isPending;

  return (
    <Button
      variant="secondary"
      size="sm"
      className="w-full justify-start"
      disabled={isDisabled}
      onClick={handleSave}
      data-testid="save-as-default-button"
    >
      {setDefaultStack.isPending ? (
        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
      ) : isAlreadyDefault ? (
        <Check className="h-4 w-4 mr-2" />
      ) : (
        <Star className="h-4 w-4 mr-2" />
      )}
      {isAlreadyDefault ? 'Current Default' : 'Save as Default'}
    </Button>
  );
}
