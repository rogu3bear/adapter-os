/**
 * Tests for WorkbenchContext
 *
 * Tests context provider, state management, localStorage persistence,
 * undo actions, and keyboard handling.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, renderHook, act, waitFor } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  WorkbenchProvider,
  useWorkbench,
  type UndoAction,
} from '@/contexts/WorkbenchContext';

// Mock storage utilities
vi.mock('@/utils/storage', () => ({
  readLocalStorage: vi.fn((key: string) => {
    return localStorage.getItem(key);
  }),
  writeLocalStorage: vi.fn((key: string, value: string) => {
    localStorage.setItem(key, value);
  }),
}));

const STORAGE_KEYS = {
  LEFT_RAIL_TAB: 'workbench:leftRail:activeTab',
  LEFT_RAIL_SCROLL: 'workbench:leftRail:scrollPositions',
  RIGHT_RAIL_COLLAPSED: 'workbench:rightRail:collapsed',
} as const;

function createWrapper() {
  return ({ children }: { children: ReactNode }) => (
    <WorkbenchProvider>{children}</WorkbenchProvider>
  );
}

describe('WorkbenchContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Provider and Hook', () => {
    it('throws error when useWorkbench is used outside provider', () => {
      // Suppress expected console.error
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      expect(() => {
        renderHook(() => useWorkbench());
      }).toThrow('useWorkbench must be used within a WorkbenchProvider');

      consoleError.mockRestore();
    });

    it('provides context value when used within provider', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current).toBeDefined();
      expect(result.current.activeLeftTab).toBe('sessions');
      expect(result.current.rightRailCollapsed).toBe(false);
    });
  });

  describe('Left Rail Tab Management', () => {
    it('initializes with default tab (sessions)', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeLeftTab).toBe('sessions');
    });

    it('restores active tab from localStorage', () => {
      localStorage.setItem(STORAGE_KEYS.LEFT_RAIL_TAB, 'datasets');

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeLeftTab).toBe('datasets');
    });

    it('changes active tab and persists to localStorage', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveLeftTab('stacks');
      });

      expect(result.current.activeLeftTab).toBe('stacks');
      expect(localStorage.getItem(STORAGE_KEYS.LEFT_RAIL_TAB)).toBe('stacks');
    });

    it('ignores invalid tab values from localStorage', () => {
      localStorage.setItem(STORAGE_KEYS.LEFT_RAIL_TAB, 'invalid-tab');

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeLeftTab).toBe('sessions');
    });
  });

  describe('Scroll Position Management', () => {
    it('initializes with empty scroll positions', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.getScrollPosition('sessions')).toBe(0);
      expect(result.current.getScrollPosition('datasets')).toBe(0);
    });

    it('restores scroll positions from localStorage', () => {
      const positions = { sessions: 100, datasets: 200 };
      localStorage.setItem(STORAGE_KEYS.LEFT_RAIL_SCROLL, JSON.stringify(positions));

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.getScrollPosition('sessions')).toBe(100);
      expect(result.current.getScrollPosition('datasets')).toBe(200);
    });

    it('saves scroll position and persists to localStorage', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.saveScrollPosition('sessions', 150);
      });

      expect(result.current.getScrollPosition('sessions')).toBe(150);

      const stored = JSON.parse(localStorage.getItem(STORAGE_KEYS.LEFT_RAIL_SCROLL) || '{}');
      expect(stored.sessions).toBe(150);
    });

    it('handles invalid JSON in localStorage scroll positions', () => {
      localStorage.setItem(STORAGE_KEYS.LEFT_RAIL_SCROLL, 'invalid-json{');

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.getScrollPosition('sessions')).toBe(0);
    });
  });

  describe('Right Rail Management', () => {
    it('initializes with right rail expanded', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.rightRailCollapsed).toBe(false);
    });

    it('restores right rail state from localStorage', () => {
      localStorage.setItem(STORAGE_KEYS.RIGHT_RAIL_COLLAPSED, 'true');

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.rightRailCollapsed).toBe(true);
    });

    it('sets right rail collapsed state and persists', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setRightRailCollapsed(true);
      });

      expect(result.current.rightRailCollapsed).toBe(true);
      expect(localStorage.getItem(STORAGE_KEYS.RIGHT_RAIL_COLLAPSED)).toBe('true');
    });

    it('toggles right rail state', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.toggleRightRail();
      });

      expect(result.current.rightRailCollapsed).toBe(true);

      act(() => {
        result.current.toggleRightRail();
      });

      expect(result.current.rightRailCollapsed).toBe(false);
    });
  });

  describe('Message Selection and Pinning', () => {
    it('initializes with no message selected or pinned', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.selectedMessageId).toBeNull();
      expect(result.current.pinnedMessageId).toBeNull();
    });

    it('selects a message', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.selectMessage('msg-123');
      });

      expect(result.current.selectedMessageId).toBe('msg-123');
    });

    it('pins a message', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.pinMessage('msg-456');
      });

      expect(result.current.pinnedMessageId).toBe('msg-456');
    });

    it('does not auto-select when message is pinned', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.pinMessage('msg-pinned');
        result.current.selectMessage('msg-pinned');
      });

      expect(result.current.selectedMessageId).toBe('msg-pinned');

      // Try to auto-select a different message - should not work
      act(() => {
        result.current.selectMessage('msg-other');
      });

      expect(result.current.selectedMessageId).toBe('msg-pinned');
    });

    it('allows explicit deselection with null even when pinned', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.pinMessage('msg-pinned');
        result.current.selectMessage('msg-pinned');
      });

      act(() => {
        result.current.selectMessage(null);
      });

      expect(result.current.selectedMessageId).toBeNull();
    });
  });

  describe('Adapter Strength Overrides', () => {
    it('initializes with empty overrides', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.strengthOverrides).toEqual({});
    });

    it('sets strength overrides', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      const overrides = { 'adapter-1': 0.5, 'adapter-2': 0.8 };

      act(() => {
        result.current.setStrengthOverrides(overrides);
      });

      expect(result.current.strengthOverrides).toEqual(overrides);
    });

    it('updates individual adapter strength', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.updateStrengthOverride('adapter-1', 0.7);
      });

      expect(result.current.strengthOverrides).toEqual({ 'adapter-1': 0.7 });

      act(() => {
        result.current.updateStrengthOverride('adapter-2', 0.9);
      });

      expect(result.current.strengthOverrides).toEqual({
        'adapter-1': 0.7,
        'adapter-2': 0.9,
      });
    });

    it('clears all strength overrides', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setStrengthOverrides({ 'adapter-1': 0.5 });
      });

      expect(result.current.strengthOverrides).toEqual({ 'adapter-1': 0.5 });

      act(() => {
        result.current.clearStrengthOverrides();
      });

      expect(result.current.strengthOverrides).toEqual({});
    });
  });

  describe('Undo Actions', () => {
    it('initializes with no undo action', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result.current.undoAction).toBeNull();
    });

    it('sets undo action', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      const action: UndoAction = {
        type: 'detach_all',
        previousStackId: 'stack-123',
        previousAdapterOverrides: { 'adapter-1': 0.5 },
        expiresAt: Date.now() + 10000,
      };

      act(() => {
        result.current.setUndoAction(action);
      });

      expect(result.current.undoAction).toEqual(action);
    });

    it('clears undo action', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      const action: UndoAction = {
        type: 'detach_all',
        previousStackId: 'stack-123',
        previousAdapterOverrides: {},
        expiresAt: Date.now() + 10000,
      };

      act(() => {
        result.current.setUndoAction(action);
      });

      expect(result.current.undoAction).not.toBeNull();

      act(() => {
        result.current.clearUndoAction();
      });

      expect(result.current.undoAction).toBeNull();
    });

    it('auto-expires undo action after timeout', async () => {
      vi.useFakeTimers();

      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      const action: UndoAction = {
        type: 'detach_all',
        previousStackId: 'stack-123',
        previousAdapterOverrides: {},
        expiresAt: Date.now() + 1000, // 1 second
      };

      act(() => {
        result.current.setUndoAction(action);
      });

      expect(result.current.undoAction).toEqual(action);

      // Fast-forward time and flush promises
      await act(async () => {
        vi.advanceTimersByTime(1100);
        await Promise.resolve();
      });

      expect(result.current.undoAction).toBeNull();

      vi.useRealTimers();
    });

    it('immediately clears expired undo action', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      const expiredAction: UndoAction = {
        type: 'detach_all',
        previousStackId: 'stack-123',
        previousAdapterOverrides: {},
        expiresAt: Date.now() - 1000, // Already expired
      };

      act(() => {
        result.current.setUndoAction(expiredAction);
      });

      expect(result.current.undoAction).toBeNull();
    });
  });

  describe('Keyboard Handling', () => {
    beforeEach(() => {
      // Clear document body
      document.body.innerHTML = '';
    });

    it('collapses right rail on escape when expanded', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      // Ensure right rail is expanded
      expect(result.current.rightRailCollapsed).toBe(false);

      let handled = false;
      act(() => {
        handled = result.current.handleGlobalEscape();
      });

      expect(handled).toBe(true);
      expect(result.current.rightRailCollapsed).toBe(true);
    });

    it('focuses chat input on escape when right rail already collapsed', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      // Collapse right rail first
      act(() => {
        result.current.setRightRailCollapsed(true);
      });

      // Add chat input to DOM
      const chatInput = document.createElement('input');
      chatInput.setAttribute('data-testid', 'chat-input');
      document.body.appendChild(chatInput);

      const focusSpy = vi.spyOn(chatInput, 'focus');

      let handled = false;
      act(() => {
        handled = result.current.handleGlobalEscape();
      });

      expect(handled).toBe(true);
      expect(focusSpy).toHaveBeenCalled();

      focusSpy.mockRestore();
    });

    it('returns false when right rail is collapsed and no chat input exists', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      // Collapse right rail
      act(() => {
        result.current.setRightRailCollapsed(true);
      });

      let handled = false;
      act(() => {
        handled = result.current.handleGlobalEscape();
      });

      expect(handled).toBe(false);
    });
  });

  describe('Integration Tests', () => {
    it('maintains independent state for all features', () => {
      const { result } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveLeftTab('datasets');
        result.current.saveScrollPosition('datasets', 100);
        result.current.setRightRailCollapsed(true);
        result.current.pinMessage('msg-1');
        result.current.updateStrengthOverride('adapter-1', 0.5);
        result.current.setUndoAction({
          type: 'detach_all',
          previousStackId: 'stack-1',
          previousAdapterOverrides: {},
          expiresAt: Date.now() + 5000,
        });
      });

      expect(result.current.activeLeftTab).toBe('datasets');
      expect(result.current.getScrollPosition('datasets')).toBe(100);
      expect(result.current.rightRailCollapsed).toBe(true);
      expect(result.current.pinnedMessageId).toBe('msg-1');
      expect(result.current.strengthOverrides).toEqual({ 'adapter-1': 0.5 });
      expect(result.current.undoAction).not.toBeNull();
    });

    it('persists state across remounts', () => {
      const { result: result1, unmount } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result1.current.setActiveLeftTab('stacks');
        result1.current.setRightRailCollapsed(true);
        result1.current.saveScrollPosition('stacks', 250);
      });

      unmount();

      const { result: result2 } = renderHook(() => useWorkbench(), {
        wrapper: createWrapper(),
      });

      expect(result2.current.activeLeftTab).toBe('stacks');
      expect(result2.current.rightRailCollapsed).toBe(true);
      expect(result2.current.getScrollPosition('stacks')).toBe(250);
    });
  });
});
