import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Alert, AlertDescription } from './ui/alert';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { Progress } from './ui/progress';
import { CheckCircle, XCircle, AlertTriangle, Download, Loader2, Copy, Terminal, Play, Square } from 'lucide-react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import type { ModelValidationResponse, ModelWithStatsResponse } from '@/api/types';
import { logger } from '@/utils/logger';

interface DownloadState {
  isDownloading: boolean;
  progress: number;
  status: string;
  error?: string;
}

interface ModelInfo extends ModelWithStatsResponse {
  validation?: ModelValidationResponse;
  validating?: boolean;
}

interface ModelSelectorProps {
  value?: string;
  onChange?: (modelId: string) => void;
  disabled?: boolean;
}

// Generate download commands based on model ID (typically a HuggingFace repo like "org/model-name")
function generateDownloadCommands(modelId: string): string[] {
  // Check if it looks like a HuggingFace repo ID (contains /)
  if (modelId.includes('/')) {
    return [
      `huggingface-cli download ${modelId} --local-dir \${AOS_MODEL_CACHE_DIR:-./var/model-cache/models}/${modelId.split('/').pop()}`,
    ];
  }
  // For simple model names, provide generic instructions
  return [
    `# Model path: \${AOS_MODEL_CACHE_DIR:-./var/model-cache/models}/${modelId}`,
    `# Ensure model files are downloaded to this directory`,
  ];
}

export function ModelSelector({ value, onChange, disabled }: ModelSelectorProps) {
  const [models, setModels] = React.useState<ModelInfo[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [showValidationDialog, setShowValidationDialog] = React.useState(false);
  const [selectedModelForDetails, setSelectedModelForDetails] = React.useState<ModelInfo | null>(null);
  const [downloadState, setDownloadState] = React.useState<DownloadState>({
    isDownloading: false,
    progress: 0,
    status: '',
  });
  const [selectedModelId, setSelectedModelId] = React.useState<string | undefined>(value);
  const [isLoadingModel, setIsLoadingModel] = React.useState(false);
  const [modelStatus, setModelStatus] = React.useState<'ready' | 'no-model' | 'loading' | 'unloading'>('no-model');

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const list = await apiClient.listModels();
        if (mounted) {
          const modelsWithValidation: ModelInfo[] = list.map((model) => ({
            ...model,
            validation: undefined,
          }));
          setModels(modelsWithValidation);

          // Mark all as validating
          setModels(currentModels =>
            currentModels.map(model => ({ ...model, validating: true }))
          );

          // Validate each model in parallel with error handling
          const validationPromises = list.map(async (model) => {
            try {
              const validation = await apiClient.validateModel(model.id);
              return { modelId: model.id, validation, error: null };
            } catch (error) {
              // Log error but continue with fallback
              logger.warn('Failed to validate model', {
                component: 'ModelSelector',
                modelId: model.id
              });
              return {
                modelId: model.id,
                validation: {
                  model_id: model.id,
                  valid: false,
                  issues: [],
                  can_load: false,
                  reason: 'Validation failed - unable to check model status',
                  status: 'error'
                } as ModelValidationResponse,
                error: error instanceof Error ? error.message : 'Unknown error'
              };
            }
          });

          const validations = await Promise.all(validationPromises);
          if (mounted) {
            setModels(currentModels =>
              currentModels.map(model => {
                const validationResult = validations.find(v => v.modelId === model.id);
                return {
                  ...model,
                  validation: validationResult?.validation,
                  validating: false
                };
              })
            );
          }
        }
      } catch (error) {
        // If listModels fails, show empty list
        if (mounted) setModels([]);
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, []);

  const handleChange = (val: string) => {
    setSelectedModelId(val);
    onChange?.(val);

    // Auto-show setup dialog for models that need setup
    const selectedModel = models.find(m => m.id === val);
    if (selectedModel && !selectedModel.validation?.can_load) {
      setSelectedModelForDetails(selectedModel);
      setShowValidationDialog(true);
    }
  };

  // Fetch model status periodically
  React.useEffect(() => {
    if (!selectedModelId) return;
    
    const fetchStatus = async () => {
      try {
        const status = await apiClient.getModelStatus(selectedModelId);
        setModelStatus(status.is_loaded ? 'ready' : 'no-model');
      } catch {
        // Ignore errors - model might not exist yet
      }
    };
    
    fetchStatus();
    const interval = setInterval(fetchStatus, 3000); // Poll every 3 seconds
    return () => clearInterval(interval);
  }, [selectedModelId]);

  const handleLoadModel = async () => {
    if (!selectedModelId) {
      toast.error('Please select a model first');
      return;
    }

    const selectedModel = models.find(m => m.id === selectedModelId);
    if (selectedModel && !selectedModel.validation?.can_load) {
      setSelectedModelForDetails(selectedModel);
      setShowValidationDialog(true);
      return;
    }

    setIsLoadingModel(true);
    setModelStatus('loading');
    try {
      await apiClient.loadBaseModel(selectedModelId);
      toast.success(`Model "${selectedModelId}" loaded successfully`);
      setModelStatus('ready');
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : 'Unknown error';
      toast.error(`Failed to load model: ${errorMsg}`);
      setModelStatus('no-model');
      logger.error('Failed to load model', {
        component: 'ModelSelector',
        modelId: selectedModelId,
        error: errorMsg,
      });
    } finally {
      setIsLoadingModel(false);
    }
  };

  const handleUnloadModel = async () => {
    if (!selectedModelId) return;

    setIsLoadingModel(true);
    setModelStatus('unloading');
    try {
      await apiClient.unloadBaseModel(selectedModelId);
      toast.success(`Model "${selectedModelId}" unloaded`);
      setModelStatus('no-model');
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : 'Unknown error';
      toast.error(`Failed to unload model: ${errorMsg}`);
      setModelStatus('ready');
      logger.error('Failed to unload model', {
        component: 'ModelSelector',
        modelId: selectedModelId,
        error: errorMsg,
      });
    } finally {
      setIsLoadingModel(false);
    }
  };

  const getStatusIcon = (model: ModelInfo) => {
    if (model.validating) {
      return <div className="w-4 h-4 border-2 border-gray-300 border-t-blue-500 rounded-full animate-spin" />;
    }
    if (!model.validation) {
      return <AlertTriangle className="w-4 h-4 text-yellow-500" />;
    }
    return model.validation.can_load ?
      <CheckCircle className="w-4 h-4 text-green-500" /> :
      <XCircle className="w-4 h-4 text-red-500" />;
  };

  const getStatusBadge = (model: ModelInfo) => {
    if (model.validating) {
      return <Badge variant="secondary">Validating...</Badge>;
    }
    if (!model.validation) {
      return <Badge variant="outline">Unknown</Badge>;
    }
    return model.validation.can_load ?
      <Badge variant="default" className="bg-green-100 text-green-800">Ready</Badge> :
      <Badge variant="destructive">Needs Setup</Badge>;
  };

  const copyCommand = async (command: string) => {
    try {
      await navigator.clipboard.writeText(command);
      toast.success('Command copied to clipboard');
    } catch (error) {
      toast.error('Failed to copy command');
    }
  };

  const handleDownloadModel = async () => {
    if (!selectedModelForDetails) return;

    setDownloadState({
      isDownloading: true,
      progress: 0,
      status: 'Starting download...',
    });

    try {
      // Try to start the download via API
      const response = await apiClient.downloadModel(selectedModelForDetails.id);

      // Poll for progress if we get a job ID back
      if (response && 'job_id' in response) {
        // TODO: Implement polling when backend supports it
        setDownloadState({
          isDownloading: false,
          progress: 100,
          status: 'Download started - check server logs for progress',
        });
        toast.success('Download started successfully');
      } else {
        setDownloadState({
          isDownloading: false,
          progress: 100,
          status: 'Complete',
        });
        toast.success('Model downloaded successfully');
        // Refresh validation status
        setShowValidationDialog(false);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : 'Unknown error';
      logger.error('Failed to download model', {
        component: 'ModelSelector',
        modelId: selectedModelForDetails.id,
        error: errorMsg,
      });

      setDownloadState({
        isDownloading: false,
        progress: 0,
        status: '',
        error: errorMsg,
      });

      // Show helpful message if download endpoint isn't available
      if (errorMsg.includes('404') || errorMsg.includes('not found')) {
        toast.error('Download not available - use the CLI commands below');
      } else {
        toast.error(`Download failed: ${errorMsg}`);
      }
    }
  };

  // Get download commands - prefer backend-provided ones, fall back to generated
  const getDownloadCommands = (model: ModelInfo): string[] => {
    if (model.validation?.download_commands && model.validation.download_commands.length > 0) {
      return model.validation.download_commands;
    }
    return generateDownloadCommands(model.name || model.id);
  };

  const isModelLoaded = modelStatus === 'ready';
  const isModelBusy = modelStatus === 'loading' || modelStatus === 'unloading';

  return (
    <>
      <div className="flex items-center gap-2">
        <Select value={selectedModelId} onValueChange={handleChange} disabled={disabled || loading}>
          <SelectTrigger
            className="w-[200px]"
            aria-label="Model selector"
            data-cy="model-selector"
          >
            <SelectValue placeholder={loading ? 'Loading models…' : 'Select model'} />
          </SelectTrigger>
          <SelectContent>
            {models.map((m) => (
              <SelectItem
                key={m.id}
                value={m.id}
                data-cy="model-option"
                data-model-id={m.id}
              >
                <div className="flex items-center gap-2">
                  {getStatusIcon(m)}
                  <div>
                    <div className="font-medium">{m.name}</div>
                    <div className="text-xs text-gray-500">{getStatusBadge(m)}</div>
                  </div>
                </div>
              </SelectItem>
            ))}
            {models.length === 0 && !loading && (
              <div className="p-2 text-center text-gray-500">
                No models available
              </div>
            )}
          </SelectContent>
        </Select>

        {/* Load/Unload Button */}
        {selectedModelId && (
          isModelLoaded ? (
            <Button
              variant="outline"
              size="sm"
              onClick={handleUnloadModel}
              disabled={isLoadingModel || isModelBusy}
              className="gap-1"
            >
              {isModelBusy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Square className="h-4 w-4" />
              )}
              Unload
            </Button>
          ) : (
            <Button
              variant="default"
              size="sm"
              onClick={handleLoadModel}
              disabled={isLoadingModel || isModelBusy}
              className="gap-1"
            >
              {isModelBusy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Play className="h-4 w-4" />
              )}
              {modelStatus === 'loading' ? 'Loading...' : 'Load'}
            </Button>
          )
        )}

        {/* Status indicator */}
        {selectedModelId && (
          <Badge variant={isModelLoaded ? 'default' : 'secondary'} className="ml-1">
            {isModelLoaded ? '● Loaded' : '○ Unloaded'}
          </Badge>
        )}
      </div>

      <Dialog open={showValidationDialog} onOpenChange={setShowValidationDialog}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {selectedModelForDetails && getStatusIcon(selectedModelForDetails)}
              Model Setup: {selectedModelForDetails?.name}
            </DialogTitle>
          </DialogHeader>

          {selectedModelForDetails && (
            <div className="space-y-4">
              <Alert>
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  {selectedModelForDetails.validation?.reason || 'This model needs to be downloaded before it can be used.'}
                </AlertDescription>
              </Alert>

              {/* Download Button */}
              {downloadState.isDownloading ? (
                <div className="space-y-2 p-4 bg-blue-50 rounded-lg">
                  <div className="flex items-center gap-2 text-blue-700">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    <span className="font-medium">{downloadState.status}</span>
                  </div>
                  <Progress value={downloadState.progress} className="h-2" />
                </div>
              ) : downloadState.error ? (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    Download failed: {downloadState.error}
                  </AlertDescription>
                </Alert>
              ) : (
                <Button
                  onClick={handleDownloadModel}
                  className="w-full"
                  size="lg"
                >
                  <Download className="w-4 h-4 mr-2" />
                  Download Model
                </Button>
              )}

              {/* CLI Commands Section */}
              <div className="border-t pt-4">
                <div className="flex items-center gap-2 mb-2">
                  <Terminal className="h-4 w-4 text-muted-foreground" />
                  <h4 className="font-medium text-sm text-muted-foreground">Or use CLI:</h4>
                </div>
                <div className="space-y-2">
                  {getDownloadCommands(selectedModelForDetails).map((command, index) => (
                    <div key={index} className="flex items-center gap-2 p-2 bg-muted/50 rounded font-mono text-xs">
                      <code className="flex-1 break-all">{command}</code>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => copyCommand(command)}
                        className="shrink-0 h-6 w-6 p-0"
                      >
                        <Copy className="h-3 w-3" />
                      </Button>
                    </div>
                  ))}
                </div>
              </div>

              <div className="flex gap-2 pt-2">
                <Button
                  variant="outline"
                  onClick={() => {
                    setShowValidationDialog(false);
                    setDownloadState({ isDownloading: false, progress: 0, status: '' });
                  }}
                  className="flex-1"
                >
                  Close
                </Button>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
