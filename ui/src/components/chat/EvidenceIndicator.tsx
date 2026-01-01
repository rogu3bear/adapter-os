/**
 * EvidenceIndicator - Consolidated compact evidence indicator
 *
 * Combines InlineEvidencePreview, EvidenceDrawerTrigger, and ProofBadge
 * into a single compact badge for use in compact mode or list views.
 *
 * Shows: "[N sources] [✓ verified]" - click to open evidence drawer
 */

import { FileText, ShieldCheck } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { useEvidenceDrawer } from '@/contexts/EvidenceDrawerContext';
import type { EvidenceItem } from './ChatMessage';
import type { ExtendedRouterDecision } from '@/api/types';

export interface EvidenceIndicatorProps {
  /** Message ID for drawer context */
  messageId: string;
  /** Evidence items for this message */
  evidence?: EvidenceItem[];
  /** Router decision for this message */
  routerDecision?: ExtendedRouterDecision | null;
  /** Request ID for trace lookup */
  requestId?: string;
  /** Trace ID if available */
  traceId?: string;
  /** Proof digest if available */
  proofDigest?: string;
  /** Whether the response is verified */
  isVerified?: boolean;
  /** When the response was verified */
  verifiedAt?: string;
  /** Token throughput statistics */
  throughputStats?: {
    tokensGenerated: number;
    latencyMs: number;
    tokensPerSecond: number;
  };
  /** Additional className */
  className?: string;
}

/**
 * Compact evidence indicator that combines source count and verification status
 * into a single clickable badge that opens the evidence drawer.
 */
export function EvidenceIndicator({
  messageId,
  evidence,
  routerDecision,
  requestId,
  traceId,
  proofDigest,
  isVerified,
  verifiedAt,
  throughputStats,
  className,
}: EvidenceIndicatorProps) {
  const { openDrawer, setMessageData, pinToMessage } = useEvidenceDrawer();

  const hasEvidence = evidence && evidence.length > 0;
  const hasProof = Boolean(requestId || traceId || proofDigest);
  const evidenceCount = evidence?.length ?? 0;

  // Calculate average relevance score for color coding
  const avgRelevance =
    hasEvidence && evidence
      ? evidence.reduce((sum, e) => sum + e.relevanceScore, 0) / evidence.length
      : 0;

  const handleClick = () => {
    setMessageData({
      evidence,
      routerDecision: routerDecision ?? undefined,
      requestId,
      traceId,
      proofDigest,
      isVerified,
      verifiedAt,
      throughputStats,
    });
    pinToMessage(messageId);
    // Open to rulebook tab if evidence exists, otherwise calculation
    openDrawer(messageId, hasEvidence ? 'rulebook' : 'calculation');
  };

  // Don't render if no evidence and no proof info
  if (!hasEvidence && !hasProof) {
    return null;
  }

  // Build tooltip content
  const tooltipContent = [];
  if (hasEvidence) {
    tooltipContent.push(
      `${evidenceCount} source${evidenceCount !== 1 ? 's' : ''}`
    );
    tooltipContent.push(`(avg ${Math.round(avgRelevance * 100)}% relevance)`);
  }
  if (isVerified && verifiedAt) {
    tooltipContent.push(
      `Verified at ${new Date(verifiedAt).toLocaleString()}`
    );
  } else if (isVerified) {
    tooltipContent.push('Verified');
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge
          variant="secondary"
          className={cn(
            'cursor-pointer hover:bg-secondary/80 transition-colors gap-1.5 h-6 px-2',
            className
          )}
          onClick={handleClick}
          data-testid={`evidence-indicator-${messageId}`}
        >
          {/* Evidence count */}
          {hasEvidence && (
            <span className="flex items-center gap-1">
              <FileText className="h-3 w-3" />
              <span
                className={cn(
                  'text-xs font-medium',
                  avgRelevance >= 0.8
                    ? 'text-green-700'
                    : avgRelevance >= 0.6
                      ? 'text-yellow-700'
                      : 'text-muted-foreground'
                )}
              >
                {evidenceCount}
              </span>
            </span>
          )}

          {/* Separator when both are present */}
          {hasEvidence && isVerified && (
            <span className="text-muted-foreground/50">|</span>
          )}

          {/* Verification status */}
          {isVerified && (
            <ShieldCheck className="h-3 w-3 text-green-600" />
          )}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        <p>{tooltipContent.join(' • ')}</p>
        <p className="text-xs text-muted-foreground mt-1">
          Click to view details
        </p>
      </TooltipContent>
    </Tooltip>
  );
}

export default EvidenceIndicator;
