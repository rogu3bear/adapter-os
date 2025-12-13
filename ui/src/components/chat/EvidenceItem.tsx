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
    document_name: string;
    page_number: number | null;
    text_preview: string;
    relevance_score: number;
    reference?: string;
    metadata_json?: string | Record<string, unknown>;
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
 * Parse document metadata from evidence item to extract navigation info
 */
function parseDocumentMetadata(item: EvidenceItemProps['item']): {
  documentId?: string;
  chunkId?: string;
  pageNumber?: number;
  bbox?: HighlightBBox;
} {
  let metadata: Record<string, unknown> = {};

  // Parse metadata_json if it's a string
  if (typeof item.metadata_json === 'string') {
    try {
      metadata = JSON.parse(item.metadata_json);
    } catch {
      // Failed to parse, metadata remains empty
    }
  } else if (item.metadata_json) {
    metadata = item.metadata_json;
  }

  // Extract IDs from metadata or top-level fields
  const documentId = (metadata.document_id || metadata.documentId || (item as any).document_id) as
    | string
    | undefined;
  const chunkId = (metadata.chunk_id || metadata.chunkId || (item as any).chunk_id) as string | undefined;
  const pageNumber = item.page_number ?? (metadata.page_number as number | undefined) ?? (metadata.pageNumber as number | undefined);

  // Extract bounding box if available
  let bbox: HighlightBBox | undefined;
  const bboxData = metadata.bbox || metadata.bounding_box || (item as any).bbox;
  if (bboxData && typeof bboxData === 'object') {
    const bboxObj = bboxData as any;
    if (
      typeof bboxObj.x === 'number' &&
      typeof bboxObj.y === 'number' &&
      typeof bboxObj.width === 'number' &&
      typeof bboxObj.height === 'number'
    ) {
      bbox = {
        x: bboxObj.x,
        y: bboxObj.y,
        width: bboxObj.width,
        height: bboxObj.height,
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
  const confidenceLevel = item.relevance_score >= 0.8 ? 'High' :
                          item.relevance_score >= 0.6 ? 'Medium' : 'Low';
  const confidenceColor = item.relevance_score >= 0.8 ? 'text-green-600' :
                          item.relevance_score >= 0.6 ? 'text-yellow-600' : 'text-red-600';

  const { documentId, chunkId, pageNumber, bbox } = parseDocumentMetadata(item);
  const hasDocumentInfo = Boolean(documentId);

  const handleClick = () => {
    if (onView && documentId) {
      onView(documentId, chunkId, pageNumber, item.text_preview, bbox);
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
            {item.document_name}
          </span>
          {item.page_number && (
            <Badge variant="secondary" className="text-xs">
              p. {item.page_number}
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
        "{item.text_preview}"
      </p>
      {isActive && hasDocumentInfo && (
        <div className="mt-2 pt-2 border-t border-blue-200">
          <p className="text-xs text-blue-700 font-medium">Currently viewing this source</p>
        </div>
      )}
    </div>
  );
}
