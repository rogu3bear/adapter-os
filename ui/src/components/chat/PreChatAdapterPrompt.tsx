/**
 * PreChatAdapterPrompt - Dialog shown before chat when adapters need loading
 *
 * Prompts the user to load cold/unloaded adapters before sending messages.
 */

import * as React from 'react';
import { AlertTriangle, Loader2, CheckCircle, Flame, Snowflake, CircleOff, Thermometer, Pin } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { AdapterState, AdapterLifecycleState } from './AdapterLoadingStatus';

// ============================================================================
// Types
// ============================================================================

export interface PreChatAdapterPromptProps {
  /** Whether the dialog is open */
  open: boolean;

  /** Called when dialog should close */
  onOpenChange: (open: boolean) => void;

  /** List of adapters with their states */
  adapters: AdapterState[];

  /** Called when user clicks "Load All Now" */
  onLoadAll: () => void;

  /** Called when user clicks "Continue Anyway" */
  onContinueAnyway: () => void;

  /** Called when user clicks "Change Stack" */
  onChangeStack?: () => void;

  /** Whether loading is in progress */
  isLoading?: boolean;
}

// ============================================================================
// State Icons
// ============================================================================

const STATE_ICONS: Record<AdapterLifecycleState, React.ElementType> = {
  hot: Flame,
  warm: Thermometer,
  cold: Snowflake,
  resident: Pin,
  unloaded: CircleOff,
};

const STATE_COLORS: Record<AdapterLifecycleState, string> = {
  hot: 'text-red-600',
  warm: 'text-orange-600',
  cold: 'text-blue-600',
  resident: 'text-purple-600',
  unloaded: 'text-gray-500',
};

function isReady(state: AdapterLifecycleState): boolean {
  return state === 'hot' || state === 'warm' || state === 'resident';
}

function getLoadTime(state: AdapterLifecycleState): string {
  switch (state) {
    case 'cold':
      return 'Est. ~5s';
    case 'unloaded':
      return 'Est. ~10s';
    default:
      return '';
  }
}

// ============================================================================
// Component
// ============================================================================

export function PreChatAdapterPrompt({
  open,
  onOpenChange,
  adapters,
  onLoadAll,
  onContinueAnyway,
  onChangeStack,
  isLoading = false,
}: PreChatAdapterPromptProps) {
  const readyAdapters = adapters.filter((a) => isReady(a.state));
  const notReadyAdapters = adapters.filter((a) => !isReady(a.state));

  // Calculate total estimated load time
  const estimatedTotalTime = notReadyAdapters.reduce((acc, a) => {
    if (a.state === 'unloaded') return acc + 10;
    if (a.state === 'cold') return acc + 5;
    return acc;
  }, 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <AlertTriangle className="h-5 w-5 text-amber-500" />
            Some adapters need loading
          </DialogTitle>
          <DialogDescription>
            {notReadyAdapters.length} of {adapters.length} adapters are not ready for inference.
            Would you like to load them now?
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-4">
          {/* Ready adapters */}
          {readyAdapters.length > 0 && (
            <div className="space-y-1">
              <p className="text-xs font-medium text-muted-foreground">Ready</p>
              {readyAdapters.map((adapter) => {
                const Icon = STATE_ICONS[adapter.state];
                return (
                  <div
                    key={adapter.id}
                    className="flex items-center justify-between px-3 py-2 rounded-md bg-green-50 border border-green-200"
                  >
                    <div className="flex items-center gap-2">
                      <CheckCircle className="h-4 w-4 text-green-600" />
                      <span className="text-sm font-medium">{adapter.name}</span>
                    </div>
                    <Badge variant="outline" className="text-xs text-green-700 border-green-300">
                      Ready
                    </Badge>
                  </div>
                );
              })}
            </div>
          )}

          {/* Not ready adapters */}
          {notReadyAdapters.length > 0 && (
            <div className="space-y-1">
              <p className="text-xs font-medium text-muted-foreground">Needs Loading</p>
              {notReadyAdapters.map((adapter) => {
                const Icon = STATE_ICONS[adapter.state];
                const colorClass = STATE_COLORS[adapter.state];
                const loadTime = getLoadTime(adapter.state);

                return (
                  <div
                    key={adapter.id}
                    className="flex items-center justify-between px-3 py-2 rounded-md bg-amber-50 border border-amber-200"
                  >
                    <div className="flex items-center gap-2">
                      {adapter.isLoading ? (
                        <Loader2 className={cn('h-4 w-4 animate-spin', colorClass)} />
                      ) : (
                        <Icon className={cn('h-4 w-4', colorClass)} />
                      )}
                      <span className="text-sm font-medium">{adapter.name}</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className={cn('text-xs', colorClass)}>
                        {adapter.isLoading ? 'Loading...' : adapter.state}
                      </Badge>
                      {loadTime && !adapter.isLoading && (
                        <span className="text-xs text-muted-foreground">{loadTime}</span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {/* Estimated time */}
          {estimatedTotalTime > 0 && !isLoading && (
            <p className="text-sm text-muted-foreground text-center">
              Estimated total load time: ~{estimatedTotalTime}s
            </p>
          )}
        </div>

        <DialogFooter className="flex-col sm:flex-row gap-2">
          {onChangeStack && (
            <Button variant="outline" onClick={onChangeStack} disabled={isLoading}>
              Change Stack
            </Button>
          )}
          <Button variant="outline" onClick={onContinueAnyway} disabled={isLoading}>
            Continue Anyway
          </Button>
          <Button onClick={onLoadAll} disabled={isLoading}>
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Loading...
              </>
            ) : (
              'Load All Now'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default PreChatAdapterPrompt;
