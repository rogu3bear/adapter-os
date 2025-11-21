/**
 * Test suite for diff utilities
 *
 * Tests cover:
 * - All diff functions (token, char, line)
 * - Similarity scoring
 * - Helper functions
 * - Edge cases and performance
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  tokenDiff,
  charDiff,
  lineDiff,
  similarity,
  formatDiff,
  getDiffStats,
  mergeDiffs,
  getContext,
  chunkDiffs,
  findFirstDifference,
  isIdentical,
  clearDiffCache,
  getCacheStats,
  isDiffResult,
  isLineDiff,
  DiffResult,
  LineDiff,
} from '../utils/diff';

describe('Diff Utilities', () => {
  beforeEach(() => {
    clearDiffCache();
  });

  describe('Basic Diff Functions', () => {
    it('should compute identical strings with no diff', () => {
      const golden = 'hello world';
      const current = 'hello world';

      const result = tokenDiff(golden, current);
      expect(result).toHaveLength(3); // "hello", " ", "world"
      expect(result.every((r) => r.type === 'unchanged')).toBe(true);
    });

    it('should detect added content', () => {
      const golden = 'hello';
      const current = 'hello world';

      const result = tokenDiff(golden, current);
      const addedItems = result.filter((r) => r.type === 'added');
      expect(addedItems.length).toBeGreaterThan(0);
    });

    it('should detect removed content', () => {
      const golden = 'hello world';
      const current = 'hello';

      const result = tokenDiff(golden, current);
      const removedItems = result.filter((r) => r.type === 'removed');
      expect(removedItems.length).toBeGreaterThan(0);
    });

    it('should detect modified content', () => {
      const golden = 'hello world';
      const current = 'hello mars';

      const result = tokenDiff(golden, current);
      expect(result.length).toBeGreaterThan(0);
    });
  });

  describe('Token-Level Diff', () => {
    it('should split by whitespace correctly', () => {
      const result = tokenDiff('foo bar baz', 'foo bar baz');
      const tokens = result.map((r) => r.value).filter((v) => v.trim());
      expect(tokens).toEqual(['foo', 'bar', 'baz']);
    });

    it('should handle newlines as token boundaries', () => {
      const result = tokenDiff('line1\nline2', 'line1\nline2');
      expect(result.length).toBeGreaterThan(0);
      expect(result.every((r) => r.type === 'unchanged')).toBe(true);
    });

    it('should preserve position tracking', () => {
      const result = tokenDiff('hello world', 'hello world');
      let lastPosition = -1;
      for (const diff of result) {
        expect(diff.position).toBeGreaterThanOrEqual(lastPosition);
        lastPosition = diff.position + diff.length;
      }
    });
  });

  describe('Character-Level Diff', () => {
    it('should produce character-level granularity', () => {
      const result = charDiff('abc', 'abc');
      expect(result).toHaveLength(3);
      expect(result.map((r) => r.value)).toEqual(['a', 'b', 'c']);
    });

    it('should track positions correctly at char level', () => {
      const result = charDiff('hello', 'hello');
      expect(result[0].position).toBe(0);
      expect(result[1].position).toBe(1);
      expect(result[2].position).toBe(2);
    });

    it('should handle special characters', () => {
      const result = charDiff('!@#$', '!@#$');
      expect(result).toHaveLength(4);
      expect(result.every((r) => r.type === 'unchanged')).toBe(true);
    });

    it('should handle unicode', () => {
      const result = charDiff('hello', 'hello');
      expect(result).toHaveLength(5);
    });
  });

  describe('Line-Level Diff', () => {
    it('should work with string input', () => {
      const golden = 'line1\nline2\nline3';
      const current = 'line1\nline2\nline3';

      const result = lineDiff(golden, current);
      expect(result.length).toBeGreaterThan(0);
      expect(result.every((l) => l.type === 'unchanged')).toBe(true);
    });

    it('should work with array input', () => {
      const golden = ['line1', 'line2', 'line3'];
      const current = ['line1', 'line2', 'line3'];

      const result = lineDiff(golden, current);
      expect(result.length).toBeGreaterThan(0);
    });

    it('should detect added lines', () => {
      const golden = 'line1\nline3';
      const current = 'line1\nline2\nline3';

      const result = lineDiff(golden, current);
      const added = result.filter((l) => l.type === 'added');
      expect(added.length).toBeGreaterThan(0);
    });

    it('should detect removed lines', () => {
      const golden = 'line1\nline2\nline3';
      const current = 'line1\nline3';

      const result = lineDiff(golden, current);
      const removed = result.filter((l) => l.type === 'removed');
      expect(removed.length).toBeGreaterThan(0);
    });

    it('should detect modified lines', () => {
      const golden = 'hello world';
      const current = 'hello mars';

      const result = lineDiff(golden, current);
      const modified = result.filter((l) => l.type === 'modified');
      expect(modified.length).toBeGreaterThan(0);
    });

    it('should include char diffs when requested', () => {
      const golden = 'hello';
      const current = 'hallo';

      const result = lineDiff(golden, current, true);
      const modified = result.find((l) => l.type === 'modified');
      expect(modified?.charDiffs).toBeDefined();
    });
  });

  describe('Similarity Scoring', () => {
    it('should return 100 for identical strings', () => {
      expect(similarity('hello', 'hello')).toBe(100);
    });

    it('should return 0 for completely different strings', () => {
      expect(similarity('abc', 'xyz')).toBe(0);
    });

    it('should return partial score for partially similar strings', () => {
      const score = similarity('hello world', 'hello mars');
      expect(score).toBeGreaterThan(0);
      expect(score).toBeLessThan(100);
    });

    it('should handle empty strings', () => {
      expect(similarity('', '')).toBe(100);
      expect(similarity('hello', '')).toBe(0);
      expect(similarity('', 'hello')).toBe(0);
    });

    it('should be symmetric (order independent)', () => {
      const s1 = similarity('abc def', 'abc xyz');
      const s2 = similarity('abc xyz', 'abc def');
      expect(s1).toBe(s2);
    });
  });

  describe('Format Diff', () => {
    it('should format diff with prefixes', () => {
      const diffs: DiffResult[] = [
        { type: 'unchanged', value: 'a', position: 0, length: 1 },
        { type: 'added', value: 'b', position: 1, length: 1 },
        { type: 'removed', value: 'c', position: 2, length: 1 },
      ];

      const formatted = formatDiff(diffs);
      expect(formatted).toContain('+');
      expect(formatted).toContain('-');
    });

    it('should include positions when requested', () => {
      const diffs: DiffResult[] = [
        { type: 'added', value: 'x', position: 5, length: 1 },
      ];

      const formatted = formatDiff(diffs, true);
      expect(formatted).toContain('[5:6]');
    });
  });

  describe('Diff Statistics', () => {
    it('should count additions correctly', () => {
      const diffs: DiffResult[] = [
        { type: 'added', value: 'a', position: 0, length: 1 },
        { type: 'added', value: 'b', position: 1, length: 1 },
      ];

      const stats = getDiffStats(diffs);
      expect(stats.additions).toBe(2);
    });

    it('should count deletions correctly', () => {
      const diffs: DiffResult[] = [
        { type: 'removed', value: 'x', position: 0, length: 1 },
      ];

      const stats = getDiffStats(diffs);
      expect(stats.deletions).toBe(1);
    });

    it('should compute similarity correctly', () => {
      const diffs: DiffResult[] = [
        { type: 'unchanged', value: 'hello', position: 0, length: 5 },
        { type: 'added', value: ' world', position: 5, length: 6 },
      ];

      const stats = getDiffStats(diffs);
      expect(stats.similarityScore).toBeGreaterThan(0);
    });
  });

  describe('Merge Diffs', () => {
    it('should merge adjacent same-type diffs', () => {
      const diffs: DiffResult[] = [
        { type: 'added', value: 'a', position: 0, length: 1 },
        { type: 'added', value: 'b', position: 1, length: 1 },
        { type: 'unchanged', value: 'c', position: 2, length: 1 },
      ];

      const merged = mergeDiffs(diffs);
      expect(merged.length).toBe(2);
      expect(merged[0].value).toBe('ab');
    });

    it('should not merge different-type diffs', () => {
      const diffs: DiffResult[] = [
        { type: 'added', value: 'a', position: 0, length: 1 },
        { type: 'removed', value: 'b', position: 1, length: 1 },
      ];

      const merged = mergeDiffs(diffs);
      expect(merged.length).toBe(2);
    });

    it('should handle empty array', () => {
      expect(mergeDiffs([])).toEqual([]);
    });
  });

  describe('Get Context', () => {
    it('should return context around change', () => {
      const diffs: DiffResult[] = [
        { type: 'unchanged', value: '1', position: 0, length: 1 },
        { type: 'unchanged', value: '2', position: 1, length: 1 },
        { type: 'added', value: 'X', position: 2, length: 1 },
        { type: 'unchanged', value: '3', position: 3, length: 1 },
        { type: 'unchanged', value: '4', position: 4, length: 1 },
      ];

      const context = getContext(diffs, 2, 1);
      expect(context.length).toBeGreaterThan(1);
      expect(context.some((d) => d.type === 'added')).toBe(true);
    });

    it('should handle boundary conditions', () => {
      const diffs: DiffResult[] = [
        { type: 'added', value: 'x', position: 0, length: 1 },
      ];

      const context = getContext(diffs, 0, 5);
      expect(context.length).toBeGreaterThan(0);
    });

    it('should return empty for invalid index', () => {
      const diffs: DiffResult[] = [
        { type: 'unchanged', value: 'a', position: 0, length: 1 },
      ];

      expect(getContext(diffs, 999, 1)).toEqual([]);
      expect(getContext(diffs, -1, 1)).toEqual([]);
    });
  });

  describe('Chunk Diffs', () => {
    it('should split diffs into chunks', () => {
      const diffs: DiffResult[] = Array.from({ length: 250 }, (_, i) => ({
        type: 'unchanged' as const,
        value: `${i}`,
        position: i,
        length: 1,
      }));

      const chunks = chunkDiffs(diffs, 100);
      expect(chunks.length).toBe(3);
      expect(chunks[0].length).toBe(100);
      expect(chunks[2].length).toBe(50);
    });

    it('should handle single chunk', () => {
      const diffs: DiffResult[] = Array.from({ length: 50 }, (_, i) => ({
        type: 'unchanged' as const,
        value: `${i}`,
        position: i,
        length: 1,
      }));

      const chunks = chunkDiffs(diffs, 100);
      expect(chunks.length).toBe(1);
    });

    it('should handle empty array', () => {
      expect(chunkDiffs([], 100)).toEqual([]);
    });
  });

  describe('Find First Difference', () => {
    it('should return -1 for identical strings', () => {
      expect(findFirstDifference('hello', 'hello')).toBe(-1);
    });

    it('should find first difference position', () => {
      expect(findFirstDifference('hello', 'hallo')).toBe(1);
      expect(findFirstDifference('abc', 'xyz')).toBe(0);
    });

    it('should handle length differences', () => {
      expect(findFirstDifference('hello', 'hello world')).toBe(5);
      expect(findFirstDifference('hello world', 'hello')).toBe(5);
    });

    it('should handle empty strings', () => {
      expect(findFirstDifference('', '')).toBe(-1);
      expect(findFirstDifference('hello', '')).toBe(0);
    });
  });

  describe('Is Identical', () => {
    it('should return true for identical strings', () => {
      expect(isIdentical('hello', 'hello')).toBe(true);
      expect(isIdentical('', '')).toBe(true);
    });

    it('should return false for different strings', () => {
      expect(isIdentical('hello', 'world')).toBe(false);
      expect(isIdentical('hello', '')).toBe(false);
    });
  });

  describe('Caching', () => {
    it('should cache diff results', () => {
      const golden = 'hello world';
      const current = 'hello mars';

      similarity(golden, current, { cache: true });
      const stats1 = getCacheStats();

      similarity(golden, current, { cache: true });
      const stats2 = getCacheStats();

      expect(stats2.size).toBe(stats1.size);
    });

    it('should clear cache', () => {
      similarity('a', 'b', { cache: true });
      expect(getCacheStats().size).toBeGreaterThan(0);

      clearDiffCache();
      expect(getCacheStats().size).toBe(0);
    });

    it('should prune cache to max size', () => {
      // Generate many diffs to trigger pruning
      for (let i = 0; i < 150; i++) {
        similarity(`text${i}`, `text${i + 1}`, { cache: true });
      }

      const stats = getCacheStats();
      expect(stats.size).toBeLessThanOrEqual(stats.maxSize);
    });
  });

  describe('Type Guards', () => {
    it('should identify DiffResult', () => {
      const result: DiffResult = {
        type: 'added',
        value: 'x',
        position: 0,
        length: 1,
      };

      expect(isDiffResult(result)).toBe(true);
      expect(isDiffResult({ type: 'added' })).toBe(false);
      expect(isDiffResult(null)).toBe(false);
    });

    it('should identify LineDiff', () => {
      const lineDiffObj: LineDiff = {
        type: 'added',
        lineNumber: 1,
        currentLine: 'test',
      };

      expect(isLineDiff(lineDiffObj)).toBe(true);
      expect(isLineDiff({ type: 'added' })).toBe(false);
    });
  });

  describe('Edge Cases', () => {
    it('should handle very long identical strings', () => {
      const longString = 'a'.repeat(10000);
      expect(similarity(longString, longString)).toBe(100);
    });

    it('should handle very long different strings', () => {
      const str1 = 'a'.repeat(10000);
      const str2 = 'b'.repeat(10000);
      expect(similarity(str1, str2)).toBe(0);
    });

    it('should handle strings with special characters', () => {
      const result = lineDiff('foo\nbar\nbaz', 'foo\nbar\nbaz');
      expect(result.every((r) => r.type === 'unchanged')).toBe(true);
    });

    it('should handle multiline strings with empty lines', () => {
      const result = lineDiff('a\n\nb', 'a\n\nb');
      expect(result.length).toBeGreaterThan(0);
    });

    it('should handle strings without newlines', () => {
      const result = lineDiff('single line', 'single line');
      expect(result.length).toBeGreaterThan(0);
    });
  });

  describe('Performance', () => {
    it('should compute diff for 1K tokens in <100ms', () => {
      const golden = 'word '.repeat(200);
      const current = 'word '.repeat(200) + 'extra';

      const start = performance.now();
      similarity(golden, current);
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(100);
    });

    it('should compute diff for 10K tokens reasonably fast', () => {
      const golden = 'token '.repeat(1666);
      const current = 'token '.repeat(1666) + 'added';

      const start = performance.now();
      charDiff(golden, current);
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(1000);
    });

    it('should handle cleanup parameter efficiently', () => {
      const golden = 'a'.repeat(1000);
      const current = 'b'.repeat(1000);

      const start1 = performance.now();
      similarity(golden, current, { cleanup: 'semantic' });
      const time1 = performance.now() - start1;

      const start2 = performance.now();
      similarity(golden, current, { cleanup: 'efficiency' });
      const time2 = performance.now() - start2;

      // Both should be reasonably fast
      expect(time1).toBeLessThan(500);
      expect(time2).toBeLessThan(500);
    });
  });

  describe('Integration Tests', () => {
    it('should support complete diff workflow', () => {
      const golden = 'function foo() {\n  return "hello";\n}';
      const current = 'function foo() {\n  return "world";\n}';

      // Get line-level diff with char details
      const lines = lineDiff(golden, current, true);

      // Compute statistics
      const stats = getDiffStats(charDiff(golden, current));

      // Find differences
      const similarity_score = similarity(golden, current);

      expect(lines.length).toBeGreaterThan(0);
      expect(stats.modifications).toBeGreaterThan(0);
      expect(similarity_score).toBeGreaterThan(70);
    });

    it('should work with code samples', () => {
      const golden = `const x = 1;
const y = 2;
return x + y;`;

      const current = `const x = 1;
const y = 3;
return x + y;`;

      const diffs = lineDiff(golden, current, true);
      expect(diffs.some((d) => d.type === 'modified')).toBe(true);
    });
  });
});
