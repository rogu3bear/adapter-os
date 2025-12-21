import { TelemetryEvent } from '@/api/types';
import { ConnectionStatus } from '@/hooks/realtime/useLiveData';

// Limits to avoid unbounded in-memory growth for the live table and pause buffer.
export const TELEMETRY_VISIBLE_MAX = 200;
export const TELEMETRY_BUFFER_MAX = 500;

export interface TelemetryEventState {
  events: TelemetryEvent[];
  buffer: TelemetryEvent[];
}

interface TelemetryFilterOptions {
  text?: string;
  level?: string;
  eventType?: string;
}

const getTimestamp = (event: TelemetryEvent): number => {
  if (typeof event.timestamp === 'number') return event.timestamp;
  const parsed = Date.parse((event.timestamp as string) ?? '');
  return Number.isFinite(parsed) ? parsed : 0;
};

const eventKey = (event: TelemetryEvent): string => {
  const ts = getTimestamp(event);
  return (
    (event as { event_id?: string }).event_id ||
    (event as { id?: string }).id ||
    `${event.event_type ?? 'unknown'}-${ts}`
  );
};

const dedupeAndSort = (events: TelemetryEvent[]): TelemetryEvent[] => {
  const seen = new Set<string>();
  const sorted = [...events].sort((a, b) => getTimestamp(b) - getTimestamp(a));
  const result: TelemetryEvent[] = [];

  for (const evt of sorted) {
    const key = eventKey(evt);
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(evt);
  }

  return result;
};

export function applyIncomingEvents(
  prev: TelemetryEventState,
  incoming: TelemetryEvent[],
  paused: boolean,
): TelemetryEventState {
  const mergedIncoming = dedupeAndSort(incoming);

  if (paused) {
    const buffered = dedupeAndSort([...mergedIncoming, ...prev.buffer]).slice(0, TELEMETRY_BUFFER_MAX);
    return { events: prev.events, buffer: buffered };
  }

  const events = dedupeAndSort([...mergedIncoming, ...prev.events]).slice(0, TELEMETRY_VISIBLE_MAX);
  return { events, buffer: [] };
}

export function flushBufferedEvents(
  prev: TelemetryEventState,
  visibleMax: number = TELEMETRY_VISIBLE_MAX,
): TelemetryEventState {
  const events = dedupeAndSort([...prev.buffer, ...prev.events]).slice(0, visibleMax);
  return { events, buffer: [] };
}

export function filterTelemetryEvents(
  events: TelemetryEvent[],
  { text, level, eventType }: TelemetryFilterOptions,
): TelemetryEvent[] {
  const textQuery = text?.trim().toLowerCase();
  const levelQuery = level?.toLowerCase();
  const typeQuery = eventType?.toLowerCase();

  const payloadToString = (payload: unknown): string => {
    if (!payload) return '';
    if (typeof payload === 'string') return payload;
    try {
      return JSON.stringify(payload);
    } catch {
      return '';
    }
  };

  return events.filter((evt) => {
    if (levelQuery && evt.level?.toLowerCase() !== levelQuery) return false;
    if (typeQuery && (evt.event_type ?? '').toLowerCase() !== typeQuery) return false;

    if (textQuery) {
      const haystack = [
        evt.message,
        evt.component,
        evt.event_type,
        payloadToString((evt as { payload?: unknown }).payload),
      ]
        .filter(Boolean)
        .map((value) => String(value).toLowerCase());

      if (!haystack.some((value) => value.includes(textQuery))) {
        return false;
      }
    }

    return true;
  });
}

export function mapConnectionToStatus(
  status: ConnectionStatus,
  sseConnected: boolean,
): 'Live' | 'Reconnecting' | 'Offline' {
  if (sseConnected || status === 'sse') return 'Live';
  if (status === 'polling') return 'Reconnecting';
  return 'Offline';
}

