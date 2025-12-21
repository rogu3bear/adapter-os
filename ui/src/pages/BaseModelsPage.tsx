import React, { useMemo, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { Database, Info, Loader2, Play, Square } from 'lucide-react';
import { toast } from 'sonner';

import { apiClient } from '@/api/services';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { formatBytes } from '@/lib/formatters';
import PageTable from '@/components/ui/PageTable';

import type { BaseModelStatus, ModelWithStatsResponse } from '@/api/types';

type BaseModelRowItem = { model: ModelWithStatsResponse; status?: BaseModelStatus };

function statusBadge(status?: BaseModelStatus) {
  if (!status) {
    return <Badge variant="secondary">Unknown</Badge>;
  }

  const variant =
    status.status === 'ready'
      ? 'default'
      : status.status === 'error'
        ? 'destructive'
        : 'secondary';

  return <Badge variant={variant}>{status.status}</Badge>;
}

function architectureText(model: ModelWithStatsResponse) {
  const arch = model.architecture;
  if (!arch) return '—';

  const parts = [
    arch.architecture,
    arch.num_layers ? `${arch.num_layers} layers` : null,
    arch.hidden_size ? `hidden ${arch.hidden_size}` : null,
    arch.vocab_size ? `vocab ${arch.vocab_size}` : null,
  ].filter(Boolean);

  return parts.join(' • ') || '—';
}

function formatMemory(mb?: number | null) {
  if (mb === null || mb === undefined) return '—';
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(2)} GB`;
  }
  return `${mb} MB`;
}

function BaseModelsAlert() {
  return (
    <Alert className="border-slate-700 bg-slate-900 text-slate-50">
      <Info className="h-4 w-4 text-blue-500" />
      <AlertTitle>Immutable base models</AlertTitle>
      <AlertDescription>
        Base models remain frozen at runtime. Adapters and routing add specialization without mutating
        base weights.
      </AlertDescription>
    </Alert>
  );
}

const SkeletonRows = () => (
  <div className="space-y-3">
    {[1, 2, 3].map((i) => (
      <div key={i} className="h-14 animate-pulse rounded-md bg-muted" />
    ))}
  </div>
);

function HashLine({ label, value }: { label: string; value?: string | null }) {
  if (!value) return null;
  return (
    <div>
      <span className="font-medium text-foreground">{label}:</span> {value}
    </div>
  );
}

function ModelNameCell({ model }: { model: ModelWithStatsResponse }) {
  return (
    <td className="px-3 py-3">
      <div className="font-medium text-foreground">{model.name}</div>
      <div className="text-xs text-foreground/80 break-all">{model.id}</div>
      {model.model_path && (
        <div className="text-xs text-foreground/70 mt-1 truncate">{model.model_path}</div>
      )}
    </td>
  );
}

function HashesCell({ model }: { model: ModelWithStatsResponse }) {
  return (
    <td className="px-3 py-3 text-xs text-foreground/80 break-all space-y-1">
      <HashLine label="Hash" value={model.hash_b3} />
      <HashLine label="Config" value={model.config_hash_b3} />
      <HashLine label="Tokenizer" value={model.tokenizer_hash_b3} />
    </td>
  );
}

function BaseModelRowView({
  model,
  status,
  onLoad,
  onUnload,
  isLoadingAction,
  isUnloadingAction,
}: BaseModelRowItem & {
  onLoad: (modelId: string) => void;
  onUnload: (modelId: string) => void;
  isLoadingAction: boolean;
  isUnloadingAction: boolean;
}) {
  const isLoaded = status?.status === 'ready';
  const isBackendLoading = status?.status === 'loading';
  const isBackendUnloading = status?.status === 'unloading';
  const disableActions = isBackendLoading || isBackendUnloading || isLoadingAction || isUnloadingAction;

  return (
    <tr className="align-top">
      <ModelNameCell model={model} />
      <HashesCell model={model} />
      <td className="px-3 py-3 text-sm text-foreground">{architectureText(model)}</td>
      <td className="px-3 py-3 text-sm text-foreground">
        {model.quantization
          ? `${model.quantization} (backend-fixed)`
          : '— (backend-fixed)'}
      </td>
      <td className="px-3 py-3 text-sm text-foreground">
        {model.size_bytes ? formatBytes(model.size_bytes) : '—'}
      </td>
      <td className="px-3 py-3 text-sm">
        <Badge variant="outline">Frozen</Badge>
      </td>
      <td className="px-3 py-3 text-sm text-foreground">{model.tenant_id || 'shared'}</td>
      <td className="px-3 py-3">{statusBadge(status)}</td>
      <td className="px-3 py-3 text-sm text-foreground">
        {formatMemory(status?.memory_usage_mb ?? null)}
      </td>
      <td className="px-3 py-3 text-sm">
        {isLoaded ? (
          <Button
            variant="outline"
            size="sm"
            disabled={disableActions}
            onClick={() => onUnload(model.id)}
            className="gap-1"
          >
            {isUnloadingAction || isBackendUnloading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Square className="h-3 w-3" />
            )}
            Unload
          </Button>
        ) : (
          <Button
            variant="default"
            size="sm"
            disabled={disableActions}
            onClick={() => onLoad(model.id)}
            className="gap-1"
          >
            {isLoadingAction || isBackendLoading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Play className="h-3 w-3" />
            )}
            {isBackendLoading ? 'Loading...' : 'Load to memory'}
          </Button>
        )}
      </td>
    </tr>
  );
}

function BaseModelsTable({
  rows,
  onLoad,
  onUnload,
  activeModelId,
  activeAction,
}: {
  rows: BaseModelRowItem[];
  onLoad: (modelId: string) => void;
  onUnload: (modelId: string) => void;
  activeModelId: string | null;
  activeAction: 'load' | 'unload' | null;
}) {
  return (
    <PageTable minWidth="md">
      <table className="min-w-full divide-y divide-border text-sm">
        <thead>
          <tr className="text-left text-xs uppercase tracking-wide text-foreground font-semibold">
            <th className="px-3 py-2">Name / ID</th>
            <th className="px-3 py-2">Hashes</th>
            <th className="px-3 py-2">Architecture</th>
            <th className="px-3 py-2">Quantization</th>
            <th className="px-3 py-2">Size</th>
            <th className="px-3 py-2">Frozen</th>
            <th className="px-3 py-2">Tenant</th>
            <th className="px-3 py-2">Status</th>
            <th className="px-3 py-2">Memory</th>
            <th className="px-3 py-2">Actions</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <BaseModelRowView
              key={row.model.id}
              {...row}
              onLoad={onLoad}
              onUnload={onUnload}
              isLoadingAction={activeAction === 'load' && activeModelId === row.model.id}
              isUnloadingAction={activeAction === 'unload' && activeModelId === row.model.id}
            />
          ))}
        </tbody>
      </table>
    </PageTable>
  );
}

function BaseModelsCard({
  rows,
  isLoading,
  selectedTenant,
  onLoad,
  onUnload,
  activeModelId,
  activeAction,
}: {
  rows: BaseModelRowItem[];
  isLoading: boolean;
  selectedTenant?: string | null;
  onLoad: (modelId: string) => void;
  onUnload: (modelId: string) => void;
  activeModelId: string | null;
  activeAction: 'load' | 'unload' | null;
}) {
  const hasRows = rows.length > 0;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <div className="flex items-center gap-2">
          <Database className="h-5 w-5 text-primary" />
          <CardTitle>Base Models</CardTitle>
        </div>
        {selectedTenant && <Badge variant="outline">Tenant: {selectedTenant}</Badge>}
      </CardHeader>
      <CardContent>
        {isLoading && !hasRows && <SkeletonRows />}
        {!isLoading && !hasRows && (
          <div className="text-sm text-foreground/80">No base models found. Import a model to get started.</div>
        )}
        {hasRows && (
          <BaseModelsTable
            rows={rows}
            onLoad={onLoad}
            onUnload={onUnload}
            activeModelId={activeModelId}
            activeAction={activeAction}
          />
        )}
      </CardContent>
    </Card>
  );
}

export default function BaseModelsPage() {
  const { selectedTenant } = useTenant();
  const [activeModelId, setActiveModelId] = useState<string | null>(null);
  const [activeAction, setActiveAction] = useState<'load' | 'unload' | null>(null);

  const {
    data: models = [],
    isLoading,
    error,
    refetch,
  } = useQuery<Array<ModelWithStatsResponse & { status?: BaseModelStatus }>>({
    queryKey: ['base-models', selectedTenant],
    queryFn: () => apiClient.listModelsWithStatus(selectedTenant || undefined),
  });

  const rows = useMemo(
    () =>
      models.map((model) => ({
        model,
        status: model.status,
      })),
    [models],
  );

  const loadMutation = useMutation({
    mutationFn: (modelId: string) => apiClient.loadBaseModel(modelId),
    onMutate: (modelId) => {
      setActiveModelId(modelId);
      setActiveAction('load');
    },
    onSuccess: (_, modelId) => {
      toast.success(`Loading model "${modelId}"...`);
      void refetch();
    },
    onError: (err, modelId) => {
      const message = err instanceof Error ? err.message : 'Failed to load model';
      toast.error(`Failed to load "${modelId}": ${message}`);
    },
    onSettled: () => {
      setActiveModelId(null);
      setActiveAction(null);
    },
  });

  const unloadMutation = useMutation({
    mutationFn: (modelId: string) => apiClient.unloadBaseModel(modelId),
    onMutate: (modelId) => {
      setActiveModelId(modelId);
      setActiveAction('unload');
    },
    onSuccess: (_, modelId) => {
      toast.success(`Unloaded model "${modelId}"`);
      void refetch();
    },
    onError: (err, modelId) => {
      const message = err instanceof Error ? err.message : 'Failed to unload model';
      toast.error(`Failed to unload "${modelId}": ${message}`);
    },
    onSettled: () => {
      setActiveModelId(null);
      setActiveAction(null);
    },
  });

  const handleLoad = (modelId: string) => loadMutation.mutate(modelId);
  const handleUnload = (modelId: string) => unloadMutation.mutate(modelId);

  const showError = error instanceof Error;

  return (
    <PageWrapper
      pageKey="base-models"
      title="Base Models"
      description="Active base models and their properties"
      brief="Base models are frozen/immutable at runtime; adapters layer on top."
      maxWidth="xl"
      contentPadding="default"
    >
      <SectionErrorBoundary sectionName="Base Models">
        <div className="space-y-4">
          <BaseModelsAlert />
          {showError && <ErrorRecovery error={error.message} onRetry={() => refetch()} />}
          <BaseModelsCard
            rows={rows}
            isLoading={isLoading}
            selectedTenant={selectedTenant}
            onLoad={handleLoad}
            onUnload={handleUnload}
            activeModelId={activeModelId}
            activeAction={activeAction}
          />
        </div>
      </SectionErrorBoundary>
    </PageWrapper>
  );
}
