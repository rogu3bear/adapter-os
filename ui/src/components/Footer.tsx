import React from 'react';
import { useQuery } from '@tanstack/react-query';

interface MetaData {
  version: string;
  build_hash: string;
  uptime: number;
  last_updated: string;
}

export const Footer: React.FC = () => {
  const { data: meta, isLoading } = useQuery<MetaData>({
    queryKey: ['/v1/meta'],
    queryFn: async () => {
      const response = await fetch('/v1/meta');
      if (!response.ok) {
        throw new Error('Failed to fetch metadata');
      }
      return response.json();
    },
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  const formatUptime = (seconds: number) => {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  };

  return (
    <footer className="border-t bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
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
            {meta && (
              <div>
                Last updated: {new Date(meta.last_updated).toLocaleString()}
              </div>
            )}
          </div>
        </div>
      </div>
    </footer>
  );
};
