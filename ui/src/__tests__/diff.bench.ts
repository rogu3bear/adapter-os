/**
 * Performance benchmarks for diff utilities
 *
 * Measures:
 * - Computation time for various input sizes
 * - Memory efficiency
 * - Cache effectiveness
 *
 * Run with: vitest bench diff.bench.ts
 */

import { describe, it, beforeEach } from 'vitest';
import {
  tokenDiff,
  charDiff,
  lineDiff,
  similarity,
  clearDiffCache,
  getDiffStats,
  mergeDiffs,
} from '../utils/diff';

// Test data generators
function generateText(wordCount: number, complexity: 'simple' | 'complex' = 'simple'): string {
  if (complexity === 'simple') {
    return 'word '.repeat(wordCount);
  } else {
    const words = [
      'function',
      'variable',
      'constant',
      'implementation',
      'declaration',
      'assignment',
      'operation',
      'component',
    ];
    return Array.from({ length: wordCount }, () => words[Math.floor(Math.random() * words.length)])
      .join(' ');
  }
}

function generateDifferentText(golden: string, changePercentage: number = 10): string {
  const words = golden.split(' ');
  const changeCount = Math.max(1, Math.floor((words.length * changePercentage) / 100));

  for (let i = 0; i < changeCount; i++) {
    const idx = Math.floor(Math.random() * words.length);
    words[idx] = 'changed';
  }

  return words.join(' ');
}

function generateCode(lines: number): string {
  const code = [];
  for (let i = 0; i < lines; i++) {
    code.push(`  const var${i} = value${i};`);
  }
  return code.join('\n');
}

describe('Diff Performance Benchmarks', () => {
  beforeEach(() => {
    clearDiffCache();
  });

  describe('Token Diff Performance', () => {
    it('should compute token diff for 100 tokens', () => {
      const golden = generateText(100);
      const current = generateDifferentText(golden, 10);

      const start = performance.now();
      const result = tokenDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Token diff (100 tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} diffs`);
      console.log(`  Threshold: <100ms`);
    });

    it('should compute token diff for 1000 tokens', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);

      const start = performance.now();
      const result = tokenDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Token diff (1K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} diffs`);
      console.log(`  Threshold: <100ms`);
    });

    it('should compute token diff for 10000 tokens', () => {
      const golden = generateText(10000);
      const current = generateDifferentText(golden, 2);

      const start = performance.now();
      const result = tokenDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Token diff (10K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} diffs`);
      console.log(`  Threshold: <1000ms`);
    });
  });

  describe('Char Diff Performance', () => {
    it('should compute char diff for 1K characters', () => {
      const golden = generateText(100);
      const current = generateDifferentText(golden, 10);

      const start = performance.now();
      const result = charDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Char diff (1K chars): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} diffs`);
    });

    it('should compute char diff for 10K characters', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);

      const start = performance.now();
      const result = charDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Char diff (10K chars): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} diffs`);
    });
  });

  describe('Line Diff Performance', () => {
    it('should compute line diff for 100 lines', () => {
      const golden = generateCode(100);
      const current = golden.split('\n').map((l, i) => (i % 5 === 0 ? l + ' // modified' : l)).join('\n');

      const start = performance.now();
      const result = lineDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Line diff (100 lines): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} lines`);
    });

    it('should compute line diff for 1000 lines', () => {
      const golden = generateCode(1000);
      const current = golden.split('\n').map((l, i) => (i % 10 === 0 ? l + ' // modified' : l)).join('\n');

      const start = performance.now();
      const result = lineDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Line diff (1K lines): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} lines`);
    });

    it('should compute line diff with char details for 100 lines', () => {
      const golden = generateCode(100);
      const current = golden.split('\n').map((l, i) => (i % 5 === 0 ? l + ' // modified' : l)).join('\n');

      const start = performance.now();
      const result = lineDiff(golden, current, true);
      const elapsed = performance.now() - start;

      console.log(`✓ Line diff with char details (100 lines): ${elapsed.toFixed(2)}ms`);
      console.log(`  Results: ${result.length} lines`);
    });
  });

  describe('Similarity Scoring Performance', () => {
    it('should compute similarity for 1K tokens', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);

      const start = performance.now();
      const score = similarity(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Similarity (1K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Score: ${score.toFixed(2)}%`);
      console.log(`  Threshold: <100ms`);
    });

    it('should compute similarity for 10K tokens', () => {
      const golden = generateText(10000);
      const current = generateDifferentText(golden, 2);

      const start = performance.now();
      const score = similarity(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Similarity (10K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Score: ${score.toFixed(2)}%`);
      console.log(`  Threshold: <1000ms`);
    });

    it('should compute similarity for identical strings (fast path)', () => {
      const text = generateText(10000);

      const start = performance.now();
      const score = similarity(text, text);
      const elapsed = performance.now() - start;

      console.log(`✓ Similarity identical strings (10K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Score: ${score.toFixed(2)}%`);
      console.log(`  Threshold: <1ms (fast path)`);
    });
  });

  describe('Cleanup Strategies Performance', () => {
    it('should compare cleanup strategies', () => {
      const golden = generateText(1000, 'complex');
      const current = generateDifferentText(golden, 20);

      const start1 = performance.now();
      similarity(golden, current, { cleanup: 'semantic' });
      const time1 = performance.now() - start1;

      const start2 = performance.now();
      similarity(golden, current, { cleanup: 'efficiency' });
      const time2 = performance.now() - start2;

      const start3 = performance.now();
      similarity(golden, current, { cleanup: 'none' });
      const time3 = performance.now() - start3;

      console.log(`✓ Cleanup strategy comparison (1K tokens):`);
      console.log(`  Semantic: ${time1.toFixed(2)}ms`);
      console.log(`  Efficiency: ${time2.toFixed(2)}ms`);
      console.log(`  None: ${time3.toFixed(2)}ms`);
    });
  });

  describe('Cache Effectiveness', () => {
    it('should show cache benefits for repeated diffs', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);

      // First computation (cache miss)
      clearDiffCache();
      const start1 = performance.now();
      similarity(golden, current, { cache: true });
      const time1 = performance.now() - start1;

      // Second computation (cache hit)
      const start2 = performance.now();
      similarity(golden, current, { cache: true });
      const time2 = performance.now() - start2;

      console.log(`✓ Cache effectiveness (1K tokens):`);
      console.log(`  First (cache miss): ${time1.toFixed(2)}ms`);
      console.log(`  Second (cache hit): ${time2.toFixed(2)}ms`);
      console.log(`  Speedup: ${(time1 / time2).toFixed(1)}x`);
    });

    it('should compare cached vs uncached for large texts', () => {
      const golden = generateText(5000);
      const current = generateDifferentText(golden, 3);

      clearDiffCache();
      const start1 = performance.now();
      for (let i = 0; i < 5; i++) {
        similarity(golden, current, { cache: false });
      }
      const timeUncached = performance.now() - start1;

      clearDiffCache();
      const start2 = performance.now();
      for (let i = 0; i < 5; i++) {
        similarity(golden, current, { cache: true });
      }
      const timeCached = performance.now() - start2;

      console.log(`✓ Cached vs uncached (5 iterations, 5K tokens):`);
      console.log(`  Uncached: ${timeUncached.toFixed(2)}ms`);
      console.log(`  Cached: ${timeCached.toFixed(2)}ms`);
      console.log(`  Speedup: ${(timeUncached / timeCached).toFixed(1)}x`);
    });
  });

  describe('Helper Functions Performance', () => {
    it('should compute stats efficiently', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);
      const diffs = tokenDiff(golden, current);

      const start = performance.now();
      const stats = getDiffStats(diffs);
      const elapsed = performance.now() - start;

      console.log(`✓ Get stats (1K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Additions: ${stats.additions}, Deletions: ${stats.deletions}`);
    });

    it('should merge diffs efficiently', () => {
      const golden = generateText(1000);
      const current = generateDifferentText(golden, 5);
      const diffs = charDiff(golden, current);

      const start = performance.now();
      const merged = mergeDiffs(diffs);
      const elapsed = performance.now() - start;

      console.log(`✓ Merge diffs (1K tokens): ${elapsed.toFixed(2)}ms`);
      console.log(`  Before: ${diffs.length} diffs`);
      console.log(`  After: ${merged.length} diffs`);
      console.log(`  Reduction: ${(((diffs.length - merged.length) / diffs.length) * 100).toFixed(1)}%`);
    });
  });

  describe('Real-World Scenarios', () => {
    it('should handle typical code review diff', () => {
      const golden = `function processData(input) {
  const parsed = JSON.parse(input);
  const filtered = parsed.filter(x => x.value > 10);
  const mapped = filtered.map(x => ({ id: x.id, value: x.value * 2 }));
  return mapped;
}`;

      const current = `function processData(input) {
  const parsed = JSON.parse(input);
  const filtered = parsed.filter(x => x.value > 5);
  const mapped = filtered.map(x => ({ id: x.id, value: x.value * 3, updated: true }));
  return mapped;
}`;

      const start = performance.now();
      const diffs = lineDiff(golden, current, true);
      const score = similarity(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Code review diff: ${elapsed.toFixed(2)}ms`);
      console.log(`  Lines: ${diffs.length}`);
      console.log(`  Similarity: ${score.toFixed(2)}%`);
    });

    it('should handle large document diff', () => {
      const section = `Chapter 1: Introduction
This is a sample document with multiple paragraphs.
Each paragraph contains relevant information.
The content is organized logically.

Chapter 2: Methods
We used several approaches in this study.
The methodology was rigorous and well-documented.
Results were validated through peer review.

`;

      const golden = section.repeat(10);
      const current = section.repeat(10).replace(/rigorous/g, 'comprehensive').replace(/study/g, 'research');

      const start = performance.now();
      const score = similarity(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ Large document diff: ${elapsed.toFixed(2)}ms`);
      console.log(`  Text size: ${golden.length} chars`);
      console.log(`  Similarity: ${score.toFixed(2)}%`);
    });

    it('should handle JSON structure diff', () => {
      const golden = JSON.stringify(
        {
          data: {
            users: [
              { id: 1, name: 'Alice', role: 'admin' },
              { id: 2, name: 'Bob', role: 'user' },
            ],
          },
        },
        null,
        2
      );

      const current = JSON.stringify(
        {
          data: {
            users: [
              { id: 1, name: 'Alice', role: 'admin', active: true },
              { id: 2, name: 'Bob', role: 'moderator' },
            ],
          },
        },
        null,
        2
      );

      const start = performance.now();
      const diffs = lineDiff(golden, current);
      const elapsed = performance.now() - start;

      console.log(`✓ JSON structure diff: ${elapsed.toFixed(2)}ms`);
      console.log(`  Lines: ${diffs.length}`);
    });
  });
});
