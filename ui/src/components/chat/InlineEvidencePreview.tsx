/**
 * InlineEvidencePreview - Compact inline preview of top citations
 *
 * Shows 1-3 top citations directly under the message,
 * with a "View all" link to open the evidence drawer.
 */

import { FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { EvidenceItem } from './ChatMessage';

interface InlineEvidencePreviewProps {
  /** Message ID for drawer context */
  messageId: string;
  /** Evidence items to preview */
  evidence: EvidenceItem[];
  /** Maximum number of items to show inline (default: 3) */
  maxItems?: number;
  /** Callback to open the full evidence drawer */
  onViewAll: () => void;
  /** Additional className */
  className?: string;
}

export function InlineEvidencePreview({
  messageId,
  evidence,
  maxItems = 3,
  onViewAll,
  className,
}: InlineEvidencePreviewProps) {
  if (!evidence || evidence.length === 0) {
    return null;
  }

  // Sort by relevance and take top items
  const sortedEvidence = [...evidence].sort(
    (a, b) => b.relevance_score - a.relevance_score
  );
  const topEvidence = sortedEvidence.slice(0, maxItems);
  const remainingCount = evidence.length - maxItems;

  return (
    <div
      className={cn('mt-2 space-y-1', className)}
      data-testid={`inline-evidence-preview-${messageId}`}
    >
      {topEvidence.map((item, index) => (
        <div
          key={`${item.chunk_id}-${index}`}
          className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
          onClick={onViewAll}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              onViewAll();
            }
          }}
        >
          <FileText className="h-3 w-3 flex-shrink-0" />
          <span className="truncate max-w-[200px]">{item.document_name}</span>
          {item.page_number && (
            <span className="text-muted-foreground/70 flex-shrink-0">
              p.{item.page_number}
            </span>
          )}
          <span
            className={cn(
              'text-[10px] px-1 py-0.5 rounded flex-shrink-0',
              item.relevance_score >= 0.8
                ? 'bg-green-100 text-green-700'
                : item.relevance_score >= 0.6
                  ? 'bg-yellow-100 text-yellow-700'
                  : 'bg-red-100 text-red-700'
            )}
          >
            {Math.round(item.relevance_score * 100)}%
          </span>
        </div>
      ))}

      {remainingCount > 0 && (
        <Button
          variant="link"
          size="sm"
          onClick={onViewAll}
          className="h-auto p-0 text-xs text-muted-foreground hover:text-primary"
          data-testid="view-all-evidence"
        >
          View all {evidence.length} sources
        </Button>
      )}
    </div>
  );
}

export default InlineEvidencePreview;
