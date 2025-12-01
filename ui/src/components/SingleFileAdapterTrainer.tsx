// Breadcrumbs are now derived statelessly from URL (see useBreadcrumbs hook)
// 【ui/src/components/BreadcrumbNavigation.tsx§1-61】 - Breadcrumb component
import React, { useState, useRef, useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import apiClient from '@/api/client';
import {
  Upload,
  FileText,
  Settings,
  Zap,
  Play,
  Download,
  CheckCircle,
  XCircle,
  Activity,
  Loader2,
  AlertCircle,
  Cpu,
  TrendingUp
} from 'lucide-react';
import type { TrainingJob, TrainingConfigRequest, InferRequest, InferResponse } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { ProgressIndicator, ContextualLoading, loadingStates } from './ui/progress-indicator';
import { SuccessFeedback, successTemplates } from './ui/success-feedback';
import { useViewTransition } from '@/hooks/useViewTransition';
import { BreadcrumbNavigation } from './BreadcrumbNavigation';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';

/**
 * Training configuration form schema for SingleFileAdapterTrainer
 *
 * Uses semantic naming format: {tenant}/{domain}/{purpose}/{revision}
 * Example: default/training/my-adapter/r001
 */
const TrainerConfigSchema = z.object({
  // Semantic naming components
  tenant: z.string()
    .min(1, 'Tenant is required')
    .max(50, 'Tenant must not exceed 50 characters')
    .regex(
      /^[a-z0-9_-]+$/,
      'Tenant must contain only lowercase letters, numbers, underscores, and hyphens'
    ),
  domain: z.string()
    .min(1, 'Domain is required')
    .max(50, 'Domain must not exceed 50 characters')
    .regex(
      /^[a-z0-9_-]+$/,
      'Domain must contain only lowercase letters, numbers, underscores, and hyphens'
    ),
  purpose: z.string()
    .min(1, 'Purpose is required')
    .max(50, 'Purpose must not exceed 50 characters')
    .regex(
      /^[a-z0-9_-]+$/,
      'Purpose must contain only lowercase letters, numbers, underscores, and hyphens'
    ),
  revision: z.string()
    .regex(
      /^r\d{3,}$/,
      'Revision must be in format rXXX (e.g., r001)'
    ),
  // Training parameters
  rank: z.number()
    .int('Rank must be an integer')
    .min(1, 'Rank must be at least 1')
    .max(64, 'Rank must not exceed 64'),
  alpha: z.number()
    .int('Alpha must be an integer')
    .min(1, 'Alpha must be at least 1')
    .max(128, 'Alpha must not exceed 128'),
  epochs: z.number()
    .int('Epochs must be an integer')
    .min(1, 'At least 1 epoch required')
    .max(20, 'Epochs must not exceed 20'),
  batchSize: z.number()
    .int('Batch size must be an integer')
    .min(1, 'Batch size must be at least 1')
    .max(32, 'Batch size must not exceed 32'),
  learningRate: z.number()
    .positive('Learning rate must be positive')
    .max(0.1, 'Learning rate must not exceed 0.1'),
});

/**
 * Helper to sanitize a string for use in semantic naming
 */
function sanitizeForSemanticName(input: string): string {
  return input
    .toLowerCase()
    .replace(/\.[^/.]+$/, '') // Remove file extension
    .replace(/[^a-z0-9_-]/g, '-') // Replace invalid chars with hyphen
    .replace(/-+/g, '-') // Collapse multiple hyphens
    .replace(/^-|-$/g, ''); // Trim hyphens from ends
}

type TrainerConfigFormData = z.infer<typeof TrainerConfigSchema>;

type TrainingStep = 'upload' | 'configure' | 'training' | 'complete';

const STEP_LABELS: Record<TrainingStep, string> = {
  upload: 'Upload File',
  configure: 'Configure Training',
  training: 'Training Progress',
  complete: 'Test & Download'
};

interface TrainingMetrics {
  loss: number;
  epoch: number;
  progress: number;
}

export function SingleFileAdapterTrainer() {
  const { can } = useRBAC();
  const [step, setStep] = useState<TrainingStep>('upload');
  const [file, setFile] = useState<File | null>(null);
  const [fileContent, setFileContent] = useState<string>('');
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [fileUploadError, setFileUploadError] = useState<Error | null>(null);

  // Configuration form with validation
  const {
    register,
    handleSubmit: handleFormSubmit,
    formState: { errors, isValid },
    setValue,
    watch,
    reset: resetForm,
  } = useForm<TrainerConfigFormData>({
    resolver: zodResolver(TrainerConfigSchema),
    mode: 'onBlur',
    defaultValues: {
      tenant: 'default',
      domain: 'training',
      purpose: '',
      revision: 'r001',
      rank: 8,
      alpha: 16,
      epochs: 3,
      batchSize: 4,
      learningRate: 0.0003,
    },
  });

  // Compute full semantic adapter name
  const tenant = watch('tenant');
  const domain = watch('domain');
  const purpose = watch('purpose');
  const revision = watch('revision');
  const fullAdapterName = tenant && domain && purpose && revision
    ? `${tenant}/${domain}/${purpose}/${revision}`
    : '';

  const formValues = watch();

  // Training state
  const [trainingJob, setTrainingJob] = useState<TrainingJob | null>(null);
  const [trainingMetrics, setTrainingMetrics] = useState<TrainingMetrics | null>(null);
  const [trainingError, setTrainingError] = useState<Error | null>(null);
  const [isTraining, setIsTraining] = useState(false);
  const [uploadError, setUploadError] = useState<Error | null>(null);

  // Testing state
  const [testPrompt, setTestPrompt] = useState('');
  const [testResult, setTestResult] = useState<InferResponse | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // View transitions for smooth navigation
  const transitionTo = useViewTransition();

  const handleFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const uploadedFile = event.target.files?.[0];
    if (uploadedFile) {
      // Validate file size (10MB max)
      if (uploadedFile.size > 10 * 1024 * 1024) {
        setFileUploadError(new Error('File size exceeds 10MB limit'));
        return;
      }

      setFileUploadError(null);
      setFile(uploadedFile);

      // Read file content for preview
      const reader = new FileReader();
      reader.onload = (e) => {
        const content = e.target?.result as string;
        setFileContent(content);
      };
      reader.onerror = () => {
        setFileUploadError(new Error('Failed to read file content'));
      };
      reader.readAsText(uploadedFile);

      // Auto-generate purpose from filename (sanitize to match semantic naming)
      const sanitizedName = sanitizeForSemanticName(uploadedFile.name);
      const purposeName = sanitizedName || 'custom-adapter';
      setValue('purpose', purposeName, { shouldValidate: true });
    }
  };

  const handleStartTraining = async (data: TrainerConfigFormData) => {
    if (!file) {
      setTrainingError(new Error('Please upload a file first'));
      return;
    }

    setIsTraining(true);
    setTrainingError(null);
    setStep('training');

    try {
      // In a real implementation, we would:
      // 1. Upload the file to a temp location
      // 2. Convert it to the training dataset format
      // 3. Start the training job via API

      // Convert form data to TrainingConfigRequest format
      const config: TrainingConfigRequest = {
        rank: data.rank,
        alpha: data.alpha,
        epochs: data.epochs,
        learning_rate: data.learningRate,
        batch_size: data.batchSize,
        targets: ['q_proj', 'v_proj'],
      };

      // Build full semantic adapter name
      const semanticAdapterName = `${data.tenant}/${data.domain}/${data.purpose}/${data.revision}`;

      // For now, we'll create a training job with the file content
      // Note: UI-only fields (dataset_path, adapters_root, package) removed
      // In production, file upload would create a dataset_id to pass here
      const response = await apiClient.startTraining({
        adapter_name: semanticAdapterName,
        config: config,
        // dataset_id: would be set after file upload creates a dataset
      });

      setTrainingJob(response as TrainingJob);

      // Poll for training progress
      pollTrainingProgress(response.id);
    } catch (error) {
      const err = error instanceof Error ? error : new Error('Training failed');
      setTrainingError(err);
      setIsTraining(false);
      setStep('configure');
      logger.error('Training failed', { component: 'SingleFileAdapterTrainer' }, err);
    }
  };

  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const pollTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  const pollTrainingProgress = async (jobId: string) => {
    // Clear any existing polling
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
    }
    if (pollTimeoutRef.current) {
      clearTimeout(pollTimeoutRef.current);
    }

    pollIntervalRef.current = setInterval(async () => {
      try {
        const job = await apiClient.getTrainingJob(jobId);
        setTrainingJob(job);

        if (job.current_epoch && job.total_epochs && job.current_loss) {
          setTrainingMetrics({
            loss: job.current_loss,
            epoch: job.current_epoch,
            progress: (job.current_epoch / job.total_epochs) * 100
          });
        }

        if (job.status === 'completed') {
          if (pollIntervalRef.current) {
            clearInterval(pollIntervalRef.current);
            pollIntervalRef.current = null;
          }
          setIsTraining(false);
          setStep('complete');
        } else if (job.status === 'failed') {
          if (pollIntervalRef.current) {
            clearInterval(pollIntervalRef.current);
            pollIntervalRef.current = null;
          }
          setTrainingError(new Error(job.error_message || 'Training failed'));
          setIsTraining(false);
        }
      } catch (error) {
        logger.error('Failed to poll training job', { component: 'SingleFileAdapterTrainer', operation: 'pollTrainingJob' }, toError(error));
        if (pollIntervalRef.current) {
          clearInterval(pollIntervalRef.current);
          pollIntervalRef.current = null;
        }
        setIsTraining(false);
        setTrainingError(new Error('Lost connection to training job'));
      }
    }, 2000); // Poll every 2 seconds

    // Cleanup after 30 minutes
    pollTimeoutRef.current = setTimeout(() => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    }, 30 * 60 * 1000);
  };

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
      }
      if (pollTimeoutRef.current) {
        clearTimeout(pollTimeoutRef.current);
      }
    };
  }, []);

  const handleTestInference = async () => {
    if (!testPrompt || !trainingJob?.adapter_id) {
      return;
    }

    setIsTesting(true);
    setTestResult(null);

    try {
      const response = await apiClient.infer({
        prompt: testPrompt,
        max_tokens: 100,
        adapters: [trainingJob.adapter_id]
      });

      setTestResult(response);
    } catch (error) {
      logger.error('Inference test failed', { component: 'SingleFileAdapterTrainer', operation: 'testInference' }, toError(error));
      setTestResult({
        schema_version: 'v1',
        id: 'error',
        text: 'Error: ' + (error instanceof Error ? error.message : 'Unknown error'),
        tokens_generated: 0,
        latency_ms: 0,
        adapters_used: [],
        finish_reason: 'error',
        trace: {
          router_decisions: [],
          evidence_spans: [],
          latency_ms: 0
        }
      });
    } finally {
      setIsTesting(false);
    }
  };

  const handleDownloadAdapter = () => {
    if (!trainingJob?.artifact_path) {
      return;
    }

    // In production, this would download the .aos file from the server
    window.open(`/api/v1/training/jobs/${trainingJob.id}/artifacts`, '_blank');
  };

  const resetTrainer = () => {
    setStep('upload');
    setFile(null);
    setFileContent('');
    resetForm();
    setTrainingJob(null);
    setTrainingMetrics(null);
    setTrainingError(null);
    setTestPrompt('');
    setTestResult(null);
  };

  const handleUploadToServer = async () => {
    setUploadError(null);
    if (!trainingJob?.artifact_path) {
      setUploadError(new Error('No artifact available to upload'));
      return;
    }

    try {
      // Download the artifact
      const response = await fetch(`/api/v1/training/jobs/${trainingJob.id}/artifacts`);
      if (!response.ok) {
        throw new Error('Failed to download artifact');
      }

      const blob = await response.blob();
      const file = new File([blob], `${trainingJob.adapter_name || 'adapter'}.aos`, { type: 'application/octet-stream' });

      // Upload via import API
      const adapter = await apiClient.importAdapter(file, true);
      // Success - could show success feedback
      
      // Optionally navigate to inference
      if (window.confirm('Adapter uploaded successfully! Would you like to chat with it now?')) {
        window.location.href = `/inference?adapter=${adapter.adapter_id}`;
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Upload failed');
      setUploadError(error);
      logger.error('Upload to server failed', { component: 'SingleFileAdapterTrainer' }, error);
    }
  };

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Breadcrumb Navigation */}
      <BreadcrumbNavigation />
      
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold">Single-File Adapter Trainer</h1>
        <p className="text-muted-foreground">
          Train a custom adapter from a single file and test it interactively
        </p>
      </div>

      {/* Progress Steps */}
      <div className="flex items-center justify-between">
        {[
          { id: 'upload', label: 'Upload File', icon: Upload },
          { id: 'configure', label: 'Configure', icon: Settings },
          { id: 'training', label: 'Training', icon: Zap },
          { id: 'complete', label: 'Test & Download', icon: CheckCircle }
        ].map((s, idx, arr) => (
          <React.Fragment key={s.id}>
            <div className="flex flex-col items-center">
              <div
                className={`w-12 h-12 rounded-full flex items-center justify-center ${
                  step === s.id
                    ? 'bg-blue-600 text-white'
                    : arr.findIndex(x => x.id === step) > idx
                    ? 'bg-green-600 text-white'
                    : 'bg-gray-200 dark:bg-gray-700 text-gray-400'
                }`}
              >
                <s.icon className="w-6 h-6" />
              </div>
              <span className="text-xs mt-2 font-medium">{s.label}</span>
            </div>
            {idx < arr.length - 1 && (
              <div
                className={`flex-1 h-1 mx-2 ${
                  arr.findIndex(x => x.id === step) > idx
                    ? 'bg-green-600'
                    : 'bg-gray-200 dark:bg-gray-700'
                }`}
              />
            )}
          </React.Fragment>
        ))}
      </div>

      {/* Step 1: Upload File */}
      {step === 'upload' && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Upload className="w-5 h-5" />
              Upload Training Data
              <GlossaryTooltip termId="trainer-file-upload" />
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {fileUploadError && (
              <ErrorRecovery
                error={fileUploadError.message}
                onRetry={() => { setFileUploadError(null); fileInputRef.current?.click(); }}
              />
            )}
            <div
              className="border-2 border-dashed rounded-lg p-12 text-center cursor-pointer hover:border-blue-500 transition-colors"
              onClick={() => fileInputRef.current?.click()}
            >
              <input
                ref={fileInputRef}
                type="file"
                onChange={handleFileUpload}
                accept=".txt,.json,.py,.js,.ts,.md"
                className="hidden"
              />
              <FileText className="w-16 h-16 text-muted-foreground mx-auto mb-4" />
              <p className="text-lg font-medium mb-2">
                {file ? file.name : 'Click to upload file'}
              </p>
              <p className="text-sm text-muted-foreground">
                Supports .txt, .json, .py, .js, .ts, .md (max 10MB)
              </p>
            </div>

            {file && (
              <div className="space-y-4">
                <div className="bg-accent p-4 rounded-lg">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-medium">File Preview</span>
                    <Badge>{(file.size / 1024).toFixed(1)} KB</Badge>
                  </div>
                  <pre className="text-xs overflow-auto max-h-48 bg-background p-3 rounded">
                    {fileContent.slice(0, 500)}
                    {fileContent.length > 500 && '...'}
                  </pre>
                </div>

                <Button onClick={() => setStep('configure')} className="w-full">
                  Continue to Configuration
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Step 2: Configure Training */}
      {step === 'configure' && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Settings className="w-5 h-5" />
              Training Configuration
            </CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleFormSubmit(handleStartTraining)} className="space-y-4">
              {/* Semantic Naming Fields */}
              <div className="space-y-4">
                <div className="flex items-center gap-2 mb-2">
                  <Label className="text-sm font-medium">Adapter Name</Label>
                  <GlossaryTooltip termId="trainer-adapter-name" />
                </div>
                
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <Label htmlFor="tenant" className="text-xs text-muted-foreground">
                      Organization
                    </Label>
                    <Input
                      id="tenant"
                      {...register('tenant')}
                      placeholder="default"
                      className={errors.tenant ? 'border-red-500' : ''}
                    />
                    {errors.tenant && (
                      <p className="text-xs text-red-500">{errors.tenant.message}</p>
                    )}
                  </div>
                  
                  <div className="space-y-1">
                    <Label htmlFor="domain" className="text-xs text-muted-foreground">
                      Domain
                    </Label>
                    <Input
                      id="domain"
                      {...register('domain')}
                      placeholder="training"
                      className={errors.domain ? 'border-red-500' : ''}
                    />
                    {errors.domain && (
                      <p className="text-xs text-red-500">{errors.domain.message}</p>
                    )}
                  </div>
                  
                  <div className="space-y-1">
                    <Label htmlFor="purpose" className="text-xs text-muted-foreground">
                      Purpose
                    </Label>
                    <Input
                      id="purpose"
                      {...register('purpose')}
                      placeholder="my-adapter"
                      className={errors.purpose ? 'border-red-500' : ''}
                    />
                    {errors.purpose && (
                      <p className="text-xs text-red-500">{errors.purpose.message}</p>
                    )}
                  </div>
                  
                  <div className="space-y-1">
                    <Label htmlFor="revision" className="text-xs text-muted-foreground">
                      Revision
                    </Label>
                    <Input
                      id="revision"
                      {...register('revision')}
                      placeholder="r001"
                      className={errors.revision ? 'border-red-500' : ''}
                    />
                    {errors.revision && (
                      <p className="text-xs text-red-500">{errors.revision.message}</p>
                    )}
                  </div>
                </div>

                {/* Full Name Preview */}
                {fullAdapterName && (
                  <div className="bg-accent/50 rounded-md p-3 border">
                    <p className="text-xs text-muted-foreground mb-1">Full Adapter Name:</p>
                    <code className="text-sm font-mono text-primary">{fullAdapterName}</code>
                  </div>
                )}
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="rank">
                    LoRA Rank
                    <GlossaryTooltip termId="trainer-rank" />
                  </Label>
                  <Input
                    id="rank"
                    type="number"
                    {...register('rank', { valueAsNumber: true })}
                    min={1}
                    max={64}
                    className={errors.rank ? 'border-red-500' : ''}
                  />
                  {errors.rank && (
                    <p className="text-sm text-red-500 mt-1">{errors.rank.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="alpha">
                    Alpha
                    <GlossaryTooltip termId="trainer-alpha" />
                  </Label>
                  <Input
                    id="alpha"
                    type="number"
                    {...register('alpha', { valueAsNumber: true })}
                    min={1}
                    max={128}
                    className={errors.alpha ? 'border-red-500' : ''}
                  />
                  {errors.alpha && (
                    <p className="text-sm text-red-500 mt-1">{errors.alpha.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="epochs">
                    Epochs
                    <GlossaryTooltip termId="trainer-epochs" />
                  </Label>
                  <Input
                    id="epochs"
                    type="number"
                    {...register('epochs', { valueAsNumber: true })}
                    min={1}
                    max={20}
                    className={errors.epochs ? 'border-red-500' : ''}
                  />
                  {errors.epochs && (
                    <p className="text-sm text-red-500 mt-1">{errors.epochs.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="batch-size">
                    Batch Size
                    <GlossaryTooltip termId="trainer-batch-size" />
                  </Label>
                  <Input
                    id="batch-size"
                    type="number"
                    {...register('batchSize', { valueAsNumber: true })}
                    min={1}
                    max={32}
                    className={errors.batchSize ? 'border-red-500' : ''}
                  />
                  {errors.batchSize && (
                    <p className="text-sm text-red-500 mt-1">{errors.batchSize.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="learning-rate">
                    Learning Rate
                    <GlossaryTooltip termId="trainer-learning-rate" />
                  </Label>
                  <Input
                    id="learning-rate"
                    type="number"
                    step="0.0001"
                    {...register('learningRate', { valueAsNumber: true })}
                    className={errors.learningRate ? 'border-red-500' : ''}
                  />
                  {errors.learningRate && (
                    <p className="text-sm text-red-500 mt-1">{errors.learningRate.message}</p>
                  )}
                </div>
              </div>

              {trainingError && errorRecoveryTemplates.trainingError(
                () => {
                  setTrainingError(null);
                  setStep('configure');
                },
                () => {
                  setTrainingError(null);
                  resetTrainer();
                }
              )}
              {uploadError && (
                <ErrorRecovery
                  error={uploadError.message}
                  onRetry={() => { setUploadError(null); handleUploadToServer(); }}
                />
              )}

              <div className="flex gap-3">
                <Button type="button" variant="outline" onClick={() => setStep('upload')}>
                  Back
                </Button>
                <Button
                  type="submit"
                  className="flex-1"
                  disabled={!can('training:start') || !isValid}
                  title={!can('training:start') ? 'Requires training:start permission' : (!isValid ? 'Please fix validation errors' : undefined)}
                >
                  <Zap className="w-4 h-4 mr-2" />
                  Start Training
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      )}

      {/* Step 3: Training Progress */}
      {step === 'training' && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="w-5 h-5 animate-pulse text-blue-500" />
              Training in Progress
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-6">
            <ContextualLoading
              type="training"
              progress={trainingMetrics?.progress}
              eta={trainingMetrics?.progress < 50 ? "2-8 minutes" : "1-4 minutes"}
            />

            {trainingMetrics && (
              <div className="space-y-4">
                <ProgressIndicator
                  progress={trainingMetrics.progress}
                  status={`Epoch ${trainingMetrics.epoch}/${formValues.epochs}`}
                  eta={trainingMetrics.progress < 50 ? "2-8 minutes" : "1-4 minutes"}
                  confidence={Math.round(trainingMetrics.progress)}
                />

                <div className="grid grid-cols-2 gap-4">
                  <Card>
                    <CardContent className="pt-6">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                        <Cpu className="w-4 h-4" />
                        Current Epoch
                      </div>
                      <div className="text-2xl font-bold">
                        {trainingMetrics.epoch} / {formValues.epochs}
                      </div>
                    </CardContent>
                  </Card>

                  <Card>
                    <CardContent className="pt-6">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                        <TrendingUp className="w-4 h-4" />
                        Training Loss
                      </div>
                      <div className="text-2xl font-bold">
                        {trainingMetrics.loss.toFixed(4)}
                      </div>
                    </CardContent>
                  </Card>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Step 4: Complete - Success & Next Steps */}
      {step === 'complete' && (
        <div className="space-y-6">
          {successTemplates.trainingComplete(
            fullAdapterName || `${formValues.tenant}/${formValues.domain}/${formValues.purpose}/${formValues.revision}`,
            () => {
              // Scroll to test section
              const testSection = document.getElementById('test-section');
              testSection?.scrollIntoView({ behavior: 'smooth' });
            },
            () => {
              handleUploadToServer();
              transitionTo('/inference?adapter=' + trainingJob?.adapter_id);
            }
          )}

          {/* Test Inference */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Play className="w-5 h-5" />
                Test Your Adapter
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="test-prompt">Test Prompt</Label>
                <Textarea
                  id="test-prompt"
                  value={testPrompt}
                  onChange={(e) => setTestPrompt(e.target.value)}
                  placeholder="Enter a test prompt to see how your adapter responds..."
                  rows={3}
                />
              </div>

              <Button
                onClick={handleTestInference}
                disabled={!testPrompt || isTesting}
                className="w-full"
              >
                {isTesting ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    Running Inference...
                  </>
                ) : (
                  <>
                    <Play className="w-4 h-4 mr-2" />
                    Test Inference
                  </>
                )}
              </Button>

              {testResult && (
                <div className="bg-accent p-4 rounded-lg">
                  <p className="text-sm font-medium mb-2">Response:</p>
                  <pre className="text-sm whitespace-pre-wrap">{testResult.text}</pre>
              {testResult.trace && (
                <div className="mt-3 pt-3 border-t text-xs text-muted-foreground">
                  <p>Latency: {testResult.latency_ms || 0}ms</p>
                  <p>Finish Reason: {testResult.finish_reason}</p>
                </div>
              )}
                </div>
              )}
            </CardContent>
          </Card>

          {/* Download & Actions */}
          <Card>
            <CardHeader>
              <CardTitle>Next Steps</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <Button onClick={handleDownloadAdapter} variant="outline" className="w-full">
                <Download className="w-4 h-4 mr-2" />
                Download Adapter (.aos file)
              </Button>
              <Button onClick={handleUploadToServer} className="w-full">
                <Upload className="w-4 h-4 mr-2" />
                Upload to Server & Chat
              </Button>
              <Button onClick={resetTrainer} variant="outline" className="w-full">
                Train Another Adapter
              </Button>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
