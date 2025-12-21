import type { AdapterActivation, AdapterHistoryEntry } from '@/api/adapter-types';

export interface AdapterActivityItem {
  label: string;
  detail?: string;
  timestamp: string;
}

interface BuildAdapterRecentActivityArgs {
  adapterId: string;
  lineageHistory?: AdapterHistoryEntry[] | null;
  activations?: AdapterActivation[] | null;
}

// Build a unified activity list for the adapter detail overview.
export function buildAdapterRecentActivity({
  adapterId,
  lineageHistory,
  activations,
}: BuildAdapterRecentActivityArgs): AdapterActivityItem[] {
  const events: AdapterActivityItem[] = [];

  lineageHistory?.forEach(entry => {
    if (!entry?.timestamp) return;
    events.push({
      label: entry.action || 'lineage',
      detail: entry.actor || (entry.details ? JSON.stringify(entry.details) : undefined),
      timestamp: entry.timestamp,
    });
  });

  activations
    ?.filter(activation => !adapterId || activation.adapter_id === adapterId)
    .forEach(activation => {
      activation.history?.forEach(point => {
        if (!point?.timestamp) return;
        events.push({
          label: 'activation',
          detail: `Activation ${point.value}% (${activation.trend})`,
          timestamp: point.timestamp,
        });
      });
    });

  return events
    .filter(event => Boolean(event.timestamp))
    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
    .slice(0, 20);
}

