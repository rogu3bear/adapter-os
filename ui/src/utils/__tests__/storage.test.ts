import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { readLocalStorage, writeLocalStorage } from '@/utils/storage';

describe('storage utilities', () => {
  describe('readLocalStorage', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('should successfully read existing value from localStorage', () => {
      localStorage.setItem('test-key', 'test-value');
      const result = readLocalStorage('test-key');
      expect(result).toBe('test-value');
    });

    it('should return null for missing key', () => {
      const result = readLocalStorage('non-existent-key');
      expect(result).toBeNull();
    });

    it('should handle empty string value', () => {
      localStorage.setItem('empty-value', '');
      const result = readLocalStorage('empty-value');
      expect(result).toBe('');
    });

    it('should handle JSON strings correctly', () => {
      const jsonData = JSON.stringify({ name: 'test', value: 123 });
      localStorage.setItem('json-key', jsonData);
      const result = readLocalStorage('json-key');
      expect(result).toBe(jsonData);
      // Caller can parse if needed
      expect(JSON.parse(result!)).toEqual({ name: 'test', value: 123 });
    });

    it('should handle special characters in key', () => {
      const specialKey = 'key!@#$%^&*()_+-=[]{}|;:,.<>?';
      localStorage.setItem(specialKey, 'special-value');
      const result = readLocalStorage(specialKey);
      expect(result).toBe('special-value');
    });

    it('should handle unicode characters in value', () => {
      const unicodeValue = '你好世界 🚀 مرحبا עברית';
      localStorage.setItem('unicode-key', unicodeValue);
      const result = readLocalStorage('unicode-key');
      expect(result).toBe(unicodeValue);
    });

    it('should handle empty string key', () => {
      localStorage.setItem('', 'empty-key-value');
      const result = readLocalStorage('');
      expect(result).toBe('empty-key-value');
    });

    it('should gracefully handle localStorage.getItem errors', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new Error('Storage access denied');
        });

      const result = readLocalStorage('error-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('should handle SecurityError (incognito mode)', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new DOMException('SecurityError', 'SecurityError');
        });

      const result = readLocalStorage('security-error-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('should handle generic storage errors gracefully', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new Error('Unknown storage error');
        });

      const result = readLocalStorage('unknown-error');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('should handle very long values', () => {
      const longValue = 'a'.repeat(100000);
      localStorage.setItem('long-value', longValue);
      const result = readLocalStorage('long-value');
      expect(result).toBe(longValue);
    });

    it('should handle null value stored in localStorage', () => {
      localStorage.setItem('null-value', 'null');
      const result = readLocalStorage('null-value');
      expect(result).toBe('null');
    });

    it('should handle undefined value stored as string', () => {
      localStorage.setItem('undefined-value', 'undefined');
      const result = readLocalStorage('undefined-value');
      expect(result).toBe('undefined');
    });

    it('should handle complex JSON with nested structures', () => {
      const complexData = JSON.stringify({
        users: [
          { id: 1, name: 'Alice', meta: { active: true } },
          { id: 2, name: 'Bob', meta: { active: false } },
        ],
        timestamp: new Date().toISOString(),
        nested: { deeply: { nested: { value: 'found' } } },
      });
      localStorage.setItem('complex-json', complexData);
      const result = readLocalStorage('complex-json');
      expect(result).toBe(complexData);
      expect(JSON.parse(result!)).toHaveProperty('users');
      expect(JSON.parse(result!).nested.deeply.nested.value).toBe('found');
    });

    it('should handle array as JSON string', () => {
      const arrayData = JSON.stringify([1, 2, 3, 4, 5]);
      localStorage.setItem('array-data', arrayData);
      const result = readLocalStorage('array-data');
      expect(result).toBe(arrayData);
      expect(JSON.parse(result!)).toEqual([1, 2, 3, 4, 5]);
    });

    it('should handle boolean stored as string', () => {
      localStorage.setItem('bool-true', 'true');
      localStorage.setItem('bool-false', 'false');
      expect(readLocalStorage('bool-true')).toBe('true');
      expect(readLocalStorage('bool-false')).toBe('false');
    });

    it('should handle number stored as string', () => {
      localStorage.setItem('number', '12345.67');
      const result = readLocalStorage('number');
      expect(result).toBe('12345.67');
      expect(parseFloat(result!)).toBe(12345.67);
    });

    it('should handle whitespace-only values', () => {
      localStorage.setItem('whitespace', '   \n\t  ');
      const result = readLocalStorage('whitespace');
      expect(result).toBe('   \n\t  ');
    });

    it('should handle HTML/XML strings', () => {
      const htmlString = '<div class="test">Hello <strong>World</strong></div>';
      localStorage.setItem('html-content', htmlString);
      const result = readLocalStorage('html-content');
      expect(result).toBe(htmlString);
    });

    it('should handle malformed JSON gracefully', () => {
      // Storage doesn't parse, so malformed JSON is just a string
      const malformedJson = '{"incomplete": "json"';
      localStorage.setItem('malformed', malformedJson);
      const result = readLocalStorage('malformed');
      expect(result).toBe(malformedJson);
      // Attempting to parse would fail, but that's the caller's responsibility
      expect(() => JSON.parse(result!)).toThrow();
    });
  });

  describe('writeLocalStorage', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('should successfully write value to localStorage', () => {
      writeLocalStorage('test-key', 'test-value');
      expect(localStorage.getItem('test-key')).toBe('test-value');
    });

    it('should overwrite existing value', () => {
      localStorage.setItem('test-key', 'old-value');
      writeLocalStorage('test-key', 'new-value');
      expect(localStorage.getItem('test-key')).toBe('new-value');
    });

    it('should handle empty string key', () => {
      writeLocalStorage('', 'empty-key-value');
      expect(localStorage.getItem('')).toBe('empty-key-value');
    });

    it('should handle empty string value', () => {
      writeLocalStorage('test-key', '');
      expect(localStorage.getItem('test-key')).toBe('');
    });

    it('should handle special characters in key and value', () => {
      const specialKey = 'key!@#$%^&*()_+-=';
      const specialValue = 'value!@#$%^&*()_+-=';
      writeLocalStorage(specialKey, specialValue);
      expect(localStorage.getItem(specialKey)).toBe(specialValue);
    });

    it('should handle unicode characters', () => {
      const unicodeValue = '你好世界 🚀 مرحبا עברית Ελληνικά';
      writeLocalStorage('unicode-key', unicodeValue);
      expect(localStorage.getItem('unicode-key')).toBe(unicodeValue);
    });

    it('should handle very long values', () => {
      const longValue = 'a'.repeat(100000);
      writeLocalStorage('long-value', longValue);
      expect(localStorage.getItem('long-value')).toBe(longValue);
    });

    it('should handle JSON stringified objects', () => {
      const data = { name: 'test', value: 123, active: true };
      const jsonString = JSON.stringify(data);
      writeLocalStorage('json-data', jsonString);
      expect(localStorage.getItem('json-data')).toBe(jsonString);
      expect(JSON.parse(localStorage.getItem('json-data')!)).toEqual(data);
    });

    it('should handle JSON stringified arrays', () => {
      const arrayData = [1, 2, 3, { nested: true }];
      const jsonString = JSON.stringify(arrayData);
      writeLocalStorage('array-data', jsonString);
      expect(localStorage.getItem('array-data')).toBe(jsonString);
      expect(JSON.parse(localStorage.getItem('array-data')!)).toEqual(arrayData);
    });

    it('should handle newlines and special whitespace', () => {
      const multilineValue = 'line1\nline2\r\nline3\ttab';
      writeLocalStorage('multiline', multilineValue);
      expect(localStorage.getItem('multiline')).toBe(multilineValue);
    });

    it('should silently handle quota exceeded error', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          throw new DOMException('QuotaExceededError', 'QuotaExceededError');
        });

      // Should not throw
      expect(() => writeLocalStorage('quota-key', 'value')).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should silently handle generic storage errors', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          throw new Error('Generic storage error');
        });

      // Should not throw
      expect(() => writeLocalStorage('error-key', 'error-value')).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should handle SecurityError (incognito mode)', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          throw new DOMException('SecurityError', 'SecurityError');
        });

      // Should not throw - graceful fallback
      expect(() => writeLocalStorage('security-key', 'value')).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should handle access denied errors gracefully', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          throw new Error('Access denied');
        });

      // Should not throw
      expect(() => writeLocalStorage('denied-key', 'value')).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should handle HTML/XML content', () => {
      const htmlContent = '<div class="test"><p>Content</p></div>';
      writeLocalStorage('html-key', htmlContent);
      expect(localStorage.getItem('html-key')).toBe(htmlContent);
    });

    it('should handle base64 encoded data', () => {
      const base64Data = btoa('test data for encoding');
      writeLocalStorage('base64-key', base64Data);
      expect(localStorage.getItem('base64-key')).toBe(base64Data);
      expect(atob(localStorage.getItem('base64-key')!)).toBe(
        'test data for encoding'
      );
    });

    it('should handle multiple rapid writes to same key', () => {
      writeLocalStorage('rapid-key', 'value1');
      writeLocalStorage('rapid-key', 'value2');
      writeLocalStorage('rapid-key', 'value3');
      expect(localStorage.getItem('rapid-key')).toBe('value3');
    });

    it('should handle null string (not null value)', () => {
      writeLocalStorage('null-string', 'null');
      expect(localStorage.getItem('null-string')).toBe('null');
    });

    it('should handle undefined string', () => {
      writeLocalStorage('undefined-string', 'undefined');
      expect(localStorage.getItem('undefined-string')).toBe('undefined');
    });

    it('should handle complex nested JSON structures', () => {
      const complexData = JSON.stringify({
        level1: {
          level2: {
            level3: {
              array: [1, 2, { deep: true }],
              value: 'nested',
            },
          },
        },
      });
      writeLocalStorage('complex-nested', complexData);
      expect(localStorage.getItem('complex-nested')).toBe(complexData);
    });

    it('should handle emojis and special unicode', () => {
      const emojiValue = '🎉🚀💻🌟⚡️🔥';
      writeLocalStorage('emoji-key', emojiValue);
      expect(localStorage.getItem('emoji-key')).toBe(emojiValue);
    });

    it('should handle zero-width characters', () => {
      const zeroWidthValue = 'text\u200Bwith\u200Bzero\u200Bwidth';
      writeLocalStorage('zero-width', zeroWidthValue);
      expect(localStorage.getItem('zero-width')).toBe(zeroWidthValue);
    });

    it('should handle URL encoded strings', () => {
      const urlEncoded = encodeURIComponent('key=value&other=data');
      writeLocalStorage('url-encoded', urlEncoded);
      expect(localStorage.getItem('url-encoded')).toBe(urlEncoded);
    });
  });

  describe('SSR scenarios (window undefined)', () => {
    let windowSpy: any;

    beforeEach(() => {
      // Mock SSR environment where window is undefined
      windowSpy = vi.spyOn(global, 'window', 'get');
    });

    afterEach(() => {
      windowSpy.mockRestore();
    });

    it('should return null when reading in SSR environment', () => {
      windowSpy.mockReturnValue(undefined);
      const result = readLocalStorage('test-key');
      expect(result).toBeNull();
    });

    it('should no-op when writing in SSR environment', () => {
      windowSpy.mockReturnValue(undefined);
      // Should not throw
      expect(() => writeLocalStorage('test-key', 'test-value')).not.toThrow();
    });

    it('should handle multiple reads in SSR environment', () => {
      windowSpy.mockReturnValue(undefined);
      expect(readLocalStorage('key1')).toBeNull();
      expect(readLocalStorage('key2')).toBeNull();
      expect(readLocalStorage('key3')).toBeNull();
    });

    it('should handle multiple writes in SSR environment', () => {
      windowSpy.mockReturnValue(undefined);
      expect(() => {
        writeLocalStorage('key1', 'value1');
        writeLocalStorage('key2', 'value2');
        writeLocalStorage('key3', 'value3');
      }).not.toThrow();
    });

    it('should handle read after write in SSR environment', () => {
      windowSpy.mockReturnValue(undefined);
      writeLocalStorage('ssr-key', 'ssr-value');
      const result = readLocalStorage('ssr-key');
      expect(result).toBeNull();
    });
  });

  describe('graceful fallback for restricted access', () => {
    it('should handle localStorage disabled in browser settings', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new DOMException(
            'Failed to read the localStorage property',
            'SecurityError'
          );
        });

      const result = readLocalStorage('restricted-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('should handle localStorage disabled on write', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          throw new DOMException(
            'Failed to write to localStorage',
            'SecurityError'
          );
        });

      expect(() =>
        writeLocalStorage('restricted-key', 'restricted-value')
      ).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should handle private browsing mode restrictions', () => {
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation(() => {
          // Safari private browsing throws QuotaExceededError even for small data
          throw new DOMException('QuotaExceededError', 'QuotaExceededError');
        });

      expect(() =>
        writeLocalStorage('private-key', 'small-value')
      ).not.toThrow();

      setItemSpy.mockRestore();
    });

    it('should handle cross-origin iframe restrictions', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new DOMException(
            'Access is denied for this document',
            'SecurityError'
          );
        });

      const result = readLocalStorage('iframe-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });

    it('should handle storage corruption errors', () => {
      const getItemSpy = vi
        .spyOn(Storage.prototype, 'getItem')
        .mockImplementation(() => {
          throw new Error('Storage corrupted');
        });

      const result = readLocalStorage('corrupted-key');
      expect(result).toBeNull();

      getItemSpy.mockRestore();
    });
  });

  describe('edge cases and type safety', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('should handle type-safe string values', () => {
      const stringValue: string = 'typed-string';
      writeLocalStorage('typed-key', stringValue);
      const result: string | null = readLocalStorage('typed-key');
      expect(result).toBe(stringValue);
    });

    it('should handle JSON serialization for objects', () => {
      interface User {
        id: number;
        name: string;
        active: boolean;
      }

      const user: User = { id: 1, name: 'Alice', active: true };
      const serialized = JSON.stringify(user);

      writeLocalStorage('user-key', serialized);
      const retrieved = readLocalStorage('user-key');

      expect(retrieved).not.toBeNull();
      const parsed: User = JSON.parse(retrieved!);
      expect(parsed).toEqual(user);
      expect(parsed.id).toBe(1);
      expect(parsed.name).toBe('Alice');
      expect(parsed.active).toBe(true);
    });

    it('should handle type-safe array serialization', () => {
      const numbers: number[] = [1, 2, 3, 4, 5];
      const serialized = JSON.stringify(numbers);

      writeLocalStorage('numbers-key', serialized);
      const retrieved = readLocalStorage('numbers-key');

      expect(retrieved).not.toBeNull();
      const parsed: number[] = JSON.parse(retrieved!);
      expect(parsed).toEqual(numbers);
    });

    it('should handle generic type patterns', () => {
      // Helper to demonstrate type-safe usage pattern
      function saveData<T>(key: string, data: T): void {
        writeLocalStorage(key, JSON.stringify(data));
      }

      function loadData<T>(key: string): T | null {
        const value = readLocalStorage(key);
        return value ? JSON.parse(value) : null;
      }

      interface Config {
        theme: 'light' | 'dark';
        fontSize: number;
      }

      const config: Config = { theme: 'dark', fontSize: 14 };
      saveData('config', config);

      const loaded = loadData<Config>('config');
      expect(loaded).toEqual(config);
      expect(loaded?.theme).toBe('dark');
    });

    it('should handle Date serialization edge case', () => {
      const now = new Date();
      const dateString = JSON.stringify(now);

      writeLocalStorage('date-key', dateString);
      const retrieved = readLocalStorage('date-key');

      expect(retrieved).not.toBeNull();
      const parsed = new Date(JSON.parse(retrieved!));
      expect(parsed.getTime()).toBe(now.getTime());
    });

    it('should handle BigInt serialization limitation', () => {
      // BigInt cannot be serialized with JSON.stringify
      // This test documents the limitation
      const bigIntValue = BigInt(9007199254740991);

      // Custom serialization needed for BigInt
      const serialized = bigIntValue.toString();
      writeLocalStorage('bigint-key', serialized);

      const retrieved = readLocalStorage('bigint-key');
      expect(retrieved).toBe(serialized);

      // Deserialization
      const deserialized = BigInt(retrieved!);
      expect(deserialized).toBe(bigIntValue);
    });

    it('should handle Map serialization pattern', () => {
      const map = new Map([
        ['key1', 'value1'],
        ['key2', 'value2'],
      ]);

      // Maps need custom serialization
      const serialized = JSON.stringify(Array.from(map.entries()));
      writeLocalStorage('map-key', serialized);

      const retrieved = readLocalStorage('map-key');
      expect(retrieved).not.toBeNull();

      const deserialized = new Map(JSON.parse(retrieved!));
      expect(deserialized.get('key1')).toBe('value1');
      expect(deserialized.get('key2')).toBe('value2');
    });

    it('should handle Set serialization pattern', () => {
      const set = new Set([1, 2, 3, 4, 5]);

      // Sets need custom serialization
      const serialized = JSON.stringify(Array.from(set));
      writeLocalStorage('set-key', serialized);

      const retrieved = readLocalStorage('set-key');
      expect(retrieved).not.toBeNull();

      const deserialized = new Set(JSON.parse(retrieved!));
      expect(deserialized.has(3)).toBe(true);
      expect(deserialized.size).toBe(5);
    });

    it('should handle circular reference gracefully', () => {
      // Circular references cause JSON.stringify to fail
      // This test documents that the caller must handle this
      interface Node {
        value: string;
        next?: Node;
      }

      const node1: Node = { value: 'first' };
      const node2: Node = { value: 'second' };
      node1.next = node2;
      node2.next = node1; // Creates cycle

      // JSON.stringify will throw
      expect(() => JSON.stringify(node1)).toThrow();

      // Caller must handle circular references before storage
      // For example, using a custom replacer or breaking the cycle
    });

    it('should handle function serialization limitation', () => {
      // Functions are not serializable with JSON
      const obj = {
        name: 'test',
        method: () => 'hello',
      };

      const serialized = JSON.stringify(obj);
      // Functions are omitted in JSON serialization
      expect(serialized).not.toContain('method');

      writeLocalStorage('func-key', serialized);
      const retrieved = readLocalStorage('func-key');
      const parsed = JSON.parse(retrieved!);

      expect(parsed.name).toBe('test');
      expect(parsed.method).toBeUndefined();
    });

    it('should handle Symbol serialization limitation', () => {
      // Symbols are not serializable with JSON
      const sym = Symbol('test');
      const obj = {
        name: 'test',
        [sym]: 'symbol-value',
      };

      const serialized = JSON.stringify(obj);
      // Symbols are omitted in JSON serialization
      expect(serialized).not.toContain('symbol-value');
    });

    it('should handle undefined values in objects', () => {
      const obj = {
        defined: 'value',
        undefined: undefined,
      };

      const serialized = JSON.stringify(obj);
      // undefined values are omitted in JSON serialization
      expect(serialized).not.toContain('undefined');

      writeLocalStorage('undefined-in-obj', serialized);
      const retrieved = readLocalStorage('undefined-in-obj');
      const parsed = JSON.parse(retrieved!);

      expect(parsed.defined).toBe('value');
      expect(parsed.undefined).toBeUndefined();
    });

    it('should handle NaN and Infinity edge cases', () => {
      const data = {
        nan: NaN,
        infinity: Infinity,
        negInfinity: -Infinity,
      };

      const serialized = JSON.stringify(data);
      writeLocalStorage('special-numbers', serialized);

      const retrieved = readLocalStorage('special-numbers');
      const parsed = JSON.parse(retrieved!);

      // JSON serializes these as null
      expect(parsed.nan).toBeNull();
      expect(parsed.infinity).toBeNull();
      expect(parsed.negInfinity).toBeNull();
    });
  });

  describe('integration scenarios', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('should handle read-write-read cycle', () => {
      const testValue = 'cycle-test';
      writeLocalStorage('cycle-key', testValue);
      const result = readLocalStorage('cycle-key');
      expect(result).toBe(testValue);
    });

    it('should handle multiple keys independently', () => {
      writeLocalStorage('key1', 'value1');
      writeLocalStorage('key2', 'value2');
      writeLocalStorage('key3', 'value3');

      expect(readLocalStorage('key1')).toBe('value1');
      expect(readLocalStorage('key2')).toBe('value2');
      expect(readLocalStorage('key3')).toBe('value3');
    });

    it('should handle mixed operations', () => {
      writeLocalStorage('mixed1', 'value1');
      expect(readLocalStorage('mixed1')).toBe('value1');

      writeLocalStorage('mixed2', 'value2');
      expect(readLocalStorage('mixed2')).toBe('value2');

      writeLocalStorage('mixed1', 'updated-value1');
      expect(readLocalStorage('mixed1')).toBe('updated-value1');
    });

    it('should handle concurrent error and success operations', () => {
      // Normal write
      writeLocalStorage('normal-key', 'normal-value');
      expect(readLocalStorage('normal-key')).toBe('normal-value');

      // Simulate error on specific key
      const setItemSpy = vi
        .spyOn(Storage.prototype, 'setItem')
        .mockImplementation((key) => {
          if (key === 'error-key') {
            throw new Error('Specific error');
          }
          // Fall through to actual implementation for other keys
          setItemSpy.mockRestore();
          localStorage.setItem(key, 'fallback-value');
          setItemSpy = vi
            .spyOn(Storage.prototype, 'setItem')
            .mockImplementation((k) => {
              if (k === 'error-key') throw new Error('Specific error');
            });
        });

      // Error case should not throw
      expect(() => writeLocalStorage('error-key', 'error-value')).not.toThrow();

      setItemSpy.mockRestore();

      // Normal operations should still work
      writeLocalStorage('another-key', 'another-value');
      expect(readLocalStorage('another-key')).toBe('another-value');
    });
  });
});
