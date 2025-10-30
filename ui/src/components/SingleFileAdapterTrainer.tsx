import React, { useState, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import apiClient from '../api/client';
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
import type { TrainingJob, TrainingConfig, InferRequest, InferResponse } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { ProgressIndicator, ContextualLoading, LoadingStates } from './ui/progress-indicator';
import { SuccessFeedback, SuccessTemplates } from './ui/success-feedback';
import { useViewTransition } from '../hooks/useViewTransition';

type TrainingStep = 'upload' | 'configure' | 'training' | 'complete';

interface TrainingMetrics {
  loss: number;
  epoch: number;
  progress: number;
}

export function SingleFileAdapterTrainer() {
  const [step, setStep] = useState<TrainingStep>('upload');
  const [file, setFile] = useState<File | null>(null);
  const [fileContent, setFileContent] = useState<string>('');
  const fileInputRef = useRef<HTMLInputElement>(null);
  
  // Configuration state
  const [adapterName, setAdapterName] = useState('');
  const [config, setConfig] = useState<TrainingConfig>({
    rank: 8,
    alpha: 16,
    targets: ['q_proj', 'v_proj'],
    epochs: 3,
    learning_rate: 0.0003,
    batch_size: 4
  });

  // Training state
  const [trainingJob, setTrainingJob] = useState<TrainingJob | null>(null);
  const [trainingMetrics, setTrainingMetrics] = useState<TrainingMetrics | null>(null);
  const [trainingError, setTrainingError] = useState<string | null>(null);
  const [isTraining, setIsTraining] = useState(false);

  // Testing state
  const [testPrompt, setTestPrompt] = useState('');
  const [testResult, setTestResult] = useState<InferResponse | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // View transitions for smooth navigation
  const transitionTo = useViewTransition();

  const handleFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const uploadedFile = event.target.files?.[0];
    if (uploadedFile) {
      setFile(uploadedFile);
      
      // Read file content for preview
      const reader = new FileReader();
      reader.onload = (e) => {
        const content = e.target?.result as string;
        setFileContent(content);
      };
      reader.readAsText(uploadedFile);

      // Auto-generate adapter name from filename
      const baseName = uploadedFile.name.replace(/\.[^/.]+$/, '');
      setAdapterName(baseName + '_adapter');
    }
  };

  const handleStartTraining = async () => {
    if (!file || !adapterName) {
      setTrainingError('Please provide a file and adapter name');
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
      
      // For now, we'll create a training job with the file content
      const response = await apiClient.startTraining({
        adapter_name: adapterName,
        config: config,
        dataset_path: file.name, // This would be a server path in production
        adapters_root: './adapters',
        package: true,
        register: true,
        adapter_id: adapterName,
        tier: 1
      });

      setTrainingJob(response as TrainingJob);

      // Poll for training progress
      pollTrainingProgress(response.id);
    } catch (error) {
      setTrainingError(error instanceof Error ? error.message : 'Training failed');
      setIsTraining(false);
      setStep('configure');
    }
  };

  const pollTrainingProgress = async (jobId: string) => {
    const pollInterval = setInterval(async () => {
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
          clearInterval(pollInterval);
          setIsTraining(false);
          setStep('complete');
        } else if (job.status === 'failed') {
          clearInterval(pollInterval);
          setTrainingError(job.error_message || 'Training failed');
          setIsTraining(false);
        }
      } catch (error) {
        logger.error('Failed to poll training job', { component: 'SingleFileAdapterTrainer', operation: 'pollTrainingJob' }, toError(error));
        clearInterval(pollInterval);
        setIsTraining(false);
        setTrainingError('Lost connection to training job');
      }
    }, 2000); // Poll every 2 seconds

    // Cleanup after 30 minutes
    setTimeout(() => clearInterval(pollInterval), 30 * 60 * 1000);
  };

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
        text: 'Error: ' + (error instanceof Error ? error.message : 'Unknown error'),
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
    setAdapterName('');
    setTrainingJob(null);
    setTrainingMetrics(null);
    setTrainingError(null);
    setTestPrompt('');
    setTestResult(null);
  };

  const handleUploadToServer = async () => {
    if (!trainingJob?.artifact_path) {
      toast.error('No artifact available to upload');
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
      toast.success(`Adapter uploaded to server: ${adapter.name}`);

      // Optionally navigate to inference
      if (window.confirm('Adapter uploaded successfully! Would you like to chat with it now?')) {
        window.location.href = `/inference?adapter=${adapter.adapter_id}`;
      }
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Upload failed';
      toast.error(`Upload to server failed: ${errorMsg}`);
    }
  };

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold">Single-File Adapter Trainer</h1>
        <p className="text-muted-foreground">
          Train a custom LoRA adapter from a single file and test it interactively
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
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
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
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="adapter-name">Adapter Name</Label>
              <Input
                id="adapter-name"
                value={adapterName}
                onChange={(e) => setAdapterName(e.target.value)}
                placeholder="my_code_adapter"
              />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="rank">LoRA Rank</Label>
                <Input
                  id="rank"
                  type="number"
                  value={config.rank}
                  onChange={(e) => setConfig({ ...config, rank: parseInt(e.target.value) })}
                  min={1}
                  max={64}
                />
                <p className="text-xs text-muted-foreground">
                  Lower = faster, Higher = more capacity
                </p>
              </div>

              <div className="space-y-2">
                <Label htmlFor="alpha">Alpha</Label>
                <Input
                  id="alpha"
                  type="number"
                  value={config.alpha}
                  onChange={(e) => setConfig({ ...config, alpha: parseInt(e.target.value) })}
                  min={1}
                  max={64}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="epochs">Epochs</Label>
                <Input
                  id="epochs"
                  type="number"
                  value={config.epochs}
                  onChange={(e) => setConfig({ ...config, epochs: parseInt(e.target.value) })}
                  min={1}
                  max={20}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="batch-size">Batch Size</Label>
                <Input
                  id="batch-size"
                  type="number"
                  value={config.batch_size}
                  onChange={(e) => setConfig({ ...config, batch_size: parseInt(e.target.value) })}
                  min={1}
                  max={32}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="learning-rate">Learning Rate</Label>
                <Input
                  id="learning-rate"
                  type="number"
                  step="0.0001"
                  value={config.learning_rate}
                  onChange={(e) => setConfig({ ...config, learning_rate: parseFloat(e.target.value) })}
                />
              </div>
            </div>

            {trainingError && (
              <div className="bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 p-4 rounded-lg flex items-start gap-3">
                <AlertCircle className="w-5 h-5 text-red-600 flex-shrink-0 mt-0.5" />
                <div>
                  <p className="font-medium text-red-900 dark:text-red-100">Training Error</p>
                  <p className="text-sm text-red-700 dark:text-red-300">{trainingError}</p>
                </div>
              </div>
            )}

            <div className="flex gap-3">
              <Button variant="outline" onClick={() => setStep('upload')}>
                Back
              </Button>
              <Button onClick={handleStartTraining} className="flex-1">
                <Zap className="w-4 h-4 mr-2" />
                Start Training
              </Button>
            </div>
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
                  status={`Epoch ${trainingMetrics.epoch}/${config.epochs}`}
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
                        {trainingMetrics.epoch} / {config.epochs}
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
          {SuccessTemplates.trainingComplete(
            adapterName,
            () => setActiveTab('test'),
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

