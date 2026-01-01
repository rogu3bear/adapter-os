import React from 'react';
import { FileText, ExternalLink } from 'lucide-react';
import { Badge } from '@/components/ui/badge';

interface HighlightBBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface EvidenceItemProps {
  item: {
    documentName: string;
    pageNumber: number | null;
    textPreview: string;
    relevanceScore: number;
    reference?: string;
    metadataJson?: string | Record<string, unknown>;
  };
  /** Callback when clicking to view the evidence source */
  onView?: (
    documentId: string,
    chunkId?: string,
    pageNumber?: number,
    highlightText?: string,
    bbox?: HighlightBBox
  ) => void;
  /** Whether this evidence item is currently highlighted/active */
  isActive?: boolean;
}

/**
 * Evidence metadata structure from API
 */
interface EvidenceMetadata {
  document_id?: string;
  documentId?: string;
  chunk_id?: string;
  chunkId?: string;
  page_number?: number;
  pageNumber?: number;
  bbox?: HighlightBBox;
  bounding_box?: HighlightBBox;
}

/**
 * Base evidence item structure
 */
interface BaseEvidenceItem {
  documentName: string;
  pageNumber: number | null;
  textPreview: string;
  relevanceScore: number;
  reference?: string;
  metadataJson?: string | Record<string, unknown>;
}

/**
 * Extended evidence item with optional fields from API
 */
interface ExtendedEvidenceItem extends BaseEvidenceItem {
  document_id?: string;
  documentId?: string;
  chunk_id?: string;
  chunkId?: string;
  bbox?: HighlightBBox;
}

/**
 * Parse document metadata from evidence item to extract navigation info
 */
function parseDocumentMetadata(item: EvidenceItemProps['item']): {
  documentId?: string;
  chunkId?: string;
  pageNumber?: number;
  bbox?: HighlightBBox;
} {
  let metadata: EvidenceMetadata = {};

  // Parse metadataJson if it's a string
  if (typeof item.metadataJson === 'string') {
    try {
      metadata = JSON.parse(item.metadataJson) as EvidenceMetadata;
    } catch {
      // Failed to parse, metadata remains empty
    }
  } else if (item.metadataJson && typeof item.metadataJson === 'object') {
    metadata = item.metadataJson as EvidenceMetadata;
  }

  // Cast item to extended type to access optional fields
  const extendedItem = item as ExtendedEvidenceItem;

  // Extract IDs from metadata or top-level fields
  const documentId = metadata.document_id || metadata.documentId || extendedItem.documentId || extendedItem.document_id;
  const chunkId = metadata.chunk_id || metadata.chunkId || extendedItem.chunkId || extendedItem.chunk_id;
  const pageNumber = item.pageNumber ?? metadata.page_number ?? metadata.pageNumber;

  // Extract bounding box if available
  let bbox: HighlightBBox | undefined;
  const bboxData = metadata.bbox || metadata.bounding_box || extendedItem.bbox;
  if (bboxData && typeof bboxData === 'object') {
    // Type guard to validate bounding box structure
    if (
      typeof bboxData.x === 'number' &&
      typeof bboxData.y === 'number' &&
      typeof bboxData.width === 'number' &&
      typeof bboxData.height === 'number'
    ) {
      bbox = {
        x: bboxData.x,
        y: bboxData.y,
        width: bboxData.width,
        height: bboxData.height,
      };
    }
  }

  // Try to parse reference field if IDs not found (e.g., "doc:123:chunk:456")
  if (!documentId && item.reference) {
    const refMatch = item.reference.match(/doc:([^:]+)/);
    if (refMatch) {
      return {
        documentId: refMatch[1],
        chunkId: item.reference.match(/chunk:([^:]+)/)?.[1],
        pageNumber: typeof pageNumber === 'number' ? pageNumber : undefined,
        bbox,
      };
    }
  }

  return {
    documentId: typeof documentId === 'string' ? documentId : undefined,
    chunkId: typeof chunkId === 'string' ? chunkId : undefined,
    pageNumber: typeof pageNumber === 'number' ? pageNumber : undefined,
    bbox,
  };
}

export function EvidenceItem({ item, onView, isActive = false }: EvidenceItemProps) {
  const confidenceLevel = item.relevanceScore >= 0.8 ? 'High' :
                          item.relevanceScore >= 0.6 ? 'Medium' : 'Low';
  const confidenceColor = item.relevanceScore >= 0.8 ? 'text-green-600' :
                          item.relevanceScore >= 0.6 ? 'text-yellow-600' : 'text-red-600';

  const { documentId, chunkId, pageNumber, bbox } = parseDocumentMetadata(item);
  const hasDocumentInfo = Boolean(documentId);

  const handleClick = () => {
    if (onView && documentId) {
      onView(documentId, chunkId, pageNumber, item.textPreview, bbox);
    }
  };

  return (
    <div
      className={`p-3 rounded-lg transition-all ${
        isActive
          ? 'bg-blue-50 border-2 border-blue-400 shadow-md'
          : 'bg-slate-50 hover:bg-slate-100 border-2 border-transparent'
      } ${hasDocumentInfo ? 'cursor-pointer' : 'cursor-default'}`}
      onClick={hasDocumentInfo ? handleClick : undefined}
    >
      <div className="flex justify-between items-start">
        <div className="flex items-center gap-2">
          <FileText className={`h-4 w-4 ${isActive ? 'text-blue-600' : 'text-slate-500'}`} />
          <span className={`font-medium text-sm ${isActive ? 'text-blue-900' : 'text-slate-900'}`}>
            {item.documentName}
          </span>
          {item.pageNumber && (
            <Badge variant="secondary" className="text-xs">
              p. {item.pageNumber}
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Badge variant="outline" className={confidenceColor}>
            {confidenceLevel}
          </Badge>
          {hasDocumentInfo && (
            <ExternalLink className={`h-3 w-3 ${isActive ? 'text-blue-500' : 'text-slate-400'}`} />
          )}
        </div>
      </div>
      <p className={`mt-2 text-sm line-clamp-2 ${isActive ? 'text-slate-700' : 'text-slate-600'}`}>
        "{item.textPreview}"
      </p>
      {isActive && hasDocumentInfo && (
        <div className="mt-2 pt-2 border-t border-blue-200">
          <p className="text-xs text-blue-700 font-medium">Currently viewing this source</p>
        </div>
      )}
    </div>
  );
}
