import React, { useState } from 'react';
import { FileText, ChevronDown, ChevronUp, CheckCircle, Download } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { EvidenceItem } from './EvidenceItem';
import { useDocumentViewerOptional } from '@/contexts/DocumentViewerContext';

interface EvidenceItemData {
  documentId: string;
  documentName: string;
  chunkId: string;
  pageNumber: number | null;
  textPreview: string;
  relevanceScore: number;
  rank: number;
}

interface Props {
  evidence: EvidenceItemData[];
  isVerified: boolean;
  verifiedAt?: string;
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
}

export function EvidenceSources({ evidence, isVerified, verifiedAt, onViewDocument }: Props) {
  const [isOpen, setIsOpen] = useState(false);
  const [activeEvidenceId, setActiveEvidenceId] = useState<string | null>(null);

  // Try to get document viewer context (may be null if not in provider)
  const viewer = useDocumentViewerOptional();

  if (evidence.length === 0) {
    return null;
  }

  /**
   * Handle viewing an evidence item - coordinates with document viewer if available
   */
  const handleViewEvidence = (
    documentId: string,
    chunkId?: string,
    pageNumber?: number,
    highlightText?: string
  ) => {
    // Track which evidence item is active
    setActiveEvidenceId(chunkId || documentId);

    // If we have a document viewer context, use it for coordinated navigation
    if (viewer) {
      if (chunkId) {
        // Scroll to specific chunk with optional page number
        viewer.scrollToChunk(chunkId, undefined, pageNumber);
      } else if (pageNumber) {
        // Just navigate to page if no chunk ID
        viewer.setPage(pageNumber);
      }

      // Ensure the document is open in the viewer
      if (viewer.activeDocumentId !== documentId) {
        viewer.openDocument(documentId);
      }
    }

    // Also call the parent's onViewDocument callback if provided
    onViewDocument?.(documentId, pageNumber, highlightText);
  };

  /**
   * Export evidence sources as JSON file
   */
  const handleExportSources = () => {
    const exportData = {
      exported_at: new Date().toISOString(),
      is_verified: isVerified,
      verified_at: verifiedAt,
      sources: evidence.map((item) => ({
        document_id: item.documentId,
        document_name: item.documentName,
        chunk_id: item.chunkId,
        page_number: item.pageNumber,
        text_preview: item.textPreview,
        relevance_score: item.relevanceScore,
        rank: item.rank,
      })),
    };

    const blob = new Blob([JSON.stringify(exportData, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `evidence-sources-${new Date().toISOString().slice(0, 10)}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  /**
   * Export evidence sources as formatted text file
   */
  const handleExportSourcesText = () => {
    let textContent = `Evidence Sources Report\n`;
    textContent += `Generated: ${new Date().toLocaleString()}\n`;
    textContent += `Verified: ${isVerified ? 'Yes' : 'No'}\n`;
    if (verifiedAt) {
      textContent += `Verified At: ${new Date(verifiedAt).toLocaleString()}\n`;
    }
    textContent += `\n${'='.repeat(80)}\n\n`;

    evidence.forEach((item, index) => {
      textContent += `Source #${index + 1} (Rank ${item.rank})\n`;
      textContent += `Document: ${item.documentName}\n`;
      if (item.pageNumber) {
        textContent += `Page: ${item.pageNumber}\n`;
      }
      textContent += `Relevance Score: ${(item.relevanceScore * 100).toFixed(1)}%\n`;
      textContent += `Preview: "${item.textPreview}"\n`;
      textContent += `\n${'-'.repeat(80)}\n\n`;
    });

    const blob = new Blob([textContent], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `evidence-sources-${new Date().toISOString().slice(0, 10)}.txt`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen} className="mt-2">
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="w-full justify-between">
          <div className="flex items-center gap-2">
            <FileText className="h-4 w-4" />
            <span>Sources ({evidence.length})</span>
            {isVerified && (
              <Badge variant="outline" className="text-green-600 border-green-200">
                <CheckCircle className="h-3 w-3 mr-1" />
                Verified
              </Badge>
            )}
          </div>
          {isOpen ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="mt-2 space-y-2">
        {evidence.map((item) => (
          <EvidenceItem
            key={item.chunkId}
            item={item}
            onView={handleViewEvidence}
            isActive={activeEvidenceId === (item.chunkId || item.documentId)}
          />
        ))}
        {verifiedAt && (
          <div className="text-xs text-muted-foreground mt-2">
            Verified at {new Date(verifiedAt).toLocaleString()}
          </div>
        )}
        <div className="flex gap-2 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleExportSources}
            className="flex items-center gap-1"
          >
            <Download className="h-3 w-3" />
            Export JSON
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleExportSourcesText}
            className="flex items-center gap-1"
          >
            <Download className="h-3 w-3" />
            Export Text
          </Button>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
