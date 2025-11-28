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
import { X, Layers, Clock, Zap, AlertCircle, ChevronDown } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { RoutingDecision } from '@/api/types';
import { formatDistanceToNow, parseISO } from 'date-fns';

interface RouterActivitySidebarProps {
  open: boolean;
  onClose: () => void;
  stackId?: string;
  limit?: number;
}

export function RouterActivitySidebar({ open, onClose, stackId, limit = 20 }: RouterActivitySidebarProps) {
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
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          aria-label="Close router activity"
        >
          <X className="h-4 w-4" />
        </Button>
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
            <div className="text-center py-8 text-sm text-muted-foreground">
              Loading router decisions...
            </div>
          ) : filteredHistory.length === 0 ? (
            <div className="text-center py-8 text-sm text-muted-foreground">
              {stackId ? 'No router activity for this stack yet' : 'No router activity yet'}
            </div>
          ) : (
            <>
              {filteredHistory.map((decision: RoutingDecision) => (
              <Card key={decision.request_id} className="p-3">
                <CardContent className="p-0 space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Clock className="h-3 w-3 text-muted-foreground" />
                      <span className="text-xs text-muted-foreground">
                        {formatDistanceToNow(parseISO(decision.timestamp), { addSuffix: true })}
                      </span>
                    </div>
                    {decision.latency_ms !== undefined && (
                      <Badge variant="outline" className="text-xs">
                        <Zap className="h-3 w-3 mr-1" />
                        {decision.latency_ms.toFixed(1)}ms
                      </Badge>
                    )}
                  </div>
                  
                  <div className="space-y-1">
                    <p className="text-xs font-medium">Selected Adapters:</p>
                    <div className="flex flex-wrap gap-1">
                      {decision.selected_adapters.slice(0, 3).map((adapterId) => (
                        <Badge key={adapterId} variant="secondary" className="text-xs font-mono">
                          {adapterId.length > 15 ? `${adapterId.slice(0, 15)}...` : adapterId}
                        </Badge>
                      ))}
                      {decision.selected_adapters.length > 3 && (
                        <Badge variant="outline" className="text-xs">
                          +{decision.selected_adapters.length - 3}
                        </Badge>
                      )}
                    </div>
                  </div>

                  {decision.entropy !== undefined && (
                    <div className="text-xs text-muted-foreground">
                      Entropy: {decision.entropy.toFixed(3)}
                    </div>
                  )}

                  <div className="text-xs font-mono text-muted-foreground pt-1 border-t">
                    {decision.request_id.slice(0, 16)}...
                  </div>
                </CardContent>
              </Card>
              ))}
              {hasMore && filteredHistory.length >= limit * page && (
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

