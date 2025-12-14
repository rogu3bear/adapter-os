import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { readLocalStorage, writeLocalStorage } from '@/utils/storage';

describe('storage utilities', () => {
  describe('readLocalStorage', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('reads existing value from localStorage', () => {
      localStorage.setItem('test-key', 'test-value');
      const result = readLocalStorage('test-key');
      expect(result).toBe('test-value');
    });

    it('returns null for non-existent key', () => {
      const result = readLocalStorage('non-existent');
      expect(result).toBeNull();
    });

    it('handles empty string key', () => {
      localStorage.setItem('', 'empty-key-value');
      const result = readLocalStorage('');
      expect(result).toBe('empty-key-value');
    });

    it('returns null when localStorage throws error', () => {
      const getItemSpy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
        throw new Error('Storage error');
      });

      const result = readLocalStorage('error-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('handles special characters in key', () => {
      const specialKey = 'key-with-special!@#$%^&*()';
      localStorage.setItem(specialKey, 'special-value');
      const result = readLocalStorage(specialKey);
      expect(result).toBe('special-value');
    });

    it('handles reading empty string value', () => {
      localStorage.setItem('empty-value', '');
      const result = readLocalStorage('empty-value');
      expect(result).toBe('');
    });
  });

  describe('writeLocalStorage', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('writes value to localStorage', () => {
      writeLocalStorage('test-key', 'test-value');
      expect(localStorage.getItem('test-key')).toBe('test-value');
    });

    it('overwrites existing value', () => {
      localStorage.setItem('test-key', 'old-value');
      writeLocalStorage('test-key', 'new-value');
      expect(localStorage.getItem('test-key')).toBe('new-value');
    });

    it('handles empty string key', () => {
      writeLocalStorage('', 'empty-key-value');
      expect(localStorage.getItem('')).toBe('empty-key-value');
    });

    it('handles empty string value', () => {
      writeLocalStorage('test-key', '');
      expect(localStorage.getItem('test-key')).toBe('');
    });

    it('handles special characters in key and value', () => {
      const specialKey = 'key!@#$%^&*()';
      const specialValue = 'value!@#$%^&*()';
      writeLocalStorage(specialKey, specialValue);
      expect(localStorage.getItem(specialKey)).toBe(specialValue);
    });

    it('silently ignores localStorage errors', () => {
      const setItemSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
        throw new Error('Storage quota exceeded');
      });

      // Should not throw
      expect(() => writeLocalStorage('error-key', 'error-value')).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('handles very long values', () => {
      const longValue = 'a'.repeat(10000);
      writeLocalStorage('long-value', longValue);
      expect(localStorage.getItem('long-value')).toBe(longValue);
    });

    it('handles unicode characters', () => {
      const unicodeValue = '你好世界 🚀 مرحبا';
      writeLocalStorage('unicode-key', unicodeValue);
      expect(localStorage.getItem('unicode-key')).toBe(unicodeValue);
    });
  });

  describe('SSR compatibility', () => {
    let windowSpy: any;

    beforeEach(() => {
      // Mock SSR environment (no window)
      windowSpy = vi.spyOn(global, 'window', 'get');
    });

    afterEach(() => {
      windowSpy.mockRestore();
    });

    it('readLocalStorage returns null in SSR', () => {
      windowSpy.mockReturnValue(undefined);
      const result = readLocalStorage('test-key');
      expect(result).toBeNull();
    });

    it('writeLocalStorage no-ops in SSR', () => {
      windowSpy.mockReturnValue(undefined);
      // Should not throw
      expect(() => writeLocalStorage('test-key', 'test-value')).not.toThrow();
    });
  });
});
