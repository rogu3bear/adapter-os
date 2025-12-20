import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { apiClient } from '@/api/services';
import type { PilotStatusResponse } from '@/api/pilot-status-types';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { CheckCircle2, RefreshCcw, XCircle } from 'lucide-react';

function formatKVs(record: Record<string, number> | undefined): string {
  if (!record) return '';
  const entries = Object.entries(record);
  if (entries.length === 0) return '';
  entries.sort(([a], [b]) => a.localeCompare(b));
  return entries.map(([k, v]) => `${k}:${v}`).join('  ');
}

function statusBadge(ok: boolean) {
  return (
    <Badge variant={ok ? 'success' : 'error'} className="gap-1">
      {ok ? <CheckCircle2 className="h-3 w-3" /> : <XCircle className="h-3 w-3" />}
      {ok ? 'OK' : 'FAIL'}
    </Badge>
  );
}

function StatusRow({
  label,
  ok,
  details,
}: {
  label: string;
  ok: boolean;
  details?: string;
}) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-border/60 py-3 last:border-b-0">
      <div className="min-w-0">
        <div className="font-medium">{label}</div>
        {details ? <div className="text-sm text-muted-foreground break-words">{details}</div> : null}
      </div>
      <div className="shrink-0">{statusBadge(ok)}</div>
    </div>
  );
}

function trainingDetails(data: PilotStatusResponse | undefined): string {
  if (!data) return '';
  if (data.training_error) return `error: ${data.training_error}`;
  if (!data.last_training_job) return 'none';
  const job = data.last_training_job;
  const parts = [
    `id=${job.id}`,
    `status=${job.status}`,
    `started_at=${job.started_at}`,
    job.completed_at ? `completed_at=${job.completed_at}` : null,
    job.adapter_name ? `adapter=${job.adapter_name}` : null,
    job.repo_id ? `repo=${job.repo_id}` : null,
  ].filter(Boolean) as string[];
  return parts.join('  ');
}

export default function PilotStatusPage() {
  const query = useQuery({
    queryKey: ['pilotStatus'],
    queryFn: () => apiClient.getPilotStatus(),
    refetchOnWindowFocus: false,
  });

  const data = query.data;

  const overallReady = useMemo(() => {
    if (!data) return false;
    return data.db_ready && data.worker_registered && data.models_seeded;
  }, [data]);

  const headerBadges = useMemo(() => {
    if (!data) {
      return [{ label: 'LOADING', variant: 'neutral' as const }];
    }
    return [
      {
        label: overallReady ? 'READY' : 'NEEDS ATTENTION',
        variant: overallReady ? ('success' as const) : ('warning' as const),
      },
    ];
  }, [data, overallReady]);

  return (
    <FeatureLayout
      title="Pilot Status"
      description="Quick checks for pilot readiness"
      maxWidth="xl"
      badges={headerBadges}
      headerActions={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => query.refetch()}
            disabled={query.isFetching}
          >
            <RefreshCcw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </div>
      }
    >
      {query.error ? (
        <Card className="border-destructive bg-destructive/10 mb-6">
          <CardContent className="pt-6">
            <div className="text-destructive">
              Failed to load pilot status: {query.error instanceof Error ? query.error.message : String(query.error)}
            </div>
          </CardContent>
        </Card>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>Checks</CardTitle>
          <CardDescription>
            Tenant: {data?.tenant_id ?? '—'}{data?.timestamp ? ` · ${new Date(data.timestamp * 1000).toLocaleString()}` : ''}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <StatusRow label="API ready" ok={data?.api_ready ?? false} />
          <StatusRow
            label="DB ready"
            ok={data?.db_ready ?? false}
            details={data?.db_error ? `error: ${data.db_error}` : undefined}
          />
          <StatusRow
            label="Worker registered"
            ok={data?.worker_registered ?? false}
            details={
              data
                ? `${data.workers_total} workers${data.workers_error ? ` · error: ${data.workers_error}` : ''}${data.worker_status_counts ? ` · ${formatKVs(data.worker_status_counts)}` : ''}`
                : undefined
            }
          />
          <StatusRow
            label="Models seeded"
            ok={data?.models_seeded ?? false}
            details={
              data
                ? `${data.models_total} models${data.models_error ? ` · error: ${data.models_error}` : ''}${data.model_names?.length ? ` · ${data.model_names.join(', ')}` : ''}`
                : undefined
            }
          />
          <div className="flex items-start justify-between gap-4 border-b border-border/60 py-3 last:border-b-0">
            <div className="min-w-0">
              <div className="font-medium">Last training job</div>
              <div className="text-sm text-muted-foreground break-words">{trainingDetails(data)}</div>
            </div>
            <div className="shrink-0">
              <Badge variant="secondary">INFO</Badge>
            </div>
          </div>
        </CardContent>
      </Card>
    </FeatureLayout>
  );
}

