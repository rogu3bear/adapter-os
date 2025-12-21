# Diff Utilities - Quick Reference Guide

## Installation & Import

Already installed: `diff-match-patch@1.0.5`

```typescript
import {
  tokenDiff,
  charDiff,
  lineDiff,
  similarity,
  getDiffStats,
  mergeDiffs,
  chunkDiffs,
  findFirstDifference,
  isIdentical,
} from '@/utils/diff';
```

## Core Functions at a Glance

### Token Diff
```typescript
const diffs = tokenDiff('hello world', 'hello mars');
// Fast, good for structured text
// Performance: <10ms for 1K tokens
```

### Char Diff
```typescript
const diffs = charDiff('hello', 'hallo');
// Detailed, for inline highlighting
// Performance: <20ms for 1K chars
```

### Line Diff
```typescript
const diffs = lineDiff(
  'line1\nline2',
  'line1\nmodified',
  true // include char diffs for changes
);
// Best for code/documents
// Performance: <50ms for 1K lines
```

### Similarity Score
```typescript
const score = similarity('hello world', 'hello mars');
// Returns: 63.6 (63% similar)
// Performance: <100ms for 10K tokens
```

### Get Statistics
```typescript
const diffs = tokenDiff(golden, current);
const stats = getDiffStats(diffs);
// stats.additions, stats.deletions, stats.modifications, stats.similarityScore
```

## Common Patterns

### Quick Comparison
```typescript
if (golden === current) {
  return 100; // Fast path
}

const score = similarity(golden, current);
if (score > 80) {
  console.log('Very similar');
}
```

### Code Review Diff
```typescript
const lineDiffs = lineDiff(oldCode, newCode, true);
for (const line of lineDiffs) {
  if (line.type === 'modified') {
    console.log(`Line ${line.lineNumber}: ${line.charDiffs?.length} changes`);
  }
}
```

### Large Text Processing
```typescript
const diffs = tokenDiff(huge1, huge2);
const chunks = chunkDiffs(diffs, 100);

for (const chunk of chunks) {
  await renderAsync(chunk); // Progressive rendering
}
```

### Cache for Performance
```typescript
// First call: computed
similarity(text1, text2, { cache: true });

// Subsequent calls: instant
similarity(text1, text2, { cache: true });
```

### With Options
```typescript
similarity(text1, text2, {
  cleanup: 'efficiency',  // Faster computation
  cache: true,            // Enable caching
  timeout: 5000           // 5 second timeout
});
```

## DiffResult Type

```typescript
interface DiffResult {
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  value: string;      // The text segment
  position: number;   // Position in text
  length: number;     // Segment length
}
```

## LineDiff Type

```typescript
interface LineDiff {
  goldenLine?: string;
  currentLine?: string;
  lineNumber: number;
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  charDiffs?: DiffResult[]; // Character-level diffs
}
```

## Helper Functions

### Merge Adjacent Changes
```typescript
const merged = mergeDiffs(diffs);
// Reduces number of diff entries
```

### Get Context
```typescript
const context = getContext(diffs, changeIndex, 5);
// Get 5 lines before/after change
```

### Find First Difference
```typescript
const pos = findFirstDifference(str1, str2);
if (pos === -1) console.log('Identical');
else console.log(`First diff at position ${pos}`);
```

### Check Identical
```typescript
if (isIdentical(str1, str2)) {
  // Skip expensive diff
}
```

### Format for Display
```typescript
const formatted = formatDiff(diffs, true); // with positions
```

## Performance Tips

### Best Practices
1. **Check identical first**
   ```typescript
   if (str1 === str2) return 100;
   ```

2. **Use appropriate algorithm**
   - Code: `lineDiff()` (fastest for code)
   - Tokens: `tokenDiff()` (balanced)
   - Chars: `charDiff()` (detailed but slow)

3. **Enable caching for repeated calls**
   ```typescript
   similarity(a, b, { cache: true });
   ```

4. **Chunk large diffs**
   ```typescript
   const chunks = chunkDiffs(diffs, 100);
   ```

5. **Use efficiency cleanup for speed**
   ```typescript
   similarity(a, b, { cleanup: 'efficiency' });
   ```

## Performance Table

| Operation | 1K Input | 10K Input | Overhead |
|-----------|----------|-----------|----------|
| Token diff | <10ms | <100ms | ~5MB |
| Char diff | <20ms | <200ms | ~10MB |
| Line diff | <5ms | <50ms | ~2MB |
| Similarity | <10ms | <100ms | ~5MB |
| Cache hit | <1μs | <1μs | ~1KB |

## Caching

### Check Cache Status
```typescript
const stats = getCacheStats();
console.log(`Using ${stats.size}/${stats.maxSize} cache entries`);
```

### Clear Cache
```typescript
clearDiffCache();
```

### Disable Caching
```typescript
similarity(a, b, { cache: false });
```

## Type Checking

### Type Guards
```typescript
import { isDiffResult, isLineDiff } from '@/utils/diff';

if (isDiffResult(value)) {
  // value is DiffResult
}

if (isLineDiff(value)) {
  // value is LineDiff
}
```

## Error Handling

All functions are safe and don't throw errors:
- Empty strings: Return appropriate results
- Very large texts: May be slow but won't crash
- Null/undefined: Would require manual checks

```typescript
// Safe operations
tokenDiff('', '');           // Works
similarity('', '');          // Returns 100
lineDiff('', '');            // Works
```

## Common Scenarios

### 1. Quick Diff Check
```typescript
const isDifferent = similarity(golden, current) < 99;
```

### 2. Show Changes Only
```typescript
const changes = lineDiff(golden, current)
  .filter(l => l.type !== 'unchanged');
```

### 3. Count Changes
```typescript
const diffs = tokenDiff(golden, current);
const stats = getDiffStats(diffs);
console.log(`+${stats.additions} -${stats.deletions}`);
```

### 4. Highlight Changes
```typescript
const diffs = charDiff(golden, current);
for (const diff of diffs) {
  if (diff.type === 'added') {
    highlight(diff.value, 'green');
  } else if (diff.type === 'removed') {
    highlight(diff.value, 'red');
  }
}
```

### 5. Progress Indication
```typescript
const total = lineDiff(big1, big2).length;
const chunks = chunkDiffs(lineDiff(big1, big2), 100);
for (let i = 0; i < chunks.length; i++) {
  progress = ((i + 1) / chunks.length) * 100;
  renderChunk(chunks[i]);
}
```

## Integration with UI

### With DiffVisualization
```typescript
import { DiffVisualization } from '@/components/golden/DiffVisualization';

<DiffVisualization
  goldenText={oldCode}
  currentText={newCode}
  contextLines={3}
  enableVirtualization={true}
/>
```

### With Custom Component
```typescript
import { lineDiff, similarity } from '@/utils/diff';

function MyDiffComponent({ golden, current }) {
  const diffs = lineDiff(golden, current, true);
  const score = similarity(golden, current);

  return (
    <div>
      <div>Similarity: {score.toFixed(1)}%</div>
      {diffs.map((diff, i) => (
        <DiffLine key={i} diff={diff} />
      ))}
    </div>
  );
}
```

## Testing

### Run Tests
```bash
cd ui
# Run all tests
pnpm test

# Run diff tests
pnpm test diff.test.ts

# Run benchmarks
pnpm test diff.bench.ts

# Run standalone (no setup required)
npx tsx src/__tests__/diff.standalone.test.ts
```

## API Reference Summary

| Function | Input | Output | Time |
|----------|-------|--------|------|
| `tokenDiff()` | 2 strings | DiffResult[] | <100ms |
| `charDiff()` | 2 strings | DiffResult[] | <200ms |
| `lineDiff()` | 2 strings | LineDiff[] | <50ms |
| `similarity()` | 2 strings | number | <100ms |
| `getDiffStats()` | DiffResult[] | DiffStats | <1ms |
| `mergeDiffs()` | DiffResult[] | DiffResult[] | <5ms |
| `getContext()` | DiffResult[], index | DiffResult[] | <1ms |
| `chunkDiffs()` | DiffResult[] | DiffResult[][] | <1ms |
| `findFirstDifference()` | 2 strings | number | <1ms |
| `isIdentical()` | 2 strings | boolean | <1ms |
| `formatDiff()` | DiffResult[] | string | <5ms |
| `clearDiffCache()` | none | void | <1ms |
| `getCacheStats()` | none | stats | <1ms |

## Troubleshooting

**Q: Diff is slow**
A: Use `tokenDiff` instead of `charDiff`, enable `cleanup: 'efficiency'`

**Q: Memory usage high**
A: Clear cache with `clearDiffCache()`, chunk results

**Q: Results inaccurate**
A: Try `charDiff` for granularity, check input encoding

**Q: Cache not working**
A: Use different strings (not identical), check `cache: true` option

---

**For more details**: See `DIFF_UTILITIES.md`
