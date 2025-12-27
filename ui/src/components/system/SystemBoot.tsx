import { useEffect, useMemo, useRef, useState } from 'react';
import { Activity, AlertTriangle, Power, Terminal } from 'lucide-react';
import { apiClient } from '@/api/services';
import { useKernelTelemetry } from '@/contexts/KernelTelemetryContext';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

type BootPhase = 'hidden' | 'booting' | 'panic';

interface BootStateSnapshot {
  phase: BootPhase;
  message?: string;
  since?: number | null;
}

interface WorkerCheck {
  ok?: boolean;
  hint?: string;
}

interface Checks {
  worker?: WorkerCheck;
  models_seeded?: { ok?: boolean };
}

interface ReadyzResponse {
  ready?: boolean;
  checks?: Checks;
}

interface DetailedHealthResponse {
  ready?: boolean;
  state?: string;
  boot_elapsed_ms?: number;
}

export function SystemBoot() {
  const telemetry = useKernelTelemetry();
  const [state, setState] = useState<BootStateSnapshot>({ phase: 'hidden' });
  const [restartPending, setRestartPending] = useState(false);
  const metricsOfflineAtRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const [health, readyz, detailed] = await Promise.all([
          apiClient.getHealthz(),
          apiClient.getReadyz() as Promise<ReadyzResponse>,
          apiClient.getSystemReady().catch(() => null) as Promise<DetailedHealthResponse | null>,
        ]);

        if (cancelled) return;

        const workerCheck = readyz.checks?.worker;

        if (workerCheck?.ok === false && workerCheck.hint) {
          setState({ phase: 'panic', message: workerCheck.hint });
          return;
        }

        const readyFlag =
          readyz.ready !== false &&
          (detailed ? detailed.ready !== false : true);

        if (readyFlag && health.status === 'healthy') {
          setState((prev) => (prev.phase === 'hidden' ? prev : { phase: 'hidden' }));
          return;
        }

        if (health.status && health.status !== 'healthy' && health.status !== 'ok') {
          setState({ phase: 'panic', message: 'Kernel health degraded' });
          return;
        }

        const detailedState = detailed?.state;
        const stateLabel = detailedState
          || (readyz.checks?.models_seeded?.ok
            ? 'Loading base model'
            : 'Seeding models');

        const bootSince = detailed?.boot_elapsed_ms ?? null;

        setState({
          phase: 'booting',
          message: stateLabel,
          since: bootSince,
        });
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : 'Kernel panic detected';
        setState({ phase: 'panic', message });
      }
    };

    poll();
    const timer = setInterval(poll, 4000);

    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (telemetry.metricsConnected) {
      metricsOfflineAtRef.current = null;
      return;
    }

    if (!metricsOfflineAtRef.current) {
      metricsOfflineAtRef.current = Date.now();
      return;
    }

    if (telemetry.metricsError && Date.now() - metricsOfflineAtRef.current > 10000) {
      setState((prev) => (prev.phase === 'panic' ? prev : { phase: 'panic', message: 'Telemetry link lost' }));
    }
  }, [telemetry.metricsConnected, telemetry.metricsError]);

  const handleRestart = async () => {
    setRestartPending(true);
    try {
      await apiClient.restartSystem();
      setState({ phase: 'booting', message: 'Restarting worker', since: null });
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Restart failed';
      setState({ phase: 'panic', message });
    } finally {
      setRestartPending(false);
    }
  };

  const showOverlay = state.phase === 'booting' || state.phase === 'panic';
  const uptimeText = useMemo(() => {
    if (!telemetry.uptimeSeconds && telemetry.uptimeSeconds !== 0) return null;
    const mins = Math.floor((telemetry.uptimeSeconds ?? 0) / 60);
    return `${mins}m uptime`;
  }, [telemetry.uptimeSeconds]);

  if (!showOverlay) {
    return null;
  }

  if (state.phase === 'panic') {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-blue-900 text-blue-50">
        <div className="max-w-xl w-full px-6 py-8 text-center space-y-4">
          <div className="flex items-center justify-center gap-2 text-lg font-semibold uppercase tracking-wide">
            <AlertTriangle className="h-5 w-5" />
            Kernel Panic
          </div>
          <p className="text-sm text-blue-100">
            {state.message || 'Worker became unreachable. Restart to recover the kernel.'}
          </p>
          <div className="flex items-center justify-center gap-3 text-xs text-blue-200">
            <Activity className="h-4 w-4" />
            <span>{telemetry.backendLabel} backend</span>
            {uptimeText && (
              <>
                <span className="opacity-50">•</span>
                <span>{uptimeText}</span>
              </>
            )}
          </div>
          <div className="flex items-center justify-center gap-2">
            <Button variant="secondary" onClick={handleRestart} disabled={restartPending}>
              {restartPending ? 'Restarting…' : 'Restart Worker'}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-40 bg-slate-950 text-emerald-200">
      <div className="max-w-4xl mx-auto h-full flex flex-col py-10 px-6">
        <div className="flex items-center gap-2 text-sm font-mono uppercase text-emerald-300">
          <Terminal className="h-4 w-4" />
          AdapterOS boot sequence
        </div>

        <div className="mt-6 flex flex-col gap-2 text-xs font-mono">
          <div className="flex items-center gap-3">
            <span className="text-emerald-400">{'>'} backend</span>
            <span>{telemetry.backendLabel}</span>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-emerald-400">{'>'} model</span>
            <span>{telemetry.baseModelName || telemetry.baseModelId || 'loading...'}</span>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-emerald-400">{'>'} status</span>
            <span>{state.message || 'Initializing kernel...'}</span>
          </div>
          {state.since && (
            <div className="flex items-center gap-3">
              <span className="text-emerald-400">{'>'} elapsed</span>
              <span>{Math.round((state.since || 0) / 1000)}s</span>
            </div>
          )}
        </div>

        <div className="mt-auto flex items-center gap-2 text-[11px] text-emerald-300">
          <Power className={cn('h-4 w-4 animate-pulse', restartPending && 'animate-none opacity-50')} />
          <span>Kernel warming up — interface will unlock once ready.</span>
        </div>
      </div>
    </div>
  );
}

export default SystemBoot;
