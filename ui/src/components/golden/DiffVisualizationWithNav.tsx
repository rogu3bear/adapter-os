import { useCallback, useState } from 'react';
import { DiffVisualization, DiffViewMode } from './DiffVisualization';
import { useDiffKeyboardNav } from '@/hooks/golden/useDiffKeyboardNav';

export interface DiffVisualizationWithNavProps {
  goldenText: string;
  currentText: string;
  className?: string;
  showLineNumbers?: boolean;
  contextLines?: number;
  enableVirtualization?: boolean;
  enableKeyboardNav?: boolean;
}

/**
 * Enhanced DiffVisualization component with keyboard navigation
 *
 * Keyboard shortcuts:
 * - N: Next change
 * - P: Previous change
 * - U: Toggle view mode
 * - Cmd/Ctrl+C: Copy to clipboard
 */
export function DiffVisualizationWithNav({
  goldenText,
  currentText,
  className,
  showLineNumbers = true,
  contextLines = 3,
  enableVirtualization = true,
  enableKeyboardNav = true,
}: DiffVisualizationWithNavProps) {
  const [mode, setMode] = useState<DiffViewMode>('side-by-side');

  const handleNext = useCallback(() => {
    // Navigate to next change - handled internally by DiffVisualization
  }, []);

  const handlePrev = useCallback(() => {
    // Navigate to previous change - handled internally by DiffVisualization
  }, []);

  const handleToggleView = useCallback(() => {
    setMode((prev) => {
      if (prev === 'side-by-side') return 'unified';
      if (prev === 'unified') return 'split';
      return 'side-by-side';
    });
  }, []);

  const handleCopy = useCallback(() => {
    // Copy handled internally by DiffVisualization
  }, []);

  useDiffKeyboardNav(handleNext, handlePrev, handleToggleView, handleCopy, enableKeyboardNav);

  return (
    <DiffVisualization
      goldenText={goldenText}
      currentText={currentText}
      mode={mode}
      onModeChange={setMode}
      className={className}
      showLineNumbers={showLineNumbers}
      contextLines={contextLines}
      enableVirtualization={enableVirtualization}
    />
  );
}

export default DiffVisualizationWithNav;
