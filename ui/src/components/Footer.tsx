import React from 'react';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';

import { MetaResponse } from '@/api/types';
import { cn, FROST_BACKGROUND } from '@/components/ui/utils';
import { formatDurationSeconds } from '@/utils/format';


interface MetaData {
  version: string;
  build_hash: string;
  uptime: number;
  last_updated: string;
}

export const Footer: React.FC = () => {
  const { data: meta, isLoading } = useQuery({
    queryKey: ['/v1/meta'],

    queryFn: async (): Promise<MetaResponse> => {
      // Citation: ui/src/api/client.ts L117-L119
      return apiClient.getMeta();
    },
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  const formatUptime = (seconds?: number) => {
    if (!seconds || seconds <= 0) return '—';
    return formatDurationSeconds(seconds);
  };

  return (
    <footer className={cn("border-t", FROST_BACKGROUND, "supports-[backdrop-filter]:bg-background/60")}>
      <div className="container mx-auto px-4 py-6">
        <div className="flex flex-col md:flex-row justify-between items-center space-y-4 md:space-y-0">
          <div className="flex flex-col md:flex-row items-center space-y-2 md:space-y-0 md:space-x-6">
            <div className="text-sm text-muted-foreground">
              <span className="font-medium">AdapterOS</span>
              {isLoading ? (
                <span className="ml-2">Loading...</span>
              ) : (
                <>
                  <span className="ml-2">v{meta?.version || 'unknown'}</span>
                  <span className="ml-2 text-xs">({meta?.build_hash?.slice(0, 8) || 'unknown'})</span>
                </>
              )}
            </div>
            {meta && (
              <div className="text-sm text-muted-foreground">
                Uptime: {formatUptime(meta.uptime)}
              </div>
            )}
          </div>
          
          <div className="text-sm text-muted-foreground">
            <div>
              Last updated: {meta?.last_updated ? new Date(meta.last_updated).toLocaleString() : '—'}
            </div>
          </div>
        </div>
      </div>
    </footer>
  );
};
