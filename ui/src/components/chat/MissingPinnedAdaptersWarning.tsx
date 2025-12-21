import React from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { AlertTriangle } from 'lucide-react';

interface MissingPinnedAdaptersWarningProps {
  unavailableAdapters: string[];
  fallbackMode?: 'stack_only' | 'partial';
  className?: string;
}

export function MissingPinnedAdaptersWarning({
  unavailableAdapters,
  fallbackMode,
  className,
}: MissingPinnedAdaptersWarningProps) {
  if (!unavailableAdapters || unavailableAdapters.length === 0) {
    return null;
  }

  const fallbackText = fallbackMode === 'stack_only'
    ? 'Routing fell back to stack-only adapters.'
    : fallbackMode === 'partial'
    ? 'Routing used available pinned adapters only.'
    : 'Routing adjusted to available adapters.';

  return (
    <Alert variant="default" className={className}>
      <AlertTriangle className="text-orange-500" />
      <AlertTitle>Pinned Adapters Unavailable</AlertTitle>
      <AlertDescription>
        <div className="space-y-1">
          <p className="text-sm">
            The following pinned adapters were requested but unavailable:
          </p>
          <ul className="list-disc list-inside text-sm text-muted-foreground">
            {unavailableAdapters.map((adapterId) => (
              <li key={adapterId} className="truncate">
                {adapterId}
              </li>
            ))}
          </ul>
          <p className="text-sm text-orange-600 dark:text-orange-400 mt-2">
            {fallbackText}
          </p>
        </div>
      </AlertDescription>
    </Alert>
  );
}
