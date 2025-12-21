/**
 * Tests for useReducedMotion hook
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useReducedMotion } from '@/hooks/ui/useReducedMotion';

describe('useReducedMotion', () => {
  let matchMediaMock: {
    matches: boolean;
    media: string;
    addEventListener: ReturnType<typeof vi.fn>;
    removeEventListener: ReturnType<typeof vi.fn>;
    addListener: ReturnType<typeof vi.fn>;
    removeListener: ReturnType<typeof vi.fn>;
    dispatchEvent: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    // Create a mock matchMedia object
    matchMediaMock = {
      matches: false,
      media: '(prefers-reduced-motion: reduce)',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    };

    // Mock window.matchMedia
    window.matchMedia = vi.fn().mockImplementation((query) => {
      matchMediaMock.media = query;
      return matchMediaMock;
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('should return false when prefers-reduced-motion is not set', () => {
    matchMediaMock.matches = false;

    const { result } = renderHook(() => useReducedMotion());

    expect(result.current).toBe(false);
  });

  it('should return true when prefers-reduced-motion is set to reduce', () => {
    matchMediaMock.matches = true;

    const { result } = renderHook(() => useReducedMotion());

    expect(result.current).toBe(true);
  });

  it('should register event listener for media query changes', () => {
    renderHook(() => useReducedMotion());

    expect(matchMediaMock.addEventListener).toHaveBeenCalledWith(
      'change',
      expect.any(Function)
    );
  });

  it('should update when media query preference changes', () => {
    matchMediaMock.matches = false;

    const { result, rerender } = renderHook(() => useReducedMotion());

    expect(result.current).toBe(false);

    // Simulate media query change
    matchMediaMock.matches = true;
    const changeHandler = matchMediaMock.addEventListener.mock.calls[0][1];
    changeHandler({ matches: true } as MediaQueryListEvent);

    rerender();
    expect(result.current).toBe(true);
  });

  it('should remove event listener on unmount', () => {
    const { unmount } = renderHook(() => useReducedMotion());

    unmount();

    expect(matchMediaMock.removeEventListener).toHaveBeenCalledWith(
      'change',
      expect.any(Function)
    );
  });

  it('should use fallback addListener for older browsers', () => {
    // Simulate older browser without addEventListener
    const oldMatchMediaMock = {
      ...matchMediaMock,
      addEventListener: undefined,
      removeEventListener: undefined,
    };

    window.matchMedia = vi.fn().mockImplementation(() => oldMatchMediaMock);

    renderHook(() => useReducedMotion());

    expect(oldMatchMediaMock.addListener).toHaveBeenCalledWith(
      expect.any(Function)
    );
  });

  it('should use fallback removeListener for older browsers on unmount', () => {
    // Simulate older browser without removeEventListener
    const oldMatchMediaMock = {
      ...matchMediaMock,
      addEventListener: undefined,
      removeEventListener: undefined,
    };

    window.matchMedia = vi.fn().mockImplementation(() => oldMatchMediaMock);

    const { unmount } = renderHook(() => useReducedMotion());

    unmount();

    expect(oldMatchMediaMock.removeListener).toHaveBeenCalledWith(
      expect.any(Function)
    );
  });

  it('should return false when matchMedia is not available', () => {
    // Simulate environment where matchMedia is not available
    const originalMatchMedia = window.matchMedia;
    // @ts-expect-error - mocking missing matchMedia
    window.matchMedia = undefined;

    const { result } = renderHook(() => useReducedMotion());

    expect(result.current).toBe(false);

    // Restore matchMedia
    window.matchMedia = originalMatchMedia;
  });

  it('should query the correct media query', () => {
    renderHook(() => useReducedMotion());

    expect(window.matchMedia).toHaveBeenCalledWith('(prefers-reduced-motion: reduce)');
  });

  it('should handle multiple re-renders without errors', () => {
    const { rerender } = renderHook(() => useReducedMotion());

    const initialCallCount = matchMediaMock.addEventListener.mock.calls.length;

    // Multiple re-renders should not cause errors
    expect(() => {
      rerender();
      rerender();
      rerender();
    }).not.toThrow();

    // Should have registered event listener at least once
    expect(matchMediaMock.addEventListener.mock.calls.length).toBeGreaterThanOrEqual(
      initialCallCount
    );
  });
});
