/**
 * Cross-tab token refresh coordination using BroadcastChannel
 * Prevents multiple tabs from triggering concurrent refresh requests
 */

export const TOKEN_REFRESH_CHANNEL = 'aos-token-refresh';

interface TokenRefreshMessage {
  type: 'refresh_start' | 'refresh_complete' | 'refresh_failed';
  timestamp: number;
  tabId: string;
}

export interface TokenCoordinator {
  isRefreshInProgress: () => boolean;
  broadcastRefreshStart: () => void;
  broadcastRefreshComplete: () => void;
  broadcastRefreshFailed: () => void;
  waitForActiveRefresh: (timeoutMs?: number) => Promise<void>;
  cleanup: () => void;
}

export function createTokenCoordinator(): TokenCoordinator {
  // Check BroadcastChannel support (not available in all environments)
  if (typeof BroadcastChannel === 'undefined') {
    // Return no-op coordinator for unsupported environments
    return {
      isRefreshInProgress: () => false,
      broadcastRefreshStart: () => {},
      broadcastRefreshComplete: () => {},
      broadcastRefreshFailed: () => {},
      waitForActiveRefresh: async () => {},
      cleanup: () => {},
    };
  }

  const channel = new BroadcastChannel(TOKEN_REFRESH_CHANNEL);
  const tabId = crypto.randomUUID();
  let refreshInProgress = false;
  let activeRefreshTabId: string | null = null;

  channel.onmessage = (event: MessageEvent<TokenRefreshMessage>) => {
    const msg = event.data;
    if (msg.tabId === tabId) return; // Ignore own messages

    switch (msg.type) {
      case 'refresh_start':
        refreshInProgress = true;
        activeRefreshTabId = msg.tabId;
        break;
      case 'refresh_complete':
      case 'refresh_failed':
        refreshInProgress = false;
        activeRefreshTabId = null;
        break;
    }
  };

  return {
    isRefreshInProgress: () => refreshInProgress,
    broadcastRefreshStart: () => {
      channel.postMessage({ type: 'refresh_start', timestamp: Date.now(), tabId });
    },
    broadcastRefreshComplete: () => {
      channel.postMessage({ type: 'refresh_complete', timestamp: Date.now(), tabId });
    },
    broadcastRefreshFailed: () => {
      channel.postMessage({ type: 'refresh_failed', timestamp: Date.now(), tabId });
    },
    waitForActiveRefresh: async (timeoutMs = 5000): Promise<void> => {
      if (!refreshInProgress) return;

      return new Promise<void>((resolve) => {
        const start = Date.now();
        const check = () => {
          if (!refreshInProgress || Date.now() - start > timeoutMs) {
            resolve();
          } else {
            setTimeout(check, 100);
          }
        };
        check();
      });
    },
    cleanup: () => channel.close(),
  };
}
