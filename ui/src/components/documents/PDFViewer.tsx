import React, { useState } from 'react';
import { Document, Page, pdfjs } from 'react-pdf';
import { ChevronLeft, ChevronRight, Download, ZoomIn, ZoomOut } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import 'react-pdf/dist/Page/AnnotationLayer.css';
import 'react-pdf/dist/Page/TextLayer.css';

// Set worker path for pdf.js
pdfjs.GlobalWorkerOptions.workerSrc = `//cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjs.version}/pdf.worker.min.js`;

interface Props {
  documentId: string;
  documentName: string;
  initialPage?: number;
  isOpen: boolean;
  onClose: () => void;
}

export function PDFViewer({ documentId, documentName, initialPage = 1, isOpen, onClose }: Props) {
  const [numPages, setNumPages] = useState<number>(0);
  const [currentPage, setCurrentPage] = useState(initialPage);
  const [scale, setScale] = useState(1.0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  const pdfUrl = `/api/v1/documents/${documentId}/download`;

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader className="flex-shrink-0">
          <DialogTitle className="flex justify-between items-center">
            <span className="truncate max-w-md">{documentName}</span>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon"
                onClick={() => setScale(s => Math.max(0.5, s - 0.1))}
                disabled={scale <= 0.5}
              >
                <ZoomOut className="h-4 w-4" />
              </Button>
              <span className="text-sm min-w-[3rem] text-center">{Math.round(scale * 100)}%</span>
              <Button
                variant="outline"
                size="icon"
                onClick={() => setScale(s => Math.min(2, s + 0.1))}
                disabled={scale >= 2}
              >
                <ZoomIn className="h-4 w-4" />
              </Button>
              <a href={pdfUrl} download>
                <Button variant="outline" size="icon">
                  <Download className="h-4 w-4" />
                </Button>
              </a>
            </div>
          </DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-auto bg-slate-100 p-4 flex justify-center">
          {isLoading && (
            <div className="flex items-center justify-center h-64">
              <div className="text-muted-foreground">Loading PDF...</div>
            </div>
          )}
          {error && (
            <div className="flex items-center justify-center h-64">
              <div className="text-destructive">{error}</div>
            </div>
          )}
          {!error && (
            <Document
              file={pdfUrl}
              onLoadSuccess={onDocumentLoadSuccess}
              onLoadError={onDocumentLoadError}
              loading={<div className="text-muted-foreground">Loading PDF...</div>}
            >
              <Page
                pageNumber={currentPage}
                scale={scale}
                renderTextLayer={true}
                renderAnnotationLayer={true}
              />
            </Document>
          )}
        </div>

        {numPages > 0 && (
          <div className="flex-shrink-0 flex justify-center items-center gap-4 py-2 border-t">
            <Button
              variant="outline"
              size="icon"
              disabled={currentPage <= 1}
              onClick={() => setCurrentPage(p => p - 1)}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="text-sm">
              Page {currentPage} of {numPages}
            </span>
            <Button
              variant="outline"
              size="icon"
              disabled={currentPage >= numPages}
              onClick={() => setCurrentPage(p => p + 1)}
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
