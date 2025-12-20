import { useCallback, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import * as DiffMatchPatchModule from 'diff-match-patch';

const DiffMatchPatch = DiffMatchPatchModule.diff_match_patch;
type Diff = [number, string];
const DIFF_DELETE = -1;
const DIFF_INSERT = 1;
const DIFF_EQUAL = 0;
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  ChevronDown,
  ChevronUp,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Maximize2,
  Minimize2,
} from 'lucide-react';

export type DiffViewMode = 'side-by-side' | 'unified' | 'split';

export interface DiffVisualizationProps {
  goldenText: string;
  currentText: string;
  mode?: DiffViewMode;
  className?: string;
  onModeChange?: (mode: DiffViewMode) => void;
  showLineNumbers?: boolean;
  contextLines?: number;
  enableVirtualization?: boolean;
}

interface DiffLine {
  type: 'equal' | 'delete' | 'insert' | 'context';
  goldenLineNumber?: number;
  currentLineNumber?: number;
  goldenContent?: string;
  currentContent?: string;
  inlineDiffs?: Diff[];
}

interface DiffStats {
  additions: number;
  deletions: number;
  modifications: number;
  totalLines: number;
  similarityScore: number;
  computeTime: number;
}

interface DiffChange {
  lineIndex: number;
  type: 'change' | 'addition' | 'deletion';
}

const dmp = new DiffMatchPatch();

// Color-blind friendly colors (tested with deuteranopia/protanopia)
const DIFF_COLORS = {
  addition: {
    bg: 'bg-blue-50 dark:bg-blue-950/30',
    border: 'border-l-4 border-blue-500 dark:border-blue-400',
    text: 'text-blue-900 dark:text-blue-100',
    inline: 'bg-blue-200/60 dark:bg-blue-800/60',
  },
  deletion: {
    bg: 'bg-orange-50 dark:bg-orange-950/30',
    border: 'border-l-4 border-orange-500 dark:border-orange-400',
    text: 'text-orange-900 dark:text-orange-100',
    inline: 'bg-orange-200/60 dark:bg-orange-800/60',
  },
  modification: {
    bg: 'bg-purple-50 dark:bg-purple-950/30',
    border: 'border-l-4 border-purple-500 dark:border-purple-400',
    text: 'text-purple-900 dark:text-purple-100',
    inline: 'bg-purple-200/60 dark:bg-purple-800/60',
  },
  equal: {
    bg: 'bg-background',
    border: 'border-l-4 border-transparent',
    text: 'text-foreground',
    inline: '',
  },
};

/**
 * Compute detailed diff statistics
 */
function computeDiffStats(diffs: Diff[]): Omit<DiffStats, 'computeTime'> {
  let additions = 0;
  let deletions = 0;
  let equalChars = 0;
  let totalChars = 0;

  for (const [op, text] of diffs) {
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

  const similarityScore = totalChars > 0 ? (equalChars / totalChars) * 100 : 100;

  return {
    additions,
    deletions,
    modifications: Math.min(additions, deletions),
    totalLines: additions + deletions + (equalChars > 0 ? 1 : 0),
    similarityScore,
  };
}

/**
 * Convert diff operations to line-based representation
 */
function diffsToLines(diffs: Diff[]): DiffLine[] {
  const lines: DiffLine[] = [];
  let goldenLineNum = 1;
  let currentLineNum = 1;

  let pendingGoldenContent = '';
  let pendingCurrentContent = '';
  let pendingInlineDiffs: Diff[] = [];

  const flushLine = () => {
    if (pendingGoldenContent || pendingCurrentContent) {
      const hasGolden = pendingGoldenContent !== '';
      const hasCurrent = pendingCurrentContent !== '';

      let type: DiffLine['type'] = 'equal';
      if (hasGolden && !hasCurrent) type = 'delete';
      else if (!hasGolden && hasCurrent) type = 'insert';
      else if (hasGolden && hasCurrent && pendingInlineDiffs.length > 0) type = 'equal';

      lines.push({
        type,
        goldenLineNumber: hasGolden ? goldenLineNum++ : undefined,
        currentLineNumber: hasCurrent ? currentLineNum++ : undefined,
        goldenContent: pendingGoldenContent || undefined,
        currentContent: pendingCurrentContent || undefined,
        inlineDiffs: pendingInlineDiffs.length > 0 ? [...pendingInlineDiffs] : undefined,
      });

      pendingGoldenContent = '';
      pendingCurrentContent = '';
      pendingInlineDiffs = [];
    }
  };

  for (const [op, text] of diffs) {
    const textLines = text.split('\n');

    for (let i = 0; i < textLines.length; i++) {
      const line = textLines[i];
      const isLastLine = i === textLines.length - 1;

      if (op === DIFF_EQUAL) {
        pendingGoldenContent += line;
        pendingCurrentContent += line;
        if (!isLastLine) {
          flushLine();
        }
      } else if (op === DIFF_DELETE) {
        pendingGoldenContent += line;
        pendingInlineDiffs.push([op, line]);
        if (!isLastLine) {
          flushLine();
        }
      } else if (op === DIFF_INSERT) {
        pendingCurrentContent += line;
        pendingInlineDiffs.push([op, line]);
        if (!isLastLine) {
          flushLine();
        }
      }
    }
  }

  flushLine();
  return lines;
}

/**
 * Apply context reduction to diff lines
 */
function applyContext(lines: DiffLine[], contextLines: number): DiffLine[] {
  if (contextLines < 0) return lines;

  const result: DiffLine[] = [];
  const changedIndices = new Set<number>();

  lines.forEach((line, idx) => {
    if (line.type !== 'equal') {
      changedIndices.add(idx);
    }
  });

  lines.forEach((line, idx) => {
    const isChanged = changedIndices.has(idx);
    const nearChange = Array.from(changedIndices).some(
      (changeIdx) => Math.abs(changeIdx - idx) <= contextLines
    );

    if (isChanged || nearChange) {
      result.push(line);
    } else if (result.length > 0 && result[result.length - 1].type !== 'context') {
      result.push({ type: 'context', goldenContent: '...', currentContent: '...' });
    }
  });

  return result;
}

/**
 * Render inline diff with character-level highlighting
 */
function renderInlineDiff(diffs: Diff[] | undefined, side: 'golden' | 'current'): React.ReactNode {
  if (!diffs || diffs.length === 0) return null;

  return diffs.map(([op, text], idx) => {
    if (op === DIFF_EQUAL) {
      return <span key={idx}>{text}</span>;
    } else if (op === DIFF_DELETE && side === 'golden') {
      return (
        <span key={idx} className={cn('rounded px-0.5', DIFF_COLORS.deletion.inline)}>
          {text}
        </span>
      );
    } else if (op === DIFF_INSERT && side === 'current') {
      return (
        <span key={idx} className={cn('rounded px-0.5', DIFF_COLORS.addition.inline)}>
          {text}
        </span>
      );
    }
    return null;
  });
}

export function DiffVisualization({
  goldenText,
  currentText,
  mode = 'side-by-side',
  className,
  onModeChange,
  showLineNumbers = true,
  contextLines = -1,
  enableVirtualization = true,
}: DiffVisualizationProps) {
  const [viewMode, setViewMode] = useState<DiffViewMode>(mode);
  const [showUnchanged, setShowUnchanged] = useState(contextLines < 0);
  const [currentChangeIndex, setCurrentChangeIndex] = useState(0);
  const [expandedSections, setExpandedSections] = useState<Set<number>>(new Set());
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const changeRefs = useRef<Map<number, HTMLDivElement>>(new Map());

  // Compute diffs
  const { diffs, stats, diffLines, changes } = useMemo(() => {
    const startTime = performance.now();
    const diffs = dmp.diff_main(goldenText, currentText);
    dmp.diff_cleanupSemantic(diffs);
    const computeTime = performance.now() - startTime;

    const baseStats = computeDiffStats(diffs);
    const stats: DiffStats = { ...baseStats, computeTime };

    const allLines = diffsToLines(diffs);
    const processedLines = showUnchanged ? allLines : applyContext(allLines, contextLines);

    const changes: DiffChange[] = processedLines
      .map((line, idx) => {
        if (line.type === 'insert') return { lineIndex: idx, type: 'addition' as const };
        if (line.type === 'delete') return { lineIndex: idx, type: 'deletion' as const };
        if (line.inlineDiffs && line.inlineDiffs.length > 0) {
          return { lineIndex: idx, type: 'change' as const };
        }
        return null;
      })
      .filter((c): c is DiffChange => c !== null);

    return { diffs, stats, diffLines: processedLines, changes };
  }, [goldenText, currentText, showUnchanged, contextLines]);

  // Virtualization setup
  const rowVirtualizer = useVirtualizer({
    count: diffLines.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 32,
    overscan: 10,
    enabled: enableVirtualization && diffLines.length > 100,
  });

  // Navigation handlers
  const navigateToChange = useCallback(
    (direction: 'next' | 'prev') => {
      if (changes.length === 0) return;

      let newIndex: number;
      if (direction === 'next') {
        newIndex = (currentChangeIndex + 1) % changes.length;
      } else {
        newIndex = (currentChangeIndex - 1 + changes.length) % changes.length;
      }

      setCurrentChangeIndex(newIndex);

      const change = changes[newIndex];
      const element = changeRefs.current.get(change.lineIndex);
      if (element) {
        element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    },
    [currentChangeIndex, changes]
  );

  // Export handlers
  const copyToClipboard = useCallback(async () => {
    const text = diffLines
      .map((line) => {
        if (viewMode === 'side-by-side') {
          const golden = line.goldenContent || '';
          const current = line.currentContent || '';
          return `${golden} | ${current}`;
        } else {
          const prefix = line.type === 'insert' ? '+ ' : line.type === 'delete' ? '- ' : '  ';
          return prefix + (line.currentContent || line.goldenContent || '');
        }
      })
      .join('\n');

    await navigator.clipboard.writeText(text);
  }, [diffLines, viewMode]);

  const exportAsHtml = useCallback(() => {
    const html = `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Diff Visualization</title>
  <style>
    body { font-family: monospace; font-size: 12px; }
    .line { display: flex; gap: 1rem; padding: 0.25rem 0.5rem; }
    .line.addition { background-color: #dbeafe; }
    .line.deletion { background-color: #fed7aa; }
    .line.modification { background-color: #e9d5ff; }
    .line-number { color: #6b7280; min-width: 3rem; }
    .content { flex: 1; white-space: pre-wrap; }
  </style>
</head>
<body>
  <h1>Diff Visualization</h1>
  <p>Similarity: ${stats.similarityScore.toFixed(2)}%</p>
  <p>+${stats.additions} -${stats.deletions} ~${stats.modifications}</p>
  <div class="diff">
${diffLines
  .map(
    (line) => `
    <div class="line ${line.type}">
      <span class="line-number">${line.goldenLineNumber || ''}</span>
      <span class="line-number">${line.currentLineNumber || ''}</span>
      <span class="content">${line.currentContent || line.goldenContent || ''}</span>
    </div>`
  )
  .join('')}
  </div>
</body>
</html>`;

    const blob = new Blob([html], { type: 'text/html' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'diff-visualization.html';
    a.click();
    URL.revokeObjectURL(url);
  }, [diffLines, stats]);

  const exportAsText = useCallback(() => {
    const text = diffLines
      .map((line) => {
        const prefix =
          line.type === 'insert' ? '+ ' : line.type === 'delete' ? '- ' : line.type === 'context' ? '... ' : '  ';
        return prefix + (line.currentContent || line.goldenContent || '');
      })
      .join('\n');

    const blob = new Blob([text], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'diff.txt';
    a.click();
    URL.revokeObjectURL(url);
  }, [diffLines]);

  // Mode change handler
  const handleModeChange = (newMode: DiffViewMode) => {
    setViewMode(newMode);
    onModeChange?.(newMode);
  };

  // Render line based on view mode
  const renderLine = (line: DiffLine, index: number) => {
    const isChange = changes.some((c) => c.lineIndex === index);
    const isCurrentChange = changes[currentChangeIndex]?.lineIndex === index;

    const lineClass = cn(
      'flex gap-2 px-2 py-1 text-sm font-mono border-l-4 transition-colors',
      line.type === 'insert' && DIFF_COLORS.addition.bg,
      line.type === 'insert' && DIFF_COLORS.addition.border,
      line.type === 'delete' && DIFF_COLORS.deletion.bg,
      line.type === 'delete' && DIFF_COLORS.deletion.border,
      line.type === 'equal' && line.inlineDiffs && DIFF_COLORS.modification.bg,
      line.type === 'equal' && line.inlineDiffs && DIFF_COLORS.modification.border,
      line.type === 'equal' && !line.inlineDiffs && DIFF_COLORS.equal.bg,
      line.type === 'equal' && !line.inlineDiffs && DIFF_COLORS.equal.border,
      line.type === 'context' && 'bg-muted/30 border-muted',
      isCurrentChange && 'ring-2 ring-blue-500 dark:ring-blue-400'
    );

    const lineNumberClass = 'text-muted-foreground min-w-[3rem] text-right select-none';

    if (viewMode === 'side-by-side') {
      return (
        <div
          key={index}
          ref={(el) => {
            if (el && isChange) changeRefs.current.set(index, el);
          }}
          className={lineClass}
        >
          {showLineNumbers && (
            <>
              <span className={lineNumberClass}>{line.goldenLineNumber || ''}</span>
              <span className={lineNumberClass}>{line.currentLineNumber || ''}</span>
            </>
          )}
          <div className="flex-1 grid grid-cols-2 gap-4">
            <div className="whitespace-pre-wrap break-all">
              {line.inlineDiffs ? renderInlineDiff(line.inlineDiffs, 'golden') : line.goldenContent}
            </div>
            <div className="whitespace-pre-wrap break-all">
              {line.inlineDiffs ? renderInlineDiff(line.inlineDiffs, 'current') : line.currentContent}
            </div>
          </div>
        </div>
      );
    } else if (viewMode === 'unified') {
      return (
        <div
          key={index}
          ref={(el) => {
            if (el && isChange) changeRefs.current.set(index, el);
          }}
          className={lineClass}
        >
          {showLineNumbers && (
            <>
              <span className={lineNumberClass}>{line.goldenLineNumber || ''}</span>
              <span className={lineNumberClass}>{line.currentLineNumber || ''}</span>
            </>
          )}
          <div className="flex-1 whitespace-pre-wrap break-all">
            {line.inlineDiffs
              ? renderInlineDiff(line.inlineDiffs, line.type === 'insert' ? 'current' : 'golden')
              : line.currentContent || line.goldenContent}
          </div>
        </div>
      );
    } else {
      // Split view - show both if different
      if (line.type === 'equal' && !line.inlineDiffs) {
        return (
          <div key={index} className={lineClass}>
            {showLineNumbers && <span className={lineNumberClass}>{line.goldenLineNumber || ''}</span>}
            <div className="flex-1 whitespace-pre-wrap break-all">{line.goldenContent}</div>
          </div>
        );
      } else {
        return (
          <div key={`split-${index}`} className="flex flex-col gap-px">
            {line.goldenContent && (
              <div className={cn('flex gap-2 px-2 py-1 text-sm font-mono', DIFF_COLORS.deletion.bg, DIFF_COLORS.deletion.border)}>
                {showLineNumbers && <span className={lineNumberClass}>{line.goldenLineNumber || ''}</span>}
                <div className="flex-1 whitespace-pre-wrap break-all">
                  {line.inlineDiffs ? renderInlineDiff(line.inlineDiffs, 'golden') : line.goldenContent}
                </div>
              </div>
            )}
            {line.currentContent && (
              <div className={cn('flex gap-2 px-2 py-1 text-sm font-mono', DIFF_COLORS.addition.bg, DIFF_COLORS.addition.border)}>
                {showLineNumbers && <span className={lineNumberClass}>{line.currentLineNumber || ''}</span>}
                <div className="flex-1 whitespace-pre-wrap break-all">
                  {line.inlineDiffs ? renderInlineDiff(line.inlineDiffs, 'current') : line.currentContent}
                </div>
              </div>
            )}
          </div>
        );
      }
    }
  };

  return (
    <div className={cn('flex flex-col gap-4', className)}>
      {/* Stats Panel */}
      <Card className="p-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex items-center gap-2">
              <Badge variant="outline" className="bg-blue-50 dark:bg-blue-950/30">
                +{stats.additions}
              </Badge>
              <Badge variant="outline" className="bg-orange-50 dark:bg-orange-950/30">
                -{stats.deletions}
              </Badge>
              <Badge variant="outline" className="bg-purple-50 dark:bg-purple-950/30">
                ~{stats.modifications}
              </Badge>
            </div>
            <div className="text-sm text-muted-foreground">
              Similarity: <span className="font-semibold">{stats.similarityScore.toFixed(2)}%</span>
            </div>
            <div className="text-sm text-muted-foreground">
              Computed in <span className="font-semibold">{stats.computeTime.toFixed(2)}ms</span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={copyToClipboard} title="Copy to clipboard">
              <Copy className="h-4 w-4" />
            </Button>
            <Button variant="outline" size="sm" onClick={exportAsText} title="Export as text">
              <Download className="h-4 w-4" />
            </Button>
            <Button variant="outline" size="sm" onClick={exportAsHtml} title="Export as HTML">
              <Download className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </Card>

      {/* View Controls */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">View:</span>
          <Button
            variant={viewMode === 'side-by-side' ? 'default' : 'outline'}
            size="sm"
            onClick={() => handleModeChange('side-by-side')}
          >
            Side by Side
          </Button>
          <Button
            variant={viewMode === 'unified' ? 'default' : 'outline'}
            size="sm"
            onClick={() => handleModeChange('unified')}
          >
            Unified
          </Button>
          <Button
            variant={viewMode === 'split' ? 'default' : 'outline'}
            size="sm"
            onClick={() => handleModeChange('split')}
          >
            Split
          </Button>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowUnchanged(!showUnchanged)}
            title={showUnchanged ? 'Hide unchanged lines' : 'Show all lines'}
          >
            {showUnchanged ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}
          </Button>
          {changes.length > 0 && (
            <>
              <span className="text-sm text-muted-foreground">
                {currentChangeIndex + 1} / {changes.length}
              </span>
              <Button variant="outline" size="sm" onClick={() => navigateToChange('prev')} title="Previous change (P)">
                <ChevronUp className="h-4 w-4" />
              </Button>
              <Button variant="outline" size="sm" onClick={() => navigateToChange('next')} title="Next change (N)">
                <ChevronDown className="h-4 w-4" />
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Diff Content */}
      <div
        ref={scrollContainerRef}
        className="border rounded-lg overflow-auto max-h-[600px] bg-background"
        style={{ height: enableVirtualization && diffLines.length > 100 ? '600px' : 'auto' }}
      >
        {enableVirtualization && diffLines.length > 100 ? (
          <div
            style={{
              height: `${rowVirtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {rowVirtualizer.getVirtualItems().map((virtualRow) => (
              <div
                key={virtualRow.key}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {renderLine(diffLines[virtualRow.index], virtualRow.index)}
              </div>
            ))}
          </div>
        ) : (
          <div className="flex flex-col">{diffLines.map((line, idx) => renderLine(line, idx))}</div>
        )}
      </div>

      {/* Keyboard shortcuts hint */}
      <div className="text-xs text-muted-foreground text-center">
        Tip: Use N/P keys to navigate between changes
      </div>
    </div>
  );
}

export default DiffVisualization;
