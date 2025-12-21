/**
 * Dev Bypass Section
 *
 * Development-only authentication bypass button.
 * Only visible when dev bypass is enabled in config.
 */

import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';

interface DevBypassSectionProps {
  /** Called when dev bypass button is clicked */
  onDevBypass: () => Promise<void>;
  /** Whether dev bypass is currently in progress */
  isLoading: boolean;
  /** Whether the button should be disabled */
  disabled: boolean;
  /** Error message from failed dev bypass attempt */
  error?: string | null;
}

export function DevBypassSection({
  onDevBypass,
  isLoading,
  disabled,
  error,
}: DevBypassSectionProps) {
  return (
    <section className="rounded-lg border border-dashed bg-muted/30 p-4 space-y-3">
      <div>
        <h3 className="text-sm font-semibold mb-0.5">Development mode</h3>
        <p className="text-xs text-muted-foreground">
          Quick access for local development
        </p>
      </div>
      {error && (
        <Alert variant="destructive" className="py-2">
          <AlertDescription className="text-xs">{error}</AlertDescription>
        </Alert>
      )}
      <Button
        type="button"
        variant="outline"
        onClick={onDevBypass}
        disabled={isLoading || disabled}
        className="w-full h-9 text-sm"
      >
        {isLoading ? (
          <>
            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            Activating...
          </>
        ) : (
          'Use dev bypass'
        )}
      </Button>
    </section>
  );
}
