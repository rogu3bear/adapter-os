import React, { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { ExternalLink, Loader2, Rocket, Tag as TagIcon } from 'lucide-react';
import PageWrapper from '@/layout/PageWrapper';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { toast } from 'sonner';
import {
  usePromoteRepoVersion,
  useRepoVersion,
  useStartTrainingFromVersion,
  useTagRepoVersion,
} from '@/hooks/useReposApi';

const normalizeTrustState = (state?: string): string => {
  switch ((state ?? 'unknown').toLowerCase()) {
    case 'warn':
      return 'allowed_with_warning';
    case 'blocked_regressed':
      return 'blocked';
    default:
      return state ?? 'unknown';
  }
};

function MetricsBlock({ metrics }: { metrics?: Record<string, number | null | undefined> }) {
  if (!metrics || Object.keys(metrics).length === 0) {
    return <div className="text-sm text-muted-foreground">No metrics yet.</div>;
  }
  return (
    <div className="grid gap-2 sm:grid-cols-2">
      {Object.entries(metrics).map(([key, value]) => (
        <div key={key} className="rounded-md border border-border/60 bg-muted/20 px-3 py-2 text-sm">
          <div className="text-xs uppercase tracking-wide text-muted-foreground">{key}</div>
          <div className="font-semibold">{value ?? '—'}</div>
        </div>
      ))}
    </div>
  );
}

function TagDialog({
  open,
  onOpenChange,
  onSave,
  isSaving,
  existingTags,
}: {
  open: boolean;
  onOpenChange: (next: boolean) => void;
  onSave: (tags: string[]) => void;
  isSaving: boolean;
  existingTags: string[];
}) {
  const [input, setInput] = useState(existingTags.join(', '));
  const tags = useMemo(
    () => input.split(',').map(t => t.trim()).filter(Boolean),
    [input]
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Tag version</DialogTitle>
          <DialogDescription>Add or overwrite tags for this version.</DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="tags">Tags (comma separated)</Label>
          <Input
            id="tags"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="prod, blue, shadow"
          />
        </div>
        <DialogFooter className="mt-4">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => onSave(tags)} disabled={isSaving}>
            {isSaving ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default function RepoVersionPage() {
  const { repoId, versionId } = useParams<{ repoId: string; versionId: string }>();
  const navigate = useNavigate();
  const [tagDialogOpen, setTagDialogOpen] = useState(false);

  const { data: version, isLoading } = useRepoVersion(repoId, versionId);

  const promote = usePromoteRepoVersion(repoId ?? '', {
    onSuccess: () => toast.success('Version promoted'),
    onError: (error) => toast.error('Promote failed', { description: error.message }),
  });

  const startTrain = useStartTrainingFromVersion(repoId ?? '', {
    onSuccess: () => toast.success('Training started from version'),
    onError: (error) => toast.error('Failed to start training', { description: error.message }),
  });

  const tagVersion = useTagRepoVersion(repoId ?? '', {
    onSuccess: () => {
      toast.success('Tags updated');
      setTagDialogOpen(false);
    },
    onError: (error) => toast.error('Failed to update tags', { description: error.message }),
  });

  const handleTagSave = (tags: string[]) => {
    if (!versionId) return;
    tagVersion.mutate({ versionId, payload: { tags } });
  };

  const isBusy = promote.isPending || startTrain.isPending || tagVersion.isPending;
  const normalizedTrust = normalizeTrustState(version?.adapter_trust_state);

  return (
    <PageWrapper
      pageKey="repo-version"
      title={version?.version ?? 'Version'}
      description="Version metadata, metrics, and promotion controls."
      secondaryActions={[
        repoId ? { label: 'Back to repo', onClick: () => navigate(`/repos/${repoId}`) } : undefined,
      ].filter(Boolean) as { label: string; onClick: () => void }[]}
    >
      {isLoading && (
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span>Loading version…</span>
        </div>
      )}

      {version && (
        <>
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <span>{version.version}</span>
                <Badge variant="outline" className="capitalize">{version.release_state}</Badge>
                <Badge variant="secondary">{version.branch}</Badge>
              </CardTitle>
              <CardDescription>ID: {version.id}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <span className="text-muted-foreground">Base model</span>
                <Badge variant="outline">{version.base_model ?? 'unknown'}</Badge>
                <Separator orientation="vertical" className="h-6" />
                <span className="text-muted-foreground">Tags</span>
                <div className="flex flex-wrap gap-1">
                  {(version.tags ?? []).map(tag => (
                    <Badge key={tag} variant="secondary">{tag}</Badge>
                  ))}
                  {(!version.tags || version.tags.length === 0) && <span className="text-xs text-muted-foreground">None</span>}
                </div>
                <Button variant="ghost" size="sm" className="gap-1" onClick={() => setTagDialogOpen(true)}>
                  <TagIcon className="h-4 w-4" />
                  Edit tags
                </Button>
              </div>

              <div className="flex flex-wrap gap-3 text-sm">
                {version.aos_hash && <Badge variant="outline">.aos hash: {version.aos_hash}</Badge>}
                {version.aos_path && <Badge variant="outline">Path: {version.aos_path}</Badge>}
                {version.training_backend && (
                  <Badge variant="outline" className="gap-1">
                    Backend: {version.training_backend}
                    {version.coreml_used && (
                      <Badge variant="secondary" className="ml-1">
                        CoreML{version.coreml_device_type ? ` (${version.coreml_device_type})` : ''}
                      </Badge>
                    )}
                  </Badge>
                )}
                {version.coreml_used && !version.training_backend && (
                  <Badge variant="secondary">
                    CoreML{version.coreml_device_type ? ` (${version.coreml_device_type})` : ''}
                  </Badge>
                )}
                {version.commit_sha && (
                  <Badge variant="outline" className="gap-1">
                    Commit: {version.commit_sha}
                    {version.commit_url && (
                      <a
                        href={version.commit_url}
                        target="_blank"
                        rel="noreferrer"
                        className="inline-flex items-center gap-1 text-primary underline"
                      >
                        <ExternalLink className="h-3 w-3" />
                      </a>
                    )}
                  </Badge>
                )}
              </div>

              {version.data_spec_summary && (
                <div className="rounded-md border border-border/60 bg-muted/20 p-3 text-sm">
                  <div className="text-xs uppercase tracking-wide text-muted-foreground">Data spec</div>
                  <div>{version.data_spec_summary}</div>
                </div>
              )}

              <div className="flex flex-wrap gap-2">
                <Badge
                  variant={
                    normalizedTrust === 'blocked'
                      ? 'destructive'
                      : normalizedTrust === 'allowed_with_warning'
                        ? 'secondary'
                        : 'outline'
                  }
                >
                  Trust: {normalizedTrust}
                </Badge>
              </div>

              {version.dataset_version_ids && version.dataset_version_ids.length > 0 && (
                <div className="rounded-md border border-border/60 bg-muted/20 p-3 text-sm space-y-1">
                  <div className="text-xs uppercase tracking-wide text-muted-foreground">Dataset versions</div>
                  <div className="flex flex-wrap gap-2">
                    {version.dataset_version_ids.map((id) => (
                      <Badge key={id} variant="outline" className="font-mono text-xs">
                        {id}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}

              <MetricsBlock metrics={version.metrics} />
            </CardContent>
          </Card>

          <div className="flex flex-wrap gap-2 mt-4">
            <Button
              variant="default"
              className="gap-2"
              disabled={isBusy}
              onClick={() => versionId && promote.mutate({ versionId })}
            >
              <Rocket className="h-4 w-4" />
              Promote
            </Button>
            <Button
              variant="outline"
              className="gap-2"
              disabled={isBusy}
              onClick={() => versionId && startTrain.mutate({ versionId, payload: {} })}
            >
              Start training
            </Button>
          </div>

          <Separator className="my-6" />

          <Card>
            <CardHeader>
              <CardTitle>Audit</CardTitle>
              <CardDescription>Created at {new Date(version.created_at).toLocaleString()}</CardDescription>
            </CardHeader>
            <CardContent className="text-sm text-muted-foreground space-y-1">
              {version.updated_at && <div>Updated: {new Date(version.updated_at).toLocaleString()}</div>}
              <div>Release state: {version.release_state}</div>
              <div>Branch: {version.branch}</div>
            </CardContent>
          </Card>

          <TagDialog
            open={tagDialogOpen}
            onOpenChange={setTagDialogOpen}
            existingTags={version.tags ?? []}
            onSave={handleTagSave}
            isSaving={tagVersion.isPending}
          />
        </>
      )}
    </PageWrapper>
  );
}
