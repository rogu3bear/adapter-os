import React, { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ArrowRight, Loader2, PlusCircle } from 'lucide-react';
import PageWrapper from '@/layout/PageWrapper';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import PageTable from '@/components/ui/PageTable';
import { Separator } from '@/components/ui/separator';
import { useCreateRepo, useRepos } from '@/hooks/api/useReposApi';
import type { RepoSummary } from '@/api/repo-types';
import type { AdapterHealthFlag } from '@/api/adapter-types';
import { HealthBadge } from '@/components/shared/TrustHealthBadge';
import { isAdapterHealthFlag } from '@/utils/typeGuards';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { toast } from 'sonner';
import { buildRepoDetailLink, buildTrainingJobsLink, buildAdaptersListLink } from '@/utils/navLinks';

export interface ReposByBaseModel {
  baseModel: string;
  repos: RepoSummary[];
}

export function groupReposByBaseModel(repos?: RepoSummary[]): ReposByBaseModel[] {
  if (!repos || repos.length === 0) {
    return [];
  }

  const grouped = repos.reduce((acc, repo) => {
    const baseModel = repo.base_model;
    if (!acc[baseModel]) {
      acc[baseModel] = [];
    }
    acc[baseModel].push(repo);
    return acc;
  }, {} as Record<string, RepoSummary[]>);

  return Object.entries(grouped).map(([baseModel, repos]) => ({
    baseModel,
    repos,
  }));
}

function CreateRepoDialog({
  open,
  onOpenChange,
  onCreate,
  isSubmitting,
}: {
  open: boolean;
  onOpenChange: (next: boolean) => void;
  onCreate: (payload: { name: string; base_model: string; default_branch: string }) => void;
  isSubmitting: boolean;
}) {
  const [name, setName] = useState('');
  const [baseModel, setBaseModel] = useState('');
  const [defaultBranch, setDefaultBranch] = useState('main');

  const submitDisabled = !name || !baseModel || !defaultBranch || isSubmitting;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create repository</DialogTitle>
          <DialogDescription>Track adapter versions per base model and branch.</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="repo-name">Repository name</Label>
            <Input
              id="repo-name"
              data-cy="repo-name-input"
              data-testid="repo-name"
              placeholder="research-notebook"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="repo-base-model">Base model</Label>
            <Input
              id="repo-base-model"
              data-cy="repo-base-model-input"
              placeholder="qwen2.5-7b"
              value={baseModel}
              onChange={(e) => setBaseModel(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="repo-default-branch">Default branch</Label>
            <Input
              id="repo-default-branch"
              data-cy="repo-default-branch-input"
              placeholder="main"
              value={defaultBranch}
              onChange={(e) => setDefaultBranch(e.target.value)}
            />
          </div>
        </div>
        <DialogFooter className="mt-4">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            onClick={() => onCreate({ name, base_model: baseModel, default_branch: defaultBranch })}
            disabled={submitDisabled}
            data-cy="repo-create-submit"
            data-testid="repo-submit"
          >
            {isSubmitting ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default function RepositoriesPage() {
  const navigate = useNavigate();
  const { data: repos, isLoading, isError } = useRepos();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [baseModelFilter, setBaseModelFilter] = useState<string>('all');
  const [healthFilter, setHealthFilter] = useState<AdapterHealthFlag | 'all'>('all');
  const [tenantFilter, setTenantFilter] = useState<string>('all');

  const createRepo = useCreateRepo({
    onSuccess: (repo) => {
      toast.success('Repository created', { description: repo.name });
      setDialogOpen(false);
      navigate(buildRepoDetailLink(repo.id));
    },
    onError: (error) => {
      toast.error('Failed to create repository', { description: error.message });
    },
  });

  const repoList: RepoSummary[] = repos ?? [];

  const baseModelOptions = useMemo(
    () => Array.from(new Set(repoList.map(r => r.base_model))).filter(Boolean),
    [repoList]
  );

  const tenantOptions = useMemo(
    () => Array.from(new Set(repoList.map(r => r.tenant_id ?? 'unknown'))),
    [repoList]
  );

  const filteredRepos = useMemo(() => {
    return repoList
      .filter(repo => {
        if (search.trim()) {
          const term = search.toLowerCase();
          if (!repo.name.toLowerCase().includes(term) && !repo.id.toLowerCase().includes(term)) {
            return false;
          }
        }
        if (baseModelFilter !== 'all' && repo.base_model !== baseModelFilter) return false;
        if (healthFilter !== 'all') {
          const activeVersion =
            repo.branches?.find(b => b.default)?.latest_active_version ||
            repo.branches?.map(b => b.latest_active_version).find(Boolean);
          const health = (activeVersion?.health_state ?? 'unknown') as AdapterHealthFlag | 'unknown';
          if (health !== healthFilter) return false;
        }
        if (tenantFilter !== 'all') {
          const tenantId = repo.tenant_id ?? 'unknown';
          if (tenantId !== tenantFilter) return false;
        }
        return true;
      })
      .sort((a, b) => new Date(b.updated_at || b.created_at).getTime() - new Date(a.updated_at || a.created_at).getTime());
  }, [repoList, search, baseModelFilter, healthFilter, tenantFilter]);

  const openDetail = (id: string) => navigate(buildRepoDetailLink(id));

  return (
    <PageWrapper
      pageKey="repos"
      title="Repositories"
      description="Adapter repositories with active versions, backend metadata, and health."
      primaryAction={{
        label: 'Create repo',
        icon: PlusCircle,
        onClick: () => setDialogOpen(true),
      }}
    >
      <div data-cy="repos-page" className="space-y-4">
      <Card className="mb-4">
        <CardHeader className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle className="text-lg">Repository overview</CardTitle>
            <CardDescription>Filter by base model, health, tenant, or name.</CardDescription>
          </div>
          <Button data-cy="create-repo-btn" data-testid="repo-create" onClick={() => setDialogOpen(true)}>
            <PlusCircle className="h-4 w-4 mr-2" />
            Create repository
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
            <span>Repos: {repoList.length}</span>
            <Separator orientation="vertical" className="hidden sm:inline h-6" />
            <span>Base models: {baseModelOptions.length}</span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={() => navigate(buildTrainingJobsLink())}>
              Training jobs
            </Button>
            <Button variant="secondary" onClick={() => navigate(buildAdaptersListLink())}>
              Adapters
            </Button>
          </div>
        </CardContent>
      </Card>

      {isLoading && (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">
            <Loader2 className="h-6 w-6 animate-spin mx-auto mb-2" />
            Loading repositories...
          </CardContent>
        </Card>
      )}

      {isError && (
        <Card>
          <CardHeader>
            <CardTitle>Unable to load repositories</CardTitle>
            <CardDescription>Check API connectivity and try again.</CardDescription>
          </CardHeader>
        </Card>
      )}

      {!isLoading && !isError && filteredRepos.length === 0 && (
        <Card>
          <CardHeader>
            <CardTitle>No repositories yet</CardTitle>
            <CardDescription>Create your first repo to track versions.</CardDescription>
          </CardHeader>
          <CardContent>
            <Button data-cy="create-repo-btn" data-testid="repo-create" onClick={() => setDialogOpen(true)}>
              <PlusCircle className="h-4 w-4 mr-2" />
              Create repository
            </Button>
          </CardContent>
        </Card>
      )}

      {!isLoading && !isError && filteredRepos.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Repositories</CardTitle>
            <CardDescription>Active versions with health and backend context.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap items-center gap-3 rounded-md border bg-card/50 p-3">
              <Input
                data-cy="repo-search"
                placeholder="Search by name or id"
                className="w-56"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <Select value={baseModelFilter} onValueChange={setBaseModelFilter}>
                <SelectTrigger className="w-[160px]">
                  <SelectValue placeholder="Base model" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All base models</SelectItem>
                  {baseModelOptions.map(option => (
                    <SelectItem key={option} value={option}>
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select value={healthFilter} onValueChange={(v) => setHealthFilter(v as AdapterHealthFlag | 'all')}>
                <SelectTrigger className="w-[160px]">
                  <SelectValue placeholder="Health" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All health</SelectItem>
                  <SelectItem value="healthy">Healthy</SelectItem>
                  <SelectItem value="degraded">Degraded</SelectItem>
                  <SelectItem value="unsafe">Unsafe</SelectItem>
                  <SelectItem value="corrupt">Corrupt</SelectItem>
                </SelectContent>
              </Select>
              <Select value={tenantFilter} onValueChange={setTenantFilter}>
                <SelectTrigger className="w-[160px]">
                  <SelectValue placeholder="Tenant" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All tenants</SelectItem>
                  {tenantOptions.map(t => (
                    <SelectItem key={t} value={t}>
                      {t === 'unknown' ? 'Unknown' : t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <PageTable data-cy="repo-table">
              <table className="min-w-full text-sm">
                <thead>
                  <tr className="text-left text-muted-foreground">
                    <th className="px-3 py-2 font-medium">Name</th>
                    <th className="px-3 py-2 font-medium">Base model</th>
                    <th className="px-3 py-2 font-medium">Active version</th>
                    <th className="px-3 py-2 font-medium">Health</th>
                    <th className="px-3 py-2 font-medium">Last training</th>
                    <th className="px-3 py-2 font-medium text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredRepos.map(repo => {
                    const activeVersion =
                      repo.branches?.find(b => b.default)?.latest_active_version ||
                      repo.branches?.map(b => b.latest_active_version).find(Boolean) ||
                      null;
                    const healthState = activeVersion?.health_state ?? 'unknown';
                    const health = isAdapterHealthFlag(healthState) ? healthState : 'unknown';
                    const healthForBadge = health === 'unknown' ? ('healthy' as AdapterHealthFlag) : health;
                    const lastTraining =
                      activeVersion?.updated_at ||
                      activeVersion?.created_at ||
                      repo.updated_at ||
                      repo.created_at;
                    return (
                      <tr
                        key={repo.id}
                        className="align-top"
                        data-cy={`repo-row-${repo.id}`}
                        data-testid={`repo-row-${repo.id}`}
                      >
                        <td className="px-3 py-3">
                          <div className="font-medium">{repo.name}</div>
                          <div className="text-xs text-muted-foreground break-all">{repo.id}</div>
                        </td>
                        <td className="px-3 py-3 text-sm text-muted-foreground">{repo.base_model}</td>
                        <td className="px-3 py-3 text-sm text-muted-foreground">
                          {activeVersion?.id || activeVersion?.version || '—'}
                        </td>
                        <td className="px-3 py-3">
                          <HealthBadge state={healthForBadge} size="sm" />
                        </td>
                        <td className="px-3 py-3 text-sm text-muted-foreground">
                          {lastTraining ? new Date(lastTraining).toLocaleString() : '—'}
                        </td>
                        <td className="px-3 py-3 text-right">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => openDetail(repo.id)}
                            className="gap-1"
                            data-cy={`repo-open-${repo.id}`}
                          >
                            Open
                            <ArrowRight className="h-4 w-4" />
                          </Button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </PageTable>
          </CardContent>
        </Card>
      )}

      <CreateRepoDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        isSubmitting={createRepo.isPending}
        onCreate={(payload) => createRepo.mutate(payload)}
      />
      </div>
    </PageWrapper>
  );
}
