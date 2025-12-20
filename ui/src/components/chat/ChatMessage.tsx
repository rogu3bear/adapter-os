import React, { memo, useState, useCallback } from 'react';
import { Download, FileText, FileJson, Package } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { RouterIndicator } from './RouterIndicator';
import { EvidenceSources } from './EvidenceSources';
import { ProofBadge } from './ProofBadge';
import { ReplayButton } from './ReplayButton';
import { ReplayResultDialog } from './ReplayResultDialog';
import { MissingPinnedAdaptersWarning } from './MissingPinnedAdaptersWarning';
import { InlineEvidencePreview } from './InlineEvidencePreview';
import { EvidenceDrawerTrigger } from './EvidenceDrawerTrigger';
import { useEvidenceDrawerOptional } from '@/contexts/EvidenceDrawerContext';
import {
  renderSingleAnswerMarkdown,
  downloadTextFile,
  generateEvidenceBundle,
  downloadEvidenceBundle,
  type ExtendedMessageExport,
} from '@/utils/export';
import type { ExtendedRouterDecision } from '@/api/types';
import type { ReplayResponse } from '@/api/replay-types';
import type { EvidenceItem, ThroughputStats, ChatMessage, ChatMessageProps } from '@/types/components';

// Re-export types for backward compatibility
export type { EvidenceItem, ThroughputStats, ChatMessage };

// Custom comparison function for memo to prevent unnecessary re-renders
function areMessagesEqual(prevProps: ChatMessageProps, nextProps: ChatMessageProps): boolean {
  const prev = prevProps.message;
  const next = nextProps.message;

  // Normalize null/undefined for boolean comparison
  const prevVerified = prev.isVerified === true;
  const nextVerified = next.isVerified === true;

  // Compare all fields that affect rendering
  return !!(
    prev.id === next.id &&
    prev.role === next.role &&
    prev.content === next.content &&
    prev.isStreaming === next.isStreaming &&
    prev.timestamp.getTime() === next.timestamp.getTime() &&
    prev.requestId === next.requestId &&
    prevVerified === nextVerified &&
    prev.verifiedAt === next.verifiedAt &&
    prev.pinnedRoutingFallback === next.pinnedRoutingFallback &&
    // Deep compare router decision
    (prev.routerDecision === next.routerDecision ||
     (!!prev.routerDecision && !!next.routerDecision &&
      prev.routerDecision.request_id === next.routerDecision.request_id &&
      JSON.stringify(prev.routerDecision.selected_adapters) === JSON.stringify(next.routerDecision.selected_adapters))) &&
    // Deep compare evidence
    (prev.evidence === next.evidence ||
     (!!prev.evidence && !!next.evidence &&
      prev.evidence.length === next.evidence.length &&
      prev.evidence.every((e, i) => e.chunk_id === next.evidence?.[i]?.chunk_id))) &&
    // Deep compare unavailable pinned adapters
    (prev.unavailablePinnedAdapters === next.unavailablePinnedAdapters ||
     (!!prev.unavailablePinnedAdapters && !!next.unavailablePinnedAdapters &&
      prev.unavailablePinnedAdapters.length === next.unavailablePinnedAdapters.length &&
      prev.unavailablePinnedAdapters.every((a, i) => a === next.unavailablePinnedAdapters?.[i]))) &&
    // Compare throughput stats
    (prev.throughputStats === next.throughputStats ||
     (!!prev.throughputStats && !!next.throughputStats &&
      prev.throughputStats.tokensGenerated === next.throughputStats.tokensGenerated &&
      prev.throughputStats.latencyMs === next.throughputStats.latencyMs)) &&
    prevProps.className === nextProps.className &&
    prevProps.isSelected === nextProps.isSelected &&
    prevProps.onSelect === nextProps.onSelect
  );
}

export const ChatMessageComponent = memo(function ChatMessageComponent({ message, className, onViewDocument, onSelect, isSelected }: ChatMessageProps) {
  const isUser = message.role === 'user';
  const [replayDialogOpen, setReplayDialogOpen] = useState(false);
  const [replayResponse, setReplayResponse] = useState<ReplayResponse | null>(null);

  // Check if drawer context is available (optional - graceful degradation)
  const drawerContext = useEvidenceDrawerOptional();
  const hasDrawer = drawerContext !== null;

  const handleReplayComplete = useCallback((response: ReplayResponse) => {
    setReplayResponse(response);
    setReplayDialogOpen(true);
  }, []);

  const handleOpenDrawer = useCallback(() => {
    if (drawerContext) {
      drawerContext.setMessageData({
        evidence: message.evidence,
        routerDecision: message.routerDecision ?? undefined,
        requestId: message.requestId,
        traceId: message.traceId,
        proofDigest: message.proofDigest,
        isVerified: message.isVerified ?? undefined,
        verifiedAt: message.verifiedAt,
        throughputStats: message.throughputStats,
      });
      drawerContext.openDrawer(message.id, 'rulebook');
    }
  }, [drawerContext, message]);

  // Convert message to export format
  const toExportFormat = useCallback((): ExtendedMessageExport => ({
    id: message.id,
    role: message.role,
    content: message.content,
    timestamp: message.timestamp.toISOString(),
    requestId: message.requestId,
    traceId: message.traceId ?? message.requestId, // Fall back to requestId if no traceId
    proofDigest: message.proofDigest,
    isVerified: message.isVerified ?? undefined,
    verifiedAt: message.verifiedAt,
    evidence: message.evidence?.map((e) => ({
      documentId: e.document_id,
      documentName: e.document_name,
      chunkId: e.chunk_id,
      pageNumber: e.page_number,
      textPreview: e.text_preview,
      relevanceScore: e.relevance_score,
      rank: e.rank,
      charRange: e.char_range,
      bbox: e.bbox,
      citationId: e.citation_id,
    })),
    routerDecision: message.routerDecision ? {
      requestId: message.routerDecision.request_id,
      selectedAdapters: message.routerDecision.selected_adapters,
      candidates: message.routerDecision.candidates?.map((c) => ({
        adapterId: c.adapter_id,
        gateQ15: c.gate_q15,
        gateFloat: c.gate_float,
        selected: c.selected,
      })),
    } : undefined,
  }), [message]);

  const handleExportMarkdown = useCallback(() => {
    try {
      const exportMessage = toExportFormat();
      const metadata = {
        exportId: `export-${Date.now().toString(36)}`,
        exportTimestamp: new Date().toISOString(),
        entityType: 'chat_session' as const,
        entityId: message.id,
        entityName: `Answer ${message.id.slice(0, 8)}`,
      };
      const markdown = renderSingleAnswerMarkdown(exportMessage, metadata);
      const filename = `answer-${message.id.slice(0, 8)}.md`;
      downloadTextFile(markdown, filename, 'text/markdown');
      toast.success('Exported as Markdown');
    } catch (error) {
      toast.error(`Export failed: ${(error as Error).message}`);
    }
  }, [message.id, toExportFormat]);

  const handleExportJson = useCallback(() => {
    try {
      const exportMessage = toExportFormat();
      const json = JSON.stringify(exportMessage, null, 2);
      const filename = `answer-${message.id.slice(0, 8)}.json`;
      downloadTextFile(json, filename, 'application/json');
      toast.success('Exported as JSON');
    } catch (error) {
      toast.error(`Export failed: ${(error as Error).message}`);
    }
  }, [message.id, toExportFormat]);

  const handleExportEvidenceBundle = useCallback(async () => {
    try {
      const exportMessage = toExportFormat();
      const bundle = await generateEvidenceBundle({ messages: [exportMessage] });
      downloadEvidenceBundle(bundle, `evidence-${message.id.slice(0, 8)}.json`);
      toast.success('Exported Evidence Bundle');
    } catch (error) {
      toast.error(`Export failed: ${(error as Error).message}`);
    }
  }, [message.id, toExportFormat]);

  const handleClick = useCallback(() => {
    if (onSelect && !message.isStreaming) {
      // Pass traceId (or requestId as fallback) for trace fetching
      const traceId = message.traceId ?? message.requestId;
      onSelect(message.id, traceId);
    }
  }, [onSelect, message.id, message.isStreaming, message.traceId, message.requestId]);

  return (
    <div
      className={cn(
        'flex flex-col gap-2 px-4 py-3 transition-colors',
        isUser ? 'items-end' : 'items-start',
        onSelect && !message.isStreaming && 'cursor-pointer hover:bg-muted/50',
        isSelected && 'bg-primary/5 ring-1 ring-primary/20 rounded-md',
        className
      )}
      role="article"
      aria-label={`${isUser ? 'User' : 'Assistant'} message`}
      aria-selected={isSelected}
      onClick={handleClick}
    >
      {/* Router indicator for assistant messages */}
      {!isUser && message.routerDecision && (
        <RouterIndicator
          decision={message.routerDecision}
          unavailablePinnedAdapters={message.unavailablePinnedAdapters}
        />
      )}

      {/* Pinned adapter warning for assistant messages */}
      {!isUser && message.unavailablePinnedAdapters && message.unavailablePinnedAdapters.length > 0 && (
        <div className="max-w-[80%] w-full">
          <MissingPinnedAdaptersWarning
            unavailableAdapters={message.unavailablePinnedAdapters}
            fallbackMode={message.pinnedRoutingFallback}
          />
        </div>
      )}

      {/* Message bubble */}
      <div
        className={cn(
          'max-w-[80%] rounded-lg px-4 py-2 text-sm',
          isUser
            ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-muted-foreground',
          message.isStreaming && 'animate-pulse'
        )}
        role={isUser ? 'user-message' : 'assistant-message'}
        aria-live={message.isStreaming ? 'polite' : 'off'}
      >
        <div className="whitespace-pre-wrap break-words">
          {message.content}
          {message.isStreaming && (
            <span
              className="inline-block w-2 h-4 ml-1 bg-current animate-pulse"
              aria-label="Streaming in progress"
              aria-live="polite"
            />
          )}
        </div>
      </div>

      {/* Evidence display for assistant messages with sources */}
      {!isUser && message.evidence && message.evidence.length > 0 && (
        <div className="max-w-[80%] w-full">
          {hasDrawer ? (
            // Use inline preview when drawer is available
            <InlineEvidencePreview
              messageId={message.id}
              evidence={message.evidence}
              maxItems={3}
              onViewAll={handleOpenDrawer}
            />
          ) : (
            // Fall back to full panel when no drawer context
            <EvidenceSources
              evidence={message.evidence}
              isVerified={message.isVerified || false}
              verifiedAt={message.verifiedAt}
              onViewDocument={onViewDocument}
            />
          )}
        </div>
      )}

      {/* Timestamp with evidence triggers and replay button */}
      <div className="flex items-center gap-2 flex-wrap">
        <time
          className="text-xs text-muted-foreground px-1"
          dateTime={message.timestamp.toISOString()}
          aria-label={`Sent at ${message.timestamp.toLocaleTimeString()}`}
        >
          {message.timestamp.toLocaleTimeString()}
        </time>
        {/* Throughput stats for assistant messages */}
        {!isUser && message.throughputStats && !message.isStreaming && (
          <span className="text-xs text-muted-foreground font-mono border-l pl-2 ml-1">
            {message.throughputStats.tokensPerSecond.toFixed(1)} tok/s
            <span className="mx-1">|</span>
            {message.throughputStats.tokensGenerated} tokens
            <span className="mx-1">|</span>
            {(message.throughputStats.latencyMs / 1000).toFixed(1)}s
          </span>
        )}
        {/* Evidence drawer triggers (only when drawer context is available) */}
        {!isUser && hasDrawer && (
          <EvidenceDrawerTrigger
            messageId={message.id}
            evidence={message.evidence}
            routerDecision={message.routerDecision}
            requestId={message.requestId}
            traceId={message.traceId}
            proofDigest={message.proofDigest}
            isVerified={message.isVerified ?? undefined}
            verifiedAt={message.verifiedAt}
            throughputStats={message.throughputStats}
          />
        )}
        {/* Legacy proof badge (only when drawer not available) */}
        {!isUser && !hasDrawer && message.isVerified && (
          <ProofBadge isVerified={message.isVerified} timestamp={message.verifiedAt} />
        )}
        {/* Replay button for assistant messages with request ID */}
        {!isUser && message.requestId && !message.isStreaming && (
          <ReplayButton
            inferenceId={message.requestId}
            onReplayComplete={handleReplayComplete}
          />
        )}
        {/* Export dropdown for assistant messages */}
        {!isUser && !message.isStreaming && (
          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2"
                    data-testid="message-export-trigger"
                  >
                    <Download className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent>
                <p>Export this answer</p>
              </TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={handleExportMarkdown}>
                <FileText className="h-4 w-4 mr-2" />
                Export as Markdown
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleExportJson}>
                <FileJson className="h-4 w-4 mr-2" />
                Export as JSON
              </DropdownMenuItem>
              {message.evidence && message.evidence.length > 0 && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={handleExportEvidenceBundle}>
                    <Package className="h-4 w-4 mr-2" />
                    Export Evidence Bundle
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>

      {/* Replay result dialog */}
      <ReplayResultDialog
        open={replayDialogOpen}
        onOpenChange={setReplayDialogOpen}
        replayResponse={replayResponse}
      />
    </div>
  );
}, areMessagesEqual);
