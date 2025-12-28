/**
 * Dashboard Chat Card Component
 *
 * Displays chat interface access with stack information and quick actions.
 */

import React, { memo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { buildChatLink } from '@/utils/navLinks';
import type { TrainingJob, AdapterStack } from '@/api/types';

/**
 * Props for the DashboardChatCard component
 */
export interface DashboardChatCardProps {
  /** Default stack for the current workspace, if set */
  defaultStack: AdapterStack | null;
  /** Most recent completed training job with a stack, if any */
  recentCompletedJobWithStack: TrainingJob | null;
  /** Map of stack IDs to stack names for display */
  stackNameLookup: Map<string, string>;
  /** Whether default stack is loading */
  loading: boolean;
  /** Error from fetching default stack, if any */
  error: Error | null;
  /** Callback to refetch default stack */
  onRefetch: () => void;
}

/**
 * Chat workflow card for the dashboard.
 *
 * Shows active stack status, recent completed training,
 * and provides quick actions for opening chat.
 */
export const DashboardChatCard = memo(function DashboardChatCard({
  defaultStack,
  recentCompletedJobWithStack,
  stackNameLookup,
  loading,
  error,
  onRefetch,
}: DashboardChatCardProps) {
  return (
    <SectionErrorBoundary sectionName="Chat">
      <Card>
        <CardHeader>
          <CardTitle>Chat with your model</CardTitle>
          <p className="text-sm text-muted-foreground">
            Use the active stack or jump to the latest trained stack.
          </p>
        </CardHeader>
        <CardContent className="space-y-4">
          {loading ? (
            <Skeleton className="h-16 w-full" />
          ) : error ? (
            errorRecoveryTemplates.genericError(error, onRefetch)
          ) : (
            <>
              <div className="space-y-1">
                <p className="text-sm font-medium">
                  Active stack: {defaultStack ? defaultStack.name : 'Not set'}
                </p>
                <p className="text-xs text-muted-foreground">
                  {defaultStack
                    ? 'Chat requests will default to this stack.'
                    : 'No default stack configured. Set one under Stacks or use a specific stack below.'}
                </p>
              </div>
              {recentCompletedJobWithStack ? (
                <div className="rounded-lg border bg-muted/40 p-3 space-y-1">
                  <p className="text-xs text-muted-foreground">
                    Most recent completed training
                  </p>
                  <p className="text-sm font-medium">
                    Stack:{' '}
                    {stackNameLookup.get(recentCompletedJobWithStack.stack_id || '') ||
                      recentCompletedJobWithStack.stack_id}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Adapter:{' '}
                    {recentCompletedJobWithStack.adapter_name ||
                      recentCompletedJobWithStack.adapter_id ||
                      '---'}
                  </p>
                </div>
              ) : null}
              <div className="flex flex-wrap gap-2">
                <Button asChild>
                  <Link to={buildChatLink({ stackId: defaultStack?.id })}>Open chat</Link>
                </Button>
                {recentCompletedJobWithStack?.stack_id && (
                  <Button variant="outline" asChild>
                    <Link
                      to={buildChatLink({ stackId: recentCompletedJobWithStack.stack_id })}
                    >
                      Chat with latest trained stack
                    </Link>
                  </Button>
                )}
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
