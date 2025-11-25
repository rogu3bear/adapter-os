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
import { RouterDetailsModal } from './RouterDetailsModal';
import { Layers, ChevronDown, Info } from 'lucide-react';
import type { ExtendedRouterDecision } from '@/api/types';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';

interface RouterIndicatorProps {
  decision: ExtendedRouterDecision | null;
  className?: string;
}

export function RouterIndicator({ decision, className }: RouterIndicatorProps) {
  const [showDetails, setShowDetails] = useState(false);

  if (!decision || !decision.selected_adapters || decision.selected_adapters.length === 0) {
    return null;
  }

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
            <span className="text-sm font-medium text-muted-foreground">Using:</span>
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

