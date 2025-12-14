import React, { useMemo } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import TrainingPage from '@/pages/Training/TrainingPage';
import TrainingJobsPage from '@/pages/Training/TrainingJobsPage';
import TrainingDatasetsPage from '@/pages/Training/DatasetsTab';
import { TemplatesTab as TrainingTemplatesPage } from '@/pages/Training/TemplatesTab';
import TrainingJobDetailPage from '@/pages/Training/TrainingJobDetail';
import DatasetDetailPage from '@/pages/Training/DatasetDetailPage';
import { useTrainingTabRouter } from '@/hooks/navigation/useTabRouter';
import apiClient from '@/api/client';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { useToast } from '@/hooks/use-toast';
import type { TrainingArtifact, TrainingJob, TrainingTemplate } from '@/api/training-types';
import { parsePreselectParams } from '@/utils/urlParams';

export default function TrainingShell() {
  const location = useLocation();
  const navigate = useNavigate();
  const { jobId, datasetId } = useParams<{ jobId?: string; datasetId?: string }>();

  const preselect = useMemo(() => parsePreselectParams(location.search, location.hash), [location.hash, location.search]);

  const { activeTab, setActiveTab, availableTabs, getTabPath } = useTrainingTabRouter();

  return (
    <PageWrapper
      pageKey="training-shell"
      title="Training"
      description="Manage training jobs, datasets, templates, and artifacts"
      maxWidth="xl"
      contentPadding="default"
    >
      <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as typeof activeTab)}>
        <TabsList className="w-full grid grid-cols-3 md:grid-cols-6">
          {availableTabs.map(tab => (
            <TabsTrigger key={tab.id} value={tab.id} asChild>
              <Link to={getTabPath(tab.id)}>{tab.label}</Link>
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="overview" className="mt-6">
          <TrainingPage preselectedAdapterId={preselect.adapterId} preselectedDatasetId={preselect.datasetId} />
        </TabsContent>
        <TabsContent value="jobs" className="mt-6">
          <TrainingJobsPage preselectedAdapterId={preselect.adapterId} preselectedDatasetId={preselect.datasetId} />
        </TabsContent>
        <TabsContent value="job-detail" className="mt-6">
          <TrainingJobDetailPage />
        </TabsContent>
        <TabsContent value="job-chat" className="mt-6">
          <div className="text-sm text-muted-foreground">Result chat view (handled by ResultChatPage route)</div>
        </TabsContent>
        <TabsContent value="datasets" className="mt-6">
          <TrainingDatasetsPage />
        </TabsContent>
        <TabsContent value="dataset-detail" className="mt-6">
          <DatasetDetailPage />
        </TabsContent>
        <TabsContent value="dataset-chat" className="mt-6">
          <div className="text-sm text-muted-foreground">Dataset chat view (handled by DatasetChatPage route)</div>
        </TabsContent>
        <TabsContent value="templates" className="mt-6">
          <TrainingTemplatesPage />
        </TabsContent>
        <TabsContent value="artifacts" className="mt-6">
          <TrainingArtifactsTab jobId={jobId} onNavigate={navigate} />
        </TabsContent>
        <TabsContent value="settings" className="mt-6">
          <TrainingSettingsTab />
        </TabsContent>
      </Tabs>
    </PageWrapper>
  );
}

function TrainingArtifactsTab({ jobId, onNavigate }: { jobId?: string; onNavigate: (path: string) => void }) {
  const { toast } = useToast();

  const {
    data: artifactsData,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['training-artifacts', jobId],
    queryFn: () => apiClient.getTrainingArtifacts(jobId as string),
    enabled: Boolean(jobId),
  });

  const {
    data: recentJobs,
    isLoading: loadingJobs,
  } = useQuery({
    queryKey: ['training-recent-jobs'],
    queryFn: () => apiClient.listTrainingJobs({ page_size: 5 }),
    enabled: !jobId,
  });

  const handleDownload = async (artifact: TrainingArtifact) => {
    if (!jobId) return;
    try {
      await apiClient.downloadArtifact(jobId, artifact.id, artifact.path?.split('/').pop());
      toast({ title: 'Download started', description: artifact.path });
    } catch (err) {
      toast({
        title: 'Download failed',
        description: err instanceof Error ? err.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  };

  if (jobId) {
    if (isLoading) return <div className="text-sm text-muted-foreground">Loading artifacts…</div>;
    if (error) {
      return (
        <div className="text-sm text-destructive">
          Failed to load artifacts: {error instanceof Error ? error.message : String(error)}
        </div>
      );
    }
    if (!artifactsData?.artifacts?.length) {
      return <div className="text-sm text-muted-foreground">No artifacts yet for this job.</div>;
    }

    return (
      <Card>
        <CardHeader>
          <CardTitle>Artifacts for Job {jobId}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {artifactsData.artifacts.map((artifact) => (
            <div
              key={artifact.id}
              className="flex items-center justify-between border rounded px-3 py-2"
            >
              <div className="space-y-1">
                <div className="font-medium">{artifact.type}</div>
                <div className="text-xs text-muted-foreground">{artifact.path}</div>
              </div>
              <Button size="sm" variant="outline" onClick={() => handleDownload(artifact)}>
                Download
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent jobs with artifacts</CardTitle>
      </CardHeader>
      <CardContent>
        {loadingJobs && <div className="text-sm text-muted-foreground">Loading jobs…</div>}
        {!loadingJobs && !recentJobs?.jobs?.length && (
          <div className="text-sm text-muted-foreground">No recent jobs found.</div>
        )}
        {recentJobs?.jobs?.length ? (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Job</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Adapter</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {recentJobs.jobs.map((job: TrainingJob) => (
                <TableRow key={job.id}>
                  <TableCell className="font-medium">{job.id}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{job.status}</Badge>
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">{job.adapter_name ?? '—'}</TableCell>
                  <TableCell className="text-right">
                    <Button size="sm" variant="outline" onClick={() => onNavigate(`/training/jobs/${job.id}`)}>
                      View job
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : null}
      </CardContent>
    </Card>
  );
}

function TrainingSettingsTab() {
  const { toast } = useToast();
  const { data: templates, isLoading } = useQuery({
    queryKey: ['training-templates'],
    queryFn: () => apiClient.listTrainingTemplates(),
  });

  if (isLoading) {
    return <div className="text-sm text-muted-foreground">Loading training settings…</div>;
  }

  if (!templates || templates.length === 0) {
    return <div className="text-sm text-muted-foreground">No templates available.</div>;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Training defaults (from templates)</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-sm text-muted-foreground">
          Using template values as defaults. Select a template to pre-fill new jobs.
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Rank</TableHead>
              <TableHead>Alpha</TableHead>
              <TableHead>Epochs</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {templates.map((tpl: TrainingTemplate) => (
              <TableRow key={tpl.id}>
                <TableCell className="font-medium">{tpl.name}</TableCell>
                <TableCell>{tpl.config?.rank ?? '—'}</TableCell>
                <TableCell>{tpl.config?.alpha ?? '—'}</TableCell>
                <TableCell>{tpl.config?.epochs ?? '—'}</TableCell>
                <TableCell className="text-right">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      toast({
                        title: 'Template selected',
                        description: `${tpl.name} will be used as defaults when starting training.`,
                      })
                    }
                  >
                    Use defaults
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

