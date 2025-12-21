import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import React from 'react';
import {
  EvidenceDrawerProvider,
  useEvidenceDrawer,
  useEvidenceDrawerOptional,
  type EvidenceDrawerTab,
} from '@/contexts/EvidenceDrawerContext';
import type { EvidenceItem } from '@/components/chat/ChatMessage';
import type { ExtendedRouterDecision } from '@/api/api-types';

// Test data
const mockEvidence: EvidenceItem[] = [
  {
    id: 'ev-1',
    type: 'doc',
    reference: 'DOC-001',
    confidence: 'high',
    description: 'Test evidence 1',
  },
  {
    id: 'ev-2',
    type: 'review',
    reference: 'REV-123',
    confidence: 'medium',
    description: 'Test evidence 2',
  },
];

const mockRouterDecision: ExtendedRouterDecision = {
  selected_adapters: ['adapter-1', 'adapter-2'],
  scores: { 'adapter-1': 0.9, 'adapter-2': 0.7 },
  timestamp: '2025-01-01T00:00:00Z',
  reasoning: 'Test reasoning',
};

// Test wrapper
function createWrapper() {
  return ({ children }: { children: React.ReactNode }) => (
    <EvidenceDrawerProvider>{children}</EvidenceDrawerProvider>
  );
}

describe('useEvidenceDrawer', () => {
  describe('initial state', () => {
    it('returns correct initial state', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current.isOpen).toBe(false);
      expect(result.current.activeMessageId).toBeNull();
      expect(result.current.activeTab).toBe('rulebook');
      expect(result.current.currentEvidence).toBeNull();
      expect(result.current.currentRouterDecision).toBeNull();
      expect(result.current.currentRequestId).toBeNull();
      expect(result.current.currentTraceId).toBeNull();
      expect(result.current.currentProofDigest).toBeNull();
      expect(result.current.currentIsVerified).toBe(false);
      expect(result.current.currentVerifiedAt).toBeNull();
    });

    it('provides all action functions', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(typeof result.current.openDrawer).toBe('function');
      expect(typeof result.current.closeDrawer).toBe('function');
      expect(typeof result.current.setActiveTab).toBe('function');
      expect(typeof result.current.setMessageData).toBe('function');
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
        result.current.openDrawer('msg-456', 'calculation');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-456');
      expect(result.current.activeTab).toBe('calculation');
    });

    it('opens drawer with trace tab', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-789', 'trace');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-789');
      expect(result.current.activeTab).toBe('trace');
    });

    it('preserves current tab when no tab specified', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Set tab first
      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');

      // Open drawer without specifying tab
      act(() => {
        result.current.openDrawer('msg-123');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeTab).toBe('calculation'); // Should preserve
    });

    it('can switch message while drawer is open', () => {
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

      // Open first
      act(() => {
        result.current.openDrawer('msg-123');
      });

      expect(result.current.isOpen).toBe(true);

      // Then close
      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
    });

    it('preserves activeMessageId when closing', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.openDrawer('msg-123');
      });

      const messageId = result.current.activeMessageId;

      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
      expect(result.current.activeMessageId).toBe(messageId);
    });

    it('can be called when already closed', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      expect(result.current.isOpen).toBe(false);

      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
    });
  });

  describe('setActiveTab', () => {
    it('sets tab to rulebook', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('rulebook');
      });

      expect(result.current.activeTab).toBe('rulebook');
    });

    it('sets tab to calculation', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');
    });

    it('sets tab to trace', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('trace');
      });

      expect(result.current.activeTab).toBe('trace');
    });

    it('switches between tabs', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('rulebook');
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

    it('can set same tab multiple times', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');

      act(() => {
        result.current.setActiveTab('calculation');
      });

      expect(result.current.activeTab).toBe('calculation');
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

    it('sets request ID', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ requestId: 'req-123' });
      });

      expect(result.current.currentRequestId).toBe('req-123');
    });

    it('sets trace ID', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ traceId: 'trace-456' });
      });

      expect(result.current.currentTraceId).toBe('trace-456');
    });

    it('sets proof digest', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ proofDigest: 'digest-abc' });
      });

      expect(result.current.currentProofDigest).toBe('digest-abc');
    });

    it('sets verification status', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ isVerified: true });
      });

      expect(result.current.currentIsVerified).toBe(true);
    });

    it('sets verified timestamp', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      const timestamp = '2025-01-01T12:00:00Z';

      act(() => {
        result.current.setMessageData({ verifiedAt: timestamp });
      });

      expect(result.current.currentVerifiedAt).toBe(timestamp);
    });

    it('sets multiple fields at once', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          routerDecision: mockRouterDecision,
          requestId: 'req-123',
          traceId: 'trace-456',
          proofDigest: 'digest-abc',
          isVerified: true,
          verifiedAt: '2025-01-01T12:00:00Z',
        });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRouterDecision).toEqual(mockRouterDecision);
      expect(result.current.currentRequestId).toBe('req-123');
      expect(result.current.currentTraceId).toBe('trace-456');
      expect(result.current.currentProofDigest).toBe('digest-abc');
      expect(result.current.currentIsVerified).toBe(true);
      expect(result.current.currentVerifiedAt).toBe('2025-01-01T12:00:00Z');
    });

    it('preserves existing fields when setting partial data', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Set initial data
      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          requestId: 'req-123',
        });
      });

      // Update only trace ID
      act(() => {
        result.current.setMessageData({ traceId: 'trace-456' });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRequestId).toBe('req-123');
      expect(result.current.currentTraceId).toBe('trace-456');
    });

    it('handles empty evidence array', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setMessageData({ evidence: [] });
      });

      expect(result.current.currentEvidence).toEqual([]);
    });

    it('can clear evidence by setting to null-like value', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Set evidence first
      act(() => {
        result.current.setMessageData({ evidence: mockEvidence });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);

      // Note: Based on implementation, setting undefined preserves current value
      // This test verifies the preservation behavior
      act(() => {
        result.current.setMessageData({});
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
    });
  });

  describe('integration scenarios', () => {
    it('handles complete workflow: open, set data, switch tab, close', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // Open drawer
      act(() => {
        result.current.openDrawer('msg-123', 'rulebook');
      });

      expect(result.current.isOpen).toBe(true);
      expect(result.current.activeMessageId).toBe('msg-123');
      expect(result.current.activeTab).toBe('rulebook');

      // Set message data
      act(() => {
        result.current.setMessageData({
          evidence: mockEvidence,
          routerDecision: mockRouterDecision,
          traceId: 'trace-123',
        });
      });

      expect(result.current.currentEvidence).toEqual(mockEvidence);
      expect(result.current.currentRouterDecision).toEqual(mockRouterDecision);

      // Switch tab
      act(() => {
        result.current.setActiveTab('trace');
      });

      expect(result.current.activeTab).toBe('trace');
      expect(result.current.isOpen).toBe(true);

      // Close drawer
      act(() => {
        result.current.closeDrawer();
      });

      expect(result.current.isOpen).toBe(false);
      // Data should persist after closing
      expect(result.current.currentEvidence).toEqual(mockEvidence);
    });

    it('handles switching between messages', () => {
      const { result } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      // First message
      act(() => {
        result.current.openDrawer('msg-1');
        result.current.setMessageData({
          evidence: mockEvidence,
          requestId: 'req-1',
        });
      });

      // Switch to second message
      act(() => {
        result.current.openDrawer('msg-2');
        result.current.setMessageData({
          evidence: [],
          requestId: 'req-2',
        });
      });

      expect(result.current.activeMessageId).toBe('msg-2');
      expect(result.current.currentEvidence).toEqual([]);
      expect(result.current.currentRequestId).toBe('req-2');
    });
  });

  describe('callback stability', () => {
    it('maintains stable function references', () => {
      const { result, rerender } = renderHook(() => useEvidenceDrawer(), {
        wrapper: createWrapper(),
      });

      const initialOpenDrawer = result.current.openDrawer;
      const initialCloseDrawer = result.current.closeDrawer;
      const initialSetActiveTab = result.current.setActiveTab;
      const initialSetMessageData = result.current.setMessageData;

      rerender();

      expect(result.current.openDrawer).toBe(initialOpenDrawer);
      expect(result.current.closeDrawer).toBe(initialCloseDrawer);
      expect(result.current.setActiveTab).toBe(initialSetActiveTab);
      expect(result.current.setMessageData).toBe(initialSetMessageData);
    });
  });
});

describe('useEvidenceDrawerOptional', () => {
  it('returns context when inside provider', () => {
    const { result } = renderHook(() => useEvidenceDrawerOptional(), {
      wrapper: createWrapper(),
    });

    expect(result.current).not.toBeNull();
    expect(result.current?.isOpen).toBe(false);
  });

  it('returns null when outside provider', () => {
    const { result } = renderHook(() => useEvidenceDrawerOptional());

    expect(result.current).toBeNull();
  });
});

describe('useEvidenceDrawer error handling', () => {
  it('throws error when used outside provider', () => {
    // Suppress console.error for this test
    const originalError = console.error;
    console.error = () => {};

    expect(() => {
      renderHook(() => useEvidenceDrawer());
    }).toThrow('useEvidenceDrawer must be used within an EvidenceDrawerProvider');

    console.error = originalError;
  });
});
