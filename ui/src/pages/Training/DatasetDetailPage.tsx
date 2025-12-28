// DatasetDetailPage - Full dataset detail view with tabs
// Displays comprehensive dataset information including overview, files, preview, and validation

import React, { useState, useCallback, useMemo } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import { ArrowLeft, RefreshCw, Trash2, CheckCircle, AlertCircle, Play, ExternalLink, Clock, MessageSquare, Shield } from 'lucide-react';
import { toast } from 'sonner';
import {
  buildTrainingDatasetsLink,
  buildTrainingJobDetailLink,
  buildTrainingJobsLink,
  buildDatasetChatLink,
  buildDatasetDetailLink,
} from '@/utils/navLinks';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import FeatureLayout from '@/layout/FeatureLayout';
import { PageAsyncBoundary, SectionAsyncBoundary } from '@/components/shared/Feedback/AsyncBoundary';

import { useTraining } from '@/hooks/training';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useModelStatus } from '@/hooks/model-loading';
import { logger } from '@/utils/logger';
import type { Dataset, DatasetValidationStatus, ValidationStatus, DatasetVersionSummary, TrustState, StartTrainingRequest } from '@/api/training-types';
import { TrainingWizard } from '@/components/TrainingWizard';
import { QuickTrainConfirmModal, type QuickTrainConfig } from '@/components/training/QuickTrainConfirmModal';
import { canUseQuickTrain } from '@/utils/trainingPreflight';
import { useLineage } from '@/hooks/observability/useLineage';
import { apiClient } from '@/api/services';
import { LineageViewer } from '@/components/lineage/LineageViewer';
import { useTenant } from '@/providers/FeatureProviders';

import DatasetOverview from './DatasetOverview';
import DatasetFiles from './DatasetFiles';
import DatasetPreview from './DatasetPreview';
import DatasetValidation from './DatasetValidation';
import { TrustOverrideDialog } from '@/components/training/TrustOverrideDialog';

type TabValue = 'overview' | 'files' | 'preview' | 'validation' | 'lineage';
type TrainingJobSummary = { id: string; status: string; progress_pct?: number };

const STATUS_CONFIG: Record<ValidationStatus, {
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
  pending: {
    icon: Clock,
    className: 'text-yellow-500',
    label: 'Pending',
  },
  skipped: {
    icon: RefreshCw,
    className: 'text-gray-500',
    label: 'Skipped',
  },
};

function StatusBadge({ status }: { status: ValidationStatus }) {
  const config = STATUS_CONFIG[status] || STATUS_CONFIG.draft;
  const Icon = config.icon;

  return (
    <Badge variant="outline" className="gap-1">
      <Icon className={`h-3 w-3 ${config.className}`} />
      <span>{config.label}</span>
    </Badge>
  );
}

const TRUST_CONFIG: Record<TrustState, { icon: React.ElementType; className: string; label: string }> = {
  allowed: { icon: CheckCircle, className: 'text-green-500', label: 'Allowed' },
  allowed_with_warning: { icon: AlertCircle, className: 'text-amber-500', label: 'Allowed w/ warning' },
  blocked: { icon: AlertCircle, className: 'text-red-500', label: 'Blocked' },
  needs_approval: { icon: Clock, className: 'text-orange-500', label: 'Needs approval' },
  unknown: { icon: Clock, className: 'text-muted-foreground', label: 'Unknown' },
};

function TrustBadge({ state }: { state: TrustState | undefined }) {
  const trustState = state ?? 'unknown';
  const config = TRUST_CONFIG[trustState] || TRUST_CONFIG.unknown;
  const Icon = config.icon;
  return (
    <Badge variant="outline" className="gap-1">
      <Icon className={`h-3 w-3 ${config.className}`} />
      <span>{config.label}</span>
    </Badge>
  );
}

const LoadingView = () => (
  <FeatureLayout title="Dataset Details">
    <LoadingState message="Loading dataset details..." />
  </FeatureLayout>
);

function ErrorView({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <FeatureLayout title="Dataset Details">
      <ErrorRecovery error={message} onRetry={onRetry} />
    </FeatureLayout>
  );
}

function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <Button variant="ghost" size="sm" onClick={onClick}>
      <ArrowLeft className="mr-2 h-4 w-4" />
      Back
    </Button>
  );
}

function HeaderSection({
  datasetId,
  status,
  canStartTraining,
  canValidate,
  canDelete,
  canOverrideTrust,
  onStartTraining,
  onValidate,
  onDelete,
  onTalkToDataset,
  onOverrideTrust,
  isValidating,
  isDeleting,
  onNavigateBack,
  trustState,
}: {
  datasetId: string;
  status: ValidationStatus;
  trustState?: TrustState;
  canStartTraining: boolean;
  canValidate: boolean;
  canDelete: boolean;
  canOverrideTrust: boolean;
  onStartTraining: () => void;
  onValidate: () => void;
  onDelete: () => void;
  onTalkToDataset: () => void;
  onOverrideTrust: () => void;
  isValidating: boolean;
  isDeleting: boolean;
  onNavigateBack: () => void;
}) {
  const trustBlocked =
    !trustState || trustState === 'unknown' || trustState === 'blocked' || trustState === 'needs_approval';
  const trustBlockedReason = trustBlocked ? 'Training disabled until dataset trust is allowed' : undefined;
  const trustWarn = trustState === 'allowed_with_warning';
  const showValidate = ((status as string) === 'draft' || status === 'invalid') && canValidate;
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-4">
        <BackButton onClick={onNavigateBack} />
        <p className="text-sm text-muted-foreground">{datasetId}</p>
        <StatusBadge status={status} />
        <TrustBadge state={trustState} />
        {canOverrideTrust && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onOverrideTrust}
            title="Override trust state"
            className="h-6 px-2 text-xs"
          >
            <Shield className="h-3 w-3 mr-1" />
            Override
          </Button>
        )}
      </div>
      <div className="flex items-center gap-2">
        {trustWarn && (
          <Badge variant="outline" className="text-amber-600 border-amber-400">
            Trust warning
          </Badge>
        )}
        {status === 'valid' && (
          <Button
            data-cy="dataset-talk"
            variant="outline"
            onClick={onTalkToDataset}
          >
            <MessageSquare className="mr-2 h-4 w-4" />
            Talk to this dataset
          </Button>
        )}
        {status === 'valid' && canStartTraining && (
          <Button
            data-cy="dataset-start-training"
            onClick={onStartTraining}
            disabled={trustBlocked}
            title={trustBlockedReason}
          >
            <Play className="mr-2 h-4 w-4" />
            Start Training Job
          </Button>
        )}
        {showValidate && (
          <Button
            data-cy="dataset-validate"
            variant="outline"
            onClick={onValidate}
            disabled={isValidating}
          >
            <CheckCircle className="mr-2 h-4 w-4" />
            {isValidating ? 'Validating...' : 'Validate'}
          </Button>
        )}
        {canDelete && (
          <Button variant="destructive" onClick={onDelete} disabled={isDeleting}>
            <Trash2 className="mr-2 h-4 w-4" />
            {isDeleting ? 'Deleting...' : 'Delete'}
          </Button>
        )}
      </div>
    </div>
  );
}

function TrainingJobRow({
  jobId,
  status,
  progress,
  onClick,
}: {
  jobId: string;
  status: string;
  progress?: number;
  onClick: () => void;
}) {
  return (
    <div
      className="flex cursor-pointer items-center justify-between rounded-lg border p-3 hover:bg-muted/50"
      onClick={onClick}
    >
      <div className="flex-1">
        <p className="font-medium">{jobId}</p>
        <p className="text-sm text-muted-foreground">
          Status: <Badge variant="outline">{status}</Badge>
          {progress !== undefined && <span className="ml-2">Progress: {progress.toFixed(1)}%</span>}
        </p>
      </div>
      <Button variant="ghost" size="sm">
        <ExternalLink className="h-4 w-4" />
      </Button>
    </div>
  );
}

function TrainingJobsCard({
  jobs,
  onNavigateJob,
  onViewAll,
}: {
  jobs: TrainingJobSummary[];
  onNavigateJob: (jobId: string) => void;
  onViewAll: () => void;
}) {
  if (!jobs || jobs.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Training Jobs Using This Dataset</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {jobs.map((job) => (
            <TrainingJobRow
              key={job.id}
              jobId={job.id}
              status={job.status}
              progress={job.progress_pct}
              onClick={() => onNavigateJob(job.id)}
            />
          ))}
        </div>
        <Button variant="outline" className="mt-4" onClick={onViewAll}>
          View All Jobs
        </Button>
      </CardContent>
    </Card>
  );
}

function OverviewTab({
  dataset,
  isLoading,
  relatedJobs,
  onNavigateJob,
  onViewAllJobs,
  versions,
  isLoadingVersions,
}: {
  dataset: Dataset;
  isLoading: boolean;
  relatedJobs: TrainingJobSummary[];
  onNavigateJob: (jobId: string) => void;
  onViewAllJobs: () => void;
  versions: DatasetVersionSummary[];
  isLoadingVersions: boolean;
}) {
  return (
    <div className="space-y-6">
      <DatasetOverview
        dataset={dataset}
        isLoading={isLoading}
        versions={versions}
        isLoadingVersions={isLoadingVersions}
        latestVersionId={dataset.dataset_version_id}
      />
      <TrainingJobsCard jobs={relatedJobs} onNavigateJob={onNavigateJob} onViewAll={onViewAllJobs} />
    </div>
  );
}

function TrainingWizardDialog({
  isOpen,
  onOpenChange,
  datasetId,
  onComplete,
  onCancel,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  datasetId?: string;
  onComplete: (jobId: string) => void;
  onCancel: () => void;
}) {
  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Start Training Job</DialogTitle>
        </DialogHeader>
        <TrainingWizard initialDatasetId={datasetId} onComplete={onComplete} onCancel={onCancel} />
      </DialogContent>
    </Dialog>
  );
}

export default function DatasetDetailPage() {
  return (
    <PageAsyncBoundary pageName="Dataset Detail">
      <DatasetDetailContent />
    </PageAsyncBoundary>
  );
}

function DatasetDetailContent() {
  const { datasetId } = useParams<{ datasetId: string }>();
  const navigate = useNavigate();
  const { can } = useRBAC();
  const { selectedTenant } = useTenant();
  const modelStatus = useModelStatus(selectedTenant ?? 'default');
  const [activeTab, setActiveTab] = useState<TabValue>('overview');
  const [isTrainingWizardOpen, setIsTrainingWizardOpen] = useState(false);
  const [isQuickTrainOpen, setIsQuickTrainOpen] = useState(false);
  const [isStartingTraining, setIsStartingTraining] = useState(false);
  const [isTrustOverrideOpen, setIsTrustOverrideOpen] = useState(false);
  const [lineageDirection, setLineageDirection] = useState<'both' | 'upstream' | 'downstream'>('both');
  const [includeEvidence, setIncludeEvidence] = useState(true);
  const [lineageCursors, setLineageCursors] = useState<Record<string, string>>({});

  const {
    data: dataset,
    isLoading,
    error,
    refetch,
  } = useTraining.useDataset(datasetId || '', {
    enabled: !!datasetId,
  });

  const {
    data: versionsData,
    isLoading: isLoadingVersions,
  } = useTraining.useDatasetVersions(datasetId || '', {
    enabled: !!datasetId,
  });

  // Fetch training jobs using this dataset (server-side filtered)
  const { data: jobsData } = useTraining.useTrainingJobs({ dataset_id: datasetId });
  const relatedJobs: TrainingJobSummary[] = jobsData?.jobs || [];

  const datasetVersionId = useMemo(() => dataset?.dataset_version_id || datasetId || '', [dataset?.dataset_version_id, datasetId]);
  const datasetVersions = versionsData?.versions || [];

  const {
    data: lineageData,
    isLoading: isLoadingLineage,
    refetch: refetchLineage,
  } = useLineage('dataset_version', datasetVersionId, {
    params: {
      direction: lineageDirection,
      include_evidence: includeEvidence,
      limit_per_level: 6,
      cursors: lineageCursors,
    },
    enabled: Boolean(datasetVersionId),
  });

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
      navigate(buildTrainingDatasetsLink());
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

  const handleTalkToDataset = useCallback(() => {
    if (!datasetId) return;
    navigate(buildDatasetChatLink(datasetId));
  }, [datasetId, navigate]);

  // Handle "Start Training" - use quick modal for valid datasets, wizard for others
  const handleStartTraining = useCallback(() => {
    if (!dataset) return;
    if (canUseQuickTrain(dataset)) {
      setIsQuickTrainOpen(true);
    } else {
      setIsTrainingWizardOpen(true);
    }
  }, [dataset]);

  // Handle quick train confirmation
  const handleQuickTrainConfirm = useCallback(
    async (config: QuickTrainConfig) => {
      if (!dataset) return;
      setIsStartingTraining(true);
      try {
        const request: StartTrainingRequest = {
          adapter_name: config.adapterName,
          dataset_id: dataset.id,
          base_model_id: modelStatus.modelId ?? undefined,
          config: {
            rank: config.rank,
            alpha: config.alpha,
            epochs: config.epochs,
            learning_rate: config.learningRate,
            batch_size: config.batchSize,
            targets: config.targets,
          },
        };
        const job = await apiClient.startTraining(request);
        setIsQuickTrainOpen(false);
        toast.success('Training started', {
          description: `Job created for adapter "${config.adapterName}"`,
          action: {
            label: 'View Progress',
            onClick: () => navigate(buildTrainingJobDetailLink(job.id)),
          },
        });
        navigate(buildTrainingJobDetailLink(job.id));
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to start training';
        toast.error(message);
        logger.error('Failed to start quick training', { component: 'DatasetDetailPage', datasetId }, err instanceof Error ? err : new Error(String(err)));
      } finally {
        setIsStartingTraining(false);
      }
    },
    [dataset, datasetId, navigate, modelStatus.modelId],
  );

  const handleNavigateLineageNode = useCallback(
    (node: { type?: string; id: string; href?: string }) => {
      if (node.href) {
        navigate(node.href);
        return;
      }
      switch (node.type) {
        case 'dataset':
        case 'dataset_version':
          navigate(buildDatasetDetailLink(node.id));
          return;
        case 'training_job':
          navigate(buildTrainingJobDetailLink(node.id));
          return;
        case 'adapter_version':
          navigate(`/adapters/${node.id}`);
          return;
        case 'document':
          navigate(`/documents/${node.id}`);
          return;
        default:
          return;
      }
    },
    [navigate],
  );

  const handleLineageLoadMore = useCallback(
    (level: { type: string; next_cursor?: string }) => {
      if (!level.next_cursor) return;
      setLineageCursors((prev) => ({
        ...prev,
        [level.type]: level.next_cursor!,
      }));
    },
    [],
  );

  if (isLoading) {
    return <LoadingView />;
  }

  if (error || !dataset) {
    return <ErrorView message={(error as Error)?.message || 'Dataset not found'} onRetry={() => refetch()} />;
  }

  return (
    <FeatureLayout title={dataset.name}>
      <div className="space-y-6">
        <HeaderSection
          datasetId={dataset.id}
          status={dataset.validation_status}
          trustState={dataset.trust_state}
          canStartTraining={can('training:start')}
          canValidate={can('dataset:validate')}
          canDelete={can('dataset:delete')}
          canOverrideTrust={can('admin')}
          onStartTraining={handleStartTraining}
          onValidate={handleValidate}
          onDelete={handleDelete}
          onTalkToDataset={handleTalkToDataset}
          onOverrideTrust={() => setIsTrustOverrideOpen(true)}
          isValidating={isValidating}
          isDeleting={isDeleting}
          onNavigateBack={() => navigate(buildTrainingDatasetsLink())}
        />

        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as TabValue)}>
          <TabsList className="grid w-full grid-cols-5">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="files">Files</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="validation">Validation</TabsTrigger>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="mt-6">
            <SectionAsyncBoundary section="dataset-overview">
              <OverviewTab
                dataset={dataset}
                isLoading={isLoading}
                versions={datasetVersions}
                isLoadingVersions={isLoadingVersions}
                relatedJobs={relatedJobs}
                onNavigateJob={(jobId) => navigate(buildTrainingJobDetailLink(jobId))}
                onViewAllJobs={() => navigate(buildTrainingJobsLink({ datasetId }))}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="files" className="mt-6">
            <SectionAsyncBoundary section="dataset-files">
              <DatasetFiles datasetId={datasetId!} isLoading={isLoading} />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="preview" className="mt-6">
            <SectionAsyncBoundary section="dataset-preview">
              <DatasetPreview datasetId={datasetId!} isLoading={isLoading} />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="validation" className="mt-6">
            <SectionAsyncBoundary section="dataset-validation">
              <DatasetValidation dataset={dataset} onValidate={handleValidate} isValidating={isValidating} />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="lineage" className="mt-6">
            <SectionAsyncBoundary section="dataset-lineage">
              <LineageViewer
                title="Dataset Lineage"
                data={lineageData ?? null}
                isLoading={isLoadingLineage}
                onRefresh={() => {
                  setLineageCursors({});
                  refetchLineage();
                }}
                direction={lineageDirection}
                includeEvidence={includeEvidence}
                onChangeDirection={setLineageDirection}
                onToggleEvidence={() => setIncludeEvidence((v) => !v)}
                onNavigateNode={handleNavigateLineageNode}
                onLoadMore={(level) => handleLineageLoadMore(level)}
              />
            </SectionAsyncBoundary>
          </TabsContent>
        </Tabs>

        <TrainingWizardDialog
          isOpen={isTrainingWizardOpen}
          onOpenChange={setIsTrainingWizardOpen}
          datasetId={datasetId}
          onComplete={(jobId) => {
            setIsTrainingWizardOpen(false);
            navigate(buildTrainingJobDetailLink(jobId));
          }}
          onCancel={() => setIsTrainingWizardOpen(false)}
        />

        {/* Quick train modal for valid datasets */}
        {dataset && (
          <QuickTrainConfirmModal
            open={isQuickTrainOpen}
            onOpenChange={setIsQuickTrainOpen}
            dataset={dataset}
            onConfirm={handleQuickTrainConfirm}
            onCancel={() => setIsQuickTrainOpen(false)}
            onAdvanced={() => {
              setIsQuickTrainOpen(false);
              setIsTrainingWizardOpen(true);
            }}
            isLoading={isStartingTraining}
          />
        )}

        {/* Trust override dialog for admin users */}
        {dataset && (
          <TrustOverrideDialog
            open={isTrustOverrideOpen}
            onOpenChange={setIsTrustOverrideOpen}
            datasetId={dataset.id}
            datasetVersionId={dataset.dataset_version_id}
            currentTrustState={dataset.trust_state}
            onSuccess={() => {
              refetch();
            }}
          />
        )}
      </div>
    </FeatureLayout>
  );
}
