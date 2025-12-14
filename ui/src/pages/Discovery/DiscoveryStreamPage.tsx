/**
 * DiscoveryStreamPage Component
 * 
 * Displays real-time repository scanning and code discovery events.
 * Shows progress bars, symbol counts, and framework detection.
 * 
 * Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8.3
 */

import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { useTimestamp } from '@/hooks/ui/useTimestamp';
import { useLiveData } from '@/hooks/realtime/useLiveData';
import apiClient from '@/api/client';

interface DiscoveryEvent {
  type: string;
  timestamp: number;
  payload: {
    repo_id?: string;
    stage?: string;
    files_parsed?: number;
    symbol_count?: number;
    framework?: string;
    content_hash?: string;
  };
}

interface ScanProgress {
  repo_id: string;
  stage: string;
  progress: number;
  files_parsed: number;
  symbols: number;
  frameworks: string[];
}

interface DiscoveryStreamPageProps {
  selectedTenant: string;
}

interface DiscoveryData {
  events: DiscoveryEvent[];
  scans: Map<string, ScanProgress>;
}

export function DiscoveryStreamPage({ selectedTenant }: DiscoveryStreamPageProps) {
  const [events, setEvents] = useState<DiscoveryEvent[]>([]);
  const [scans, setScans] = useState<Map<string, ScanProgress>>(new Map());

  const getProgressByStage = (stage: string): number => {
    switch (stage) {
      case 'parsing':
        return 20;
      case 'indexing':
        return 40;
      case 'analyzing':
        return 60;
      case 'building':
        return 80;
      case 'completed':
        return 100;
      default:
        return 10;
    }
  };

  const handleSSEMessage = useCallback((data: unknown) => {
    const discoveryEvent = data as DiscoveryEvent;

    setEvents((prev) => [discoveryEvent, ...prev].slice(0, 50));

    const repoId = discoveryEvent.payload.repo_id;
    if (!repoId) return;

    setScans((prev) => {
      const updated = new Map(prev);
      const existing = updated.get(repoId) || {
        repo_id: repoId,
        stage: 'started',
        progress: 0,
        files_parsed: 0,
        symbols: 0,
        frameworks: [],
      };

      switch (discoveryEvent.type) {
        case 'repo_scan_started':
          existing.stage = 'parsing';
          existing.progress = 10;
          break;
        case 'repo_scan_progress':
          existing.stage = discoveryEvent.payload.stage || existing.stage;
          existing.files_parsed = discoveryEvent.payload.files_parsed || existing.files_parsed;
          existing.progress = getProgressByStage(existing.stage);
          break;
        case 'symbol_indexed':
          existing.symbols = discoveryEvent.payload.symbol_count || existing.symbols;
          existing.progress = 60;
          break;
        case 'framework_detected':
          if (discoveryEvent.payload.framework && !existing.frameworks.includes(discoveryEvent.payload.framework)) {
            existing.frameworks.push(discoveryEvent.payload.framework);
          }
          break;
        case 'repo_scan_completed':
          existing.stage = 'completed';
          existing.progress = 100;
          existing.symbols = discoveryEvent.payload.symbol_count || existing.symbols;
          break;
      }

      updated.set(repoId, existing);
      return updated;
    });
  }, []);

  // Use standardized live data hook
  const { sseConnected } = useLiveData<DiscoveryData>({
    sseEndpoint: `/v1/streams/discovery?tenant=${selectedTenant}`,
    sseEventType: 'discovery',
    fetchFn: async () => ({ events: [], scans: new Map() }),
    pollingSpeed: 'fast',
    enabled: true,
    onSSEMessage: handleSSEMessage,
    operationName: 'DiscoveryStream',
  });

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold">Discovery Stream</h1>
        <p className="text-gray-600 dark:text-gray-400 mt-2">
          Live repository scanning and indexing
        </p>
      </div>

      {/* Active Scans */}
      <div className="space-y-4">
        {scans.size === 0 && (
          <Card>
            <CardContent className="py-12 text-center text-gray-500">
              No active scans. Discovery events will appear here when repositories are scanned.
            </CardContent>
          </Card>
        )}
        {Array.from(scans.values()).map((scan) => (
          <Card key={scan.repo_id}>
            <CardHeader>
              <div className="flex justify-between items-center">
                <CardTitle>{scan.repo_id}</CardTitle>
                <Badge variant="outline">{scan.stage}</Badge>
              </div>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                <Progress value={scan.progress} className="w-full" />

                <div className="grid grid-cols-3 gap-4 text-sm">
                  <div>
                    <div className="text-gray-600 dark:text-gray-400">Files</div>
                    <div className="text-2xl font-bold">{scan.files_parsed}</div>
                  </div>
                  <div>
                    <div className="text-gray-600 dark:text-gray-400">Symbols</div>
                    <div className="text-2xl font-bold">{scan.symbols}</div>
                  </div>
                  <div>
                    <div className="text-gray-600 dark:text-gray-400">Frameworks</div>
                    <div className="text-2xl font-bold">{scan.frameworks.length}</div>
                  </div>
                </div>

                {scan.frameworks.length > 0 && (
                  <div className="flex gap-2 flex-wrap">
                    {scan.frameworks.map((fw, idx) => (
                      <Badge key={idx} variant="secondary">
                        {fw}
                      </Badge>
                    ))}
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Event Log */}
      <Card>
        <CardHeader>
          <CardTitle>Event Log</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-1 max-h-64 overflow-y-auto text-sm font-mono">
            {events.length === 0 && (
              <p className="text-gray-500 text-center py-4">Waiting for events...</p>
            )}
            {events.map((event, idx) => (
              <div key={idx} className="flex gap-2 items-center">
                <span className="text-gray-500 dark:text-gray-400">
                  {useTimestamp(new Date(event.timestamp).toISOString())}
                </span>
                <span className="text-blue-600 dark:text-blue-400">{event.payload.repo_id}</span>
                <span>{event.type}</span>
                {event.payload.framework && (
                  <Badge variant="outline">{event.payload.framework}</Badge>
                )}
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

