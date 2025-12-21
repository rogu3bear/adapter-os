# Diff Utilities Documentation

Efficient text and data comparison utilities for AdapterOS UI. Provides multiple diff algorithms optimized for different text sizes and use cases.

## Algorithm Choice: Myers' Algorithm

### Why Myers' Algorithm?

- **Fast**: O(ND) where N is text size and D is number of differences
- **Optimal**: Produces shortest edit script
- **Proven**: Industry standard (used by Git, diff, patch)
- **Well-tested**: Implemented via battle-tested `diff-match-patch` library

### Implementation Details

- Uses `DiffMatchPatch` library (v1.0.5)
- Supports configurable cleanup strategies:
  - `semantic`: Optimize for human readability (default)
  - `efficiency`: Optimize for computational speed
  - `none`: Raw diff without cleanup

## Core Functions

### Token-Level Diff

```typescript
function tokenDiff(golden: string, current: string, options?: DiffOptions): DiffResult[]
```

**Use When:** Comparing structured text where tokens have semantic meaning
- Code snippets (language tokens)
- Structured logs
- Command outputs

**Example:**
```typescript
const result = tokenDiff('hello world', 'hello mars');
// Returns: [{type: 'unchanged', value: 'hello'}, {type: 'unchanged', value: ' '}, ...]
```

**Performance:**
- 1K tokens: <10ms
- 10K tokens: <100ms
- 100K tokens: <1s

### Character-Level Diff

```typescript
function charDiff(golden: string, current: string, options?: DiffOptions): DiffResult[]
```

**Use When:** Fine-grained highlighting needed
- Inline diffs in text editors
- Character-by-character comparison
- Unicode text handling

**Example:**
```typescript
const result = charDiff('hello', 'hallo');
// Returns: [{type: 'unchanged', value: 'h'}, {type: 'removed', value: 'e'}, ...]
```

**Performance:** 5-10x slower than token diff for same input

### Line-Level Diff

```typescript
function lineDiff(
  golden: string | string[],
  current: string | string[],
  includeCharDiffs?: boolean,
  options?: DiffOptions
): LineDiff[]
```

**Use When:** Comparing code or documents line-by-line
- Code review diffs
- Document comparison
- Version control diffs

**Features:**
- Works with strings or string arrays
- Optional character-level diffs for modified lines
- Tracks line numbers

**Example:**
```typescript
const result = lineDiff(
  'line1\nline2\nline3',
  'line1\nmodified\nline3',
  true // include char diffs for changes
);
```

**Performance:**
- 100 lines: <5ms
- 1000 lines: <50ms
- 10000 lines: <500ms

### Similarity Scoring

```typescript
function similarity(golden: string, current: string, options?: DiffOptions): number
```

**Returns:** 0-100 percentage indicating text similarity

**Use Cases:**
- Quick before/after diff checks
- Similarity thresholds in validation
- Progress indication

**Example:**
```typescript
const score = similarity('hello world', 'hello mars');
// Returns: ~63.6 (64% similar)
```

**Performance:**
- Fast path (identical strings): <1ms
- Fast path (empty strings): <1ms
- General case: <100ms for 10K tokens

## Result Types

### DiffResult

```typescript
interface DiffResult {
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  value: string;           // The text segment
  position: number;        // Position in original text
  length: number;          // Length of segment
}
```

### LineDiff

```typescript
interface LineDiff {
  goldenLine?: string;
  currentLine?: string;
  lineNumber: number;
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  charDiffs?: DiffResult[]; // Optional character-level diffs
}
```

### DiffStats

```typescript
interface DiffStats {
  additions: number;
  deletions: number;
  modifications: number;
  totalLines: number;
  similarityScore: number;
  computeTime: number;
}
```

## Helper Functions

### Get Diff Statistics

```typescript
function getDiffStats(diffs: DiffResult[] | Diff[]): Omit<DiffStats, 'computeTime'>
```

Computes additions, deletions, modifications, and similarity score.

```typescript
const diffs = tokenDiff(golden, current);
const stats = getDiffStats(diffs);
console.log(`+${stats.additions} -${stats.deletions} ~${stats.modifications}`);
```

### Merge Adjacent Changes

```typescript
function mergeDiffs(diffs: DiffResult[]): DiffResult[]
```

Combines consecutive changes of the same type to reduce noise.

```typescript
const original = charDiff(golden, current);     // Many small diffs
const merged = mergeDiffs(original);             // Fewer, larger diffs
```

### Get Context Around Change

```typescript
function getContext(diffs: DiffResult[], changeIndex: number, contextLines: number = 3): DiffResult[]
```

Returns the change plus surrounding lines for context.

```typescript
const context = getContext(diffs, changeIndex, 5); // 5 lines before/after
```

### Chunk Diffs for Rendering

```typescript
function chunkDiffs(diffs: DiffResult[], chunkSize: number = 100): DiffResult[][]
```

Splits large diffs into manageable chunks for progressive rendering.

```typescript
const chunks = chunkDiffs(diffs, 100);
for (const chunk of chunks) {
  // Render progressively
}
```

### Find First Difference

```typescript
function findFirstDifference(golden: string, current: string): number
```

Fast operation to find position of first difference.

```typescript
const pos = findFirstDifference(golden, current);
if (pos === -1) {
  console.log('Strings are identical');
}
```

### Check Identical Strings

```typescript
function isIdentical(golden: string, current: string): boolean
```

Fast path check before expensive diff operations.

```typescript
if (isIdentical(golden, current)) {
  return; // Skip diff computation
}
```

## Caching

Diffs are automatically cached to improve performance for repeated comparisons.

```typescript
// First call: computed
similarity(text1, text2, { cache: true });

// Second call: cached (much faster)
similarity(text1, text2, { cache: true });
```

**Cache Management:**
- Automatic pruning when exceeding 100 entries
- LRU eviction policy
- Clearable with `clearDiffCache()`

```typescript
// Check cache stats
const stats = getCacheStats();
console.log(`Cache size: ${stats.size}/${stats.maxSize}`);

// Clear if needed
clearDiffCache();
```

## DiffOptions

```typescript
interface DiffOptions {
  cleanup?: 'semantic' | 'efficiency' | 'none';
  timeout?: number;        // Timeout in milliseconds
  cache?: boolean;         // Enable caching (default: true)
  useWorker?: boolean;     // Use Web Worker for large diffs (future)
}
```

### Cleanup Strategies

**Semantic (default):**
```typescript
similarity(golden, current, { cleanup: 'semantic' })
// Optimizes for human readability
// Best for code and documents
```

**Efficiency:**
```typescript
similarity(golden, current, { cleanup: 'efficiency' })
// Optimizes for computational speed
// Best for large automated comparisons
```

**None:**
```typescript
similarity(golden, current, { cleanup: 'none' })
// Raw diff without post-processing
// Use for research/debugging
```

## Performance Characteristics

### Time Complexity

| Function | Input Size | Typical Time | Threshold |
|----------|-----------|--------------|-----------|
| Token Diff | 1K tokens | <10ms | <100ms |
| Token Diff | 10K tokens | <100ms | <1s |
| Char Diff | 1K chars | <20ms | <200ms |
| Line Diff | 100 lines | <5ms | <50ms |
| Line Diff | 1K lines | <50ms | <500ms |
| Similarity | 10K tokens | <100ms | <1s |

### Memory Usage

- Small texts (<1K): Minimal, <1MB
- Medium texts (1K-100K): <10MB
- Large texts (>100K): Stream or chunk

**Cache Memory:** ~100KB for 100 cached diffs

## Integration Examples

### With DiffVisualization Component

```typescript
import { lineDiff, getDiffStats, similarity } from '@/utils/diff';

// In component
const diffs = lineDiff(goldenText, currentText, true);
const stats = getDiffStats(diffs);
const score = similarity(goldenText, currentText);

// Pass to DiffVisualization or EpsilonHeatmap
```

### In Custom Comparison Logic

```typescript
import { tokenDiff, mergeDiffs, chunkDiffs } from '@/utils/diff';

// Get token-level diff
const diffs = tokenDiff(golden, current);

// Merge for cleaner output
const merged = mergeDiffs(diffs);

// Chunk for rendering
const chunks = chunkDiffs(merged, 50);

// Process each chunk
for (const chunk of chunks) {
  renderDiffChunk(chunk);
}
```

### For Search/Filter Enhancement

```typescript
import { similarity } from '@/utils/diff';

// Find similar texts
const candidates = texts.filter(
  text => similarity(searchTerm, text) > 70 // >70% similar
);
```

## Type Guards

```typescript
import { isDiffResult, isLineDiff } from '@/utils/diff';

if (isDiffResult(value)) {
  // Type narrowing for DiffResult
}

if (isLineDiff(value)) {
  // Type narrowing for LineDiff
}
```

## Best Practices

### 1. Use Appropriate Algorithm

```typescript
// For code: line diff (coarse) + char diffs (fine)
lineDiff(golden, current, true);

// For structured text: token diff
tokenDiff(golden, current);

// For inline editing: char diff
charDiff(golden, current);
```

### 2. Check Identical Before Computing

```typescript
if (golden === current) {
  return { similarity: 100, changes: 0 };
}

// Only compute if different
const diffs = lineDiff(golden, current);
```

### 3. Use Caching for Repeated Comparisons

```typescript
// Automatically cached
similarity(text1, text2, { cache: true });
similarity(text1, text2, { cache: true }); // Instant
```

### 4. Chunk Large Diffs

```typescript
const diffs = tokenDiff(huge1, huge2);
const chunks = chunkDiffs(diffs, 100);

// Render progressively
chunks.forEach(chunk => renderAsync(chunk));
```

### 5. Handle Timeouts for Very Large Texts

```typescript
const diffs = tokenDiff(enormous1, enormous2, {
  timeout: 5000, // 5 second limit
  cleanup: 'efficiency' // Faster cleanup
});
```

## Testing

Full test coverage provided:

```bash
# Run all tests
pnpm test diff

# Run with coverage
pnpm test:coverage diff

# Run benchmarks
pnpm test diff.bench.ts
```

### Test Categories

- **Basic functionality**: All diff functions
- **Edge cases**: Empty strings, identical strings, special characters
- **Performance**: Time and memory metrics
- **Caching**: Cache hit/miss, pruning
- **Integration**: Real-world scenarios (code, documents, JSON)

## Troubleshooting

### Slow Diff Computation

**Problem:** Diff takes >1s for 10K tokens

**Solutions:**
```typescript
// 1. Use efficiency cleanup
similarity(text1, text2, { cleanup: 'efficiency' });

// 2. Check if texts are identical first
if (isIdentical(text1, text2)) return 100;

// 3. Use token diff instead of char diff
tokenDiff(text1, text2); // Faster than charDiff
```

### Out of Memory

**Problem:** Memory usage exceeds limits

**Solutions:**
```typescript
// 1. Clear cache
clearDiffCache();

// 2. Use chunking for rendering
const chunks = chunkDiffs(diffs, 50);

// 3. Use Web Worker for very large diffs (future feature)
lineDiff(huge1, huge2, false, { useWorker: true });
```

### Inaccurate Diffs

**Problem:** Diffs don't look right

**Solutions:**
```typescript
// 1. Try semantic cleanup (default)
similarity(text1, text2, { cleanup: 'semantic' });

// 2. Use char diff for granularity
charDiff(text1, text2);

// 3. Check input text for encoding issues
```

## API Reference

### Core Functions
- `tokenDiff()` - Token-level comparison
- `charDiff()` - Character-level comparison
- `lineDiff()` - Line-level comparison
- `similarity()` - Similarity percentage

### Helpers
- `getDiffStats()` - Diff statistics
- `mergeDiffs()` - Merge adjacent changes
- `getContext()` - Get surrounding lines
- `chunkDiffs()` - Split for rendering
- `findFirstDifference()` - First diff position
- `isIdentical()` - Quick equality check
- `formatDiff()` - Format for display

### Cache Management
- `clearDiffCache()` - Clear cache
- `getCacheStats()` - Cache statistics

### Type Guards
- `isDiffResult()` - Check DiffResult type
- `isLineDiff()` - Check LineDiff type

## License

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

Uses `diff-match-patch` (Apache 2.0 license)
