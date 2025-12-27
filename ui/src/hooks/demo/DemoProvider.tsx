import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import type { ReactNode } from 'react';
import type { SystemMetrics } from '@/api/api-types';
import type { RecentActivityEvent } from '@/api/auth-types';
import { useAuth } from '@/providers/CoreProviders';
import {
  getDemoActivitySeed,
  getDemoDefaultModel,
  getDemoMoEModel,
  getDemoScriptPrompt,
  getDemoSystemToastMessages,
  isDemoMvpMode,
  type DemoModelState,
} from '@/config/demo';

interface DemoContextValue {
  enabled: boolean;
  simulateTraffic: boolean;
  setSimulateTraffic: (value: boolean) => void;
  activeModel: DemoModelState;
  modelSwitching: boolean;
  switchToMoE: () => void;
  seededActivity: RecentActivityEvent[];
  simulatedMetrics: { metrics: SystemMetrics; updatedAt: Date } | null;
  demoScript: string;
}

const DemoContext = createContext<DemoContextValue | null>(null);

function buildSimulatedMetrics(tick: number): SystemMetrics {
  const cpu = 55 + 22 * Math.sin(tick / 4);
  const requests = 900 + 320 * (Math.sin(tick / 3) + 1);
  const tokens = 1200 + 650 * Math.abs(Math.sin(tick / 2.5));
  const latency = 85 + 25 * Math.sin(tick / 5 + Math.PI / 4);
  const memory = 62 + 8 * Math.sin(tick / 4 + Math.PI / 6);
  const errorRate = 0.5 + 0.4 * Math.abs(Math.sin(tick / 6));
  return {
    cpu_usage_percent: Math.min(96, Math.max(28, cpu)),
    memory_usage_percent: Math.min(92, Math.max(35, memory)),
    tokens_per_second: Math.max(0, Math.round(tokens)),
    active_sessions: Math.max(40, Math.round(requests / 8)),
    latency_p95_ms: Math.max(40, Math.round(latency)),
    error_rate: Number(errorRate.toFixed(2)),
    network_rx_bytes: Math.round(requests * 64),
    network_tx_bytes: Math.round(requests * 72),
    adapter_count: 18,
    disk_usage_percent: 71 + (Math.sin(tick / 9) * 4),
    gpu_utilization_percent: 62 + (Math.sin(tick / 7) * 12),
  };
}

export function DemoProvider({ children }: { children: ReactNode }) {
  const { sessionMode, user } = useAuth();
  const [simulateTraffic, setSimulateTraffic] = useState(false);
  const [activeModel, setActiveModel] = useState<DemoModelState>(getDemoDefaultModel);
  const [modelSwitching, setModelSwitching] = useState(false);
  const [seededActivity, setSeededActivity] = useState<RecentActivityEvent[]>([]);
  const [simulatedMetrics, setSimulatedMetrics] = useState<{ metrics: SystemMetrics; updatedAt: Date } | null>(null);
  const toastTimerRef = useRef<number | null>(null);
  const metricsTimerRef = useRef<number | null>(null);
  const lastUserIdRef = useRef<string | null>(user?.id ?? null);

  const enabled = isDemoMvpMode(sessionMode);
  const demoScript = useMemo(() => getDemoScriptPrompt(), []);

  const resetDemoState = useCallback(() => {
    setSimulateTraffic(false);
    setActiveModel(getDemoDefaultModel());
    setModelSwitching(false);
    setSeededActivity([]);
    setSimulatedMetrics(null);
  }, []);

  // Reset when user changes or demo mode is disabled
  useEffect(() => {
    if (!enabled) {
      resetDemoState();
      return;
    }
    if (lastUserIdRef.current !== user?.id) {
      lastUserIdRef.current = user?.id ?? null;
      resetDemoState();
    }
  }, [enabled, resetDemoState, user?.id]);

  // Seed recent activity when entering demo mode
  useEffect(() => {
    if (!enabled) return;
    setSeededActivity(getDemoActivitySeed());
  }, [enabled]);

  // Background narrative toasts
  useEffect(() => {
    if (!enabled) return;
    const messages = getDemoSystemToastMessages();
    let cancelled = false;

    const scheduleToast = () => {
      const delay = 4500 + Math.random() * 5200;
      toastTimerRef.current = window.setTimeout(() => {
        if (cancelled) return;
        const message = messages[Math.floor(Math.random() * messages.length)];
        toast.info(message, { duration: 3200 });
        scheduleToast();
      }, delay) as unknown as number;
    };

    scheduleToast();

    return () => {
      cancelled = true;
      if (toastTimerRef.current) {
        clearTimeout(toastTimerRef.current);
        toastTimerRef.current = null;
      }
    };
  }, [enabled]);

  // Simulated metrics wave
  useEffect(() => {
    if (!enabled || !simulateTraffic) {
      if (metricsTimerRef.current) {
        clearInterval(metricsTimerRef.current);
        metricsTimerRef.current = null;
      }
      setSimulatedMetrics(null);
      return;
    }
    let tick = 0;
    metricsTimerRef.current = window.setInterval(() => {
      tick += 1;
      setSimulatedMetrics({
        metrics: buildSimulatedMetrics(tick),
        updatedAt: new Date(),
      });
    }, 950) as unknown as number;

    return () => {
      if (metricsTimerRef.current) {
        clearInterval(metricsTimerRef.current);
        metricsTimerRef.current = null;
      }
    };
  }, [enabled, simulateTraffic]);

  const switchToMoE = useCallback(() => {
    if (!enabled || modelSwitching) return;
    setModelSwitching(true);

    const loadingId = toast.loading('Unloading 7B...', { duration: 2800 });

    window.setTimeout(() => {
      toast.loading('Mounting 30B MoE (Rank 64)...', {
        id: loadingId,
        duration: 3000,
      });
    }, 1100);

    window.setTimeout(() => {
      setActiveModel(getDemoMoEModel());
      toast.success('30B MoE ready for inference', { id: loadingId, duration: 3200 });
      setModelSwitching(false);
    }, 3000);
  }, [enabled, modelSwitching]);

  const value: DemoContextValue = useMemo(
    () => ({
      enabled,
      simulateTraffic,
      setSimulateTraffic,
      activeModel,
      modelSwitching,
      switchToMoE,
      seededActivity,
      simulatedMetrics,
      demoScript,
    }),
    [
      activeModel,
      demoScript,
      enabled,
      modelSwitching,
      seededActivity,
      simulateTraffic,
      simulatedMetrics,
      switchToMoE,
    ],
  );

  return <DemoContext.Provider value={value}>{children}</DemoContext.Provider>;
}

export function useDemoMode(): DemoContextValue {
  const ctx = useContext(DemoContext);
  if (!ctx) {
    return {
      enabled: false,
      simulateTraffic: false,
      setSimulateTraffic: () => {},
      activeModel: getDemoDefaultModel(),
      modelSwitching: false,
      switchToMoE: () => {},
      seededActivity: [],
      simulatedMetrics: null,
      demoScript: getDemoScriptPrompt(),
    };
  }
  return ctx;
}
