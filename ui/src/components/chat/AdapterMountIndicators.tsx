import React from 'react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { AdapterLifecycleState } from '@/hooks/model-loading/types';

type MountStage = 'request' | 'load' | 'active';

export interface AdapterMountItem {
  adapterId: string;
  name: string;
  state: AdapterLifecycleState | string;
  isLoading?: boolean;
}

export interface AdapterMountTransition {
  adapterId: string;
  name?: string;
  from?: AdapterLifecycleState | string;
  to: AdapterLifecycleState | string;
  timestamp: number;
}

export interface AdapterMountIndicatorsProps {
  adapters: AdapterMountItem[];
  transitions: AdapterMountTransition[];
  activeAdapterId?: string | null;
  isStreaming?: boolean;
}

const STAGES: Array<{ key: MountStage; label: string }> = [
  { key: 'request', label: 'Request' },
  { key: 'load', label: 'LoRA Load' },
  { key: 'active', label: 'Active' },
];

function getStage(state: AdapterLifecycleState | string, isLoading?: boolean): MountStage {
  if (isLoading) return 'load';
  if (state === 'warm' || state === 'hot' || state === 'resident') return 'active';
  if (state === 'cold' || state === 'unloaded') return 'load';
  return 'request';
}

function stageIndex(stage: MountStage): number {
  return STAGES.findIndex((s) => s.key === stage);
}

export function AdapterMountIndicators({
  adapters,
  transitions,
  activeAdapterId,
  isStreaming,
}: AdapterMountIndicatorsProps) {
  return (
    <div className="rounded-md border bg-background shadow-inner">
      <div className="flex items-center justify-between px-3 py-2 border-b">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide">
          <span className={cn('h-2 w-2 rounded-full', isStreaming ? 'bg-emerald-500 animate-pulse' : 'bg-muted-foreground/50')} />
          Adapter Mounts
        </div>
        {isStreaming ? (
          <Badge variant="outline" className="text-[11px]">Live</Badge>
        ) : (
          <Badge variant="secondary" className="text-[11px]">Idle</Badge>
        )}
      </div>

      <div className="divide-y">
        {adapters.map((adapter) => {
          const stage = getStage(adapter.state, adapter.isLoading);
          const currentIndex = stageIndex(stage);

          return (
            <div
              key={adapter.adapterId}
              className={cn(
                'px-3 py-2 space-y-1 transition-colors',
                activeAdapterId === adapter.adapterId && 'bg-primary/5'
              )}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="font-mono text-xs text-foreground">
                    {adapter.name || adapter.adapterId}
                  </span>
                  {activeAdapterId === adapter.adapterId && (
                    <Badge variant="outline" className="text-[11px]">Active route</Badge>
                  )}
                </div>
                <span className="text-[11px] uppercase text-muted-foreground">
                  {STAGES[currentIndex]?.label ?? 'Request'}
                </span>
              </div>
              <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
                {STAGES.map((stageDef, idx) => (
                  <div key={`${adapter.adapterId}-${stageDef.key}`} className="flex items-center gap-1">
                    <div
                      className={cn(
                        'h-2 w-2 rounded-full',
                        idx < currentIndex
                          ? 'bg-emerald-500'
                          : idx === currentIndex
                            ? 'bg-emerald-400'
                            : 'bg-border'
                      )}
                    />
                    <span className={cn(
                      'uppercase tracking-wide',
                      idx <= currentIndex ? 'text-foreground' : 'text-muted-foreground'
                    )}>
                      {stageDef.label}
                    </span>
                    {idx < STAGES.length - 1 && <div className="h-px w-6 bg-border" />}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>

      <div className="px-3 py-2 bg-muted/40 rounded-b-md text-xs text-muted-foreground">
        <div className="font-semibold text-foreground text-xs mb-1">Recent hot-swaps</div>
        <div className="space-y-1">
          {transitions.length === 0 ? (
            <div className="text-muted-foreground">No adapter mounts yet.</div>
          ) : (
            transitions.slice(0, 4).map((transition) => {
              const fromStage = transition.from ? getStage(transition.from) : 'request';
              const toStage = getStage(transition.to);
              const fromLabel = STAGES.find((s) => s.key === fromStage)?.label ?? fromStage;
              const toLabel = STAGES.find((s) => s.key === toStage)?.label ?? toStage;
              return (
                <div
                  key={`${transition.adapterId}-${transition.timestamp}`}
                  className="flex items-center justify-between"
                >
                  <span className="font-mono text-[11px] text-foreground">
                    {transition.name || transition.adapterId}
                  </span>
                  <span className="text-[11px] uppercase">
                    {fromLabel} → {toLabel}
                  </span>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
