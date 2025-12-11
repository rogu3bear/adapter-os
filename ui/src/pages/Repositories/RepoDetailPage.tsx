import React, { useMemo } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { Loader2, RefreshCw, Rocket, RotateCcw, Tag } from 'lucide-react';
import PageWrapper from '@/layout/PageWrapper';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import PageTable from '@/components/ui/PageTable';
import { Separator } from '@/components/ui/separator';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { toast } from 'sonner';
import {
  repoKeys,
  usePromoteRepoVersion,
  useRepo,
  useRepoTimeline,
  useRepoTrainingJobs,
  useRepoVersions,
  useRollbackRepoVersion,
  useStartTrainingFromVersion,
  useUpdateRepo,
} from '@/hooks/useReposApi';
import type { RepoTimelineEvent, RepoVersionSummary } from '@/api/repo-types';
import { useQueryClient } from '@tanstack/react-query';
import { computeVersionGuards } from './versionGuards';

function StatusBadge({ status }: { status?: string }) {
  if (!status) return <Badge variant="secondary">unknown</Badge>;
  const variant =
    status === 'healthy' ? 'default' :
    status === 'degraded' ? 'secondary' :
    status === 'archived' ? 'outline' :
    'secondary';
  return <Badge variant={variant} className="capitalize">{status}</Badge>;
}

function TimelineList({ events }: { events: RepoTimelineEvent[] }) {
  if (!events.length) {
    return <div className="text-sm text-muted-foreground">No events yet.</div>;
  }
  const sorted = [...events].sort((a, b) => b.timestamp.localeCompare(a.timestamp));
  return (
    <div className="space-y-3">
      {sorted.map(event => (
        <div key={event.id} className="rounded-md border border-border/50 bg-muted/30 p-3">
          <div className="flex items-center gap-2 text-sm">
            <Badge variant="outline" className="capitalize">{event.type.replaceAll('_', ' ')}</Badge>
            <span className="text-muted-foreground">{new Date(event.timestamp).toLocaleString()}</span>
          </div>
          <div className="mt-1 font-medium">{event.title}</div>
          {event.description && <div className="text-sm text-muted-foreground">{event.description}</div>}
        </div>
      ))}
    </div>
  );
}

function VersionRow({
  version,
  onPromote,
  onRollback,
  onView,
  onTrain,
  disabled,
}: {
  version: RepoVersionSummary;
  onPromote: () => void;
  onRollback: () => void;
  onView: () => void;
  onTrain: () => void;
  disabled: boolean;
}) {
  const { promoteDisabledReason, trainDisabledReason } = computeVersionGuards(version);
  const datasetCount = version.dataset_version_ids?.length ?? 0;
  const serveable = version.serveable ?? false;
  const serveableReason = version.serveable_reason ?? (serveable ? undefined : 'Not serveable');
  return (
    <tr className="align-top">
      <td className="px-3 py-3">
        <div className="font-medium">{version.version}</div>
        <div className="text-xs text-muted-foreground">{version.id}</div>
      </td>
      <td className="px-3 py-3 text-sm"><Badge variant="outline">{version.branch}</Badge></td>
      <td className="px-3 py-3 text-sm space-y-1">
        <div className="capitalize">{version.release_state}</div>
        <Badge
          variant={serveable ? 'secondary' : 'destructive'}
          title={serveableReason}
          className="text-xs"
        >
          {serveable ? 'Serveable' : 'Not serveable'}
        </Badge>
      </td>
      <td className="px-3 py-3 text-sm space-y-1">
        <div className="flex flex-wrap gap-1">
          {version.training_backend && (
            <Badge variant="outline">Backend: {version.training_backend}</Badge>
          )}
          {version.coreml_used && (
            <Badge variant="secondary">
              CoreML{version.coreml_device_type ? ` (${version.coreml_device_type})` : ''}
            </Badge>
          )}
        </div>
        {datasetCount > 0 ? (
          <div className="text-xs text-muted-foreground">{datasetCount} dataset version{datasetCount > 1 ? 's' : ''}</div>
        ) : (
          <span className="text-xs text-muted-foreground">No datasets</span>
        )}
      </td>
      <td className="px-3 py-3 text-sm">
        {version.metrics ? (
          <div className="space-y-1 text-xs text-muted-foreground">
            {version.metrics.reward !== undefined && <div>Reward: {version.metrics.reward ?? '—'}</div>}
            {version.metrics.latency_p50_ms !== undefined && <div>p50: {version.metrics.latency_p50_ms ?? '—'} ms</div>}
          </div>
        ) : (
          <span className="text-muted-foreground text-xs">—</span>
        )}
      </td>
      <td className="px-3 py-3 text-sm">
        <div className="flex flex-wrap gap-1">
          {(version.tags ?? []).map(tag => (
            <Badge key={tag} variant="secondary">{tag}</Badge>
          ))}
          {(!version.tags || version.tags.length === 0) && <span className="text-xs text-muted-foreground">—</span>}
        </div>
      </td>
      <td className="px-3 py-3 text-sm text-muted-foreground">{new Date(version.created_at).toLocaleString()}</td>
      <td className="px-3 py-3 text-right space-x-1">
        <Button variant="ghost" size="sm" onClick={onView}>View</Button>
        <Button
          variant="outline"
          size="sm"
          onClick={onPromote}
          disabled={disabled || Boolean(promoteDisabledReason)}
          title={promoteDisabledReason}
          data-cy={`version-promote-${version.id}`}
        >
          <Rocket className="mr-1 h-4 w-4" />
          Promote
        </Button>
        <Button variant="ghost" size="sm" onClick={onRollback} disabled={disabled}>
          <RotateCcw className="mr-1 h-4 w-4" />
          Rollback
        </Button>
        <Button
          variant="default"
          size="sm"
          onClick={onTrain}
          disabled={disabled || Boolean(trainDisabledReason)}
          title={trainDisabledReason}
          data-cy={`version-train-${version.id}`}
        >
          <RefreshCw className="mr-1 h-4 w-4" />
          Train
        </Button>
      </td>
    </tr>
  );
}

export default function RepoDetailPage() {
  const { repoId } = useParams<{ repoId: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const { data: repo, isLoading: repoLoading } = useRepo(repoId);
  const { data: versions, isLoading: versionsLoading } = useRepoVersions(repoId);
  const { data: timeline } = useRepoTimeline(repoId);
  const { data: trainingJobs } = useRepoTrainingJobs(repoId);

  const updateRepo = useUpdateRepo({
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: repoKeys.detail(repoId ?? '') });
      toast.success('Default branch updated');
    },
    onError: (error) => toast.error('Failed to update repo', { description: error.message }),
  });

  const promote = usePromoteRepoVersion(repoId ?? '', {
    onSuccess: () => toast.success('Version promoted'),
    onError: (error) => toast.error('Promote failed', { description: error.message }),
  });
  const rollback = useRollbackRepoVersion(repoId ?? '', {
    onSuccess: () => toast.success('Rollback scheduled'),
    onError: (error) => toast.error('Rollback failed', { description: error.message }),
  });
  const startTrain = useStartTrainingFromVersion(repoId ?? '', {
    onSuccess: () => toast.success('Training started'),
    onError: (error) => toast.error('Failed to start training', { description: error.message }),
  });

  const branchOptions = useMemo(() => repo?.branches ?? [], [repo?.branches]);
  const promoteReadyIds = useMemo(
    () =>
      (versions ?? [])
        .filter(v => (v.release_state === 'ready' || v.release_state === 'deprecated') && v.serveable !== false)
        .map(v => v.id),
    [versions]
  );

  const handleBranchChange = (branch: string) => {
    if (!repoId) return;
    updateRepo.mutate({ id: repoId, data: { default_branch: branch } });
  };

  if (!repoId) {
    return <div className="text-sm text-muted-foreground">Missing repo id.</div>;
  }

  const isBusy = promote.isPending || rollback.isPending || startTrain.isPending;

  return (
    <PageWrapper
      pageKey="repo-detail"
      title={repo?.name ?? 'Repository'}
      description="Timeline, versions, and training runs for this repository."
      badges={repo ? [{ label: repo.base_model, variant: 'secondary' }] : undefined}
      secondaryActions={[
        repo?.default_branch ? { label: `Default: ${repo.default_branch}`, icon: Tag } : undefined,
      ].filter(Boolean) as { label: string; icon?: React.ComponentType }[]}
    >
      {repoLoading && (
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span>Loading repository…</span>
        </div>
      )}

      {repo && (
        <Card className="mb-4">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <span>{repo.name}</span>
              <StatusBadge status={repo.status} />
            </CardTitle>
            <CardDescription>ID: {repo.id}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap items-center gap-3 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Base model</span>
                <Badge variant="outline">{repo.base_model}</Badge>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Default branch</span>
                <Select
                  value={repo.default_branch}
                  onValueChange={handleBranchChange}
                  disabled={updateRepo.isPending}
                >
                  <SelectTrigger className="w-[180px]">
                    <SelectValue placeholder="Choose branch" />
                  </SelectTrigger>
                  <SelectContent>
                    {branchOptions.map(branch => (
                      <SelectItem key={branch.name} value={branch.name}>
                        {branch.name} {branch.default ? '(default)' : ''}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Tags</span>
                <div className="flex flex-wrap gap-1">
                  {(repo.tags ?? []).map(tag => (
                    <Badge key={tag} variant="secondary">{tag}</Badge>
                  ))}
                  {(!repo.tags || repo.tags.length === 0) && (
                    <span className="text-xs text-muted-foreground">None</span>
                  )}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Versions</CardTitle>
            <CardDescription>Promote, rollback, or train from repository versions.</CardDescription>
          </CardHeader>
          <CardContent>
            {versionsLoading && (
              <div className="flex items-center gap-2 text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>Loading versions…</span>
              </div>
            )}
            {!versionsLoading && (
              <PageTable>
                <table className="min-w-full text-sm">
                  <thead>
                    <tr className="text-left text-muted-foreground">
                      <th className="px-3 py-2 font-medium">Version</th>
                      <th className="px-3 py-2 font-medium">Branch</th>
                      <th className="px-3 py-2 font-medium">Release state</th>
                      <th className="px-3 py-2 font-medium">Data / backend</th>
                      <th className="px-3 py-2 font-medium">Key metrics</th>
                      <th className="px-3 py-2 font-medium">Tags</th>
                      <th className="px-3 py-2 font-medium">Created</th>
                      <th className="px-3 py-2 font-medium text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(versions ?? []).map(v => (
                      <VersionRow
                        key={v.id}
                        version={v}
                        disabled={isBusy}
                        onPromote={() => promote.mutate({ versionId: v.id })}
                        onRollback={() => rollback.mutate({ versionId: v.id })}
                        onView={() => navigate(`/repos/${repoId}/versions/${v.id}`)}
                        onTrain={() => startTrain.mutate({ versionId: v.id, payload: {} })}
                      />
                    ))}
                    {(versions ?? []).length === 0 && (
                      <tr>
                        <td colSpan={7} className="px-3 py-6 text-center text-sm text-muted-foreground">
                          No versions yet.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </PageTable>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Timeline</CardTitle>
            <CardDescription>Adapter version history and training events.</CardDescription>
          </CardHeader>
          <CardContent>
            <TimelineList events={timeline ?? []} />
          </CardContent>
        </Card>
      </div>

      <Separator className="my-6" />

      <Card>
        <CardHeader>
          <CardTitle>Training jobs</CardTitle>
          <CardDescription>Links to jobs and produced versions.</CardDescription>
        </CardHeader>
        <CardContent>
          <PageTable>
            <table className="min-w-full text-sm">
              <thead>
                <tr className="text-left text-muted-foreground">
                  <th className="px-3 py-2 font-medium">Job</th>
                  <th className="px-3 py-2 font-medium">Status</th>
                  <th className="px-3 py-2 font-medium">Version</th>
                  <th className="px-3 py-2 font-medium">Started</th>
                  <th className="px-3 py-2 font-medium text-right">Open</th>
                </tr>
              </thead>
              <tbody>
                {(trainingJobs ?? []).map(job => (
                  <tr key={job.id}>
                    <td className="px-3 py-3 text-sm font-medium">{job.id}</td>
                    <td className="px-3 py-3 text-sm capitalize">{job.status}</td>
                    <td className="px-3 py-3 text-sm">
                      {job.version_id ? (
                        <Button variant="link" size="sm" onClick={() => navigate(`/repos/${repoId}/versions/${job.version_id}`)}>
                          {job.version_id}
                        </Button>
                      ) : (
                        <span className="text-muted-foreground text-xs">—</span>
                      )}
                    </td>
                    <td className="px-3 py-3 text-sm text-muted-foreground">
                      {job.created_at ? new Date(job.created_at).toLocaleString() : '—'}
                    </td>
                    <td className="px-3 py-3 text-right">
                      <Button variant="ghost" size="sm" onClick={() => navigate(`/training/jobs/${job.id}`)}>
                        Open
                      </Button>
                    </td>
                  </tr>
                ))}
                {(trainingJobs ?? []).length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-3 py-6 text-center text-sm text-muted-foreground">
                      No training jobs yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </PageTable>
        </CardContent>
      </Card>
    </PageWrapper>
  );
}
