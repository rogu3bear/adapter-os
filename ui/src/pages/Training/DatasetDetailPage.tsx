// DatasetDetailPage - Full dataset detail view with tabs
// Displays comprehensive dataset information including overview, files, preview, and validation

import React, { useState, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, RefreshCw, Trash2, CheckCircle, AlertCircle, Play, ExternalLink } from 'lucide-react';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import FeatureLayout from '@/layout/FeatureLayout';

import { useTraining } from '@/hooks/useTraining';
import { useRBAC } from '@/hooks/useRBAC';
import { logger } from '@/utils/logger';
import type { Dataset, DatasetValidationStatus } from '@/api/training-types';
import { TrainingWizard } from '@/components/TrainingWizard';

import DatasetOverview from './DatasetOverview';
import DatasetFiles from './DatasetFiles';
import DatasetPreview from './DatasetPreview';
import DatasetValidation from './DatasetValidation';

type TabValue = 'overview' | 'files' | 'preview' | 'validation';

const STATUS_CONFIG: Record<DatasetValidationStatus, {
  icon: React.ElementType;
  className: string;
  label: string;
}> = {
  draft: {
    icon: RefreshCw,
    className: 'text-yellow-500',
    label: 'Draft',
  },
  validating: {
    icon: RefreshCw,
    className: 'text-blue-500 animate-spin',
    label: 'Validating',
  },
  valid: {
    icon: CheckCircle,
    className: 'text-green-500',
    label: 'Valid',
  },
  invalid: {
    icon: AlertCircle,
    className: 'text-red-500',
    label: 'Invalid',
  },
  failed: {
    icon: AlertCircle,
    className: 'text-red-500',
    label: 'Failed',
  },
};

function StatusBadge({ status }: { status: DatasetValidationStatus }) {
  const config = STATUS_CONFIG[status] || STATUS_CONFIG.draft;
  const Icon = config.icon;

  return (
    <Badge variant="outline" className="gap-1">
      <Icon className={`h-3 w-3 ${config.className}`} />
      <span>{config.label}</span>
    </Badge>
  );
}

export default function DatasetDetailPage() {
  const { datasetId } = useParams<{ datasetId: string }>();
  const navigate = useNavigate();
  const { can } = useRBAC();
  const [activeTab, setActiveTab] = useState<TabValue>('overview');
  const [isTrainingWizardOpen, setIsTrainingWizardOpen] = useState(false);

  const {
    data: dataset,
    isLoading,
    error,
    refetch,
  } = useTraining.useDataset(datasetId || '', {
    enabled: !!datasetId,
  });

  // Fetch training jobs using this dataset (server-side filtered)
  const { data: jobsData } = useTraining.useTrainingJobs({ dataset_id: datasetId });
  const relatedJobs = jobsData?.jobs || [];

  const { mutateAsync: validateDataset, isPending: isValidating } = useTraining.useValidateDataset({
    onSuccess: () => {
      toast.success('Dataset validation started');
      refetch();
    },
    onError: (err) => {
      toast.error(`Failed to validate dataset: ${err.message}`);
      logger.error('Failed to validate dataset', { component: 'DatasetDetailPage', datasetId }, err);
    },
  });

  const { mutateAsync: deleteDataset, isPending: isDeleting } = useTraining.useDeleteDataset({
    onSuccess: () => {
      toast.success('Dataset deleted');
      navigate('/training/datasets');
    },
    onError: (err) => {
      toast.error(`Failed to delete dataset: ${err.message}`);
      logger.error('Failed to delete dataset', { component: 'DatasetDetailPage', datasetId }, err);
    },
  });

  const handleValidate = useCallback(async () => {
    if (!datasetId) return;
    await validateDataset(datasetId);
  }, [datasetId, validateDataset]);

  const handleDelete = useCallback(async () => {
    if (!datasetId) return;
    if (window.confirm('Are you sure you want to delete this dataset? This action cannot be undone.')) {
      await deleteDataset(datasetId);
    }
  }, [datasetId, deleteDataset]);


  if (isLoading) {
    return (
      <FeatureLayout>
        <LoadingState message="Loading dataset details..." />
      </FeatureLayout>
    );
  }

  if (error || !dataset) {
    return (
      <FeatureLayout>
        <ErrorRecovery
          error={error || new Error('Dataset not found')}
          onRetry={() => refetch()}
          onBack={() => navigate('/training/datasets')}
        />
      </FeatureLayout>
    );
  }

  return (
    <FeatureLayout>
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => navigate('/training/datasets')}
            >
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back
            </Button>
            <div>
              <h1 className="text-2xl font-bold">{dataset.name}</h1>
              <p className="text-sm text-muted-foreground mt-1">
                {dataset.id}
              </p>
            </div>
            <StatusBadge status={dataset.validation_status} />
          </div>

          <div className="flex items-center gap-2">
            {dataset.validation_status === 'valid' && can('training:start') && (
              <Button
                onClick={() => setIsTrainingWizardOpen(true)}
              >
                <Play className="h-4 w-4 mr-2" />
                Start Training Job
              </Button>
            )}
            {(dataset.validation_status === 'draft' || dataset.validation_status === 'invalid') && can('dataset:validate') && (
              <Button
                variant="outline"
                onClick={handleValidate}
                disabled={isValidating}
              >
                <CheckCircle className="h-4 w-4 mr-2" />
                {isValidating ? 'Validating...' : 'Validate'}
              </Button>
            )}
            {can('dataset:delete') && (
              <Button
                variant="destructive"
                onClick={handleDelete}
                disabled={isDeleting}
              >
                <Trash2 className="h-4 w-4 mr-2" />
                {isDeleting ? 'Deleting...' : 'Delete'}
              </Button>
            )}
          </div>
        </div>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as TabValue)}>
          <TabsList className="grid w-full grid-cols-4">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="files">Files</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="validation">Validation</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="mt-6">
            <div className="space-y-6">
              <DatasetOverview
                dataset={dataset}
                isLoading={isLoading}
              />
              
              {/* Training Jobs Section */}
              {relatedJobs.length > 0 && (
                <Card>
                  <CardHeader>
                    <CardTitle>Training Jobs Using This Dataset</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="space-y-2">
                      {relatedJobs.map(job => (
                        <div
                          key={job.id}
                          className="flex items-center justify-between p-3 border rounded-lg hover:bg-muted/50 cursor-pointer"
                          onClick={() => navigate(`/training/jobs/${job.id}`)}
                        >
                          <div className="flex-1">
                            <p className="font-medium">{job.id}</p>
                            <p className="text-sm text-muted-foreground">
                              Status: <Badge variant="outline">{job.status}</Badge>
                              {job.progress_pct !== undefined && (
                                <span className="ml-2">Progress: {job.progress_pct.toFixed(1)}%</span>
                              )}
                            </p>
                          </div>
                          <Button variant="ghost" size="sm">
                            <ExternalLink className="h-4 w-4" />
                          </Button>
                        </div>
                      ))}
                    </div>
                    <Button
                      variant="outline"
                      className="mt-4"
                      onClick={() => navigate(`/training/jobs?dataset_id=${datasetId}`)}
                    >
                      View All Jobs
                    </Button>
                  </CardContent>
                </Card>
              )}
            </div>
          </TabsContent>

          <TabsContent value="files" className="mt-6">
            <DatasetFiles
              datasetId={datasetId!}
              isLoading={isLoading}
            />
          </TabsContent>

          <TabsContent value="preview" className="mt-6">
            <DatasetPreview
              datasetId={datasetId!}
              isLoading={isLoading}
            />
          </TabsContent>

          <TabsContent value="validation" className="mt-6">
            <DatasetValidation
              dataset={dataset}
              onValidate={handleValidate}
              isValidating={isValidating}
            />
          </TabsContent>
        </Tabs>

        {/* Training Wizard Dialog */}
        <Dialog open={isTrainingWizardOpen} onOpenChange={setIsTrainingWizardOpen}>
          <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>Start Training Job</DialogTitle>
            </DialogHeader>
            <TrainingWizard
              initialDatasetId={datasetId}
              onComplete={(jobId) => {
                setIsTrainingWizardOpen(false);
                navigate(`/training/jobs/${jobId}`);
              }}
              onCancel={() => setIsTrainingWizardOpen(false)}
            />
          </DialogContent>
        </Dialog>
      </div>
    </FeatureLayout>
  );
}

