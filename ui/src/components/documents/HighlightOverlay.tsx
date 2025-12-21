/**
 * HighlightOverlay - Renders highlight boxes over PDF pages
 *
 * Supports two types of highlights:
 * - citation: Yellow highlights for cited evidence
 * - search: Blue highlights for search results
 *
 * Highlights are positioned using bounding boxes with coordinates relative
 * to the PDF page, then scaled to match the current zoom level.
 */

import React from 'react';
import { cn } from '@/lib/utils';

interface HighlightBBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Highlight {
  id: string;
  page: number;
  bbox?: HighlightBBox;
  style?: 'citation' | 'search';
}

interface HighlightOverlayProps {
  /** Array of highlights to render */
  highlights: Highlight[];
  /** Current page number being displayed */
  currentPage: number;
  /** Current zoom scale (1.0 = 100%) */
  scale: number;
  /** Additional className for the overlay container */
  className?: string;
}

/**
 * Renders highlight boxes on the current PDF page
 *
 * The overlay is positioned absolutely over the PDF canvas and uses
 * pointer-events-none to allow interaction with the underlying PDF.
 */
export function HighlightOverlay({
  highlights,
  currentPage,
  scale,
  className,
}: HighlightOverlayProps) {
  // Filter highlights for the current page only
  const pageHighlights = highlights.filter((h) => h.page === currentPage && h.bbox);

  // No highlights to render
  if (pageHighlights.length === 0) return null;

  return (
    <div
      className={cn('absolute inset-0 pointer-events-none', className)}
      role="presentation"
      aria-hidden="true"
    >
      {pageHighlights.map((highlight) => {
        if (!highlight.bbox) return null;

        const isCitation = highlight.style === 'citation';
        const isSearch = highlight.style === 'search';

        return (
          <div
            key={highlight.id}
            className={cn(
              'absolute rounded transition-all duration-200',
              isCitation && 'bg-yellow-300/40 border border-yellow-500',
              isSearch && 'bg-blue-300/40 border border-blue-500',
              !isCitation && !isSearch && 'bg-purple-300/40 border border-purple-500'
            )}
            style={{
              left: `${highlight.bbox.x * scale}px`,
              top: `${highlight.bbox.y * scale}px`,
              width: `${highlight.bbox.width * scale}px`,
              height: `${highlight.bbox.height * scale}px`,
            }}
            title={`${isCitation ? 'Citation' : isSearch ? 'Search result' : 'Highlight'} on page ${highlight.page}`}
          />
        );
      })}
    </div>
  );
}

export default HighlightOverlay;
