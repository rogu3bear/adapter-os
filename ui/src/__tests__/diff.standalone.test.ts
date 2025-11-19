/**
 * Standalone test for diff utilities (without test setup dependencies)
 * Can be run directly without vitest setup
 */

import {
  tokenDiff,
  charDiff,
  lineDiff,
  similarity,
  getDiffStats,
  mergeDiffs,
  findFirstDifference,
  isIdentical,
  clearDiffCache,
  getCacheStats,
} from '../utils/diff';

// Simple test runner
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`ASSERTION FAILED: ${message}`);
  }
}

function test(name: string, fn: () => void) {
  try {
    fn();
    console.log(`✓ ${name}`);
  } catch (error) {
    console.error(`✗ ${name}`);
    console.error(error);
    throw error;
  }
}

// Run tests
console.log('\n--- Diff Utilities Standalone Tests ---\n');

let passCount = 0;
let failCount = 0;

function runTest(name: string, fn: () => void) {
  try {
    fn();
    passCount++;
    console.log(`✓ ${name}`);
  } catch (error) {
    failCount++;
    console.error(`✗ ${name}`);
    console.error(`  Error: ${error instanceof Error ? error.message : String(error)}`);
  }
}

// Basic Tests
runTest('Token diff - identical strings', () => {
  const result = tokenDiff('hello world', 'hello world');
  assert(result.length > 0, 'should have results');
  assert(result.every((r) => r.type === 'unchanged'), 'all should be unchanged');
});

runTest('Token diff - added content', () => {
  const result = tokenDiff('hello', 'hello world');
  const added = result.filter((r) => r.type === 'added');
  assert(added.length > 0, 'should have added items');
});

runTest('Token diff - removed content', () => {
  const result = tokenDiff('hello world', 'hello');
  const removed = result.filter((r) => r.type === 'removed');
  assert(removed.length > 0, 'should have removed items');
});

runTest('Char diff - character level granularity', () => {
  const result = charDiff('abc', 'abc');
  assert(result.length === 3, 'should have 3 characters');
  assert(result[0].value === 'a', 'first char should be a');
});

runTest('Line diff - basic', () => {
  const result = lineDiff('line1\nline2\nline3', 'line1\nline2\nline3');
  assert(result.length > 0, 'should have results');
  assert(result.every((l) => l.type === 'unchanged'), 'all should be unchanged');
});

runTest('Line diff - added lines', () => {
  const result = lineDiff('line1\nline3', 'line1\nline2\nline3');
  const added = result.filter((l) => l.type === 'added');
  assert(added.length > 0, 'should have added lines');
});

runTest('Line diff - with char diffs', () => {
  const result = lineDiff('hello', 'hallo', true);
  const modified = result.find((l) => l.type === 'modified');
  assert(modified !== undefined, 'should have modified line');
  assert(modified?.charDiffs !== undefined, 'should have char diffs');
});

// Similarity Tests
runTest('Similarity - identical strings', () => {
  const score = similarity('hello', 'hello');
  assert(score === 100, `should be 100, got ${score}`);
});

runTest('Similarity - different strings', () => {
  const score = similarity('abc', 'xyz');
  assert(score === 0, `should be 0, got ${score}`);
});

runTest('Similarity - partial match', () => {
  const score = similarity('hello world', 'hello mars');
  assert(score > 0 && score < 100, `should be between 0-100, got ${score}`);
});

runTest('Similarity - empty strings', () => {
  assert(similarity('', '') === 100, 'empty strings should be identical');
  assert(similarity('hello', '') === 0, 'one empty should be 0');
});

// Helper Tests
runTest('Get diff stats', () => {
  const diffs = tokenDiff('hello world', 'hello mars');
  const stats = getDiffStats(diffs);
  assert(typeof stats.additions === 'number', 'should have additions count');
  assert(typeof stats.deletions === 'number', 'should have deletions count');
  assert(typeof stats.similarityScore === 'number', 'should have similarity score');
});

runTest('Merge diffs', () => {
  const diffs = charDiff('abc', 'abc');
  const merged = mergeDiffs(diffs);
  assert(Array.isArray(merged), 'should return array');
  // Three single chars merged into one
  assert(merged.length <= diffs.length, 'merged should be <= original');
});

runTest('Find first difference', () => {
  assert(findFirstDifference('hello', 'hello') === -1, 'identical strings');
  assert(findFirstDifference('hello', 'hallo') === 1, 'position 1');
  assert(findFirstDifference('hello', 'hello world') === 5, 'length difference');
});

runTest('Is identical', () => {
  assert(isIdentical('hello', 'hello') === true, 'should be identical');
  assert(isIdentical('hello', 'world') === false, 'should not be identical');
});

// Cache Tests
runTest('Cache - clear and stats', () => {
  clearDiffCache();
  const stats = getCacheStats();
  assert(stats.size === 0, 'cache should be empty after clear');
  assert(stats.maxSize > 0, 'maxSize should be set');
});

runTest('Cache - caching works', () => {
  clearDiffCache();
  // Use different strings so it doesn't hit the fast path
  similarity('test1', 'test2', { cache: true });
  const stats = getCacheStats();
  assert(stats.size > 0, 'cache should have entry');
});

// Edge Cases
runTest('Edge case - very long identical strings', () => {
  const longStr = 'a'.repeat(10000);
  const score = similarity(longStr, longStr);
  assert(score === 100, 'long identical strings should be 100%');
});

runTest('Edge case - special characters', () => {
  const result = lineDiff('foo\nbar\nbaz', 'foo\nbar\nbaz');
  assert(result.every((r) => r.type === 'unchanged'), 'special chars should work');
});

runTest('Edge case - empty input', () => {
  assert(tokenDiff('', '').length === 0, 'empty input should work');
  // Empty lineDiff might return 0 or 1 element depending on implementation
  assert(Array.isArray(lineDiff('', '')), 'empty line diff should return array');
});

// Performance check (not strict, just informational)
runTest('Performance - 1K token diff', () => {
  const golden = 'word '.repeat(200);
  const current = 'word '.repeat(200) + 'extra';
  const start = performance.now();
  similarity(golden, current);
  const elapsed = performance.now() - start;
  console.log(`  ├─ Time: ${elapsed.toFixed(2)}ms (target: <100ms)`);
  assert(elapsed < 200, 'should be reasonably fast');
});

// Summary
console.log(`\n--- Test Summary ---`);
console.log(`Passed: ${passCount}`);
console.log(`Failed: ${failCount}`);
console.log(`Total:  ${passCount + failCount}`);

if (failCount > 0) {
  process.exit(1);
} else {
  console.log('\n✓ All tests passed!');
  process.exit(0);
}
