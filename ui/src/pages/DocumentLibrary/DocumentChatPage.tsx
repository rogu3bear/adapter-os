/**
 * DocumentChatPage - Document-specific chat interface with responsive split-view
 *
 * Route: /documents/:documentId/chat
 *
 * Provides a responsive layout with:
 * - Mobile (<768px): Chat only with sheet drawer for PDF
 * - Tablet (768px-1024px): Collapsible PDF panel
 * - Desktop (>1024px): Full resizable split-view
 *
 * Accessibility features:
 * - Dynamic page title for browser tab and screen readers
 * - Skip links for keyboard navigation
 * - Focus management on page load and navigation
 * - Semantic landmarks and headings
 * - ARIA labels and live regions for status updates
 */

import React, { useEffect, useRef } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { ArrowLeft, FileText, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import DocumentChatLayout from '@/components/documents/DocumentChatLayout';
import { useDocument } from '@/hooks/documents';
import { Link } from 'react-router-dom';
import { buildTelemetryViewerLink, buildDocumentsLink } from '@/utils/navLinks';

interface DocumentChatParams {
  documentId: string;
}

export default function DocumentChatPage() {
  const { documentId } = useParams<keyof DocumentChatParams>() as DocumentChatParams;
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { data: document, isLoading, error } = useDocument(documentId);
  const mainContentRef = useRef<HTMLElement>(null);
  const headerRef = useRef<HTMLElement>(null);
  const collectionIdFromParam = searchParams.get('collectionId') || undefined;
  const mainContentId = 'document-chat-main-content';

  // Set page title dynamically
  useEffect(() => {
    if (document) {
      globalThis.document.title = `Chat: ${document.name} - AdapterOS`;
    } else {
      globalThis.document.title = 'Document Chat - AdapterOS';
    }

    return () => {
      globalThis.document.title = 'AdapterOS';
    };
  }, [document]);

  // Focus management on page load
  useEffect(() => {
    if (document && headerRef.current) {
      headerRef.current.focus();
    }
  }, [document]);

  // Handle navigation back to document library
  const handleBack = () => {
    navigate(buildDocumentsLink());
  };

  // Skip link handler
  const handleSkipToMain = (e: React.MouseEvent<HTMLAnchorElement>) => {
    e.preventDefault();
    mainContentRef.current?.focus();
  };

  if (isLoading) {
    return (
      <div className="h-full flex flex-col">
        {/* Skip link for keyboard users */}
        <a
          href={`#${mainContentId}`}
          className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded"
          onClick={handleSkipToMain}
        >
          Skip to main content
        </a>

        {/* Responsive Header - Loading */}
        <div className="p-2 sm:p-4 border-b flex items-center gap-2 sm:gap-4 bg-background">
          <Skeleton className="h-8 w-8" />
          <Skeleton className="h-6 w-32 sm:w-48" />
        </div>
        <main
          id={mainContentId}
          ref={mainContentRef}
          className="flex-1 p-4"
          role="main"
          aria-label="Document chat interface"
          aria-busy="true"
          tabIndex={-1}
        >
          <div className="h-full" role="status" aria-live="polite">
            <Skeleton className="h-full w-full" />
            <span className="sr-only">Loading document...</span>
          </div>
        </main>
      </div>
    );
  }

  if (error || !document) {
    return (
      <div className="h-full flex flex-col p-4 sm:p-6">
        {/* Skip link */}
        <a
          href={`#${mainContentId}`}
          className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded"
          onClick={handleSkipToMain}
        >
          Skip to main content
        </a>

        <main
          id={mainContentId}
          ref={mainContentRef}
          tabIndex={-1}
          className="flex-1"
          role="main"
          aria-label="Document chat interface"
        >
          <Button variant="ghost" onClick={handleBack} className="mb-4 w-fit">
            <ArrowLeft className="mr-2 h-4 w-4" />
            <span className="hidden sm:inline">Back to Documents</span>
            <span className="sm:hidden">Back</span>
          </Button>
          <Alert variant="destructive" role="alert">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Error</AlertTitle>
            <AlertDescription>{error?.message || 'Document not found'}</AlertDescription>
          </Alert>
        </main>
      </div>
    );
  }

  // Check if document is indexed
  if (document.status !== 'indexed') {
    return (
      <div className="h-full flex flex-col p-4 sm:p-6">
        {/* Skip link */}
        <a
          href={`#${mainContentId}`}
          className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded"
          onClick={handleSkipToMain}
        >
          Skip to main content
        </a>

        <main
          id={mainContentId}
          ref={mainContentRef}
          tabIndex={-1}
          className="flex-1"
          role="main"
          aria-label="Document chat interface"
        >
          <Button variant="ghost" onClick={handleBack} className="mb-4 w-fit">
            <ArrowLeft className="mr-2 h-4 w-4" />
            <span className="hidden sm:inline">Back to Documents</span>
            <span className="sm:hidden">Back</span>
          </Button>
          <Alert role="status" aria-live="polite">
            <FileText className="h-4 w-4" />
            <AlertTitle>Document Not Ready</AlertTitle>
            <AlertDescription>
              This document is currently {document.status}. Chat will be available once indexing is complete.
            </AlertDescription>
          </Alert>
        </main>
      </div>
    );
  }

  // Use collectionId from URL param, or fall back to undefined
  const collectionId = collectionIdFromParam;

  return (
    <div className="h-full flex flex-col">
      {/* Skip link for keyboard users */}
      <a
        href={`#${mainContentId}`}
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded"
        onClick={handleSkipToMain}
      >
        Skip to main content
      </a>

      {/* Responsive Header */}
      <header
        ref={headerRef}
        className="p-2 sm:p-4 border-b flex items-center gap-2 sm:gap-4 bg-background"
        tabIndex={-1}
        role="banner"
      >
        <Button
          variant="ghost"
          size="sm"
          onClick={handleBack}
          aria-label="Back to documents library"
        >
          <ArrowLeft className="mr-2 h-4 w-4" />
          <span className="hidden sm:inline">Back</span>
        </Button>

        {/* Document title - responsive truncation */}
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <FileText className="h-4 w-4 sm:h-5 sm:w-5 text-muted-foreground flex-shrink-0" aria-hidden="true" />
          <h1 className="text-base sm:text-lg font-semibold truncate">
            {document.name}
          </h1>
        </div>

        {/* Chunk count - hidden on mobile */}
        <div
          className="hidden md:block text-sm text-muted-foreground whitespace-nowrap"
          role="status"
          aria-label={`Document contains ${document.chunk_count || 0} chunks`}
        >
          {document.chunk_count ? `${document.chunk_count} chunks` : 'Processing...'}
        </div>
      </header>

      {/* Main content - responsive split-view */}
      <main
        id={mainContentId}
        ref={mainContentRef}
        className="flex-1 overflow-hidden"
        tabIndex={-1}
        role="main"
        aria-label="Document chat interface"
      >
        <DocumentChatLayout
          document={document}
          tenantId={document.tenant_id}
          collectionId={collectionId}
        />
      </main>

      <div className="p-4 border-t text-sm text-muted-foreground">
        <Link to={buildTelemetryViewerLink()} className="underline underline-offset-4">
          View telemetry for this session
        </Link>
      </div>

      {/* Keyboard shortcut help - screen reader only */}
      <div className="sr-only" role="region" aria-label="Keyboard shortcuts">
        <p>Press F6 to switch between chat and PDF panels.</p>
        <p>Use arrow keys to navigate PDF pages, plus and minus keys to zoom.</p>
        <p>Press Home or End to jump to first or last page.</p>
      </div>
    </div>
  );
}
