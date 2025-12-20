/**
 * Router Activity Sidebar
 * Shows router decision history for the current chat session
 */
import React, { useState, useMemo } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Skeleton } from '@/components/ui/skeleton';
import { X, Layers, Clock, Zap, AlertCircle, ChevronDown, Trash2 } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { RoutingDecision } from '@/api/types';
import type { RouterDecision } from '@/hooks/chat/useChatRouterDecisions';
import { formatDistanceToNow, parseISO } from 'date-fns';

type CombinedDecision = {
  id: string;
  timestamp: string;
  adapters: string[];
  isLocal: boolean;
  latency?: number;
  entropy?: number;
  adapterId?: string;
  adapterName?: string;
  confidence?: number;
};

interface RouterActivitySidebarProps {
  open: boolean;
  onClose: () => void;
  stackId?: string;
  limit?: number;
  /** Local decision history from useChatRouterDecisions hook */
  decisions?: RouterDecision[];
  /** Most recent decision */
  lastDecision?: RouterDecision | null;
  /** Callback to clear local decision history */
  onClear?: () => void;
}

export function RouterActivitySidebar({ 
  open, 
  onClose, 
  stackId, 
  limit = 20,
  decisions = [],
  lastDecision,
  onClear,
}: RouterActivitySidebarProps) {
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);

  const { data: routingHistory, isLoading, error, refetch } = useQuery({
    queryKey: ['routing-history', stackId, limit],
    queryFn: () => apiClient.getRoutingHistory(limit),
    enabled: open,
    refetchInterval: 10000, // Refresh every 10 seconds when open
  });

  // Filter by stack if provided
  const filteredHistory = useMemo(() => {
    if (!routingHistory) return [];
    if (!stackId) return routingHistory;
    // Note: RoutingDecision doesn't have stack_id, so we filter by adapter presence
    // This is a limitation - ideally routing decisions would include stack_id
    return routingHistory;
  }, [routingHistory, stackId]);

  // Combine local decisions with API history, prioritizing local decisions
  const allDecisions: CombinedDecision[] = useMemo(() => {
    const localDecisions: CombinedDecision[] = decisions.map(decision => ({
      id: decision.messageId,
      timestamp: decision.timestamp.toISOString(),
      adapters: decision.routingPath || [],
      adapterId: decision.adapterId,
      adapterName: decision.adapterName,
      confidence: decision.confidence,
      isLocal: true,
    }));
    
    const apiDecisions: CombinedDecision[] = filteredHistory.map((decision: RoutingDecision) => {
      const candidates = decision.candidates || [];
      const scoreValues = candidates.map(c => c.raw_score);
      const latencyMs = decision.total_inference_latency_us ? decision.total_inference_latency_us / 1000 : undefined;
      return {
        id: decision.request_id ?? decision.id,
        timestamp: decision.timestamp,
        adapters: decision.adapters_used || [],
        latency: latencyMs,
        entropy: decision.entropy,
        adapterId: decision.stack_hash ?? undefined,
        adapterName: candidates[0]?.adapter_name ?? undefined,
        confidence: scoreValues.length ? Math.max(...scoreValues) : undefined,
        isLocal: false,
      };
    });

    // Combine and sort by timestamp (most recent first)
    return [...localDecisions, ...apiDecisions].sort((a, b) => 
      new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );
  }, [decisions, filteredHistory]);

  const handleLoadMore = () => {
    setPage(prev => prev + 1);
    // In a real implementation, this would fetch more data
    // For now, we'll just show what we have
  };

  if (!open) return null;

  return (
    <div className="absolute right-0 top-0 bottom-0 w-96 bg-background border-l z-10 flex flex-col">
      <div className="border-b px-4 py-3 flex items-center justify-between">
        <h3 className="font-semibold text-sm flex items-center gap-2">
          <Layers className="h-4 w-4" />
          Router Activity
        </h3>
        <div className="flex items-center gap-2">
          {onClear && decisions.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onClear}
              aria-label="Clear decision history"
              title="Clear local decision history"
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={onClose}
            aria-label="Close router activity"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-4 space-y-3">
          {error ? (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                Failed to load router activity. 
                <Button variant="link" size="sm" onClick={() => refetch()} className="ml-2 p-0 h-auto">
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : isLoading ? (
            <div className="space-y-3" aria-label="Loading router activity">
              {Array.from({ length: 4 }).map((_, idx) => (
                <Card key={`router-activity-skel-${idx}`} className="p-3">
                  <CardContent className="p-0 space-y-2">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Skeleton className="h-3 w-16" />
                        <Skeleton className="h-4 w-10" />
                        <Skeleton className="h-4 w-12" />
                      </div>
                      <Skeleton className="h-4 w-16" />
                    </div>
                    <div className="space-y-1">
                      <Skeleton className="h-4 w-32" />
                      <div className="flex gap-2">
                        <Skeleton className="h-5 w-20" />
                        <Skeleton className="h-5 w-16" />
                      </div>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : allDecisions.length === 0 ? (
            <div className="text-center py-8 text-sm text-muted-foreground">
              {stackId ? 'No router activity for this stack yet' : 'No router activity yet'}
            </div>
          ) : (
            <>
              {allDecisions.map((decision) => (
              <Card key={decision.id} className="p-3">
                <CardContent className="p-0 space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Clock className="h-3 w-3 text-muted-foreground" />
                      <span className="text-xs text-muted-foreground">
                        {formatDistanceToNow(parseISO(decision.timestamp), { addSuffix: true })}
                      </span>
                      {decision.isLocal && (
                        <Badge variant="secondary" className="text-xs">
                          Local
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-1">
                      {decision.latency !== undefined && (
                        <Badge variant="outline" className="text-xs">
                          <Zap className="h-3 w-3 mr-1" />
                          {decision.latency.toFixed(1)}ms
                        </Badge>
                      )}
                      {decision.confidence !== undefined && (
                        <Badge variant="outline" className="text-xs">
                          {Math.round(decision.confidence * 100)}%
                        </Badge>
                      )}
                    </div>
                  </div>
                  
                  <div className="space-y-1">
                    <p className="text-xs font-medium">Selected Adapters:</p>
                    <div className="flex flex-wrap gap-1">
                      {decision.adapters && decision.adapters.length > 0 ? (
                        <>
                          {decision.adapters.slice(0, 3).map((adapterId, idx) => (
                            <Badge key={`${decision.id}-${idx}`} variant="secondary" className="text-xs font-mono">
                              {adapterId.length > 15 ? `${adapterId.slice(0, 15)}...` : adapterId}
                            </Badge>
                          ))}
                          {decision.adapters.length > 3 && (
                            <Badge variant="outline" className="text-xs">
                              +{decision.adapters.length - 3}
                            </Badge>
                          )}
                        </>
                      ) : (
                        <span className="text-xs text-muted-foreground">No adapters</span>
                      )}
                    </div>
                    {decision.adapterName && (
                      <p className="text-xs text-muted-foreground mt-1">
                        Primary: {decision.adapterName}
                      </p>
                    )}
                  </div>

                  {decision.entropy !== undefined && (
                    <div className="text-xs text-muted-foreground">
                      Entropy: {decision.entropy.toFixed(3)}
                    </div>
                  )}

                  <div className="text-xs font-mono text-muted-foreground pt-1 border-t">
                    {decision.id.slice(0, 16)}...
                  </div>
                </CardContent>
              </Card>
              ))}
              {hasMore && allDecisions.length >= limit * page && (
                <div className="flex justify-center pt-4">
                  <Button variant="outline" size="sm" onClick={handleLoadMore}>
                    <ChevronDown className="h-4 w-4 mr-2" />
                    Load More
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

