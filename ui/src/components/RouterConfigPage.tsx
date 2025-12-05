import { useCallback, useEffect, useState } from 'react';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';
import apiClient from '@/api/client';
import { RouterAdapterSummary, RouterConfigView, RoutingPolicy } from '@/api/types';
import { logger } from '@/utils/logger';

interface RouterConfigPageProps {
  selectedTenant: string;
}

function InfoRow({ label, value }: { label: string; value: string | number | undefined }) {
  return (
    <div className="flex justify-between text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value ?? '—'}</span>
    </div>
  );
}

function renderPolicy(policy?: RoutingPolicy) {
  if (!policy) {
    return <p className="text-sm text-muted-foreground">No routing policy configured for this tenant.</p>;
  }

  return (
    <div className="space-y-2 text-sm">
      <InfoRow label="Allowed stacks" value={policy.allowed_stack_ids?.join(', ') || 'Any'} />
      <InfoRow label="Allowed adapters" value={policy.allowed_adapter_ids?.join(', ') || 'Any'} />
      <InfoRow label="Denied adapters" value={policy.denied_adapter_ids?.join(', ') || 'None'} />
      <InfoRow
        label="Max adapters per token"
        value={policy.max_adapters_per_token ?? 'Router default (k_sparse)'}
      />
      <InfoRow label="Pin enforcement" value={policy.pin_enforcement ?? 'warn'} />
      <InfoRow label="Require stack" value={policy.require_stack ? 'Yes' : 'No'} />
      <InfoRow label="Require pins" value={policy.require_pins ? 'Yes' : 'No'} />
    </div>
  );
}

function renderAdapters(adapters: RouterAdapterSummary[]) {
  if (!adapters.length) {
    return <p className="text-sm text-muted-foreground">No adapters found for the effective routing set.</p>;
  }

  return (
    <div className="space-y-2">
      {adapters.map((adapter) => (
        <div
          key={adapter.adapter_id}
          className="flex items-center justify-between rounded-md border border-border p-3"
        >
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <span className="font-medium">{adapter.adapter_id}</span>
              {adapter.in_default_stack && <Badge variant="secondary">default stack</Badge>}
            </div>
            <div className="text-xs text-muted-foreground">
              {adapter.name ? `${adapter.name} • ` : ''}
              {adapter.category ?? 'category: unknown'} • {adapter.tier ?? 'tier: unknown'}
            </div>
          </div>
          <div className="text-xs text-muted-foreground text-right">
            <div>scope: {adapter.scope ?? 'n/a'}</div>
            <div>rank: {adapter.rank ?? 'n/a'} | alpha: {adapter.alpha ?? 'n/a'}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

export function RouterConfigPage({ selectedTenant }: RouterConfigPageProps) {
  const [config, setConfig] = useState<RouterConfigView | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const loadRouterConfig = useCallback(async () => {
    if (!selectedTenant) return;

    setIsLoading(true);
    setError(null);

    try {
      const response = await apiClient.getRouterConfig(selectedTenant);
      setConfig(response);
    } catch (err) {
      const parsed = err instanceof Error ? err : new Error('Failed to load router configuration');
      setError(parsed);
      logger.error('Failed to load router configuration', {
        component: 'RouterConfigPage',
        tenant: selectedTenant,
        error: parsed.message,
      });
    } finally {
      setIsLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    loadRouterConfig();
  }, [loadRouterConfig]);

  const isEmpty = !config && !isLoading;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold">Router Configuration</h1>
          <p className="text-sm text-muted-foreground">
            Read-only view of the router parameters and effective adapter set used during inference.
          </p>
        </div>
        <Button variant="outline" onClick={loadRouterConfig} disabled={isLoading}>
          <RefreshCw className={`mr-2 h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Failed to load router configuration</AlertTitle>
          <AlertDescription>{error.message}</AlertDescription>
        </Alert>
      )}

      {isLoading && (
        <Card>
          <CardHeader>
            <CardTitle>Loading router configuration…</CardTitle>
            <CardDescription>Fetching manifest-backed router settings.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-4 w-1/3" />
            <Skeleton className="h-4 w-1/4" />
          </CardContent>
        </Card>
      )}

      {isEmpty && (
        <Alert>
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>No router configuration available</AlertTitle>
          <AlertDescription>
            No configuration was found for this tenant. The router will fall back to manifest defaults.
          </AlertDescription>
        </Alert>
      )}

      {config && (
        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Router Parameters</CardTitle>
              <CardDescription>
                Derived from the active manifest so values match worker routing during inference.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              <div className="flex flex-wrap gap-2">
                <Badge variant="secondary">k={config.router.k_sparse}</Badge>
                <Badge variant="secondary">tau={config.router.tau}</Badge>
                <Badge variant="secondary">entropy floor={config.router.entropy_floor}</Badge>
                <Badge variant="secondary">quant={config.router.gate_quant}</Badge>
                <Badge variant="secondary">sample tokens={config.router.sample_tokens_full}</Badge>
                <Badge variant="outline">{config.router.algorithm}</Badge>
                {config.manifest_hash && <Badge variant="default">manifest {config.manifest_hash}</Badge>}
              </div>
              <Separator />
              <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                <InfoRow label="K-sparse (top-k)" value={config.router.k_sparse} />
                <InfoRow label="Tau (temperature)" value={config.router.tau} />
                <InfoRow label="Entropy floor" value={config.router.entropy_floor} />
                <InfoRow label="Gate quantization" value={config.router.gate_quant} />
                <InfoRow label="Sample full tokens" value={config.router.sample_tokens_full} />
                <InfoRow label="Algorithm" value={config.router.algorithm} />
                <InfoRow label="Warmup enabled" value={config.router.warmup ? 'Yes' : 'No'} />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Routing Policy</CardTitle>
              <CardDescription>Tenant execution policy constraints applied before routing.</CardDescription>
            </CardHeader>
            <CardContent>{renderPolicy(config.routing_policy)}</CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Adapters in Scope</CardTitle>
              <CardDescription>
                Effective adapter set from the tenant&apos;s default stack (if configured) or manifest adapters.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {config.stack ? (
                <div className="text-sm text-muted-foreground">
                  Default stack <span className="font-medium">{config.stack.stack_id}</span>
                  {config.stack.version !== undefined && ` • version ${config.stack.version}`}
                  {config.stack.lifecycle_state && ` • ${config.stack.lifecycle_state}`}
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">
                  No default stack set; showing manifest/tenant adapter set.
                </div>
              )}
              {renderAdapters(config.adapters)}
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

