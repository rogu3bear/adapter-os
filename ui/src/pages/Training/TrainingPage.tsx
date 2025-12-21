import { useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageErrorsProvider } from '@/components/ui/page-error-boundary';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { TrainingJobsTab } from './TrainingJobsTab';
import { DatasetsTab } from './DatasetsTab';
import { TemplatesTab } from './TemplatesTab';
import { BehaviorEventsTab } from './BehaviorEventsTab';
import { Brain, Database, FileText, Activity } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { TrainingWizard } from '@/components/TrainingWizard';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { parsePreselectParams } from '@/utils/urlParams';
import { buildTrainingJobsLink, buildTrainingDatasetsLink, buildTrainingJobDetailLink } from '@/utils/navLinks';

function TrainingPageContent({ preselectedAdapterId, preselectedDatasetId }: { preselectedAdapterId?: string; preselectedDatasetId?: string }) {
  const [activeTab, setActiveTab] = useState('jobs');
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const preselect = useMemo(() => parsePreselectParams(location.search, location.hash), [location.hash, location.search]);
  const adapterId = preselectedAdapterId ?? preselect.adapterId;
  const datasetId = preselectedDatasetId ?? preselect.datasetId;

  useEffect(() => {
    const shouldOpen = (location.state as { openTrainingWizard?: boolean } | null)?.openTrainingWizard;
    if (shouldOpen) {
      setIsWizardOpen(true);
      navigate(location.pathname, { replace: true, state: {} });
    }
  }, [location.pathname, location.state, navigate]);

  return (
    <div className="space-y-6" data-cy="training-page">
      <Card>
        <CardHeader>
          <CardTitle>Start training</CardTitle>
          <CardDescription>Go from template to job without leaving the Training page.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div className="text-sm text-muted-foreground">
            Use defaults from Settings and jump directly into jobs or datasets.
          </div>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={() => setIsWizardOpen(true)}>
              Train new adapter from template
            </Button>
            <Button variant="outline" size="sm" onClick={() => navigate(buildTrainingJobsLink())}>
              Browse existing jobs
            </Button>
            <Button variant="outline" size="sm" onClick={() => navigate(buildTrainingDatasetsLink())}>
              Manage datasets
            </Button>
          </div>
        </CardContent>
      </Card>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full max-w-[calc(var(--base-unit)*200)] grid-cols-4">
          <TabsTrigger value="jobs" className="flex items-center gap-2">
            <Brain className="h-4 w-4" />
            Training Jobs
          </TabsTrigger>
          <TabsTrigger value="datasets" className="flex items-center gap-2">
            <Database className="h-4 w-4" />
            Datasets
          </TabsTrigger>
          <TabsTrigger value="templates" className="flex items-center gap-2">
            <FileText className="h-4 w-4" />
            Templates
          </TabsTrigger>
          <TabsTrigger value="behavior" className="flex items-center gap-2">
            <Activity className="h-4 w-4" />
            Behavior
          </TabsTrigger>
        </TabsList>

        <TabsContent value="jobs" className="mt-6">
          <SectionErrorBoundary sectionName="Training Jobs">
            <TrainingJobsTab preselectedAdapterId={adapterId} preselectedDatasetId={datasetId} />
          </SectionErrorBoundary>
        </TabsContent>

        <TabsContent value="datasets" className="mt-6">
          <SectionErrorBoundary sectionName="Datasets">
            <DatasetsTab />
          </SectionErrorBoundary>
        </TabsContent>

        <TabsContent value="templates" className="mt-6">
          <SectionErrorBoundary sectionName="Templates">
            <TemplatesTab />
          </SectionErrorBoundary>
        </TabsContent>

        <TabsContent value="behavior" className="mt-6">
          <SectionErrorBoundary sectionName="Behavior Events">
            <BehaviorEventsTab selectedTenant="default" />
          </SectionErrorBoundary>
        </TabsContent>
      </Tabs>

      <Dialog open={isWizardOpen} onOpenChange={setIsWizardOpen}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Training Wizard</DialogTitle>
          </DialogHeader>
          <TrainingWizard
            onComplete={(jobId) => {
              setIsWizardOpen(false);
              navigate(buildTrainingJobDetailLink(jobId));
            }}
            onCancel={() => setIsWizardOpen(false)}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default function TrainingPage({
  preselectedAdapterId,
  preselectedDatasetId,
}: {
  preselectedAdapterId?: string;
  preselectedDatasetId?: string;
}) {
  return (
    <DensityProvider pageKey="training">
      <FeatureLayout
        title="Training"
        description="Manage training jobs, datasets, and templates"
        customHeader={
          <IaPageHeader
            cluster="Build"
            title="Training"
            description="Manage training jobs, datasets, and templates"
          />
        }
      >
        <PageErrorsProvider>
          <TrainingPageContent
            preselectedAdapterId={preselectedAdapterId}
            preselectedDatasetId={preselectedDatasetId}
          />
        </PageErrorsProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}
