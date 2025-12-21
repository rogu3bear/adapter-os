/**
 * Tests for EvidenceDrawerContext
 *
 * Tests context provider, state management, drawer open/close behavior,
 * and message data handling.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  EvidenceDrawerProvider,
  useEvidenceDrawer,
  useEvidenceDrawerOptional,
  type EvidenceDrawerTab,
} from '@/contexts/EvidenceDrawerContext';
import type { EvidenceItem } from '@/components/chat/ChatMessage';
import type { ExtendedRouterDecision } from '@/api/api-types';

function createWrapper() {
  return ({ children }: { children: ReactNode }) => (
    <EvidenceDrawerProvider>{children}</EvidenceDrawerProvider>
  );
}

const mockEvidence: EvidenceItem[] = [
  {
    id: 'ev1',
    type: 'document',
    content: 'Test evidence content',
    source: 'doc1.pdf',
    page: 1,
  },
];

const mockRouterDecision: ExtendedRouterDecision = {
  selected_adapter_ids: ['adapter-1', 'adapter-2'],
  gates_q15: [16384, 8192],
  scores: [0.8, 0.6],
  k: 2,
  total_adapters: 5,
};

describe('EvidenceDrawerContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Provider and Hook', () => {
    it('throws error when useEvidenceDrawer is used outside provider', () => {
      // Suppress expected console.error
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      expect(() => {
        renderHook(() => useEvidenceDrawer());
      }).toThrow('useEvidenceDrawer must be used within an EvidenceDrawerProvider');

      consoleError.mockRestore();
    });

    it('returns null when useEvidenceDrawerOptional is used outside provider', () => {
      const { result } = renderHook(() => useEvidenceDrawerOptional());
      expect(result.current).toBeNull();
    });

    it('provides context value when used within provider', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current).toBeDefined();
      expect(result.current.isOpen).toBe(false);
      expect(result.current.activeMessageId).toBeNull();
    });
  });

  describe('Initial State', () => {
    it('initializes with drawer closed', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current.isOpen).toBe(false);
      expect(result.current.activeMessageId).toBeNull();
      expect(result.current.activeTab).toBe('rulebook');
    });

    it('initializes with null message data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current.currentEvidence).toBeNull();
      expect(result.current.currentRouterDecision).toBeNull();
      expect(result.current.currentRequestId).toBeNull();
      expect(result.current.currentTraceId).toBeNull();
      expect(result.current.currentProofDigest).toBeNull();
      expect(result.current.currentIsVerified).toBe(false);
      expect(result.current.currentVerifiedAt).toBeNull();
    });
  });

  describe('openDrawer', () => {
    it('opens drawer for a message', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-123');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-123');
    });

    it('opens drawer with specific tab', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-456', 'trace');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-456');
      expect(result.current.activeTab).toBe('trace');
    });

    it('preserves existing tab if no tab specified', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');

      act(() => {
        result.current.openDrawer('msg-789');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeTab).toBe('calculation');
    });

    it('switches message while keeping drawer open', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-1');
      });

      expect(result.current.activeMessageId).toBe('msg-1');

      act(() => {
        result.current.openDrawer('msg-2');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-2');
    });
  });

  describe('closeDrawer', () => {
    it('closes the drawer', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-close');
      });

      expect(result.current.isOpen).toBe(true);

      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
    });

    it('preserves message ID when closing', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-preserve');
      });

      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
      expect(result.current.activeMessageId).toBe('msg-preserve');
    });
  });

  describe('setActiveTab', () => {
    it('changes the active tab', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeTab).toBe('rulebook');

      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');

      act(() => {
        result.current.setActiveTab('trace');
      });

      expect(result.current.activeTab).toBe('trace');
    });

    it('can switch tabs while drawer is open', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-tab');
      });

      const tabs: EvidenceDrawerTab[] = ['rulebook', 'calculation', 'trace'];

      tabs.forEach((tab) => {
        act(() => {
          result.current.setActiveTab(tab);
        });

        expect(result.current.activeTab).toBe(tab);
        expect(result.current.isOpen).toBe(true);
      });
    });
  });

  describe('setMessageData', () => {
    it('sets evidence data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ evidence: mockEvidence });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
    });

    it('sets router decision data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ routerDecision: mockRouterDecision });
      });

      expect(result.current.currentRouterDecision).toEqual(mockRouterDecision);
    });

    it('sets request and trace IDs', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          requestId: 'req-123',
          traceId: 'trace-456',
        });
      });

      expect(result.current.currentRequestId).toBe('req-123');
      expect(result.current.currentTraceId).toBe('trace-456');
    });

    it('sets proof verification data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          proofDigest: 'digest-abc',
          isVerified: true,
          verifiedAt: '2024-01-01T12:00:00Z',
        });
      });

      expect(result.current.currentProofDigest).toBe('digest-abc');
      expect(result.current.currentIsVerified).toBe(true);
      expect(result.current.currentVerifiedAt).toBe('2024-01-01T12:00:00Z');
    });

    it('sets multiple data fields at once', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          routerDecision: mockRouterDecision,
          requestId: 'req-multi',
          traceId: 'trace-multi',
          proofDigest: 'digest-multi',
          isVerified: true,
          verifiedAt: '2024-01-01T12:00:00Z',
        });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRouterDecision).toEqual(mockRouterDecision);
      expect(result.current.currentRequestId).toBe('req-multi');
      expect(result.current.currentTraceId).toBe('trace-multi');
      expect(result.current.currentProofDigest).toBe('digest-multi');
      expect(result.current.currentIsVerified).toBe(true);
      expect(result.current.currentVerifiedAt).toBe('2024-01-01T12:00:00Z');
    });

    it('preserves existing data when updating partial fields', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          requestId: 'req-1',
        });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRequestId).toBe('req-1');

      act(() => {
        result.current.setMessageData({
          routerDecision: mockRouterDecision,
        });
      });

      // Previous data should still be there
      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRequestId).toBe('req-1');
      // New data should be added
      expect(result.current.currentRouterDecision).toEqual(mockRouterDecision);
    });
  });

  describe('Integration Tests', () => {
    it('supports complete workflow: open, set data, change tabs, close', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Open drawer
      act(() => {
        result.current.openDrawer('msg-workflow', 'rulebook');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-workflow');

      // Set message data
      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          routerDecision: mockRouterDecision,
          requestId: 'req-workflow',
        });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);

      // Change tab
      act(() => {
        result.current.setActiveTab('trace');
      });

      expect(result.current.activeTab).toBe('trace');

      // Close drawer
      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
    });

    it('supports switching between messages with different data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Message 1
      act(() => {
        result.current.openDrawer('msg-1');
        result.current.setMessageData({
          requestId: 'req-1',
          evidence: mockEvidence,
        });
      });

      expect(result.current.activeMessageId).toBe('msg-1');
      expect(result.current.currentRequestId).toBe('req-1');

      // Message 2
      const evidence2: EvidenceItem[] = [
        {
          id: 'ev2',
          type: 'citation',
          content: 'Different evidence',
          source: 'doc2.pdf',
        },
      ];

      act(() => {
        result.current.openDrawer('msg-2');
        result.current.setMessageData({
          requestId: 'req-2',
          evidence: evidence2,
        });
      });

      expect(result.current.activeMessageId).toBe('msg-2');
      expect(result.current.currentRequestId).toBe('req-2');
      expect(result.current.currentEvidence).toEqual(evidence2);
    });

    it('maintains referential stability of callback functions', () => {
      const { result, rerender } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      const openDrawer1 = result.current.openDrawer;
      const closeDrawer1 = result.current.closeDrawer;
      const setActiveTab1 = result.current.setActiveTab;
      const setMessageData1 = result.current.setMessageData;

      rerender();

      const openDrawer2 = result.current.openDrawer;
      const closeDrawer2 = result.current.closeDrawer;
      const setActiveTab2 = result.current.setActiveTab;
      const setMessageData2 = result.current.setMessageData;

      expect(openDrawer1).toBe(openDrawer2);
      expect(closeDrawer1).toBe(closeDrawer2);
      expect(setActiveTab1).toBe(setActiveTab2);
      expect(setMessageData1).toBe(setMessageData2);
    });

    it('can reopen drawer after closing', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-reopen');
      });

      expect(result.current.isOpen).toBe(true);

      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);

      act(() => {
        result.current.openDrawer('msg-reopen');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-reopen');
    });
  });

  describe('Edge Cases', () => {
    it('handles empty evidence array', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ evidence: [] });
      });

      expect(result.current.currentEvidence).toEqual([]);
    });

    it('handles verification data with false values', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          isVerified: false,
          verifiedAt: null,
        });
      });

      expect(result.current.currentIsVerified).toBe(false);
      expect(result.current.currentVerifiedAt).toBeNull();
    });

    it('allows opening drawer multiple times for same message', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-same');
      });

      expect(result.current.isOpen).toBe(true);

      act(() => {
        result.current.openDrawer('msg-same', 'calculation');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-same');
      expect(result.current.activeTab).toBe('calculation');
    });
  });
});
