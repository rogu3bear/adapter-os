import React, { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { FlowStep, type FlowStepStatus } from '@/components/GuidedFlow/FlowStep';
import { FlowProgress } from '@/components/GuidedFlow/FlowProgress';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { useTraining } from '@/hooks/useTraining';
import { useAdapterStacks, useCreateAdapterStack } from '@/hooks/useAdmin';
import { TrainingWizard } from '@/components/TrainingWizard';
import { ChatInterface } from '@/components/ChatInterface';
import { useTenant } from '@/layout/LayoutProvider';
import { ArrowRight, ArrowLeft, CheckCircle, Play } from 'lucide-react';
import { toast } from 'sonner';
import type { Dataset } from '@/api/training-types';
import type { TrainingJob } from '@/api/training-types';

const TOTAL_STEPS = 6;

export default function GuidedFlowPage() {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [currentStep, setCurrentStep] = useState(1);
  const [datasetId, setDatasetId] = useState<string | undefined>();
  const [jobId, setJobId] = useState<string | undefined>();
  const [adapterId, setAdapterId] = useState<string | undefined>();
  const [stackId, setStackId] = useState<string | undefined>();
  const [isTrainingWizardOpen, setIsTrainingWizardOpen] = useState(false);
  const [isStackDialogOpen, setIsStackDialogOpen] = useState(false);

  const { data: dataset } = useTraining.useDataset(datasetId || '', {
    enabled: !!datasetId,
  });
  const { data: job } = useTraining.useTrainingJob(jobId || '', {
    enabled: !!jobId,
  });
  const createStack = useCreateAdapterStack();

  const getStepStatus = (step: number): FlowStepStatus => {
    if (step < currentStep) return 'completed';
    if (step === currentStep) return 'active';
    return 'pending';
  };

  const handleNext = useCallback(() => {
    if (currentStep < TOTAL_STEPS) {
      setCurrentStep(currentStep + 1);
    }
  }, [currentStep]);

  const handleBack = useCallback(() => {
    if (currentStep > 1) {
      setCurrentStep(currentStep - 1);
    }
  }, [currentStep]);

  const handleDatasetCreated = useCallback((newDatasetId: string) => {
    setDatasetId(newDatasetId);
    toast.success('Dataset created successfully');
  }, []);

  const handleTrainingStarted = useCallback((newJobId: string) => {
    setJobId(newJobId);
    setIsTrainingWizardOpen(false);
    toast.success('Training job started');
    setCurrentStep(4); // Move to monitoring step
  }, []);

  const handleTrainingCompleted = useCallback(() => {
    if (job?.status === 'completed' && job.adapter_id) {
      setAdapterId(job.adapter_id);
      toast.success('Training completed! Adapter created.');
      setCurrentStep(5); // Move to stack creation
    }
  }, [job]);

  // Monitor job completion
  React.useEffect(() => {
    if (job?.status === 'completed' && job.adapter_id && currentStep === 4) {
      handleTrainingCompleted();
    }
  }, [job, currentStep, handleTrainingCompleted]);

  const handleCreateStack = useCallback(async () => {
    if (!adapterId) {
      toast.error('No adapter available');
      return;
    }

    try {
      const stack = await createStack.mutateAsync({
        name: `stack-${adapterId.slice(0, 8)}`,
        description: `Stack created from guided flow`,
        adapters: [
          {
            adapter_id: adapterId,
            gate: 32767,
          },
        ],
      });
      setStackId(stack.id);
      setIsStackDialogOpen(false);
      toast.success('Stack created successfully');
      setCurrentStep(6); // Move to chat
    } catch (error) {
      const err = error instanceof Error ? error : new Error('Failed to create stack');
      toast.error(err.message);
    }
  }, [adapterId, createStack]);

  return (
    <FeatureLayout title="Guided LoRA Flow" description="Complete workflow from dataset to chat">
      <div className="space-y-6">
        <FlowProgress currentStep={currentStep} totalSteps={TOTAL_STEPS} />

        {/* Step 1: Upload Dataset */}
        <FlowStep
          stepNumber={1}
          title="Upload Dataset"
          description="Upload your training data"
          status={getStepStatus(1)}
        >
          {currentStep === 1 ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Upload your dataset files to get started with training.
              </p>
              <Button onClick={() => navigate('/training/datasets')}>
                Go to Datasets
              </Button>
              {datasetId && dataset && (
                <Card className="mt-4 border-green-500">
                  <CardContent className="pt-4">
                    <div className="flex items-center justify-between">
                      <div>
                        <p className="font-medium">{dataset.name}</p>
                        <p className="text-sm text-muted-foreground">{dataset.id}</p>
                        <Badge variant="outline" className="mt-2">
                          {dataset.validation_status}
                        </Badge>
                      </div>
                      <CheckCircle className="h-5 w-5 text-green-500" />
                    </div>
                  </CardContent>
                </Card>
              )}
              {datasetId && (
                <Button onClick={handleNext} className="mt-4">
                  Next: Validate Dataset <ArrowRight className="h-4 w-4 ml-2" />
                </Button>
              )}
            </div>
          ) : datasetId && dataset ? (
            <div className="text-sm text-muted-foreground">
              Dataset: {dataset.name} ({dataset.id})
            </div>
          ) : null}
        </FlowStep>

        {/* Step 2: Validate Dataset */}
        <FlowStep
          stepNumber={2}
          title="Validate Dataset"
          description="Ensure your dataset is ready for training"
          status={getStepStatus(2)}
        >
          {currentStep === 2 && datasetId ? (
            <div className="space-y-4">
              {dataset?.validation_status === 'valid' ? (
                <>
                  <div className="flex items-center gap-2 text-green-600">
                    <CheckCircle className="h-5 w-5" />
                    <span>Dataset is valid and ready for training</span>
                  </div>
                  <Button onClick={handleNext}>
                    Next: Configure Training <ArrowRight className="h-4 w-4 ml-2" />
                  </Button>
                </>
              ) : (
                <>
                  <p className="text-sm text-muted-foreground">
                    Dataset validation status: <Badge>{dataset?.validation_status || 'unknown'}</Badge>
                  </p>
                  <Button onClick={() => navigate(`/training/datasets/${datasetId}`)}>
                    Validate Dataset
                  </Button>
                </>
              )}
            </div>
          ) : dataset?.validation_status === 'valid' ? (
            <div className="text-sm text-green-600">Dataset validated</div>
          ) : null}
        </FlowStep>

        {/* Step 3: Configure Training */}
        <FlowStep
          stepNumber={3}
          title="Configure Training"
          description="Set up your training parameters"
          status={getStepStatus(3)}
        >
          {currentStep === 3 ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Configure your training job with essential parameters.
              </p>
              <Button onClick={() => setIsTrainingWizardOpen(true)}>
                <Play className="h-4 w-4 mr-2" />
                Start Training Wizard
              </Button>
            </div>
          ) : jobId ? (
            <div className="text-sm text-muted-foreground">
              Training job: {jobId}
            </div>
          ) : null}
        </FlowStep>

        {/* Step 4: Monitor Training */}
        <FlowStep
          stepNumber={4}
          title="Monitor Training"
          description="Watch your adapter train in real-time"
          status={getStepStatus(4)}
        >
          {currentStep === 4 && jobId ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Training job is in progress. Monitor its progress below.
              </p>
              <Button onClick={() => navigate(`/training/jobs/${jobId}`)}>
                View Training Job
              </Button>
              {job?.status === 'completed' && job.adapter_id && (
                <div className="mt-4">
                  <div className="flex items-center gap-2 text-green-600">
                    <CheckCircle className="h-5 w-5" />
                    <span>Training completed! Adapter created.</span>
                  </div>
                  <Button onClick={handleNext} className="mt-2">
                    Next: Create Stack <ArrowRight className="h-4 w-4 ml-2" />
                  </Button>
                </div>
              )}
            </div>
          ) : job?.status === 'completed' ? (
            <div className="text-sm text-green-600">Training completed</div>
          ) : null}
        </FlowStep>

        {/* Step 5: Create Stack */}
        <FlowStep
          stepNumber={5}
          title="Create Stack"
          description="Combine your adapter into a stack"
          status={getStepStatus(5)}
        >
          {currentStep === 5 ? (
            <div className="space-y-4">
              {adapterId ? (
                <>
                  <p className="text-sm text-muted-foreground">
                    Create a stack with your trained adapter to use it in chat.
                  </p>
                  {!stackId ? (
                    <Button onClick={() => setIsStackDialogOpen(true)}>
                      Create Stack
                    </Button>
                  ) : (
                    <>
                      <div className="flex items-center gap-2 text-green-600">
                        <CheckCircle className="h-5 w-5" />
                        <span>Stack created successfully</span>
                      </div>
                      <Button onClick={handleNext}>
                        Next: Test in Chat <ArrowRight className="h-4 w-4 ml-2" />
                      </Button>
                    </>
                  )}
                </>
              ) : (
                <p className="text-sm text-muted-foreground">
                  Waiting for training to complete...
                </p>
              )}
            </div>
          ) : stackId ? (
            <div className="text-sm text-muted-foreground">Stack created</div>
          ) : null}
        </FlowStep>

        {/* Step 6: Test in Chat */}
        <FlowStep
          stepNumber={6}
          title="Test in Chat"
          description="Try your adapter in a conversation"
          status={getStepStatus(6)}
        >
          {currentStep === 6 && stackId ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Your adapter is ready! Test it in the chat interface below.
              </p>
              <div className="border rounded-lg h-[500px]">
                <ChatInterface selectedTenant={selectedTenant} initialStackId={stackId} />
              </div>
            </div>
          ) : stackId ? (
            <div className="text-sm text-muted-foreground">Ready to test</div>
          ) : null}
        </FlowStep>

        {/* Navigation */}
        <div className="flex items-center justify-between pt-4 border-t">
          <Button variant="outline" onClick={handleBack} disabled={currentStep === 1}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <Button variant="outline" onClick={() => navigate('/dashboard')}>
            Exit Flow
          </Button>
        </div>

        {/* Training Wizard Dialog */}
        <Dialog open={isTrainingWizardOpen} onOpenChange={setIsTrainingWizardOpen}>
          <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>Start Training Job</DialogTitle>
            </DialogHeader>
            <TrainingWizard
              initialDatasetId={datasetId}
              onComplete={handleTrainingStarted}
              onCancel={() => setIsTrainingWizardOpen(false)}
            />
          </DialogContent>
        </Dialog>

        {/* Create Stack Dialog */}
        <Dialog open={isStackDialogOpen} onOpenChange={setIsStackDialogOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Create Stack</DialogTitle>
            </DialogHeader>
            <div className="space-y-4 py-4">
              <p className="text-sm text-muted-foreground">
                Create a stack with adapter: {adapterId}
              </p>
              <Button onClick={handleCreateStack} disabled={createStack.isPending}>
                {createStack.isPending ? 'Creating...' : 'Create Stack'}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      </div>
    </FeatureLayout>
  );
}

