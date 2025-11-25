/**
 * DocumentViewerContext - Manages state for document viewing in chat interface
 *
 * Provides shared state and actions for coordinating between chat evidence
 * and the PDF viewer in split-view layouts.
 */

import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';

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
  });

  const openDocument = useCallback((documentId: string) => {
    setState((prev) => ({
      ...prev,
      activeDocumentId: documentId,
      currentPage: 1,
      highlightedChunkId: null,
      highlightText: null,
    }));
  }, []);

  const closeDocument = useCallback(() => {
    setState((prev) => ({
      ...prev,
      activeDocumentId: null,
      currentPage: 1,
      highlightedChunkId: null,
      highlightText: null,
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
      }));
    },
    []
  );

  const clearHighlight = useCallback(() => {
    setState((prev) => ({
      ...prev,
      highlightedChunkId: null,
      highlightText: null,
    }));
  }, []);

  const value: DocumentViewerContextValue = {
    ...state,
    openDocument,
    closeDocument,
    setPage,
    setScale,
    scrollToChunk,
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
