/**
 * EvidenceDrawerTrigger - Per-message trigger for opening the evidence drawer
 *
 * Displays an evidence icon (opens Rulebook tab) and a proof badge (opens Calculation tab).
 */

import { FileText, ShieldCheck, Activity } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { cn } from '@/components/ui/utils';
import { useEvidenceDrawer } from '@/contexts/EvidenceDrawerContext';
import type { EvidenceItem } from './ChatMessage';
import type { ExtendedRouterDecision } from '@/api/types';

interface EvidenceDrawerTriggerProps {
  /** Message ID to associate with this trigger */
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
  throughputStats?: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number };
}

export function EvidenceDrawerTrigger({
  messageId,
  evidence,
  routerDecision,
  requestId,
  traceId,
  proofDigest,
  isVerified,
  verifiedAt,
  throughputStats,
}: EvidenceDrawerTriggerProps) {
  const { openDrawer, setMessageData, pinToMessage } = useEvidenceDrawer();

  const hasEvidence = evidence && evidence.length > 0;
  const hasProof = Boolean(requestId || traceId || proofDigest);

  const handleOpenRulebook = () => {
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
    openDrawer(messageId, 'rulebook');
  };

  const handleOpenCalculation = () => {
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
    openDrawer(messageId, 'calculation');
  };

  const handleOpenTrace = () => {
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
    openDrawer(messageId, 'trace');
  };

  // Don't render if no evidence and no proof info
  if (!hasEvidence && !hasProof) {
    return null;
  }

  return (
    <div className="flex items-center gap-1">
      {/* Evidence/Rulebook trigger */}
      {hasEvidence && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleOpenRulebook}
              className="h-7 px-2 gap-1"
              data-testid="evidence-drawer-trigger-rulebook"
            >
              <FileText className="h-4 w-4 text-muted-foreground" />
              <Badge variant="secondary" className="h-5 px-1.5 text-xs">
                {evidence.length}
              </Badge>
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            <p>View {evidence.length} source{evidence.length !== 1 ? 's' : ''}</p>
          </TooltipContent>
        </Tooltip>
      )}

      {/* Proof/Calculation trigger */}
      {hasProof && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleOpenCalculation}
              className="h-7 px-2"
              data-testid="evidence-drawer-trigger-calculation"
            >
              <ShieldCheck
                className={cn(
                  'h-4 w-4',
                  isVerified ? 'text-green-600' : 'text-muted-foreground'
                )}
              />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            <p>
              {isVerified
                ? `Verified${verifiedAt ? ` at ${new Date(verifiedAt).toLocaleString()}` : ''}`
                : 'View proof details'}
            </p>
          </TooltipContent>
        </Tooltip>
      )}

      {/* Trace trigger */}
      {traceId && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleOpenTrace}
              className="h-7 px-2"
              data-testid="evidence-drawer-trigger-trace"
            >
              <Activity className="h-4 w-4 text-muted-foreground" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            <p>View trace</p>
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  );
}

export default EvidenceDrawerTrigger;
