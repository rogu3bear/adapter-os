import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Alert, AlertDescription } from './ui/alert';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { CheckCircle, XCircle, AlertTriangle, Download, Code } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import type { OpenAIModelInfo, ModelValidationResponse } from '../api/types';

interface ModelInfo extends OpenAIModelInfo {
  validation?: ModelValidationResponse;
  validating?: boolean;
}

interface ModelSelectorProps {
  value?: string;
  onChange?: (modelId: string) => void;
  disabled?: boolean;
}

export function ModelSelector({ value, onChange, disabled }: ModelSelectorProps) {
  const [models, setModels] = React.useState<ModelInfo[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [showValidationDialog, setShowValidationDialog] = React.useState(false);
  const [selectedModelForDetails, setSelectedModelForDetails] = React.useState<ModelInfo | null>(null);

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const list = await apiClient.listModels();
        if (mounted) {
          const modelsWithValidation: ModelInfo[] = list.map(m => ({ ...m }));
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
              console.warn(`Failed to validate model ${model.id}:`, error);
              return {
                modelId: model.id,
                validation: {
                  model_id: model.id,
                  model_name: model.id,
                  can_load: false,
                  reason: 'Validation failed - unable to check model status'
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
    onChange?.(val);
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

  const handleShowDetails = (model: ModelInfo) => {
    setSelectedModelForDetails(model);
    setShowValidationDialog(true);
  };

  const copyCommand = async (command: string) => {
    try {
      await navigator.clipboard.writeText(command);
      toast.success('Command copied to clipboard');
    } catch (error) {
      toast.error('Failed to copy command');
    }
  };

  return (
    <>
      <Select value={value} onValueChange={handleChange} disabled={disabled || loading}>
        <SelectTrigger className="w-[280px]" aria-label="Model selector">
          <SelectValue placeholder={loading ? 'Loading models…' : 'Select model'} />
        </SelectTrigger>
        <SelectContent>
          {models.map((m) => (
            <div key={m.id} className="flex items-center justify-between p-2 hover:bg-gray-50 cursor-pointer">
              <div className="flex items-center gap-2 flex-1" onClick={() => handleChange(m.id)}>
                {getStatusIcon(m)}
                <div className="flex-1">
                  <div className="font-medium">{m.id}</div>
                  <div className="text-xs text-gray-500">{getStatusBadge(m)}</div>
                </div>
              </div>
              {!m.validation?.can_load && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleShowDetails(m);
                  }}
                  className="ml-2"
                >
                  <Code className="w-4 h-4" />
                </Button>
              )}
            </div>
          ))}
          {models.length === 0 && !loading && (
            <div className="p-2 text-center text-gray-500">
              No models available
            </div>
          )}
        </SelectContent>
      </Select>

      <Dialog open={showValidationDialog} onOpenChange={setShowValidationDialog}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {selectedModelForDetails && getStatusIcon(selectedModelForDetails)}
              Model Setup: {selectedModelForDetails?.id}
            </DialogTitle>
          </DialogHeader>

          {selectedModelForDetails?.validation && (
            <div className="space-y-4">
              <Alert>
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  {selectedModelForDetails.validation.reason || 'This model requires setup before it can be used.'}
                </AlertDescription>
              </Alert>

              {selectedModelForDetails.validation.download_commands && (
                <div>
                  <h4 className="font-medium mb-2">Setup Commands:</h4>
                  <div className="space-y-2">
                    {selectedModelForDetails.validation.download_commands.map((command, index) => (
                      <div key={index} className="flex items-center gap-2 p-2 bg-gray-50 rounded font-mono text-sm">
                        <code className="flex-1">{command}</code>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => copyCommand(command)}
                          className="shrink-0"
                        >
                          Copy
                        </Button>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex gap-2 pt-4">
                <Button
                  onClick={() => setShowValidationDialog(false)}
                  className="flex-1"
                >
                  Close
                </Button>
                {selectedModelForDetails.validation.download_commands?.some(cmd => cmd.includes('huggingface-cli') || cmd.includes('git lfs')) && (
                  <Button
                    variant="outline"
                    onClick={async () => {
                      const downloadCmd = selectedModelForDetails.validation?.download_commands?.find(cmd =>
                        cmd.includes('huggingface-cli download')
                      );
                      if (downloadCmd) await copyCommand(downloadCmd);
                    }}
                    className="flex-1"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    Copy Download Command
                  </Button>
                )}
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
