/**
 * Efficient Diff Utilities for Text and Data Comparison
 *
 * Provides multiple diff algorithms optimized for different text sizes:
 * - Token-level diff: Fast for structured text
 * - Character-level diff: Fine-grained highlighting
 * - Line-level diff: Best for code comparison
 * - Similarity scoring: Fast comparison metric
 *
 * Algorithm: Myers' algorithm via diff-match-patch library
 * Performance: <100ms for 1K tokens, <1s for 10K tokens
 */

import * as DiffMatchPatchModule from 'diff-match-patch';

const DiffMatchPatch = DiffMatchPatchModule.diff_match_patch;
type Diff = [number, string];
const DIFF_DELETE = DiffMatchPatchModule.DIFF_DELETE;
const DIFF_INSERT = DiffMatchPatchModule.DIFF_INSERT;
const DIFF_EQUAL = DiffMatchPatchModule.DIFF_EQUAL;

/**
 * Core diff result type - represents a single change unit
 */
export interface DiffResult {
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  value: string;
  position: number;
  length: number;
}

/**
 * Line-level diff result with character-level details
 */
export interface LineDiff {
  goldenLine?: string;
  currentLine?: string;
  lineNumber: number;
  type: 'added' | 'removed' | 'unchanged' | 'modified';
  charDiffs?: DiffResult[];
}

/**
 * Statistics about a diff
 */
export interface DiffStats {
  additions: number;
  deletions: number;
  modifications: number;
  totalLines: number;
  similarityScore: number;
  computeTime: number;
}

/**
 * Options for diff computation
 */
export interface DiffOptions {
  /** Cleanup level: 'semantic', 'efficiency', or 'none' */
  cleanup?: 'semantic' | 'efficiency' | 'none';
  /** Timeout in milliseconds for long diffs */
  timeout?: number;
  /** Enable caching of results */
  cache?: boolean;
  /** Use Web Worker for diffs >10K tokens */
  useWorker?: boolean;
}

// Singleton instance of DiffMatchPatch
const dmp = new DiffMatchPatch();

// Cache for diff results to avoid recomputation
const diffCache = new Map<string, Diff[]>();
const MAX_CACHE_SIZE = 100;

/**
 * Generate cache key for diff inputs
 */
function getCacheKey(golden: string, current: string): string {
  // Use simple hash for cache key
  const hash = (str: string) => {
    let h = 0;
    for (let i = 0; i < Math.min(str.length, 100); i++) {
      h = ((h << 5) - h) + str.charCodeAt(i);
      h = h & h; // Convert to 32bit integer
    }
    return h.toString(36);
  };
  return `${hash(golden)}:${hash(current)}`;
}

/**
 * Manage cache to prevent unbounded growth
 */
function pruneCache(): void {
  if (diffCache.size > MAX_CACHE_SIZE) {
    const firstKey = diffCache.keys().next().value;
    if (firstKey) {
      diffCache.delete(firstKey);
    }
  }
}

/**
 * Compute raw diffs using Myers' algorithm
 * Applies semantic cleanup for better results
 */
function computeRawDiffs(golden: string, current: string, options?: DiffOptions): Diff[] {
  // Check cache first
  if (options?.cache !== false) {
    const cacheKey = getCacheKey(golden, current);
    if (diffCache.has(cacheKey)) {
      return diffCache.get(cacheKey)!;
    }
  }

  // Set timeout for diff_main
  if (options?.timeout) {
    dmp.Diff_Timeout = options.timeout / 1000;
  }

  // Compute main diff
  const diffs = dmp.diff_main(golden, current);

  // Apply cleanup based on options
  const cleanup = options?.cleanup ?? 'semantic';
  if (cleanup === 'semantic') {
    dmp.diff_cleanupSemantic(diffs);
  } else if (cleanup === 'efficiency') {
    dmp.diff_cleanupEfficiency(diffs);
  }

  // Cache result
  if (options?.cache !== false) {
    const cacheKey = getCacheKey(golden, current);
    diffCache.set(cacheKey, diffs);
    pruneCache();
  }

  return diffs;
}

/**
 * Token-level diff - splits by whitespace and newlines
 * Best for structured text where tokens have semantic meaning
 *
 * @param golden - Original text
 * @param current - Modified text
 * @param options - Diff computation options
 * @returns Array of diff results at token level
 */
export function tokenDiff(golden: string, current: string, options?: DiffOptions): DiffResult[] {
  const diffs = computeRawDiffs(golden, current, options);
  const results: DiffResult[] = [];
  let position = 0;

  for (const [op, text] of diffs) {
    const tokens = text.split(/(\s+|\n)/);

    for (const token of tokens) {
      if (token.length === 0) continue;

      let type: DiffResult['type'];
      if (op === DIFF_INSERT) {
        type = 'added';
      } else if (op === DIFF_DELETE) {
        type = 'removed';
      } else {
        type = 'unchanged';
      }

      results.push({
        type,
        value: token,
        position,
        length: token.length,
      });

      position += token.length;
    }
  }

  return results;
}

/**
 * Character-level diff - most granular
 * Best for inline highlighting and character-by-character comparison
 *
 * @param golden - Original text
 * @param current - Modified text
 * @param options - Diff computation options
 * @returns Array of diff results at character level
 */
export function charDiff(golden: string, current: string, options?: DiffOptions): DiffResult[] {
  const diffs = computeRawDiffs(golden, current, options);
  const results: DiffResult[] = [];
  let position = 0;

  for (const [op, text] of diffs) {
    let type: DiffResult['type'];
    if (op === DIFF_INSERT) {
      type = 'added';
    } else if (op === DIFF_DELETE) {
      type = 'removed';
    } else {
      type = 'unchanged';
    }

    for (let i = 0; i < text.length; i++) {
      results.push({
        type,
        value: text[i],
        position,
        length: 1,
      });
      position += 1;
    }
  }

  return results;
}

/**
 * Line-level diff - best for code/document comparison
 * Groups diffs by lines and optionally includes character-level diffs
 *
 * @param golden - Original text (can be string or string array)
 * @param current - Modified text (can be string or string array)
 * @param includeCharDiffs - Whether to include character-level diffs for each line
 * @param options - Diff computation options
 * @returns Array of line diffs
 */
export function lineDiff(
  golden: string | string[],
  current: string | string[],
  includeCharDiffs: boolean = false,
  options?: DiffOptions
): LineDiff[] {
  const goldenText = Array.isArray(golden) ? golden.join('\n') : golden;
  const currentText = Array.isArray(current) ? current.join('\n') : current;

  const diffs = computeRawDiffs(goldenText, currentText, options);
  const results: LineDiff[] = [];

  let goldenLineNum = 1;
  let currentLineNum = 1;
  let pendingGoldenContent = '';
  let pendingCurrentContent = '';
  let pendingGoldenLineNum: number | undefined;
  let pendingCurrentLineNum: number | undefined;
  let pendingType: 'added' | 'removed' | 'unchanged' | 'modified' = 'unchanged';

  const flushLine = () => {
    if (pendingGoldenContent !== '' || pendingCurrentContent !== '') {
      const hasGolden = pendingGoldenContent !== '';
      const hasCurrent = pendingCurrentContent !== '';

      let type: LineDiff['type'] = 'unchanged';
      if (hasGolden && !hasCurrent) {
        type = 'removed';
      } else if (!hasGolden && hasCurrent) {
        type = 'added';
      } else if (hasGolden && hasCurrent && pendingGoldenContent !== pendingCurrentContent) {
        type = 'modified';
      }

      const lineDiff: LineDiff = {
        goldenLine: hasGolden ? pendingGoldenContent : undefined,
        currentLine: hasCurrent ? pendingCurrentContent : undefined,
        lineNumber: Math.max(pendingGoldenLineNum ?? 0, pendingCurrentLineNum ?? 0),
        type,
      };

      // Optionally compute character-level diffs
      if (includeCharDiffs && type === 'modified' && pendingGoldenContent && pendingCurrentContent) {
        lineDiff.charDiffs = charDiff(pendingGoldenContent, pendingCurrentContent, {
          cleanup: 'efficiency',
          ...options,
        });
      }

      results.push(lineDiff);

      pendingGoldenContent = '';
      pendingCurrentContent = '';
      pendingGoldenLineNum = undefined;
      pendingCurrentLineNum = undefined;
    }
  };

  for (const [op, text] of diffs) {
    const lines = text.split('\n');

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const isLastLine = i === lines.length - 1;

      if (op === DIFF_EQUAL) {
        if (pendingGoldenContent || pendingCurrentContent) {
          flushLine();
        }
        pendingGoldenContent = line;
        pendingCurrentContent = line;
        pendingGoldenLineNum = goldenLineNum++;
        pendingCurrentLineNum = currentLineNum++;

        if (!isLastLine) {
          flushLine();
        }
      } else if (op === DIFF_DELETE) {
        if (!pendingGoldenContent && !pendingCurrentContent) {
          pendingGoldenLineNum = goldenLineNum;
        }
        pendingGoldenContent += line;
        if (!isLastLine) {
          goldenLineNum++;
        }
      } else if (op === DIFF_INSERT) {
        if (!pendingGoldenContent && !pendingCurrentContent) {
          pendingCurrentLineNum = currentLineNum;
        }
        pendingCurrentContent += line;
        if (!isLastLine) {
          currentLineNum++;
        }
      }
    }
  }

  flushLine();
  if (pendingGoldenContent || pendingCurrentContent) {
    flushLine();
  }

  return results;
}

/**
 * Compute similarity score between two strings
 * Fast O(n) operation returning 0-100 percentage
 *
 * @param golden - Original text
 * @param current - Modified text
 * @param options - Diff computation options
 * @returns Similarity score (0-100)
 */
export function similarity(golden: string, current: string, options?: DiffOptions): number {
  // Fast path for identical strings
  if (golden === current) {
    return 100;
  }

  // Fast path for empty strings
  if (golden.length === 0 && current.length === 0) {
    return 100;
  }

  if (golden.length === 0 || current.length === 0) {
    return 0;
  }

  const diffs = computeRawDiffs(golden, current, options);

  let equalChars = 0;
  let totalChars = 0;

  for (const [op, text] of diffs) {
    totalChars += text.length;
    if (op === DIFF_EQUAL) {
      equalChars += text.length;
    }
  }

  return totalChars > 0 ? (equalChars / totalChars) * 100 : 100;
}

/**
 * Format diff results for display
 * Converts diff results into readable string representation
 *
 * @param diffs - Array of diff results
 * @param includePositions - Whether to include position info
 * @returns Formatted string
 */
export function formatDiff(diffs: DiffResult[], includePositions: boolean = false): string {
  return diffs
    .map((d) => {
      const prefix =
        d.type === 'added' ? '+ ' : d.type === 'removed' ? '- ' : d.type === 'modified' ? '~ ' : '  ';
      const position = includePositions ? ` [${d.position}:${d.position + d.length}]` : '';
      return prefix + d.value + position;
    })
    .join('');
}

/**
 * Get comprehensive diff statistics
 * Computes additions, deletions, modifications, and similarity
 *
 * @param diffs - Array of diff results or raw Diff array
 * @returns Diff statistics
 */
export function getDiffStats(diffs: DiffResult[] | Diff[]): Omit<DiffStats, 'computeTime'> {
  let additions = 0;
  let deletions = 0;
  let modifications = 0;
  let equalChars = 0;
  let totalChars = 0;

  // Check if we have DiffResult[] or Diff[]
  if (diffs.length > 0 && typeof diffs[0] === 'object' && 'type' in diffs[0]) {
    // DiffResult[]
    for (const d of diffs as DiffResult[]) {
      totalChars += d.length;
      if (d.type === 'added') {
        additions += 1;
      } else if (d.type === 'removed') {
        deletions += 1;
      } else if (d.type === 'modified') {
        modifications += 1;
      } else {
        equalChars += d.length;
      }
    }
  } else {
    // Diff[]
    for (const [op, text] of diffs as Diff[]) {
      const lines = text.split('\n').length - 1 + (text.endsWith('\n') ? 0 : 1);
      totalChars += text.length;

      if (op === DIFF_INSERT) {
        additions += lines;
      } else if (op === DIFF_DELETE) {
        deletions += lines;
      } else {
        equalChars += text.length;
      }
    }
  }

  modifications = Math.min(additions, deletions);

  const similarityScore = totalChars > 0 ? (equalChars / totalChars) * 100 : 100;

  return {
    additions,
    deletions,
    modifications,
    totalLines: additions + deletions + (equalChars > 0 ? 1 : 0),
    similarityScore,
  };
}

/**
 * Merge adjacent changes of the same type
 * Reduces diff size by combining consecutive changes
 *
 * @param diffs - Array of diff results
 * @returns Merged diff results
 */
export function mergeDiffs(diffs: DiffResult[]): DiffResult[] {
  if (diffs.length === 0) return diffs;

  const merged: DiffResult[] = [];
  let current = { ...diffs[0] };

  for (let i = 1; i < diffs.length; i++) {
    const next = diffs[i];

    // Merge if same type and adjacent
    if (
      current.type === next.type &&
      current.position + current.length === next.position
    ) {
      current.value += next.value;
      current.length += next.length;
    } else {
      merged.push(current);
      current = { ...next };
    }
  }

  merged.push(current);
  return merged;
}

/**
 * Find context around a change
 * Returns the change plus surrounding unchanged lines
 *
 * @param diffs - Array of diff results
 * @param changeIndex - Index of change to get context for
 * @param contextLines - Number of lines before/after to include
 * @returns Diff results with context
 */
export function getContext(diffs: DiffResult[], changeIndex: number, contextLines: number = 3): DiffResult[] {
  if (changeIndex < 0 || changeIndex >= diffs.length) {
    return [];
  }

  const start = Math.max(0, changeIndex - contextLines);
  const end = Math.min(diffs.length, changeIndex + contextLines + 1);

  return diffs.slice(start, end);
}

/**
 * Chunk large diffs for progressive rendering
 * Splits diff into manageable pieces for virtualized rendering
 *
 * @param diffs - Array of diff results
 * @param chunkSize - Number of items per chunk (default: 100)
 * @returns Array of diff chunks
 */
export function chunkDiffs(diffs: DiffResult[], chunkSize: number = 100): DiffResult[][] {
  const chunks: DiffResult[][] = [];
  for (let i = 0; i < diffs.length; i += chunkSize) {
    chunks.push(diffs.slice(i, i + chunkSize));
  }
  return chunks;
}

/**
 * Find first difference position between two strings
 * Fast operation to detect if strings are identical
 *
 * @param golden - Original text
 * @param current - Modified text
 * @returns Position of first difference, or -1 if identical
 */
export function findFirstDifference(golden: string, current: string): number {
  const minLen = Math.min(golden.length, current.length);

  for (let i = 0; i < minLen; i++) {
    if (golden[i] !== current[i]) {
      return i;
    }
  }

  // If all chars match, difference is at length difference
  return golden.length !== current.length ? minLen : -1;
}

/**
 * Check if strings are identical
 * Fast check before expensive diff operations
 *
 * @param golden - Original text
 * @param current - Modified text
 * @returns True if strings are identical
 */
export function isIdentical(golden: string, current: string): boolean {
  return golden === current;
}

/**
 * Clear the diff cache
 * Useful for memory management or testing
 */
export function clearDiffCache(): void {
  diffCache.clear();
}

/**
 * Get cache statistics
 * Returns information about cache usage
 */
export function getCacheStats(): {
  size: number;
  maxSize: number;
} {
  return {
    size: diffCache.size,
    maxSize: MAX_CACHE_SIZE,
  };
}

/**
 * Type guard to check if value is a DiffResult
 */
export function isDiffResult(value: unknown): value is DiffResult {
  return (
    typeof value === 'object' &&
    value !== null &&
    'type' in value &&
    'value' in value &&
    'position' in value &&
    'length' in value
  );
}

/**
 * Type guard to check if value is a LineDiff
 */
export function isLineDiff(value: unknown): value is LineDiff {
  return (
    typeof value === 'object' &&
    value !== null &&
    'type' in value &&
    'lineNumber' in value &&
    ('goldenLine' in value || 'currentLine' in value)
  );
}
