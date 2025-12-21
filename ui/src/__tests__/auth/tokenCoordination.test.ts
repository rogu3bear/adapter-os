/**
 * Tests for cross-tab token refresh coordination
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createTokenCoordinator, TOKEN_REFRESH_CHANNEL } from '@/auth/tokenCoordination';

describe('createTokenCoordinator', () => {
  let mockChannel: {
    postMessage: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;
    onmessage: ((event: MessageEvent) => void) | null;
  };

  beforeEach(() => {
    mockChannel = {
      postMessage: vi.fn(),
      close: vi.fn(),
      onmessage: null,
    };

    // Mock BroadcastChannel
    vi.stubGlobal('BroadcastChannel', vi.fn().mockImplementation(() => mockChannel));
    vi.stubGlobal('crypto', {
      randomUUID: () => 'test-tab-id-123',
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe('when BroadcastChannel is available', () => {
    it('creates a coordinator with all methods', () => {
      const coordinator = createTokenCoordinator();

      expect(coordinator).toHaveProperty('isRefreshInProgress');
      expect(coordinator).toHaveProperty('broadcastRefreshStart');
      expect(coordinator).toHaveProperty('broadcastRefreshComplete');
      expect(coordinator).toHaveProperty('broadcastRefreshFailed');
      expect(coordinator).toHaveProperty('waitForActiveRefresh');
      expect(coordinator).toHaveProperty('cleanup');
    });

    it('creates BroadcastChannel with correct name', () => {
      createTokenCoordinator();
      expect(BroadcastChannel).toHaveBeenCalledWith(TOKEN_REFRESH_CHANNEL);
    });

    it('initially reports no refresh in progress', () => {
      const coordinator = createTokenCoordinator();
      expect(coordinator.isRefreshInProgress()).toBe(false);
    });

    it('broadcasts refresh_start message', () => {
      const coordinator = createTokenCoordinator();
      coordinator.broadcastRefreshStart();

      expect(mockChannel.postMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'refresh_start',
          tabId: 'test-tab-id-123',
        })
      );
    });

    it('broadcasts refresh_complete message', () => {
      const coordinator = createTokenCoordinator();
      coordinator.broadcastRefreshComplete();

      expect(mockChannel.postMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'refresh_complete',
          tabId: 'test-tab-id-123',
        })
      );
    });

    it('broadcasts refresh_failed message', () => {
      const coordinator = createTokenCoordinator();
      coordinator.broadcastRefreshFailed();

      expect(mockChannel.postMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'refresh_failed',
          tabId: 'test-tab-id-123',
        })
      );
    });

    it('tracks refresh in progress from other tabs', () => {
      const coordinator = createTokenCoordinator();

      // Simulate message from another tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      expect(coordinator.isRefreshInProgress()).toBe(true);
    });

    it('clears refresh in progress on complete from other tab', () => {
      const coordinator = createTokenCoordinator();

      // Start refresh from other tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      expect(coordinator.isRefreshInProgress()).toBe(true);

      // Complete refresh from other tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_complete',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      expect(coordinator.isRefreshInProgress()).toBe(false);
    });

    it('clears refresh in progress on failed from other tab', () => {
      const coordinator = createTokenCoordinator();

      // Start refresh from other tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      // Failed refresh from other tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_failed',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      expect(coordinator.isRefreshInProgress()).toBe(false);
    });

    it('ignores messages from own tab', () => {
      const coordinator = createTokenCoordinator();

      // Simulate message from own tab (same tabId)
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'test-tab-id-123', // Same as our tab
          timestamp: Date.now(),
        },
      } as MessageEvent);

      // Should still be false since we ignore own messages
      expect(coordinator.isRefreshInProgress()).toBe(false);
    });

    it('closes channel on cleanup', () => {
      const coordinator = createTokenCoordinator();
      coordinator.cleanup();

      expect(mockChannel.close).toHaveBeenCalled();
    });

    it('waitForActiveRefresh resolves immediately when no refresh in progress', async () => {
      const coordinator = createTokenCoordinator();

      const start = Date.now();
      await coordinator.waitForActiveRefresh();
      const elapsed = Date.now() - start;

      expect(elapsed).toBeLessThan(50); // Should resolve almost immediately
    });

    it('waitForActiveRefresh waits for refresh to complete', async () => {
      vi.useFakeTimers();
      const coordinator = createTokenCoordinator();

      // Start refresh from other tab
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      const waitPromise = coordinator.waitForActiveRefresh(1000);

      // Advance time
      await vi.advanceTimersByTimeAsync(200);

      // Complete refresh
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_complete',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      await vi.advanceTimersByTimeAsync(100);
      await waitPromise;

      expect(coordinator.isRefreshInProgress()).toBe(false);
      vi.useRealTimers();
    });

    it('waitForActiveRefresh times out after specified duration', async () => {
      vi.useFakeTimers();
      const coordinator = createTokenCoordinator();

      // Start refresh from other tab (never completes)
      mockChannel.onmessage?.({
        data: {
          type: 'refresh_start',
          tabId: 'other-tab-id',
          timestamp: Date.now(),
        },
      } as MessageEvent);

      const waitPromise = coordinator.waitForActiveRefresh(500);

      // Advance past timeout
      await vi.advanceTimersByTimeAsync(600);
      await waitPromise;

      // Should have timed out and resolved
      expect(true).toBe(true); // Promise resolved
      vi.useRealTimers();
    });
  });

  describe('when BroadcastChannel is unavailable', () => {
    beforeEach(() => {
      vi.stubGlobal('BroadcastChannel', undefined);
    });

    it('returns no-op coordinator', () => {
      const coordinator = createTokenCoordinator();

      expect(coordinator.isRefreshInProgress()).toBe(false);
      expect(() => coordinator.broadcastRefreshStart()).not.toThrow();
      expect(() => coordinator.broadcastRefreshComplete()).not.toThrow();
      expect(() => coordinator.broadcastRefreshFailed()).not.toThrow();
      expect(() => coordinator.cleanup()).not.toThrow();
    });

    it('waitForActiveRefresh resolves immediately', async () => {
      const coordinator = createTokenCoordinator();

      const start = Date.now();
      await coordinator.waitForActiveRefresh();
      const elapsed = Date.now() - start;

      expect(elapsed).toBeLessThan(50);
    });
  });
});
