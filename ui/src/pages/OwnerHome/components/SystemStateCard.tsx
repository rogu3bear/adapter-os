import React from 'react';
import { useNavigate } from 'react-router-dom';
import type { SystemStateResponse } from '@/api/system-state-types';
import { buildSystemMemoryLink } from '@/utils/navLinks';
import { formatGB, formatPercent, formatRelativeTime } from '@/lib/formatters';

interface Props {
  data: SystemStateResponse | null;
  isLoading: boolean;
  error: Error | null;
  isLive: boolean;
  lastUpdated: Date | null;
  onRefresh?: () => void;
}

// Local wrapper for formatMB that handles the specific signature needed here
function formatMB(mb: number): string {
  return `${mb.toFixed(1)} MB`;
}

export function SystemStateCard({ data, isLoading, error, isLive, lastUpdated, onRefresh }: Props) {
  const navigate = useNavigate();

  if (isLoading && !data) {
    return (
      <div>
        <h2>System State</h2>
        <div className="animate-pulse h-20 w-full" />
      </div>
    );
  }

  if (error && !data) {
    return (
      <div>
        <h2>System State</h2>
        <p>Failed to load system state</p>
        {onRefresh && (
          <button onClick={onRefresh}>Retry</button>
        )}
      </div>
    );
  }

  if (!data) {
    return (
      <div>
        <h2>System State</h2>
        <p>No data</p>
      </div>
    );
  }

  const { memory } = data;
  const topAdapters = (memory.top_adapters || []).slice(0, 5);
  const pressure = (memory.pressure_level || '').toUpperCase();
  const headroom = memory.headroom_percent ?? 0;

  const usedMb = memory.used_mb ?? 0;
  const totalMb = memory.total_mb ?? 0;
  const percent = totalMb > 0 ? (usedMb / totalMb) * 100 : 0;

  return (
    <div>
      <div className="flex items-center gap-2">
        <h2>System State</h2>
        {isLive ? (
          <span>Live</span>
        ) : (
          <span>{lastUpdated ? formatRelativeTime(lastUpdated) : 'Unknown'}</span>
        )}
      </div>

      <section>
        <h3>Memory Pressure</h3>
        <div>{pressure}</div>
        <div>{formatGB(usedMb)} used</div>
        <div>{formatGB(totalMb)} total</div>
        <div>{formatPercent(percent)}</div>
        {headroom < 15 && <div>Low headroom ({headroom.toFixed(1)}%)</div>}
      </section>

      <section>
        <h3>Top Adapters by Memory</h3>
        {topAdapters.length === 0 ? (
          <div>No adapters loaded</div>
        ) : (
          <ol>
            {topAdapters.map((adapter, idx) => (
              <li key={adapter.adapter_id || idx}>
                <span>{`${idx + 1}.`}</span>{' '}
                <span>{adapter.name}</span>{' '}
                <span>{formatMB(adapter.memory_mb ?? 0)}</span>
              </li>
            ))}
          </ol>
        )}
        <div>{`${topAdapters.length} shown`}</div>
      </section>

      <section>
        <button onClick={() => navigate(buildSystemMemoryLink())}>View memory details</button>
      </section>
    </div>
  );
}

