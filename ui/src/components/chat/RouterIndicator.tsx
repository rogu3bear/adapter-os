import React, { useState, useMemo } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { RouterDetailsModal } from './RouterDetailsModal';
import { Layers, ChevronDown, Info, AlertTriangle, Activity } from 'lucide-react';
import type { ExtendedRouterDecision } from '@/api/types';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';
import { Link } from 'react-router-dom';

interface RouterIndicatorProps {
  decision: ExtendedRouterDecision | null;
  className?: string;
  unavailablePinnedAdapters?: string[];
}

export function RouterIndicator({ decision, className, unavailablePinnedAdapters }: RouterIndicatorProps) {
  const [showDetails, setShowDetails] = useState(false);

  if (!decision || !decision.selected_adapters || decision.selected_adapters.length === 0) {
    return null;
  }

  const hasUnavailablePinned = unavailablePinnedAdapters && unavailablePinnedAdapters.length > 0;

  // Fetch adapter details efficiently - batch fetch only the ones we need
  const adapterIds = decision.selected_adapters.slice(0, 3); // Only fetch names for displayed adapters
  const { data: adapterNames, isLoading: isLoadingNames } = useQuery({
    queryKey: ['adapters', 'names', adapterIds.join(',')],
    queryFn: async () => {
      // Batch fetch adapter details
      const nameMap = new Map<string, string>();
      await Promise.allSettled(
        adapterIds.map(async (adapterId) => {
          try {
            const adapter = await apiClient.getAdapter(adapterId);
            nameMap.set(adapterId, adapter.name || adapterId);
          } catch (error) {
            // If adapter not found or error, fall back to ID
            logger.error('Failed to fetch adapter name', {
              component: 'RouterIndicator',
              adapterId,
            }, error instanceof Error ? error : new Error('Unknown error'));
            nameMap.set(adapterId, adapterId);
          }
        })
      );
      return nameMap;
    },
    enabled: adapterIds.length > 0,
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
    retry: 1, // Only retry once
  });

  // Create map of adapter ID to name with loading/error handling
  const adapterNameMap = useMemo(() => {
    return adapterNames || new Map<string, string>();
  }, [adapterNames]);

  const adapterCount = decision.selected_adapters.length;
  const kValue = decision.k_value || adapterCount;
  const selectedAdapters = decision.selected_adapters.slice(0, 3); // Show first 3 adapters inline
  const remainingCount = adapterCount - selectedAdapters.length;

  return (
    <>
      <div className={className}>
        <div className="flex items-center gap-2 flex-wrap">
          {/* Prominent adapter badges */}
          <div className="flex items-center gap-1.5 flex-wrap">
            <Layers className="h-4 w-4 text-primary" />
            <div className="flex items-center gap-1">
              <span className="text-sm font-medium text-muted-foreground">Using:</span>
              {hasUnavailablePinned && (
                <TooltipProvider>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Badge variant="outline" className="bg-orange-50 dark:bg-orange-950 border-orange-300 dark:border-orange-700 text-orange-700 dark:text-orange-300 px-1.5 py-0.5 h-5">
                        <AlertTriangle className="h-3 w-3" />
                      </Badge>
                    </TooltipTrigger>
                    <TooltipContent>
                      <p className="text-xs">
                        {unavailablePinnedAdapters!.length} pinned adapter{unavailablePinnedAdapters!.length > 1 ? 's' : ''} unavailable
                      </p>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              )}
            </div>
            {selectedAdapters.map((adapterId, idx) => {
              const adapterName = adapterNameMap.get(adapterId) || (isLoadingNames ? '...' : adapterId);
              const displayName = adapterName.length > 20 ? `${adapterName.slice(0, 20)}...` : adapterName;
              return (
                <Badge
                  key={adapterId}
                  variant="secondary"
                  className="text-xs px-2 py-0.5"
                  title={adapterName !== adapterId && adapterName !== '...' ? `${adapterName} (${adapterId})` : adapterId}
                >
                  {displayName}
                </Badge>
              );
            })}
            {remainingCount > 0 && (
              <Badge variant="outline" className="text-xs">
                +{remainingCount} more
              </Badge>
            )}
          </div>
          
          {/* Info button for details */}
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs gap-1"
            onClick={() => setShowDetails(true)}
          >
            <Info className="h-3.5 w-3.5" />
            <span>Details</span>
            {decision.entropy !== undefined && (
              <span className="text-muted-foreground ml-1">
                (entropy: {decision.entropy.toFixed(2)})
              </span>
            )}
          </Button>
          {decision.request_id && (
            <Button
              asChild
              variant="outline"
              size="sm"
              className="h-7 text-xs gap-1"
            >
              <Link to={`/telemetry/viewer?requestId=${encodeURIComponent(decision.request_id)}`}>
                <Activity className="h-3.5 w-3.5" />
                View telemetry
              </Link>
            </Button>
          )}
        </div>
      </div>

      {showDetails && decision && (
        <RouterDetailsModal
          decision={decision}
          open={showDetails}
          onOpenChange={setShowDetails}
        />
      )}
    </>
  );
}

