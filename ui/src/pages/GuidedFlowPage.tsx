import React, { useState, useCallback, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { FlowStep, type FlowStepStatus } from '@/components/GuidedFlow/FlowStep';
import { FlowProgress } from '@/components/GuidedFlow/FlowProgress';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { useTraining } from '@/hooks/training';
import { useAdapterStacks } from '@/hooks/admin/useAdmin';
import { ChatInterface } from '@/components/ChatInterface';
import { useTenant } from '@/providers/FeatureProviders';
import { ArrowRight, ArrowLeft, CheckCircle, Play, UploadCloud } from 'lucide-react';
import { toast } from 'sonner';
import type { Dataset, DatasetValidationStatus, TrainingJob } from '@/api/training-types';
import { buildTrainingDatasetsLink, buildDatasetDetailLink, buildTrainingJobDetailLink, buildChatLink, buildAdaptersRegisterLink, buildDashboardLink } from '@/utils/navLinks';

const TOTAL_STEPS = 3;

function StatusBadge({ status }: { status: DatasetValidationStatus | string }) {
  const variant =
    status === 'valid'
      ? 'default'
      : status === 'validating'
        ? 'secondary'
        : status === 'invalid'
          ? 'destructive'
          : 'outline';
  return (
    <Badge variant={variant as 'default' | 'secondary' | 'destructive' | 'outline'} className="capitalize">
      {status || 'unknown'}
    </Badge>
  );
}

export default function GuidedFlowPage() {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [currentStep, setCurrentStep] = useState(1);
  const [datasetId, setDatasetId] = useState<string | undefined>();
  const [datasetName, setDatasetName] = useState<string | undefined>();
  const [jobId, setJobId] = useState<string | undefined>();
  const [stackId, setStackId] = useState<string | undefined>();
  const [selectedFiles, setSelectedFiles] = useState<FileList | null>(null);

  const { data: stacks = [] } = useAdapterStacks();
  const { data: dataset } = useTraining.useDataset(datasetId || '', {
    enabled: !!datasetId,
    refetchInterval: (query) => {
      const d = query.state.data;
      if (!d) return false;
      return d.validation_status !== 'valid' ? 2000 : false;
    },
  });

  const { data: job } = useTraining.useTrainingJob(jobId || '', {
    enabled: !!jobId,
  });

  const { data: bootstrap } = useTraining.useChatBootstrap(
    job?.status === 'completed' ? jobId : undefined
  );

  const { mutateAsync: createDataset, isPending: isUploading } = useTraining.useCreateDataset({
    onSuccess: (resp) => {
      const newId =
        ('dataset_id' in resp ? resp.dataset_id : undefined) ||
        ('id' in resp ? resp.id : undefined);
      setDatasetId(newId as string);
      setDatasetName(newId as string);
      toast.success('Dataset uploaded');
      setCurrentStep(1);
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to upload dataset');
    },
  });

  const { mutateAsync: validateDataset, isPending: isValidating } = useTraining.useValidateDataset({
    onSuccess: () => {
      toast.message('Validation started', { description: 'Polling until validation finishes.' });
    },
    onError: (err) => toast.error(err.message || 'Validation failed'),
  });

  const getStepStatus = (step: number): FlowStepStatus => {
    if (step < currentStep) return 'completed';
    if (step === currentStep) return 'active';
    return 'pending';
  };

  const handleBack = useCallback(() => {
    if (currentStep > 1) {
      setCurrentStep(currentStep - 1);
    }
  }, [currentStep]);

  const handleTrainingStarted = useCallback((newJobId: string) => {
    setJobId(newJobId);
    toast.success('Training job started');
    setCurrentStep(3);
  }, []);

  // Auto-advance when dataset becomes valid
  useEffect(() => {
    if (dataset?.validation_status === 'valid' && currentStep === 1) {
      toast.success('Dataset validated and ready for training');
      setCurrentStep(2);
    }
  }, [currentStep, dataset?.validation_status]);

  // Capture stack once training completes
  useEffect(() => {
    if (bootstrap?.ready && bootstrap.stack_id) {
      setStackId(bootstrap.stack_id);
      if (currentStep === 3) {
        toast.success('Training completed! Stack ready for chat.');
      }
    }
  }, [currentStep, bootstrap]);

  useEffect(() => {
    if (job?.status === 'completed' && currentStep < 3) {
      setCurrentStep(3);
    }
  }, [currentStep, job?.status]);

  const handleUpload = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedFiles || selectedFiles.length === 0) {
      toast.error('Please select a file to upload');
      return;
    }
    const primaryName = datasetName || selectedFiles[0]?.name || 'doc-chat-dataset';
    await createDataset({
      name: primaryName,
      source_type: 'uploaded_files',
      files: Array.from(selectedFiles),
    });
  };

  const handleValidate = async () => {
    if (!datasetId) return;
    await validateDataset(datasetId);
  };

  const selectedStackName = stackId
    ? stacks.find((s) => s.id === stackId)?.name || stackId
    : undefined;

  return (
    <PageWrapper pageKey="guided-flow" title="Doc → Chat Quickstart" description="Bring a file, train a LoRA, and chat with it in one screen.">
      <div className="space-y-6">
        <FlowProgress currentStep={currentStep} totalSteps={TOTAL_STEPS} />

        {/* Step 1: Upload & Validate */}
        <FlowStep
          stepNumber={1}
          title="Upload & Validate"
          description="Drop a file, we’ll turn it into a dataset and validate it."
          status={getStepStatus(1)}
        >
          <div className="space-y-4">
            <form onSubmit={handleUpload} className="space-y-3">
              <div className="flex flex-col gap-2">
                <label className="text-sm font-medium">Select file(s)</label>
                <Input
                  type="file"
                  multiple
                  accept=".jsonl,.txt,.md,.pdf"
                  onChange={(e) => setSelectedFiles(e.target.files)}
                />
              </div>
              <Button type="submit" disabled={isUploading}>
                <UploadCloud className="h-4 w-4 mr-2" />
                {isUploading ? 'Uploading...' : 'Upload dataset'}
              </Button>
            </form>

            {dataset && (
              <Card className="border-muted">
                <CardContent className="pt-4 flex items-center justify-between">
                  <div>
                    <p className="font-medium">{dataset.name}</p>
                    <p className="text-xs text-muted-foreground">{dataset.id}</p>
                    <div className="mt-2 flex items-center gap-2 text-sm">
                      <span>Status:</span>
                      <StatusBadge status={dataset.validation_status} />
                    </div>
                    {dataset.validation_status === 'invalid' && dataset.validation_errors && (
                      <p className="text-xs text-destructive mt-2">
                        {dataset.validation_errors}
                      </p>
                    )}
                  </div>
                  {dataset.validation_status === 'valid' ? (
                    <CheckCircle className="h-5 w-5 text-green-500" />
                  ) : null}
                </CardContent>
              </Card>
            )}

            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                disabled={!datasetId || isValidating}
                onClick={handleValidate}
              >
                {isValidating ? 'Validating…' : 'Validate dataset'}
              </Button>
              <Button
                variant="ghost"
                onClick={() => navigate(buildTrainingDatasetsLink())}
              >
                Open full dataset view
              </Button>
            </div>

            {dataset?.validation_status === 'valid' && (
              <Button onClick={() => setCurrentStep(2)}>
                Next: Train on this dataset <ArrowRight className="h-4 w-4 ml-2" />
              </Button>
            )}
            {dataset?.validation_status === 'invalid' && (
              <div className="text-sm text-destructive space-y-2">
                <div>Dataset is invalid. Fix issues on the dataset detail page before training.</div>
                <Button variant="link" className="px-0" onClick={() => navigate(buildDatasetDetailLink(dataset.id))}>
                  Open dataset detail
                </Button>
              </div>
            )}
          </div>
        </FlowStep>

        {/* Step 2: Train */}
        <FlowStep
          stepNumber={2}
          title="Train"
          description="Kick off a LoRA job on the validated dataset."
          status={getStepStatus(2)}
        >
          {dataset?.validation_status !== 'valid' ? (
            <p className="text-sm text-muted-foreground">
              Upload and validate a dataset first to unlock training.
            </p>
          ) : (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Use the full-featured training wizard to configure and start your training job.
              </p>
              <Button onClick={() => navigate(buildAdaptersRegisterLink(), { state: { preselectedDatasetId: datasetId } })}>
                <Play className="h-4 w-4 mr-2" />
                Open Training Wizard
              </Button>
              {jobId && (
                <div className="text-sm">
                  Training job: <Badge variant="outline">{jobId}</Badge>{' '}
                  <span className="text-muted-foreground">({job?.status || 'starting'})</span>
                </div>
              )}
              {job?.status === 'failed' && (
                <div className="text-sm text-destructive">
                  Training failed{job.error_message ? `: ${job.error_message}` : ''}. You can retry with the wizard.
                </div>
              )}
              {job?.status === 'completed' && bootstrap?.stack_id && (
                <div className="flex items-center gap-2 text-green-600">
                  <CheckCircle className="h-5 w-5" />
                  <span>Training complete. Stack ready.</span>
                </div>
              )}
            </div>
          )}
        </FlowStep>

        {/* Step 3: Chat */}
        <FlowStep
          stepNumber={3}
          title="Chat with your doc"
          description="Use the trained stack in chat."
          status={getStepStatus(3)}
        >
          {currentStep < 3 ? (
            <p className="text-sm text-muted-foreground">
              Start training first to enable chat.
            </p>
          ) : job?.status && job?.status !== 'completed' ? (
            <div className="space-y-3">
              <p className="text-sm text-muted-foreground">
                Training is {job.status}. You can monitor the job while we finish.
              </p>
              <Button onClick={() => navigate(buildTrainingJobDetailLink(jobId!))} variant="outline">
                View training job
              </Button>
            </div>
          ) : job?.status === 'failed' ? (
            <div className="space-y-3">
              <p className="text-sm text-destructive">
                Training failed{job.error_message ? `: ${job.error_message}` : ''}. Chat will use the default stack until a successful training run completes.
              </p>
              <Button variant="outline" onClick={() => setCurrentStep(2)}>
                Retry training
              </Button>
            </div>
          ) : (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                {stackId
                  ? `Chat is using stack: ${selectedStackName || stackId}.`
                  : 'Chat will use the default stack for this workspace.'}
              </p>
              <div className="border rounded-lg h-[calc(var(--base-unit)*125)]">
                <ChatInterface selectedTenant={selectedTenant} initialStackId={stackId} />
              </div>
              <Button variant="ghost" onClick={() => navigate(buildChatLink())}>
                Open full chat view
              </Button>
            </div>
          )}
        </FlowStep>

        {/* Navigation */}
        <div className="flex items-center justify-between pt-4 border-t">
          <Button variant="outline" onClick={handleBack} disabled={currentStep === 1}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <Button variant="outline" onClick={() => navigate(buildDashboardLink())}>
            Exit Flow
          </Button>
        </div>
      </div>
    </PageWrapper>
  );
}
