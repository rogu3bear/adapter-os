import { useMemo, useState } from 'react';
import { Activity, Clock, Cpu, HardDrive } from 'lucide-react';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';
import { useKernelTelemetry } from '@/contexts/KernelTelemetryContext';
import { apiClient } from '@/api/services';
import { Button } from '@/components/ui/button';

interface KernelStatusBarProps {
  className?: string;
  onDetachAll?: () => void;
  showEmergencyStop?: boolean;
}

function formatDuration(seconds?: number | null): string {
  if (!seconds && seconds !== 0) return '—';
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hrs > 0) return `${hrs}h ${mins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

function formatVram(used?: number | null, total?: number | null): string {
  if (used === undefined || total === undefined) return '—';
  if (used === null || total === null) return '—';
  const format = (value: number) => (value >= 1024 ? `${(value / 1024).toFixed(1)} GB` : `${value.toFixed(0)} MB`);
  return `${format(used)} / ${format(total)}`;
}

export function KernelStatusBar({ className, onDetachAll, showEmergencyStop = true }: KernelStatusBarProps) {
  const telemetry = useKernelTelemetry();
  const [isStopping, setIsStopping] = useState(false);

  const vramLabel = useMemo(
    () => formatVram(telemetry.vramUsedMb, telemetry.vramTotalMb),
    [telemetry.vramUsedMb, telemetry.vramTotalMb]
  );

  const uptimeLabel = useMemo(() => formatDuration(telemetry.uptimeSeconds), [telemetry.uptimeSeconds]);

  const handlePanic = async () => {
    if (isStopping) return;
    setIsStopping(true);
    try {
      await apiClient.stopSystem();
      toast.warning('Detaching all adapters...', { description: 'Emergency stop requested' });
      window.dispatchEvent(new CustomEvent('aos:detach-all'));
      onDetachAll?.();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Emergency stop failed';
      toast.error(message);
    } finally {
      setIsStopping(false);
    }
  };

  return (
    <div
      className={cn(
        'flex items-center gap-3 border-b border-border/60 bg-gradient-to-r from-slate-950 via-slate-900 to-slate-950 px-4 py-1 text-xs text-slate-100',
        'shadow-sm',
        className
      )}
      role="status"
      aria-live="polite"
    >
      <div className="flex items-center gap-2 font-mono uppercase tracking-wide text-[11px] text-slate-300">
        <Activity className="h-3.5 w-3.5 text-emerald-400" />
        Kernel
      </div>

      <div className="flex items-center gap-2 text-slate-200">
        <Cpu className="h-3.5 w-3.5 text-emerald-300" />
        <span className="font-semibold">{telemetry.backendLabel}</span>
      </div>

      <div className={cn('flex items-center gap-2', telemetry.metricsStale && 'text-amber-200')}>
        <HardDrive className="h-3.5 w-3.5" />
        <span className="font-semibold">{vramLabel}</span>
        {telemetry.metricsStale && <span className="text-[10px] uppercase">stale</span>}
      </div>

      <div className="flex items-center gap-2 truncate text-slate-200">
        <span className="font-semibold truncate max-w-[200px]">
          {telemetry.baseModelName || telemetry.baseModelId || 'No base model'}
        </span>
      </div>

      <div className="flex items-center gap-1 text-slate-200">
        <Clock className="h-3.5 w-3.5" />
        <span className="font-semibold">{uptimeLabel}</span>
      </div>

      <div className="ml-auto flex items-center gap-2">
        {!telemetry.metricsConnected && (
          <span className="text-[10px] uppercase text-amber-200">
            Telemetry {telemetry.metricsError ? 'error' : 'reconnecting'}
          </span>
        )}
        {showEmergencyStop && (
          <Button
            size="sm"
            variant="destructive"
            onClick={handlePanic}
            disabled={isStopping}
            className="h-7 px-3"
          >
            {isStopping ? 'Detaching…' : 'Emergency Stop'}
          </Button>
        )}
      </div>
    </div>
  );
}

export default KernelStatusBar;
