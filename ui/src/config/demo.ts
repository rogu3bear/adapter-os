import type { SessionMode, RecentActivityEvent } from '@/api/auth-types';

export interface DemoModelState {
  id: string;
  name: string;
  sizeBytes?: number;
  format?: string;
  backend?: string;
  memoryUsageMb?: number;
  updatedAt?: string;
}

const DEMO_SYSTEM_TOASTS = [
  'Tenant A mounted Adapter X',
  'Auto-scaling worker node...',
  'Router promoting MoE experts for Finance tenant',
  'Draining legacy 7B workers…',
  'Batching high-priority eval traffic',
  'Rolling restart of telemetry agent',
  'Tenant B shipped a new adapter revision',
];

const DEMO_SCRIPT_PROMPT = `You are running an AdapterOS chaos drill for a multi-tenant MoE stack.
- Switch routing to the 30B MoE (rank-64) backend for high-TPS tenants.
- Keep Tenant A on the finance adapters; Tenant B on multilingual safety adapters.
- Run a 2-minute synthetic load test at 1.2k RPM and capture latency P95 + tail token stats.
- Generate an audit note summarizing model swaps, adapter mounts, and scaling decisions.`;

export function getDemoSystemToastMessages(): string[] {
  return DEMO_SYSTEM_TOASTS;
}

export function getDemoScriptPrompt(): string {
  return DEMO_SCRIPT_PROMPT;
}

export function getDemoDefaultModel(): DemoModelState {
  return {
    id: 'demo-7b',
    name: 'AOS 7B Instruct',
    sizeBytes: 7_200_000_000,
    format: 'gguf',
    backend: 'Metal',
    memoryUsageMb: 9200,
  };
}

export function getDemoMoEModel(): DemoModelState {
  return {
    id: 'demo-30b-moe',
    name: '30B MoE (Rank 64)',
    sizeBytes: 32_000_000_000,
    format: 'safetensors',
    backend: 'MoE',
    memoryUsageMb: 43_000,
  };
}

export function getDemoActivitySeed(): RecentActivityEvent[] {
  const now = Date.now();
  const minutes = (n: number) => new Date(now - n * 60_000).toISOString();
  return [
    {
      id: 'demo-act-1',
      actor: 'Tenant A',
      action: 'mounted adapter',
      target: 'Finance Guard v5',
      event_type: 'adapter_mounted',
      message: 'Tenant A mounted Finance Guard v5 on AOS-7B',
      timestamp: minutes(2),
      level: 'info',
      component: 'adapter',
    },
    {
      id: 'demo-act-2',
      actor: 'Tenant B',
      action: 'started training job',
      target: 'Safety-Multilingual v3',
      event_type: 'training_started',
      message: 'Training job safety-multilingual-v3 queued on 8x A100',
      timestamp: minutes(6),
      level: 'info',
      component: 'training',
    },
    {
      id: 'demo-act-3',
      actor: 'System',
      action: 'autoscaled',
      target: 'worker pool',
      event_type: 'autoscale',
      message: 'Auto-scaling worker node: gpu-pool-2 (A100-80G)',
      timestamp: minutes(9),
      level: 'info',
      component: 'orchestrator',
    },
    {
      id: 'demo-act-4',
      actor: 'Tenant A',
      action: 'published',
      target: 'Audit log',
      event_type: 'audit',
      message: 'Audit event: policy override granted for RetrievalStack-12',
      timestamp: minutes(14),
      level: 'warning',
      component: 'audit',
    },
    {
      id: 'demo-act-5',
      actor: 'Tenant B',
      action: 'completed training job',
      target: 'Adapter X v2',
      event_type: 'training_complete',
      message: 'Training job Adapter X v2 completed with P95 latency 82ms',
      timestamp: minutes(18),
      level: 'info',
      component: 'training',
    },
  ];
}

const readEnv = (): Record<string, string | undefined> => {
  const meta = typeof import.meta !== 'undefined' ? (import.meta as { env?: Record<string, string> }) : undefined;
  return meta?.env ?? {};
};

export function isDemoEnvEnabled(): boolean {
  const env = readEnv();
  return env.VITE_DEMO_MODE === 'true';
}

export function isDemoSessionMode(sessionMode?: SessionMode | null): boolean {
  return sessionMode === 'dev_bypass';
}

export function isDemoMvpMode(sessionMode?: SessionMode | null): boolean {
  return isDemoEnvEnabled() || isDemoSessionMode(sessionMode);
}

export function getDemoEntryPath(): string {
  return '/chat';
}
