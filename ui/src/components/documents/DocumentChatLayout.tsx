/**
 * DocumentChatLayout - Responsive split-view layout for document chat
 *
 * Responsive breakpoints:
 * - Mobile (<768px): Chat-only with PDF in sheet drawer
 * - Tablet (768px-1024px): Collapsible PDF panel
 * - Desktop (>1024px): Full resizable split-view
 *
 * Panel sizes persist to localStorage for user preference.
 *
 * Accessibility features:
 * - ARIA labels and landmarks for screen readers
 * - Keyboard navigation (F6) to switch between panels
 * - Focus management for panel transitions
 */

import React, { useRef, useEffect, useState } from 'react';
import { FileText, X, ChevronLeft, ChevronRight } from 'lucide-react';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '@/components/ui/resizable';
import { Button } from '@/components/ui/button';
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { ChatInterface } from '@/components/ChatInterface';
import PDFViewerEmbedded, { PDFViewerEmbeddedRef } from './PDFViewerEmbedded';
import { DocumentViewerProvider, useDocumentViewer } from '@/contexts/DocumentViewerContext';
import { useDocumentsApi } from '@/hooks/useDocumentsApi';
import type { Document } from '@/api/document-types';
import { logger, toError } from '@/utils/logger';

const STORAGE_KEY = 'document-chat-panel-sizes';
const STORAGE_KEY_COLLAPSED = 'document-chat-pdf-collapsed';

interface DocumentChatLayoutProps {
  /** Document to display */
  document: Document;
  /** Collection ID if chatting within a collection context */
  collectionId?: string;
  /** Tenant ID for chat context */
  tenantId: string;
  /** Optional initial adapter stack ID */
  initialStackId?: string;
}

function DocumentChatLayoutInner({
  document,
  collectionId,
  tenantId,
  initialStackId,
}: DocumentChatLayoutProps) {
  const pdfViewerRef = useRef<PDFViewerEmbeddedRef>(null);
  const chatPanelRef = useRef<HTMLDivElement>(null);
  const pdfPanelRef = useRef<HTMLDivElement>(null);
  const { downloadDocument } = useDocumentsApi();
  const { currentPage, highlightText, setPage } = useDocumentViewer();
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [activePanelIndex, setActivePanelIndex] = useState<0 | 1>(0); // 0 = chat, 1 = pdf

  // Mobile sheet state
  const [mobileSheetOpen, setMobileSheetOpen] = useState(false);

  // Tablet/Desktop collapsed state
  const [isPdfCollapsed, setIsPdfCollapsed] = useState(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY_COLLAPSED);
      return stored === 'true';
    } catch {
      return false;
    }
  });

  // Load default panel sizes from localStorage
  const [defaultSizes, setDefaultSizes] = useState<number[]>(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      return stored ? JSON.parse(stored) : [50, 50];
    } catch {
      return [50, 50];
    }
  });

  // Save panel sizes to localStorage
  const handleLayoutChange = (sizes: number[]) => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(sizes));
  };

  // Save collapsed state to localStorage
  const togglePdfPanel = () => {
    const newCollapsed = !isPdfCollapsed;
    setIsPdfCollapsed(newCollapsed);
    localStorage.setItem(STORAGE_KEY_COLLAPSED, String(newCollapsed));
  };

  // Fetch PDF blob for viewing
  useEffect(() => {
    let mounted = true;

    async function fetchPdf() {
      try {
        const blob = await downloadDocument(document.document_id);
        if (mounted) {
          const url = URL.createObjectURL(blob);
          setPdfUrl(url);
        }
      } catch (error) {
        logger.error('Document chat PDF fetch failed', {
          component: 'DocumentChatLayout',
          operation: 'fetchPDF',
          errorType: 'pdf_fetch_failure',
          details: 'Failed to fetch PDF document for chat context',
          documentId: document.document_id
        }, toError(error));
      }
    }

    if (document.mime_type === 'application/pdf') {
      fetchPdf();
    }

    return () => {
      mounted = false;
      if (pdfUrl) {
        URL.revokeObjectURL(pdfUrl);
      }
    };
  }, [document.document_id, document.mime_type, downloadDocument, pdfUrl]);

  // Handle evidence navigation from chat
  const handleViewDocument = (
    documentId: string,
    pageNumber?: number,
    highlight?: string
  ) => {
    if (documentId === document.document_id && pdfViewerRef.current) {
      if (pageNumber) {
        pdfViewerRef.current.goToPage(pageNumber);
      }
      if (highlight) {
        pdfViewerRef.current.scrollToText(highlight);
      }

      // On mobile, open the sheet when navigating to document
      if (window.innerWidth < 768) {
        setMobileSheetOpen(true);
      } else {
        // On desktop, switch focus to PDF panel
        setActivePanelIndex(1);
        pdfPanelRef.current?.focus();
      }
    }
  };

  // Keyboard navigation: F6 to switch between panels (desktop only)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle F6 on desktop layout
      if (window.innerWidth < 1024) return;

      if (e.key === 'F6') {
        e.preventDefault();
        const newIndex = activePanelIndex === 0 ? 1 : 0;
        setActivePanelIndex(newIndex as 0 | 1);

        // Focus the newly active panel
        const targetRef = newIndex === 0 ? chatPanelRef : pdfPanelRef;
        targetRef.current?.focus();

        // Announce panel switch to screen readers
        const panelName = newIndex === 0 ? 'Chat' : 'PDF Viewer';
        const announcement = window.document.createElement('div');
        announcement.setAttribute('role', 'status');
        announcement.setAttribute('aria-live', 'polite');
        announcement.className = 'sr-only';
        announcement.textContent = `Switched to ${panelName} panel`;
        window.document.body.appendChild(announcement);
        setTimeout(() => announcement.remove(), 1000);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
    // Note: 'document' in dependency warning refers to global DOM object, not the component prop
  }, [activePanelIndex]);

  // PDF Viewer Component
  const PDFViewer = () => (
    pdfUrl ? (
      <PDFViewerEmbedded
        ref={pdfViewerRef}
        src={pdfUrl}
        currentPage={currentPage}
        onPageChange={setPage}
        highlightText={highlightText ?? undefined}
        documentName={document.name}
      />
    ) : (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        {document.mime_type === 'application/pdf'
          ? 'Loading PDF...'
          : 'PDF preview not available for this file type'}
      </div>
    )
  );

  return (
    <>
      {/* Mobile Layout (<768px): Chat only with sheet for PDF */}
      <div className="h-full md:hidden">
        <div className="h-full relative" role="region" aria-label="Chat conversation">
          <ChatInterface
            selectedTenant={tenantId}
            initialStackId={initialStackId}
            documentContext={{
              documentId: document.document_id,
              documentName: document.name,
              collectionId,
            }}
            onViewDocument={handleViewDocument}
          />

          {/* Floating button to open PDF */}
          {document.mime_type === 'application/pdf' && (
            <Button
              onClick={() => setMobileSheetOpen(true)}
              className="absolute bottom-4 right-4 shadow-lg"
              size="lg"
              aria-label={`View PDF: ${document.name}`}
            >
              <FileText className="mr-2 h-5 w-5" aria-hidden="true" />
              View PDF
            </Button>
          )}
        </div>

        {/* Mobile Sheet for PDF */}
        <Sheet open={mobileSheetOpen} onOpenChange={setMobileSheetOpen}>
          <SheetContent
            side="right"
            className="w-full p-0"
            aria-label={`PDF viewer: ${document.name}`}
          >
            <SheetHeader className="p-4 border-b">
              <SheetTitle className="flex items-center gap-2">
                <FileText className="h-5 w-5" aria-hidden="true" />
                {document.name}
              </SheetTitle>
            </SheetHeader>
            <div className="h-[calc(100%-5rem)]">
              <PDFViewer />
            </div>
          </SheetContent>
        </Sheet>
      </div>

      {/* Tablet Layout (768px-1024px): Collapsible panel */}
      <div className="h-full hidden md:flex lg:hidden">
        <ResizablePanelGroup
          direction="horizontal"
          onLayout={handleLayoutChange}
          className="h-full"
          role="group"
          aria-label="Document chat and viewer layout"
        >
          {/* Chat Panel */}
          <ResizablePanel
            defaultSize={isPdfCollapsed ? 100 : defaultSizes[0]}
            minSize={30}
          >
            <div
              className="h-full relative"
              role="region"
              aria-label="Chat conversation panel"
            >
              <ChatInterface
                selectedTenant={tenantId}
                initialStackId={initialStackId}
                documentContext={{
                  documentId: document.document_id,
                  documentName: document.name,
                  collectionId,
                }}
                onViewDocument={handleViewDocument}
              />

              {/* Toggle button for tablet */}
              {!isPdfCollapsed && document.mime_type === 'application/pdf' && (
                <Button
                  onClick={togglePdfPanel}
                  variant="outline"
                  size="sm"
                  className="absolute top-4 right-4 shadow-sm z-10"
                  aria-label="Hide PDF panel"
                >
                  <ChevronRight className="h-4 w-4" aria-hidden="true" />
                </Button>
              )}
            </div>
          </ResizablePanel>

          {!isPdfCollapsed && (
            <>
              <ResizableHandle
                withHandle
                aria-label="Resize panels. Use arrow keys to adjust."
              />

              {/* PDF Viewer Panel */}
              <ResizablePanel defaultSize={defaultSizes[1]} minSize={20}>
                <div
                  className="h-full relative"
                  role="region"
                  aria-label={`PDF viewer: ${document.name}`}
                >
                  <Button
                    onClick={togglePdfPanel}
                    variant="ghost"
                    size="sm"
                    className="absolute top-2 left-2 z-10"
                    aria-label="Hide PDF panel"
                  >
                    <X className="h-4 w-4" aria-hidden="true" />
                  </Button>
                  <PDFViewer />
                </div>
              </ResizablePanel>
            </>
          )}

          {/* Show PDF button when collapsed */}
          {isPdfCollapsed && document.mime_type === 'application/pdf' && (
            <Button
              onClick={togglePdfPanel}
              variant="outline"
              size="sm"
              className="absolute top-4 right-4 shadow-sm"
              aria-label="Show PDF panel"
            >
              <ChevronLeft className="h-4 w-4 mr-2" aria-hidden="true" />
              Show PDF
            </Button>
          )}
        </ResizablePanelGroup>
      </div>

      {/* Desktop Layout (>1024px): Full split-view */}
      <div className="h-full hidden lg:block">
        <ResizablePanelGroup
          direction="horizontal"
          onLayout={handleLayoutChange}
          className="h-full"
          role="group"
          aria-label="Document chat and viewer layout. Press F6 to switch between panels."
        >
          {/* Chat Panel */}
          <ResizablePanel defaultSize={defaultSizes[0]} minSize={30}>
            <div
              ref={chatPanelRef}
              role="region"
              aria-label="Chat conversation panel"
              tabIndex={-1}
              className="h-full focus:outline-hidden focus:ring-2 focus:ring-blue-500 focus:ring-inset"
              data-panel-active={activePanelIndex === 0}
            >
              <ChatInterface
                selectedTenant={tenantId}
                initialStackId={initialStackId}
                documentContext={{
                  documentId: document.document_id,
                  documentName: document.name,
                  collectionId,
                }}
                onViewDocument={handleViewDocument}
              />
            </div>
          </ResizablePanel>

          <ResizableHandle
            withHandle
            aria-label="Resize panels. Use arrow keys to adjust."
          />

          {/* PDF Viewer Panel */}
          <ResizablePanel defaultSize={defaultSizes[1]} minSize={20}>
            <div
              ref={pdfPanelRef}
              role="region"
              aria-label={`PDF viewer: ${document.name}`}
              tabIndex={-1}
              className="h-full focus:outline-hidden focus:ring-2 focus:ring-blue-500 focus:ring-inset"
              data-panel-active={activePanelIndex === 1}
            >
              <PDFViewer />
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </>
  );
}

/**
 * DocumentChatLayout with context provider
 */
export default function DocumentChatLayout(props: DocumentChatLayoutProps) {
  return (
    <DocumentViewerProvider initialDocumentId={props.document.document_id}>
      <DocumentChatLayoutInner {...props} />
    </DocumentViewerProvider>
  );
}
