/**
 * PDFViewerEmbedded - Embeddable PDF viewer for split-view layouts
 *
 * A refactored version of PDFViewer that works without a Dialog wrapper.
 * Supports controlled navigation via props and exposes navigation methods via ref.
 *
 * Accessibility features:
 * - ARIA labels for all controls
 * - Keyboard shortcuts: Arrow keys (navigation), +/- (zoom), Home/End (first/last page)
 * - Screen reader announcements for page changes and zoom level
 * - Semantic landmarks and status regions
 */

import React, { forwardRef, useImperativeHandle, useState, useCallback, useEffect, useRef } from 'react';
import { Document, Page, pdfjs } from 'react-pdf';
import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut, Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import 'react-pdf/dist/esm/Page/AnnotationLayer.css';
import 'react-pdf/dist/esm/Page/TextLayer.css';

// Set worker path for pdf.js
pdfjs.GlobalWorkerOptions.workerSrc = `//cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjs.version}/pdf.worker.min.js`;

export interface PDFViewerEmbeddedRef {
  goToPage: (page: number) => void;
  scrollToText: (text: string) => void;
}

interface PDFViewerEmbeddedProps {
  /** URL or blob URL of the PDF document */
  src: string;
  /** Current page number (controlled) */
  currentPage?: number;
  /** Callback when page changes */
  onPageChange?: (page: number) => void;
  /** Text to highlight in the document */
  highlightText?: string;
  /** Additional class name */
  className?: string;
  /** Document name for download */
  documentName?: string;
}

const PDFViewerEmbedded = forwardRef<PDFViewerEmbeddedRef, PDFViewerEmbeddedProps>(
  ({ src, currentPage = 1, onPageChange, highlightText, className, documentName = 'document.pdf' }, ref) => {
    const [page, setPage] = useState(currentPage);
    const [numPages, setNumPages] = useState<number>(0);
    const [scale, setScale] = useState(1.0);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const viewerRef = useRef<HTMLDivElement>(null);

    // Sync internal page state with controlled prop
    useEffect(() => {
      if (currentPage !== undefined && currentPage !== page) {
        setPage(currentPage);
      }
    }, [currentPage, page]);

    const onDocumentLoadSuccess = ({ numPages }: { numPages: number }) => {
      setNumPages(numPages);
      setIsLoading(false);
      setError(null);
    };

    const onDocumentLoadError = (error: Error) => {
      console.error('PDF load error:', error);
      setError('Failed to load PDF document');
      setIsLoading(false);
    };

    // Announce changes to screen readers (defined before useImperativeHandle to avoid hoisting issues)
    const announceToScreenReader = useCallback((message: string) => {
      const announcement = window.document.createElement('div');
      announcement.setAttribute('role', 'status');
      announcement.setAttribute('aria-live', 'polite');
      announcement.setAttribute('aria-atomic', 'true');
      announcement.className = 'sr-only';
      announcement.textContent = message;
      window.document.body.appendChild(announcement);
      setTimeout(() => announcement.remove(), 1000);
    }, []);

    // Expose navigation methods via ref
    useImperativeHandle(ref, () => ({
      goToPage: (targetPage: number) => {
        const newPage = Math.max(1, Math.min(targetPage, numPages));
        setPage(newPage);
        onPageChange?.(newPage);
      },
      scrollToText: (text: string) => {
        // Search for text in the rendered text layer and scroll to it
        if (!viewerRef.current || !text) return;

        // Wait for text layer to render, then search
        requestAnimationFrame(() => {
          const textLayer = viewerRef.current?.querySelector('.react-pdf__Page__textContent');
          if (!textLayer) {
            announceToScreenReader(`Text search: "${text}" - searching...`);
            return;
          }

          // Find all text spans in the text layer
          const textSpans = textLayer.querySelectorAll('span');
          const searchTermLower = text.toLowerCase();

          for (const span of textSpans) {
            const spanText = span.textContent?.toLowerCase() || '';
            if (spanText.includes(searchTermLower)) {
              // Scroll the span into view
              span.scrollIntoView({ behavior: 'smooth', block: 'center' });

              // Highlight the found text temporarily
              const originalBg = (span as HTMLElement).style.backgroundColor;
              (span as HTMLElement).style.backgroundColor = 'rgba(255, 255, 0, 0.5)';
              (span as HTMLElement).style.transition = 'background-color 0.3s';

              // Remove highlight after 3 seconds
              setTimeout(() => {
                (span as HTMLElement).style.backgroundColor = originalBg;
              }, 3000);

              announceToScreenReader(`Found text: "${text}"`);
              return;
            }
          }

          // Text not found on current page
          announceToScreenReader(`Text "${text}" not found on current page`);
        });
      },
    }));

    const handlePreviousPage = useCallback(() => {
      if (page > 1) {
        const newPage = page - 1;
        setPage(newPage);
        onPageChange?.(newPage);
        announceToScreenReader(`Page ${newPage} of ${numPages}`);
      }
    }, [page, numPages, onPageChange, announceToScreenReader]);

    const handleNextPage = useCallback(() => {
      if (page < numPages) {
        const newPage = page + 1;
        setPage(newPage);
        onPageChange?.(newPage);
        announceToScreenReader(`Page ${newPage} of ${numPages}`);
      }
    }, [page, numPages, onPageChange, announceToScreenReader]);

    const handleZoomIn = useCallback(() => {
      setScale((prev) => {
        const newScale = Math.min(prev + 0.1, 2.0);
        announceToScreenReader(`Zoom ${Math.round(newScale * 100)}%`);
        return newScale;
      });
    }, [announceToScreenReader]);

    const handleZoomOut = useCallback(() => {
      setScale((prev) => {
        const newScale = Math.max(prev - 0.1, 0.5);
        announceToScreenReader(`Zoom ${Math.round(newScale * 100)}%`);
        return newScale;
      });
    }, [announceToScreenReader]);

    const handleDownload = useCallback(() => {
      const link = document.createElement('a');
      link.href = src;
      link.download = documentName;
      link.click();
      announceToScreenReader(`Downloading ${documentName}`);
    }, [src, documentName, announceToScreenReader]);

    // Keyboard shortcuts for navigation and zoom
    useEffect(() => {
      const handleKeyDown = (e: KeyboardEvent) => {
        // Only handle shortcuts when viewer is focused or active
        if (!viewerRef.current?.contains(document.activeElement)) {
          return;
        }

        switch (e.key) {
          case 'ArrowLeft':
            e.preventDefault();
            handlePreviousPage();
            break;
          case 'ArrowRight':
            e.preventDefault();
            handleNextPage();
            break;
          case '+':
          case '=':
            e.preventDefault();
            handleZoomIn();
            break;
          case '-':
          case '_':
            e.preventDefault();
            handleZoomOut();
            break;
          case 'Home':
            e.preventDefault();
            setPage(1);
            onPageChange?.(1);
            announceToScreenReader(`Page 1 of ${numPages}`);
            break;
          case 'End':
            e.preventDefault();
            if (numPages > 0) {
              setPage(numPages);
              onPageChange?.(numPages);
              announceToScreenReader(`Page ${numPages} of ${numPages}`);
            }
            break;
        }
      };

      window.addEventListener('keydown', handleKeyDown);
      return () => window.removeEventListener('keydown', handleKeyDown);
    }, [page, numPages, handlePreviousPage, handleNextPage, handleZoomIn, handleZoomOut, onPageChange, announceToScreenReader]);

    return (
      <div ref={viewerRef} className={cn('flex flex-col h-full', className)}>
        {/* Toolbar */}
        <div className="flex items-center justify-between p-2 border-b bg-muted/30 flex-shrink-0" role="toolbar" aria-label="PDF viewer controls">
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={handlePreviousPage}
              disabled={page <= 1}
              aria-label="Previous page (Left arrow)"
              title="Previous page (Left arrow)"
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="text-sm min-w-[6rem] text-center" role="status" aria-live="polite" aria-atomic="true">
              {numPages > 0 ? `Page ${page} of ${numPages}` : 'Loading...'}
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleNextPage}
              disabled={page >= numPages || numPages === 0}
              aria-label="Next page (Right arrow)"
              title="Next page (Right arrow)"
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleZoomOut}
              disabled={scale <= 0.5}
              aria-label="Zoom out (Minus key)"
              title="Zoom out (Minus key)"
            >
              <ZoomOut className="h-4 w-4" />
            </Button>
            <span className="text-sm w-12 text-center" role="status" aria-live="polite" aria-atomic="true">
              {Math.round(scale * 100)}%
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleZoomIn}
              disabled={scale >= 2.0}
              aria-label="Zoom in (Plus key)"
              title="Zoom in (Plus key)"
            >
              <ZoomIn className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleDownload}
              aria-label={`Download ${documentName}`}
              title="Download PDF"
            >
              <Download className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* PDF Content */}
        <div
          className="flex-1 overflow-auto bg-slate-100 p-4 flex justify-center"
          role="document"
          aria-label={`PDF document: ${documentName}`}
          tabIndex={0}
        >
          {isLoading && (
            <div className="flex items-center justify-center h-64" role="status" aria-live="polite">
              <div className="text-muted-foreground">Loading PDF...</div>
            </div>
          )}
          {error && (
            <div className="flex items-center justify-center h-64" role="alert" aria-live="assertive">
              <div className="text-destructive">{error}</div>
            </div>
          )}
          {!error && (
            <Document
              file={src}
              onLoadSuccess={onDocumentLoadSuccess}
              onLoadError={onDocumentLoadError}
              loading={<div className="text-muted-foreground" role="status" aria-live="polite">Loading PDF...</div>}
            >
              <Page
                pageNumber={page}
                scale={scale}
                renderTextLayer={true}
                renderAnnotationLayer={true}
                aria-label={`Page ${page} of ${numPages}`}
              />
            </Document>
          )}
        </div>

        {/* Highlight indicator */}
        {highlightText && (
          <div className="p-2 border-t bg-yellow-50 text-sm flex-shrink-0" role="status" aria-live="polite">
            <span className="text-yellow-700">Highlighting: </span>
            <span className="font-medium">{highlightText}</span>
          </div>
        )}
      </div>
    );
  }
);

PDFViewerEmbedded.displayName = 'PDFViewerEmbedded';

export default PDFViewerEmbedded;
