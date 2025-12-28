import { useEffect, useMemo } from 'react';
import { RefreshCw, Shield } from 'lucide-react';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import { useSystemStatus } from '@/hooks/system/useSystemStatus';
import type { AneMemoryStatus, DriftStatus, StatusIndicator } from '@/api/system-status-types';

type Severity = 'ok' | 'warn' | 'critical' | 'unknown';

interface StatusRowProps {
  label: string;
  value: string;
  severity: Severity;
  hint?: string | null;
}

interface SectionProps {
  title: string;
  severity: Severity;
  children: React.ReactNode;
}

interface SystemStatusDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenantId?: string | null;
}

const SEVERITY_ORDER: Severity[] = ['unknown', 'ok', 'warn', 'critical'];

const SEVERITY_STYLES: Record<Severity, string> = {
  ok: 'bg-emerald-50 text-emerald-700 border-emerald-200',
  warn: 'bg-amber-50 text-amber-800 border-amber-200',
  critical: 'bg-red-50 text-red-700 border-red-200',
  unknown: 'bg-slate-100 text-slate-700 border-slate-200',
};

const SEVERITY_LABEL: Record<Severity, string> = {
  ok: 'OK',
  warn: 'WARN',
  critical: 'CRITICAL',
  unknown: 'UNKNOWN',
};

function resolveSeverity(value: StatusIndicator): Severity {
  if (value === null || value === undefined) return 'unknown';
  if (typeof value === 'boolean') return value ? 'ok' : 'critical';
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) return 'unknown';
    if (value >= 80) return 'critical';
    if (value >= 60) return 'warn';
    return 'ok';
  }
  const normalized = String(value).toLowerCase();
  if (['ok', 'pass', 'healthy', 'ready', 'enabled', 'on', 'true', 'low'].includes(normalized)) return 'ok';
  if (['warn', 'warning', 'partial', 'degraded', 'medium', 'starting'].includes(normalized)) return 'warn';
  if (['fail', 'failed', 'error', 'critical', 'unhealthy', 'panic', 'false', 'high'].includes(normalized)) {
    return 'critical';
  }
  return 'unknown';
}

function resolveDriftSeverity(drift: DriftStatus | string | null | undefined): Severity {
  if (!drift) return 'unknown';
  if (typeof drift === 'string') return resolveSeverity(drift);
  return resolveSeverity(drift.status ?? drift.detail ?? null);
}

function pickSeverity(values: Severity[]): Severity {
  return values.reduce((winner, current) => {
    if (SEVERITY_ORDER.indexOf(current) > SEVERITY_ORDER.indexOf(winner)) return current;
    return winner;
  }, 'unknown' as Severity);
}

function formatIndicator(value: StatusIndicator): string {
  if (value === null || value === undefined) return 'Unknown';
  if (typeof value === 'boolean') return value ? 'Enabled' : 'Disabled';
  if (typeof value === 'number') return Number.isFinite(value) ? `${value}` : 'Unknown';
  const trimmed = `${value}`.trim();
  return trimmed.length ? trimmed : 'Unknown';
}

function formatInferenceReady(value: StatusIndicator): string {
  if (value === true || value === 'true') return 'Ready';
  if (value === false || value === 'false') return 'Not ready';
  if (typeof value === 'string' && value.toLowerCase() === 'unknown') return 'Unknown';
  return formatIndicator(value);
}

/** Blocker severity mapping - some blockers are warnings, not critical */
const BLOCKER_SEVERITY: Record<string, Severity> = {
  // Critical blockers - prevent inference
  boot_failed: 'critical',
  database_unavailable: 'critical',
  worker_missing: 'critical',
  no_model_loaded: 'critical',
  system_booting: 'warn', // System is starting, not failed
  // Warning blockers - degraded but not blocking
  telemetry_degraded: 'warn',
};

function getBlockerSeverity(blocker: string): Severity {
  return BLOCKER_SEVERITY[blocker] ?? 'critical'; // Default to critical for unknown blockers
}

function getBlockersSeverity(blockers: string[] | null | undefined): Severity {
  if (blockers === null || blockers === undefined) return 'unknown';
  if (blockers.length === 0) return 'ok';
  // Return the highest severity among all blockers
  return pickSeverity(blockers.map(getBlockerSeverity));
}

function formatBlockers(blockers: string[] | null | undefined): string {
  if (blockers === null || blockers === undefined) return 'Unknown';
  if (!blockers.length) return 'None';
  return blockers.map((blocker) => blocker.replace(/_/g, ' ')).join(', ');
}

function formatAneMemory(ane: AneMemoryStatus | null | undefined): string {
  if (!ane) return 'Unknown';
  const parts: string[] = [];
  if (typeof ane.usedMb === 'number' && typeof ane.totalMb === 'number') {
    parts.push(`${Math.round(ane.usedMb)} / ${Math.round(ane.totalMb)} MB`);
  } else if (typeof ane.usedMb === 'number') {
    parts.push(`${Math.round(ane.usedMb)} MB`);
  }
  if (ane.pressure !== undefined && ane.pressure !== null) {
    parts.push(`${formatIndicator(ane.pressure)}%`);
  }
  return parts.length ? parts.join(' • ') : 'Unknown';
}

function formatUpdatedAt(date: Date | null): string {
  if (!date) return 'Never';
  const delta = Date.now() - date.getTime();
  if (delta < 0) return 'Just now';
  const seconds = Math.floor(delta / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function StatusBadge({ severity }: { severity: Severity }) {
  return (
    <Badge variant="outline" className={cn('text-[11px] font-semibold', SEVERITY_STYLES[severity])}>
      {SEVERITY_LABEL[severity]}
    </Badge>
  );
}

function StatusRow({ label, value, severity, hint }: StatusRowProps) {
  return (
    <div className="flex items-start justify-between gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-2.5">
      <div className="space-y-0.5">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
        <span className="truncate max-w-[160px] text-right">{value}</span>
        <StatusBadge severity={severity} />
      </div>
    </div>
  );
}

function Section({ title, severity, children }: SectionProps) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <div className="text-sm font-semibold uppercase tracking-wide text-muted-foreground">{title}</div>
        <StatusBadge severity={severity} />
      </div>
      <div className="space-y-2">{children}</div>
    </div>
  );
}

export function SystemStatusDrawer({ open, onOpenChange, tenantId }: SystemStatusDrawerProps) {
  const { data, loading, error, source, lastUpdated, stale, refetch } = useSystemStatus({
    enabled: open,
    tenantId,
    autoRefreshOnModelEvents: true,
  });

  useEffect(() => {
    if (open) void refetch();
  }, [open, refetch]);

  const integritySeverity = useMemo(
    () =>
      pickSeverity([
        resolveSeverity(data?.integrity?.localSecureMode ?? null),
        resolveSeverity(data?.integrity?.strictMode ?? null),
        resolveSeverity(data?.integrity?.pfDeny ?? null),
        resolveDriftSeverity(data?.integrity?.drift),
      ]),
    [data?.integrity],
  );

  const readinessSeverity = useMemo(
    () =>
      pickSeverity([
        resolveSeverity(data?.readiness?.db ?? null),
        resolveSeverity(data?.readiness?.migrations ?? null),
        resolveSeverity(data?.readiness?.workers ?? null),
        resolveSeverity(data?.readiness?.modelsSeeded ?? null),
      ]),
    [data?.readiness],
  );

  const inferenceSeverity = useMemo(() => {
    const readySeverity = resolveSeverity(data?.inferenceReady ?? null);
    const blockerSeverity = getBlockersSeverity(data?.inferenceBlockers);
    return pickSeverity([readySeverity, blockerSeverity]);
  }, [data?.inferenceBlockers, data?.inferenceReady]);

  const kernelSeverity = useMemo(
    () =>
      pickSeverity([
        data?.kernel?.activeModel ? 'ok' : 'warn',
        resolveSeverity(data?.kernel?.umaPressure ?? null),
        resolveSeverity(data?.kernel?.aneMemory?.pressure ?? null),
      ]),
    [data?.kernel],
  );

  const bootSeverity = useMemo(() => {
    const phase = data?.boot?.phase?.toLowerCase() ?? '';
    const degraded = data?.boot?.degradedReasons?.length ? 'warn' : 'unknown';
    const phaseSeverity =
      phase.includes('fail') || phase.includes('panic')
        ? 'critical'
        : phase
          ? 'warn'
          : 'unknown';
    return pickSeverity([phaseSeverity as Severity, degraded as Severity]);
  }, [data?.boot]);

  const driftDetail =
    typeof data?.integrity?.drift === 'string'
      ? data?.integrity?.drift
      : data?.integrity?.drift?.detail;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full sm:max-w-xl">
        <SheetHeader>
          <div className="flex items-center justify-between gap-2">
            <div>
              <SheetTitle className="flex items-center gap-2">
                <Shield className="h-4 w-4 text-primary" />
                System Status
              </SheetTitle>
              <SheetDescription className="flex items-center gap-2">
                Integrity, readiness, and kernel signals. Unknown states stay grey until the backend responds.
              </SheetDescription>
            </div>
            <Button variant="outline" size="sm" onClick={() => void refetch()} disabled={loading}>
              <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              <span className="sr-only">Refresh</span>
            </Button>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="secondary" className="text-[11px]">
              Source: {source === 'native' ? '/v1/system/status' : 'fallback'}
            </Badge>
            <Badge variant="outline" className="text-[11px]">
              Updated {formatUpdatedAt(lastUpdated)}
            </Badge>
            {stale && (
              <Badge variant="warning" className="text-[11px]">
                Stale snapshot (endpoint error)
              </Badge>
            )}
            {error && !stale && (
              <Badge variant="destructive" className="text-[11px]">
                Load failed
              </Badge>
            )}
          </div>
        </SheetHeader>

        <Separator className="my-4" />

        <ScrollArea className="h-[calc(100vh-180px)] pr-3">
          <div className="space-y-6 pb-6">
            <Section title="Integrity" severity={integritySeverity}>
              <StatusRow
                label="Local secure mode"
                value={formatIndicator(data?.integrity?.localSecureMode ?? null)}
                severity={resolveSeverity(data?.integrity?.localSecureMode ?? null)}
                hint="Zero egress / local-only enforcement"
              />
              <StatusRow
                label="Strict mode"
                value={formatIndicator(data?.integrity?.strictMode ?? null)}
                severity={resolveSeverity(data?.integrity?.strictMode ?? null)}
                hint="Determinism/strict routing mode"
              />
              <StatusRow
                label="PF deny"
                value={formatIndicator(data?.integrity?.pfDeny ?? null)}
                severity={resolveSeverity(data?.integrity?.pfDeny ?? null)}
                hint="Packet filter / egress deny requirement"
              />
              <StatusRow
                label="Drift check"
                value={
                  typeof data?.integrity?.drift === 'string'
                    ? data.integrity.drift
                    : formatIndicator(data?.integrity?.drift?.status ?? null)
                }
                severity={resolveDriftSeverity(data?.integrity?.drift)}
                hint={driftDetail || 'Last determinism check result'}
              />
            </Section>

            <Section title="Readiness" severity={readinessSeverity}>
              <StatusRow
                label="Database"
                value={formatIndicator(data?.readiness?.db ?? null)}
                severity={resolveSeverity(data?.readiness?.db ?? null)}
                hint="Connectivity and query latency"
              />
              <StatusRow
                label="Migrations"
                value={formatIndicator(data?.readiness?.migrations ?? null)}
                severity={resolveSeverity(data?.readiness?.migrations ?? null)}
                hint="Migration signature and apply state"
              />
              <StatusRow
                label="Workers"
                value={formatIndicator(data?.readiness?.workers ?? null)}
                severity={resolveSeverity(data?.readiness?.workers ?? null)}
                hint="Online workers responding"
              />
              <StatusRow
                label="Models seeded"
                value={formatIndicator(data?.readiness?.modelsSeeded ?? null)}
                severity={resolveSeverity(data?.readiness?.modelsSeeded ?? null)}
                hint="Base models present and discoverable"
              />
              <StatusRow
                label="Phase"
                value={data?.readiness?.phase || 'Unknown'}
                severity={resolveSeverity(data?.readiness?.phase ?? null)}
                hint={data?.readiness?.bootTraceId ? `Boot trace: ${data.readiness.bootTraceId}` : undefined}
              />
            </Section>

            <Section title="Inference" severity={inferenceSeverity}>
              <StatusRow
                label="Inference ready"
                value={formatInferenceReady(data?.inferenceReady ?? null)}
                severity={resolveSeverity(data?.inferenceReady ?? null)}
                hint="Workers online with a loaded base model"
              />
              <StatusRow
                label="Blockers"
                value={formatBlockers(data?.inferenceBlockers)}
                severity={getBlockersSeverity(data?.inferenceBlockers)}
                hint="Active model mismatch or missing dependencies"
              />
            </Section>

            <Section title="Kernel" severity={kernelSeverity}>
              <StatusRow
                label="Active model"
                value={data?.kernel?.activeModel || 'None'}
                severity={data?.kernel?.activeModel ? 'ok' : 'warn'}
                hint="Currently loaded base model"
              />
              <StatusRow
                label="Active plan"
                value={data?.kernel?.activePlan || 'Unknown'}
                severity={data?.kernel?.activePlan ? 'ok' : 'unknown'}
                hint="Active stack / plan the router is targeting"
              />
              <StatusRow
                label="Adapters"
                value={
                  data?.kernel?.activeAdapters !== null && data?.kernel?.activeAdapters !== undefined
                    ? `${data.kernel.activeAdapters}${data.kernel.hotAdapters ? ` (${data.kernel.hotAdapters} hot)` : ''}`
                    : 'Unknown'
                }
                severity={data?.kernel?.activeAdapters ? 'ok' : 'unknown'}
                hint="Adapters loaded across tenants"
              />
              <StatusRow
                label="ANE memory"
                value={formatAneMemory(data?.kernel?.aneMemory)}
                severity={resolveSeverity(data?.kernel?.aneMemory?.pressure ?? null)}
                hint="Apple Neural Engine allocation and usage"
              />
              <StatusRow
                label="UMA pressure"
                value={formatIndicator(data?.kernel?.umaPressure ?? null)}
                severity={resolveSeverity(data?.kernel?.umaPressure ?? null)}
                hint="Unified memory pressure signal"
              />
            </Section>

            <Section title="Boot" severity={bootSeverity}>
              <StatusRow
                label="Phase"
                value={data?.boot?.phase || 'Starting'}
                severity={resolveSeverity(data?.boot?.phase ?? null)}
                hint="Current boot stage"
              />
              <StatusRow
                label="Degraded reasons"
                value={
                  data?.boot?.degradedReasons?.length
                    ? data.boot.degradedReasons.join(', ')
                    : 'None reported'
                }
                severity={data?.boot?.degradedReasons?.length ? 'warn' : 'ok'}
                hint="Critical or non-critical degraded components"
              />
              <StatusRow
                label="Boot trace id"
                value={data?.boot?.bootTraceId || 'Unknown'}
                severity={data?.boot?.bootTraceId ? 'ok' : 'unknown'}
                hint={data?.boot?.lastError || undefined}
              />
            </Section>

            <Section title="Components" severity={pickSeverity((data?.components || []).map((c) => resolveSeverity(c.status ?? null)))}>
              {(data?.components || []).length === 0 ? (
                <div className="rounded-lg border border-dashed border-border/60 bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
                  No component health reported
                </div>
              ) : (
                <div className="space-y-2">
                  {data?.components?.map((component, index) => (
                    <StatusRow
                      key={component.name || `${component.status || 'component'}-${index}`}
                      label={component.name || 'Component'}
                      value={formatIndicator(component.status ?? null)}
                      severity={resolveSeverity(component.status ?? null)}
                      hint={component.message || undefined}
                    />
                  ))}
                </div>
              )}
            </Section>
          </div>
        </ScrollArea>
      </SheetContent>
    </Sheet>
  );
}

export default SystemStatusDrawer;
