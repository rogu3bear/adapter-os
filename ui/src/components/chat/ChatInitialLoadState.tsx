/**
 * ChatInitialLoadState - Wrapper component for ChatPage initial load states
 *
 * Renders the appropriate UI based on the load state:
 * - Loading (no timeout): skeleton
 * - Loading (soft timeout at 10s): timeout warning with retry
 * - Loading (hard timeout at 30s): hard timeout error with retry
 * - Error (auth): permission denied
 * - Error (backend_down): fetch error panel
 * - Error (no_workers): no workers guidance
 * - Error (system_not_ready): system not ready banner with auto-retry
 * - Success: children (chat content)
 *
 * 【2025-01-20†ui-never-spins-forever】
 */

import type { ReactNode } from 'react';
import type { ChatInitialLoadState as LoadState } from '@/hooks/chat/useChatInitialLoad';
import { ChatSkeleton } from '@/components/skeletons/ChatSkeleton';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { FetchErrorPanel } from '@/components/ui/fetch-error-panel';
import { ChatTimeoutWarning } from './ChatTimeoutWarning';
import { NoWorkersPanel } from './NoWorkersPanel';
import { SystemNotReadyBanner } from './SystemNotReadyBanner';

export interface ChatInitialLoadStateProps {
  /** The load state from useChatInitialLoad */
  loadState: LoadState;
  /** The chat content to render when successful */
  children: ReactNode;
}

export function ChatInitialLoadState({ loadState, children }: ChatInitialLoadStateProps) {
  const {
    isLoading,
    isSuccess,
    isError,
    isTimedOut,
    isHardTimedOut,
    errorType,
    errors,
    refetchAll,
    isAutoRetrying,
    nextRetryInSeconds,
  } = loadState;

  // Success - render children
  if (isSuccess) {
    return <>{children}</>;
  }

  // Hard timeout (30s) - show error with retry
  if (errorType === 'hard_timeout' && isHardTimedOut) {
    return (
      <div className="flex min-h-[400px] items-center justify-center p-4">
        <FetchErrorPanel
          title="Loading Timeout"
          description="The page has been loading for over 30 seconds. The server may be down or experiencing issues."
          onRetry={refetchAll}
          showDemoHints
        />
      </div>
    );
  }

  // Error states - show appropriate error UI
  if (isError) {
    switch (errorType) {
      case 'auth':
        return (
          <div className="flex min-h-[400px] items-center justify-center p-4">
            <PermissionDenied
              message="Authentication required to access chat. Please log in to continue."
              showBackButton={false}
            />
          </div>
        );

      case 'backend_down':
        return (
          <div className="flex min-h-[400px] items-center justify-center p-4">
            <FetchErrorPanel
              title="Unable to reach the backend"
              description="The UI can't connect to the AdapterOS control plane API."
              error={errors[0]}
              onRetry={refetchAll}
              showDemoHints
            />
          </div>
        );

      case 'no_workers':
        return <NoWorkersPanel onRetry={refetchAll} isRetrying={isLoading} />;

      case 'system_not_ready':
        return (
          <SystemNotReadyBanner
            onRetry={refetchAll}
            isAutoRetrying={isAutoRetrying}
            nextRetryInSeconds={nextRetryInSeconds}
            isRetrying={isLoading}
          />
        );

      case 'unknown':
      default:
        return (
          <div className="flex min-h-[400px] items-center justify-center p-4">
            <FetchErrorPanel
              title="Failed to load chat data"
              description="An error occurred while loading the initial data."
              error={errors[0]}
              onRetry={refetchAll}
              showDemoHints={false}
            />
          </div>
        );
    }
  }

  // Timed out but still loading - show warning
  if (isTimedOut && isLoading) {
    return <ChatTimeoutWarning onRetry={refetchAll} isRetrying={isLoading} />;
  }

  // Still loading (not timed out) - show skeleton
  if (isLoading) {
    return <ChatSkeleton />;
  }

  // Fallback - shouldn't reach here, but render children if we do
  return <>{children}</>;
}

export default ChatInitialLoadState;
