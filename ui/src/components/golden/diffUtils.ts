/**
 * Utilities for diff operations
 */

import * as DiffMatchPatchModule from 'diff-match-patch';

const DiffMatchPatch = DiffMatchPatchModule.diff_match_patch;
type Diff = [number, string];

const dmp = new DiffMatchPatch();

/**
 * Calculate Levenshtein distance between two strings
 */
export function levenshteinDistance(str1: string, str2: string): number {
  const diffs = dmp.diff_main(str1, str2);
  return dmp.diff_levenshtein(diffs);
}

/**
 * Calculate similarity score (0-100) between two strings
 */
export function calculateSimilarity(str1: string, str2: string): number {
  const maxLength = Math.max(str1.length, str2.length);
  if (maxLength === 0) return 100;

  const distance = levenshteinDistance(str1, str2);
  return ((maxLength - distance) / maxLength) * 100;
}

/**
 * Compute word-level diff instead of character-level
 */
export function wordDiff(text1: string, text2: string): Diff[] {
  const diffs = dmp.diff_main(text1, text2);
  dmp.diff_cleanupSemantic(diffs);
  return diffs;
}

/**
 * Compute line-level diff
 */
export function lineDiff(text1: string, text2: string): Diff[] {
  const lineArray1 = dmp.diff_linesToChars_(text1, text2);
  const lineText1 = lineArray1.chars1;
  const lineText2 = lineArray1.chars2;
  const lineArray = lineArray1.lineArray;

  const diffs = dmp.diff_main(lineText1, lineText2, false);
  dmp.diff_charsToLines_(diffs, lineArray);
  return diffs;
}

/**
 * Create a unified diff format string
 */
export function createUnifiedDiff(
  text1: string,
  text2: string,
  context = 3,
  filename1 = 'golden',
  filename2 = 'current'
): string {
  const diffs = dmp.diff_main(text1, text2);
  dmp.diff_cleanupSemantic(diffs);

  const lines: string[] = [];
  lines.push(`--- ${filename1}`);
  lines.push(`+++ ${filename2}`);

  const text1Lines = text1.split('\n');
  const text2Lines = text2.split('\n');

  let lineNum1 = 0;
  let lineNum2 = 0;

  for (const [op, text] of diffs) {
    const diffLines = text.split('\n');
    const lineCount = diffLines.length - (text.endsWith('\n') ? 1 : 0);

    if (op === 0) {
      // Equal
      lineNum1 += lineCount;
      lineNum2 += lineCount;
    } else if (op === -1) {
      // Delete
      for (let i = 0; i < lineCount; i++) {
        lines.push(`-${diffLines[i]}`);
      }
      lineNum1 += lineCount;
    } else {
      // Insert
      for (let i = 0; i < lineCount; i++) {
        lines.push(`+${diffLines[i]}`);
      }
      lineNum2 += lineCount;
    }
  }

  return lines.join('\n');
}

/**
 * Extract changed regions with context
 */
export interface DiffRegion {
  startLine: number;
  endLine: number;
  changes: Array<{
    type: 'addition' | 'deletion' | 'modification';
    line: number;
    content: string;
  }>;
}

export function extractDiffRegions(text1: string, text2: string, context = 3): DiffRegion[] {
  const diffs = dmp.diff_main(text1, text2);
  dmp.diff_cleanupSemantic(diffs);

  const regions: DiffRegion[] = [];
  let currentRegion: DiffRegion | null = null;
  let lineNum = 0;

  for (const [op, text] of diffs) {
    const diffLines = text.split('\n');
    const lineCount = diffLines.length - (text.endsWith('\n') ? 1 : 0);

    if (op !== 0) {
      // Start new region if needed
      if (!currentRegion) {
        currentRegion = {
          startLine: Math.max(0, lineNum - context),
          endLine: lineNum + lineCount + context,
          changes: [],
        };
      }

      // Add changes
      for (let i = 0; i < lineCount; i++) {
        currentRegion.changes.push({
          type: op === -1 ? 'deletion' : 'addition',
          line: lineNum + i,
          content: diffLines[i],
        });
      }

      currentRegion.endLine = lineNum + lineCount + context;
    } else {
      // Check if we should close current region
      if (currentRegion && lineNum - currentRegion.endLine > context * 2) {
        regions.push(currentRegion);
        currentRegion = null;
      }
    }

    lineNum += lineCount;
  }

  // Add last region if exists
  if (currentRegion) {
    regions.push(currentRegion);
  }

  return regions;
}

/**
 * Format diff statistics as human-readable string
 */
export function formatDiffStats(stats: {
  additions: number;
  deletions: number;
  modifications: number;
  similarityScore: number;
}): string {
  const parts: string[] = [];

  if (stats.additions > 0) parts.push(`+${stats.additions} additions`);
  if (stats.deletions > 0) parts.push(`-${stats.deletions} deletions`);
  if (stats.modifications > 0) parts.push(`~${stats.modifications} modifications`);

  const summary = parts.length > 0 ? parts.join(', ') : 'No changes';
  return `${summary} (${stats.similarityScore.toFixed(1)}% similar)`;
}

/**
 * Truncate long diff for preview
 */
export function truncateDiff(text: string, maxLines = 50, maxCharsPerLine = 100): string {
  const lines = text.split('\n');

  if (lines.length <= maxLines) {
    return lines.map((line) => (line.length > maxCharsPerLine ? line.slice(0, maxCharsPerLine) + '...' : line)).join('\n');
  }

  const truncated = lines.slice(0, maxLines).map((line) => (line.length > maxCharsPerLine ? line.slice(0, maxCharsPerLine) + '...' : line));

  truncated.push(`... (${lines.length - maxLines} more lines)`);
  return truncated.join('\n');
}

/**
 * Check if diff is too large for inline rendering
 */
export function isDiffTooLarge(text1: string, text2: string, threshold = 10000): boolean {
  return text1.length + text2.length > threshold;
}

/**
 * Optimize diff for large texts by using line-mode
 */
export function optimizedDiff(text1: string, text2: string): Diff[] {
  const THRESHOLD = 10000;

  if (text1.length + text2.length < THRESHOLD) {
    const diffs = dmp.diff_main(text1, text2);
    dmp.diff_cleanupSemantic(diffs);
    return diffs;
  }

  // Use line-based diff for large texts
  return lineDiff(text1, text2);
}
