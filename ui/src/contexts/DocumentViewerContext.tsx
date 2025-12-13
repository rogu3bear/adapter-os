/**
 * DocumentViewerContext - Manages state for document viewing in chat interface
 *
 * Provides shared state and actions for coordinating between chat evidence
 * and the PDF viewer in split-view layouts.
 */

import { createContext, useContext, useState, useCallback, ReactNode } from 'react';

export interface HighlightCharRange {
  start: number;
  end: number;
}

export interface HighlightBBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface DocumentViewerState {
  /** Currently active document ID */
  activeDocumentId: string | null;
  /** Current page number */
  currentPage: number;
  /** Zoom scale (1.0 = 100%) */
  scale: number;
  /** Currently highlighted chunk ID */
  highlightedChunkId: string | null;
  /** Text to highlight */
  highlightText: string | null;
  /** Character range to highlight on current page */
  highlightCharRange: HighlightCharRange | null;
  /** Bounding box to highlight on current page */
  highlightBBox: HighlightBBox | null;
  /** Page number for the highlight */
  highlightPage: number | null;
}

interface DocumentViewerActions {
  /** Open a document in the viewer */
  openDocument: (documentId: string) => void;
  /** Close the current document */
  closeDocument: () => void;
  /** Navigate to a specific page */
  setPage: (page: number) => void;
  /** Set zoom scale */
  setScale: (scale: number) => void;
  /** Scroll to and highlight a specific chunk */
  scrollToChunk: (chunkId: string, text?: string, page?: number) => void;
  /** Highlight a character range on a specific page */
  highlightRange: (page: number, start: number, end: number) => void;
  /** Highlight a bounding box on a specific page */
  highlightBbox: (page: number, bbox: HighlightBBox) => void;
  /** Clear highlight */
  clearHighlight: () => void;
}

interface DocumentViewerContextValue extends DocumentViewerState, DocumentViewerActions {}

const DocumentViewerContext = createContext<DocumentViewerContextValue | null>(null);

interface DocumentViewerProviderProps {
  children: ReactNode;
  /** Initial document ID to open */
  initialDocumentId?: string;
}

export function DocumentViewerProvider({
  children,
  initialDocumentId,
}: DocumentViewerProviderProps) {
  const [state, setState] = useState<DocumentViewerState>({
    activeDocumentId: initialDocumentId ?? null,
    currentPage: 1,
    scale: 1.0,
    highlightedChunkId: null,
    highlightText: null,
    highlightCharRange: null,
    highlightBBox: null,
    highlightPage: null,
  });

  const openDocument = useCallback((documentId: string) => {
    setState((prev) => ({
      ...prev,
      activeDocumentId: documentId,
      currentPage: 1,
      highlightedChunkId: null,
      highlightText: null,
      highlightCharRange: null,
      highlightBBox: null,
      highlightPage: null,
    }));
  }, []);

  const closeDocument = useCallback(() => {
    setState((prev) => ({
      ...prev,
      activeDocumentId: null,
      currentPage: 1,
      highlightedChunkId: null,
      highlightText: null,
      highlightCharRange: null,
      highlightBBox: null,
      highlightPage: null,
    }));
  }, []);

  const setPage = useCallback((page: number) => {
    setState((prev) => ({
      ...prev,
      currentPage: page,
    }));
  }, []);

  const setScale = useCallback((scale: number) => {
    setState((prev) => ({
      ...prev,
      scale: Math.max(0.5, Math.min(3.0, scale)),
    }));
  }, []);

  const scrollToChunk = useCallback(
    (chunkId: string, text?: string, page?: number) => {
      setState((prev) => ({
        ...prev,
        highlightedChunkId: chunkId,
        highlightText: text ?? null,
        currentPage: page ?? prev.currentPage,
        highlightCharRange: null,
        highlightBBox: null,
        highlightPage: page ?? null,
      }));
    },
    []
  );

  const highlightRange = useCallback((page: number, start: number, end: number) => {
    setState((prev) => ({
      ...prev,
      highlightCharRange: { start, end },
      highlightBBox: null,
      highlightPage: page,
      currentPage: page,
    }));
  }, []);

  const highlightBbox = useCallback((page: number, bbox: HighlightBBox) => {
    setState((prev) => ({
      ...prev,
      highlightCharRange: null,
      highlightBBox: bbox,
      highlightPage: page,
      currentPage: page,
    }));
  }, []);

  const clearHighlight = useCallback(() => {
    setState((prev) => ({
      ...prev,
      highlightedChunkId: null,
      highlightText: null,
      highlightCharRange: null,
      highlightBBox: null,
      highlightPage: null,
    }));
  }, []);

  const value: DocumentViewerContextValue = {
    ...state,
    openDocument,
    closeDocument,
    setPage,
    setScale,
    scrollToChunk,
    highlightRange,
    highlightBbox,
    clearHighlight,
  };

  return (
    <DocumentViewerContext.Provider value={value}>
      {children}
    </DocumentViewerContext.Provider>
  );
}

/**
 * Hook to access document viewer context
 * @throws Error if used outside of DocumentViewerProvider
 */
export function useDocumentViewer(): DocumentViewerContextValue {
  const context = useContext(DocumentViewerContext);
  if (!context) {
    throw new Error(
      'useDocumentViewer must be used within a DocumentViewerProvider'
    );
  }
  return context;
}

/**
 * Hook to access document viewer context without throwing
 * Returns null if outside provider
 */
export function useDocumentViewerOptional(): DocumentViewerContextValue | null {
  return useContext(DocumentViewerContext);
}

export default DocumentViewerContext;
