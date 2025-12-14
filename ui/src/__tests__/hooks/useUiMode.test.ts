import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode, UI_MODE_OPTIONS, UI_MODE_STORAGE_KEY } from '@/config/ui-mode';

// Mock storage utilities
const mockReadLocalStorage = vi.fn();
const mockWriteLocalStorage = vi.fn();

vi.mock('@/utils/storage', () => ({
  readLocalStorage: (...args: unknown[]) => mockReadLocalStorage(...args),
  writeLocalStorage: (...args: unknown[]) => mockWriteLocalStorage(...args),
}));

describe('useUiMode', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns User mode as default when no stored value', () => {
      mockReadLocalStorage.mockReturnValue(null);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
      expect(mockReadLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY);
    });

    it('loads stored User mode from localStorage', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('loads stored Builder mode from localStorage', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Builder);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.Builder);
    });

    it('loads stored Audit mode from localStorage', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Audit);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.Audit);
    });

    it('defaults to User mode for invalid stored value', () => {
      mockReadLocalStorage.mockReturnValue('invalid-mode');

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('defaults to User mode for empty string', () => {
      mockReadLocalStorage.mockReturnValue('');

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('returns available modes', () => {
      mockReadLocalStorage.mockReturnValue(null);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.availableModes).toEqual(UI_MODE_OPTIONS);
      expect(result.current.availableModes).toContain(UiMode.User);
      expect(result.current.availableModes).toContain(UiMode.Builder);
      expect(result.current.availableModes).toContain(UiMode.Audit);
    });

    it('returns setUiMode function', () => {
      mockReadLocalStorage.mockReturnValue(null);

      const { result } = renderHook(() => useUiMode());

      expect(typeof result.current.setUiMode).toBe('function');
    });
  });

  describe('setUiMode', () => {
    it('updates mode to Builder', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.uiMode).toBe(UiMode.Builder);
      expect(mockWriteLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY, UiMode.Builder);
    });

    it('updates mode to Audit', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      act(() => {
        result.current.setUiMode(UiMode.Audit);
      });

      expect(result.current.uiMode).toBe(UiMode.Audit);
      expect(mockWriteLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY, UiMode.Audit);
    });

    it('updates mode back to User', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Builder);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.Builder);

      act(() => {
        result.current.setUiMode(UiMode.User);
      });

      expect(result.current.uiMode).toBe(UiMode.User);
      expect(mockWriteLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY, UiMode.User);
    });

    it('persists mode change to localStorage', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(mockWriteLocalStorage).toHaveBeenCalledTimes(1);
      expect(mockWriteLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY, UiMode.Builder);
    });

    it('handles multiple mode changes', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.uiMode).toBe(UiMode.Builder);

      act(() => {
        result.current.setUiMode(UiMode.Audit);
      });

      expect(result.current.uiMode).toBe(UiMode.Audit);

      act(() => {
        result.current.setUiMode(UiMode.User);
      });

      expect(result.current.uiMode).toBe(UiMode.User);

      expect(mockWriteLocalStorage).toHaveBeenCalledTimes(3);
    });
  });

  describe('setUiMode callback stability', () => {
    it('maintains stable reference across re-renders', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result, rerender } = renderHook(() => useUiMode());

      const firstSetUiMode = result.current.setUiMode;

      rerender();

      const secondSetUiMode = result.current.setUiMode;

      expect(firstSetUiMode).toBe(secondSetUiMode);
    });

    it('setUiMode reference does not change after mode update', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      const initialSetUiMode = result.current.setUiMode;

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.setUiMode).toBe(initialSetUiMode);
    });
  });

  describe('edge cases', () => {
    it('handles localStorage read error gracefully', () => {
      mockReadLocalStorage.mockReturnValue(null);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('handles setting same mode multiple times', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.uiMode).toBe(UiMode.Builder);

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.uiMode).toBe(UiMode.Builder);
      expect(mockWriteLocalStorage).toHaveBeenCalledTimes(2);
    });

    it('handles case-sensitive mode values', () => {
      // Uppercase should not match
      mockReadLocalStorage.mockReturnValue('BUILDER');

      const { result } = renderHook(() => useUiMode());

      // Should default to User since 'BUILDER' is not valid
      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('handles numeric values in storage', () => {
      mockReadLocalStorage.mockReturnValue('123');

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });

    it('handles object values in storage', () => {
      mockReadLocalStorage.mockReturnValue('{"mode": "builder"}');

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);
    });
  });

  describe('localStorage integration', () => {
    it('reads from correct storage key on mount', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Builder);

      renderHook(() => useUiMode());

      expect(mockReadLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY);
      expect(mockReadLocalStorage).toHaveBeenCalledTimes(1);
    });

    it('writes to correct storage key on update', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      act(() => {
        result.current.setUiMode(UiMode.Audit);
      });

      expect(mockWriteLocalStorage).toHaveBeenCalledWith(UI_MODE_STORAGE_KEY, UiMode.Audit);
    });

    it('does not write to storage on initial mount', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Builder);

      renderHook(() => useUiMode());

      expect(mockWriteLocalStorage).not.toHaveBeenCalled();
    });
  });

  describe('mode transitions', () => {
    it('transitions from User to Builder to Audit', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.uiMode).toBe(UiMode.User);

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.uiMode).toBe(UiMode.Builder);

      act(() => {
        result.current.setUiMode(UiMode.Audit);
      });

      expect(result.current.uiMode).toBe(UiMode.Audit);
    });

    it('can transition to any mode from any mode', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.Audit);

      const { result } = renderHook(() => useUiMode());

      // Audit -> User
      act(() => {
        result.current.setUiMode(UiMode.User);
      });
      expect(result.current.uiMode).toBe(UiMode.User);

      // User -> Audit
      act(() => {
        result.current.setUiMode(UiMode.Audit);
      });
      expect(result.current.uiMode).toBe(UiMode.Audit);

      // Audit -> Builder
      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });
      expect(result.current.uiMode).toBe(UiMode.Builder);

      // Builder -> User
      act(() => {
        result.current.setUiMode(UiMode.User);
      });
      expect(result.current.uiMode).toBe(UiMode.User);
    });
  });

  describe('availableModes', () => {
    it('returns all three modes', () => {
      mockReadLocalStorage.mockReturnValue(null);

      const { result } = renderHook(() => useUiMode());

      expect(result.current.availableModes).toHaveLength(3);
      expect(result.current.availableModes).toEqual([
        UiMode.User,
        UiMode.Builder,
        UiMode.Audit,
      ]);
    });

    it('availableModes remains constant across mode changes', () => {
      mockReadLocalStorage.mockReturnValue(UiMode.User);

      const { result } = renderHook(() => useUiMode());

      const initialModes = result.current.availableModes;

      act(() => {
        result.current.setUiMode(UiMode.Builder);
      });

      expect(result.current.availableModes).toEqual(initialModes);
    });
  });
});
