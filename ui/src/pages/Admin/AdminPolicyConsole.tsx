import { useEffect, useMemo, useState } from 'react';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { toast } from 'sonner';
import { formatBytes } from '@/utils/format';
import apiClient from '@/api/client';
import type {
  AdapterRepositorySummary,
  CoreMLMode,
  RepoAssuranceTier,
} from '@/api/repo-types';
import type {
  Dataset,
  DatasetTrustOverrideRequest,
  TrustState,
} from '@/api/training-types';
import type { TenantStorageUsageResponse } from '@/api/api-types';
import { AlertTriangle, ShieldCheck, HardDrive, Database } from 'lucide-react';

type RepoPolicyDraft = {
  coreml_mode: CoreMLMode;
  repo_tier: RepoAssuranceTier;
  auto_rollback_on_trust_regress: boolean;
};

const trustOptions: TrustState[] = ['allowed', 'allowed_with_warning', 'blocked', 'needs_approval'];
const coremlModes: CoreMLMode[] = ['coreml_strict', 'coreml_preferred', 'backend_auto'];
const repoTiers: RepoAssuranceTier[] = ['high_assurance', 'normal'];

export function AdminPolicyConsole() {

  // Repository policies
  const [repos, setRepos] = useState<AdapterRepositorySummary[]>([]);
  const [reposLoading, setReposLoading] = useState(true);
  const [repoError, setRepoError] = useState<string | null>(null);
  const [editingRepo, setEditingRepo] = useState<AdapterRepositorySummary | null>(null);
  const [policyDraft, setPolicyDraft] = useState<RepoPolicyDraft | null>(null);
  const [showPolicyConfirm, setShowPolicyConfirm] = useState(false);
  const [savingPolicy, setSavingPolicy] = useState(false);

  // Dataset trust overrides
  const [datasets, setDatasets] = useState<Dataset[]>([]);
  const [datasetsLoading, setDatasetsLoading] = useState(true);
  const [datasetError, setDatasetError] = useState<string | null>(null);
  const [selectedDataset, setSelectedDataset] = useState<Dataset | null>(null);
  const [overrideState, setOverrideState] = useState<TrustState>('allowed');
  const [overrideReason, setOverrideReason] = useState('');
  const [applyingOverride, setApplyingOverride] = useState(false);

  // Storage / quotas
  const [usage, setUsage] = useState<TenantStorageUsageResponse | null>(null);
  const [usageLoading, setUsageLoading] = useState(true);
  const [usageError, setUsageError] = useState<string | null>(null);

  useEffect(() => {
    setReposLoading(true);
    apiClient
      .listAdapterRepositories()
      .then(setRepos)
      .catch(err => {
        setRepoError(err instanceof Error ? err.message : 'Failed to load repositories');
      })
      .finally(() => setReposLoading(false));
  }, []);

  useEffect(() => {
    setDatasetsLoading(true);
    apiClient
      .listDatasets()
      .then(resp => setDatasets(resp.datasets))
      .catch(err => setDatasetError(err instanceof Error ? err.message : 'Failed to load datasets'))
      .finally(() => setDatasetsLoading(false));
  }, []);

  useEffect(() => {
    setUsageLoading(true);
    apiClient
      .getTenantStorageUsage()
      .then(setUsage)
      .catch(err => setUsageError(err instanceof Error ? err.message : 'Failed to load storage usage'))
      .finally(() => setUsageLoading(false));
  }, []);

  const openPolicyEditor = (repo: AdapterRepositorySummary) => {
    const policy = repo.training_policy;
    setEditingRepo(repo);
    setPolicyDraft({
      coreml_mode: policy?.coreml_mode ?? 'coreml_preferred',
      repo_tier: policy?.repo_tier ?? 'normal',
      auto_rollback_on_trust_regress: policy?.auto_rollback_on_trust_regress ?? false,
    });
    setShowPolicyConfirm(false);
  };

  const savePolicy = async () => {
    if (!editingRepo || !policyDraft) return;
    setSavingPolicy(true);
    try {
      const updated = await apiClient.updateAdapterRepositoryPolicy(editingRepo.id, {
        coreml_mode: policyDraft.coreml_mode,
        repo_tier: policyDraft.repo_tier,
        auto_rollback_on_trust_regress: policyDraft.auto_rollback_on_trust_regress,
      });
      setRepos(prev =>
        prev.map(r =>
          r.id === editingRepo.id
            ? { ...r, training_policy: { ...updated } }
            : r
        )
      );
      toast.success(`${editingRepo.name} policy saved`);
      setEditingRepo(null);
      setPolicyDraft(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update policy');
    } finally {
      setSavingPolicy(false);
      setShowPolicyConfirm(false);
    }
  };

  const applyTrustOverride = async () => {
    if (!selectedDataset || !overrideReason.trim()) return;
    setApplyingOverride(true);
    const payload: DatasetTrustOverrideRequest = {
      override_state: overrideState,
      reason: overrideReason.trim(),
    };
    try {
      const resp = await apiClient.applyDatasetTrustOverride(selectedDataset.id, payload);
      setDatasets(prev =>
        prev.map(d =>
          d.id === selectedDataset.id
            ? { ...d, trust_state: resp.effective_trust_state ?? overrideState, trust_reason: overrideReason.trim() }
            : d
        )
      );
      toast.success(`${selectedDataset.name} trust set to ${payload.override_state}`);
      setSelectedDataset(null);
      setOverrideReason('');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to apply override');
    } finally {
      setApplyingOverride(false);
    }
  };

  const isDangerousOverride = useMemo(() => {
    if (!selectedDataset) return false;
    const current = (selectedDataset.trust_state ?? 'unknown').toLowerCase();
    const target = overrideState.toLowerCase();
    const movingFromBlocked =
      current.includes('block') || current === 'needs_approval';
    const movingToAllowed =
      target === 'allowed' || target === 'allowed_with_warning';
    return movingFromBlocked && movingToAllowed;
  }, [selectedDataset, overrideState]);

  const usageTotals = useMemo(() => {
    if (!usage) return null;
    const totalBytes = usage.dataset_bytes + usage.artifact_bytes;
    const pctOfSoft = Math.min(100, Math.round((totalBytes / usage.soft_limit_bytes) * 100));
    const pctOfHard = Math.min(100, Math.round((totalBytes / usage.hard_limit_bytes) * 100));
    return { totalBytes, pctOfSoft, pctOfHard };
  }, [usage]);

  return (
    <div className="space-y-4">
      <Tabs defaultValue="repos">
        <TabsList>
          <TabsTrigger value="repos">Repository Policies</TabsTrigger>
          <TabsTrigger value="datasets">Dataset Trust Overrides</TabsTrigger>
          <TabsTrigger value="storage">Storage & Quotas</TabsTrigger>
        </TabsList>

        <TabsContent value="repos">
          <Card>
            <CardHeader>
              <CardTitle>Repository Policies</CardTitle>
              <Dialog open={Boolean(editingRepo)} onOpenChange={open => !open && setEditingRepo(null)}>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Edit repository policy</DialogTitle>
                    {editingRepo && (
                      <DialogDescription>
                        {editingRepo.name} ({editingRepo.base_model_id ?? 'no base model'})
                      </DialogDescription>
                    )}
                  </DialogHeader>
                  {editingRepo && policyDraft && (
                    <div className="space-y-4">
                      <div className="space-y-2">
                        <label className="text-sm font-medium">Backend policy mode</label>
                        <Select
                          value={policyDraft.coreml_mode}
                          onValueChange={v => setPolicyDraft({ ...policyDraft, coreml_mode: v as CoreMLMode })}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="Select mode" />
                          </SelectTrigger>
                          <SelectContent>
                            {coremlModes.map(mode => (
                              <SelectItem key={mode} value={mode}>
                                {mode}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="space-y-2">
                        <label className="text-sm font-medium">Assurance tier</label>
                        <Select
                          value={policyDraft.repo_tier}
                          onValueChange={v => setPolicyDraft({ ...policyDraft, repo_tier: v as RepoAssuranceTier })}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="Select tier" />
                          </SelectTrigger>
                          <SelectContent>
                            {repoTiers.map(tier => (
                              <SelectItem key={tier} value={tier}>
                                {tier}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="flex items-center justify-between rounded border p-3">
                        <div>
                          <p className="text-sm font-medium">Auto rollback on trust regress</p>
                          <p className="text-xs text-muted-foreground">
                            If trust falls, automatically revert to prior version.
                          </p>
                        </div>
                        <Switch
                          checked={policyDraft.auto_rollback_on_trust_regress}
                          onCheckedChange={checked =>
                            setPolicyDraft({ ...policyDraft, auto_rollback_on_trust_regress: checked })
                          }
                        />
                      </div>
                      {showPolicyConfirm && (
                        <div className="rounded-md border p-3 text-sm space-y-2">
                          <p className="font-semibold">Review changes</p>
                          <p>Backend mode: {policyDraft.coreml_mode}</p>
                          <p>Assurance tier: {policyDraft.repo_tier}</p>
                          <p>Auto rollback: {policyDraft.auto_rollback_on_trust_regress ? 'Enabled' : 'Disabled'}</p>
                        </div>
                      )}
                  </div>
                  )}
                  <DialogFooter className="gap-2">
                    {!showPolicyConfirm && (
                      <Button variant="outline" onClick={() => setShowPolicyConfirm(true)}>
                        Review & Save
                      </Button>
                    )}
                    {showPolicyConfirm && (
                      <Button onClick={savePolicy} disabled={savingPolicy}>
                        {savingPolicy ? 'Saving...' : 'Confirm'}
                      </Button>
                    )}
                    <Button variant="ghost" onClick={() => setEditingRepo(null)}>
                      Cancel
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            </CardHeader>
            <CardContent>
              {reposLoading && <p className="text-sm text-muted-foreground">Loading repositories…</p>}
              {repoError && <p className="text-sm text-red-600">{repoError}</p>}
              {!reposLoading && !repoError && (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Base Model</TableHead>
                      <TableHead>Assurance Tier</TableHead>
                      <TableHead>Backend Policy</TableHead>
                      <TableHead>Auto Rollback</TableHead>
                      <TableHead className="text-right">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {repos.map(repo => (
                      <TableRow key={repo.id}>
                        <TableCell>
                          <div className="font-medium">{repo.name}</div>
                          <div className="text-xs text-muted-foreground">{repo.id}</div>
                        </TableCell>
                        <TableCell>{repo.base_model_id ?? '—'}</TableCell>
                        <TableCell>{repo.training_policy?.repo_tier ?? 'normal'}</TableCell>
                        <TableCell>{repo.training_policy?.coreml_mode ?? 'coreml_preferred'}</TableCell>
                        <TableCell>
                          <Badge variant={repo.training_policy?.auto_rollback_on_trust_regress ? 'default' : 'secondary'}>
                            {repo.training_policy?.auto_rollback_on_trust_regress ? 'Enabled' : 'Disabled'}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-right">
                          <Button size="sm" variant="outline" onClick={() => openPolicyEditor(repo)}>
                            Edit
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                    {repos.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-muted-foreground">
                          No repositories found.
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="datasets">
          <Card>
            <CardHeader>
              <CardTitle>Dataset Trust Overrides</CardTitle>
              <Dialog open={Boolean(selectedDataset)} onOpenChange={open => !open && setSelectedDataset(null)}>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Override trust state</DialogTitle>
                    {selectedDataset && (
                      <DialogDescription>
                        {selectedDataset.name} (current: {selectedDataset.trust_state ?? 'unknown'})
                      </DialogDescription>
                    )}
                  </DialogHeader>
                  {selectedDataset && (
                    <div className="space-y-3">
                      <div className="space-y-2">
                        <label className="text-sm font-medium">Override state</label>
                        <Select value={overrideState} onValueChange={v => setOverrideState(v as TrustState)}>
                          <SelectTrigger>
                            <SelectValue placeholder="Select state" />
                          </SelectTrigger>
                          <SelectContent>
                            {trustOptions.map(state => (
                              <SelectItem key={state} value={state}>
                                {state}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="space-y-2">
                        <label className="text-sm font-medium">Justification (required)</label>
                        <Textarea
                          value={overrideReason}
                          onChange={e => setOverrideReason(e.target.value)}
                          placeholder="Provide audit-ready justification"
                        />
                      </div>
                      {isDangerousOverride && (
                        <div className="flex items-start gap-2 rounded-md border border-amber-500/60 bg-amber-50 p-3 text-sm text-amber-900">
                          <AlertTriangle className="h-4 w-4 mt-0.5 text-amber-700" />
                          <p>Warning: moving from blocked/needs_approval to allowed.</p>
                        </div>
                      )}
                    </div>
                  )}
                  <DialogFooter className="gap-2">
                    <Button
                      onClick={applyTrustOverride}
                      disabled={applyingOverride || !overrideReason.trim()}
                    >
                      {applyingOverride ? 'Applying…' : 'Apply Override'}
                    </Button>
                    <Button variant="ghost" onClick={() => setSelectedDataset(null)}>
                      Cancel
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            </CardHeader>
            <CardContent>
              {datasetsLoading && <p className="text-sm text-muted-foreground">Loading datasets…</p>}
              {datasetError && <p className="text-sm text-red-600">{datasetError}</p>}
              {!datasetsLoading && !datasetError && (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Dataset</TableHead>
                      <TableHead>Trust</TableHead>
                      <TableHead>Validation / Safety</TableHead>
                      <TableHead className="text-right">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {datasets.map(ds => (
                      <TableRow key={ds.id}>
                        <TableCell>
                          <div className="font-medium">{ds.name}</div>
                          <div className="text-xs text-muted-foreground">{ds.id}</div>
                        </TableCell>
                        <TableCell>
                          <Badge
                            variant={
                              (ds.trust_state ?? '').toLowerCase().includes('block')
                                ? 'destructive'
                                : 'secondary'
                            }
                          >
                            {ds.trust_state ?? 'unknown'}
                          </Badge>
                          {ds.trust_reason && (
                            <div className="text-xs text-muted-foreground mt-1">{ds.trust_reason}</div>
                          )}
                        </TableCell>
                        <TableCell>
                          <div className="text-sm">{ds.validation_status}</div>
                          {ds.overall_safety_status && (
                            <div className="text-xs text-muted-foreground">
                              Safety: {ds.overall_safety_status}
                            </div>
                          )}
                        </TableCell>
                        <TableCell className="text-right">
                          <Button size="sm" variant="outline" onClick={() => {
                            setSelectedDataset(ds);
                            setOverrideState(ds.trust_state ?? 'allowed');
                            setOverrideReason('');
                          }}>
                            Override
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                    {datasets.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={4} className="text-center text-muted-foreground">
                          No datasets found.
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="storage">
          <Card>
            <CardHeader>
              <CardTitle>Storage & Quotas</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {usageLoading && <p className="text-sm text-muted-foreground">Loading usage…</p>}
              {usageError && <p className="text-sm text-red-600">{usageError}</p>}
              {!usageLoading && usage && usageTotals && (
                <div className="space-y-3">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <ShieldCheck className="h-4 w-4" />
                    Tenant: {usage.tenant_id}
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div className="rounded border p-3">
                      <div className="flex items-center gap-2 text-sm font-medium">
                        <Database className="h-4 w-4" />
                        Dataset storage
                      </div>
                      <div className="text-lg font-semibold">{formatBytes(usage.dataset_bytes)}</div>
                    </div>
                    <div className="rounded border p-3">
                      <div className="flex items-center gap-2 text-sm font-medium">
                        <HardDrive className="h-4 w-4" />
                        Artifact storage
                      </div>
                      <div className="text-lg font-semibold">{formatBytes(usage.artifact_bytes)}</div>
                    </div>
                  </div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span>Soft limit</span>
                      <span>{formatBytes(usage.soft_limit_bytes)} ({usageTotals.pctOfSoft}%)</span>
                    </div>
                    <div className="h-2 w-full rounded bg-muted">
                      <div
                        className={`h-2 rounded ${usage.soft_exceeded ? 'bg-amber-600' : 'bg-blue-600'}`}
                        style={{ width: `${usageTotals.pctOfSoft}%` }}
                      />
                    </div>
                    <div className="flex items-center justify-between text-sm">
                      <span>Hard limit</span>
                      <span>{formatBytes(usage.hard_limit_bytes)} ({usageTotals.pctOfHard}%)</span>
                    </div>
                    <div className="h-2 w-full rounded bg-muted">
                      <div
                        className={`h-2 rounded ${usage.hard_exceeded ? 'bg-red-600' : 'bg-green-600'}`}
                        style={{ width: `${usageTotals.pctOfHard}%` }}
                      />
                    </div>
                  </div>
                  {(usage.soft_exceeded || usage.hard_exceeded) && (
                    <div className={`rounded-md border p-3 ${usage.hard_exceeded ? 'border-red-500 bg-red-50 text-red-900' : 'border-amber-500 bg-amber-50 text-amber-900'}`}>
                      {usage.hard_exceeded
                        ? 'Hard limit exceeded. New ingest or training will be blocked.'
                        : 'Approaching hard limit (soft limit exceeded). Monitor ingest/training volume.'}
                    </div>
                  )}
                  <div className="text-xs text-muted-foreground">
                    Versions: {usage.dataset_versions} datasets / {usage.adapter_versions} adapter versions
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}

export default AdminPolicyConsole;
