import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import type { RunMetadata } from '@/types/components';
import { Download } from 'lucide-react';
import { cn } from '@/lib/utils';

interface RunEvidencePanelProps {
  evidence?: RunMetadata;
  traceId?: string | null;
  fallbackPolicyMask?: string | null;
  fallbackPlanId?: string | null;
  manifestFallback?: string | null;
  workspaceIdFallback?: string | null;
  pending?: boolean;
  showSeedValue?: boolean;
  onExport?: () => void;
  className?: string;
}

export function RunEvidencePanel({
  evidence,
  traceId,
  fallbackPolicyMask,
  fallbackPlanId,
  manifestFallback,
  workspaceIdFallback,
  pending = false,
  showSeedValue = false,
  onExport,
  className,
}: RunEvidencePanelProps) {
  const evidenceRecord = evidence as Record<string, unknown> | undefined;
  const lookup = (keys: string[]): string | undefined => {
    for (const key of keys) {
      const value = evidenceRecord?.[key];
      if (value === undefined || value === null) continue;
      if (typeof value === 'string') return value;
      if (typeof value === 'number' || typeof value === 'boolean') return String(value);
    }
    return undefined;
  };

  const runIdPrimary = lookup(['runId', 'run_id']);
  const runId = runIdPrimary || evidence?.requestId || evidence?.traceId || traceId || undefined;
  const runIdFallback = !runIdPrimary && Boolean(runId);

  const workspaceIdPrimary = lookup(['workspaceId', 'workspace_id', 'tenantId', 'tenant_id']);
  const workspaceId = workspaceIdPrimary || workspaceIdFallback || undefined;
  const workspaceFallback = !workspaceIdPrimary && Boolean(workspaceIdFallback);

  const manifestPrimary = lookup(['manifestHashB3', 'manifest_hash_b3']);
  const manifestHashB3 = manifestPrimary || manifestFallback || undefined;
  const manifestFallbackUsed = !manifestPrimary && Boolean(manifestFallback);

  const policyMaskPrimary = lookup(['policyMaskDigestB3', 'policy_mask_digest_b3', 'policy_mask_digest']);
  const policyMaskDigestB3 = policyMaskPrimary || fallbackPolicyMask || undefined;
  const policyMaskFallbackUsed = !policyMaskPrimary && Boolean(fallbackPolicyMask);

  const planPrimary = lookup(['planId', 'plan_id']);
  const planId = planPrimary || fallbackPlanId || undefined;
  const planFallbackUsed = !planPrimary && Boolean(fallbackPlanId);
  const routerSeed = lookup(['routerSeed', 'router_seed']);
  const tick = lookup(['tick']);
  const workerId = lookup(['workerId', 'worker_id']);
  const reasoningMode = lookup(['reasoningMode', 'reasoning_mode']);
  const determinismVersion = lookup(['determinismVersion', 'determinism_version']);
  const bootTraceId = lookup(['bootTraceId', 'boot_trace_id']);
  const createdAt = lookup(['createdAt', 'created_at']);

  const notSetLabel = pending ? 'Pending' : 'Not set';
  const routerSeedLabel =
    routerSeed
      ? (showSeedValue ? routerSeed : 'hidden')
      : pending
        ? 'Pending'
        : 'Not set';

  const rows = [
    { label: 'run_id', value: runId ?? notSetLabel, fallback: runIdFallback },
    { label: 'workspace_id', value: workspaceId ?? notSetLabel, fallback: workspaceFallback },
    { label: 'manifest_hash_b3', value: manifestHashB3 ?? notSetLabel, fallback: manifestFallbackUsed },
    { label: 'plan_id', value: planId ?? notSetLabel, fallback: planFallbackUsed },
    { label: 'policy_mask_digest_b3', value: policyMaskDigestB3 ?? notSetLabel, fallback: policyMaskFallbackUsed },
    { label: 'router_seed', value: routerSeedLabel },
    { label: 'tick', value: tick ?? notSetLabel },
    { label: 'worker_id', value: workerId ?? notSetLabel },
    { label: 'reasoning_mode', value: reasoningMode ?? notSetLabel },
    { label: 'determinism_version', value: determinismVersion ?? notSetLabel },
    { label: 'boot_trace_id', value: bootTraceId ?? notSetLabel },
    { label: 'created_at', value: createdAt ?? notSetLabel },
  ];

  return (
    <div className={cn('rounded-md border border-dashed bg-muted/30 p-3 text-xs', className)}>
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-sm">Run evidence</span>
          {pending && <Badge variant="outline">streaming</Badge>}
        </div>
        {onExport && (
          <Button
            size="xs"
            variant="ghost"
            className="gap-1"
            onClick={onExport}
            disabled={!runId}
          >
            <Download className="h-3 w-3" />
            Export evidence
          </Button>
        )}
      </div>
      <div className="mt-2 grid gap-2 sm:grid-cols-2">
        {rows.map((row) => (
          <div key={row.label} className="flex items-center justify-between gap-2">
            <span className="text-muted-foreground">{row.label}</span>
            <span className="font-mono text-[11px] break-all text-foreground">
              {row.value ?? '—'}
              {row.fallback ? ' (fallback)' : ''}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export default RunEvidencePanel;
