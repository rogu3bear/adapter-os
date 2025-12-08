import React, { useState, useEffect, useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Slider } from '@/components/ui/slider';
import { Switch } from '@/components/ui/switch';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Brain,
  Database,
  Settings,
  AlertTriangle,
  Loader2,
  CheckCircle,
  Info,
  Power,
  Cpu,
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';
import type {
  TrainingTemplate,
  StartTrainingRequest,
  TrainingConfigRequest,
  Dataset,
} from '@/api/training-types';
import type { ModelWithStatsResponse, BaseModelStatus } from '@/api/api-types';

interface StartTrainingFormProps {
  onSuccess: (jobId: string) => void;
  onCancel: () => void;
  initialTemplate?: TrainingTemplate;
}

export function resolveDatasetPrefill(datasets: Dataset[], desiredId?: string): string | undefined {
  if (!desiredId) return undefined;
  const match = datasets.find((d) => d.id === desiredId);
  return match ? desiredId : undefined;
}

const DEFAULT_CONFIG: TrainingConfigRequest = {
  learning_rate: 1e-4,
  epochs: 3,
  batch_size: 4,
  rank: 16,
  alpha: 32,
  warmup_steps: 100,
  weight_decay: 0.01,
  gradient_clip: 1.0,
  max_seq_length: 512,
  gradient_accumulation_steps: 4,
  save_steps: 500,
  eval_steps: 500,
  logging_steps: 100,
};

export function StartTrainingForm({
  onSuccess,
  onCancel,
  initialTemplate,
  preselectedAdapterId,
  preselectedDatasetId,
}: StartTrainingFormProps & { preselectedAdapterId?: string; preselectedDatasetId?: string }) {
  const [activeTab, setActiveTab] = useState('basic');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [adapterName, setAdapterName] = useState(preselectedAdapterId ?? '');
  const [datasetId, setDatasetId] = useState(preselectedDatasetId ?? '');
  const [templateId, setTemplateId] = useState('');
  const [config, setConfig] = useState<TrainingConfigRequest>(DEFAULT_CONFIG);
  const [loraTier, setLoraTier] = useState<'micro' | 'standard' | 'max'>('micro');
  const [loraScope, setLoraScope] = useState<'project' | 'tenant'>('project');

  // Data from API
  const [templates, setTemplates] = useState<TrainingTemplate[]>([]);
  const [datasets, setDatasets] = useState<Dataset[]>([]);
  const [models, setModels] = useState<ModelWithStatsResponse[]>([]);
  const [baseModelStatus, setBaseModelStatus] = useState<BaseModelStatus | null>(null);
  const [isLoadingData, setIsLoadingData] = useState(true);
  const [isLoadingModel, setIsLoadingModel] = useState(false);
  const [selectedModelToLoad, setSelectedModelToLoad] = useState<string>('');

  // Validation state
  const [nameError, setNameError] = useState<string | null>(null);

  // Get selected dataset validation status
  const selectedDataset = datasets.find(d => d.id === datasetId);
  const isDatasetValid = !datasetId || selectedDataset?.validation_status === 'valid';

  // Check if a base model is loaded (from runtime status)
  const isModelLoaded = useMemo(() => {
    if (!baseModelStatus) return false;
    return baseModelStatus.status === 'ready' ||
           baseModelStatus.status === 'loaded' || // legacy
           baseModelStatus.is_loaded === true;
  }, [baseModelStatus]);

  // Get available models that can be loaded (not the currently loaded one)
  const availableModels = useMemo(() => {
    if (!baseModelStatus?.model_id) return models;
    return models.filter(m => m.id !== baseModelStatus.model_id);
  }, [models, baseModelStatus]);

  // Load templates, datasets, models, and base model status
  useEffect(() => {
    async function loadData() {
      setIsLoadingData(true);
      try {
        const [templatesRes, datasetsRes, modelsRes, modelStatusRes] = await Promise.all([
          apiClient.listTrainingTemplates(),
          apiClient.listDatasets(),
          apiClient.listModels(),
          apiClient.getBaseModelStatus().catch(() => null), // May not be available
        ]);

        setTemplates(templatesRes);
        setDatasets(datasetsRes.datasets || []);
        setModels(modelsRes || []);
        setBaseModelStatus(modelStatusRes);

        const prefillId = resolveDatasetPrefill(datasetsRes.datasets || [], preselectedDatasetId);
        if (prefillId) {
          setDatasetId(prefillId);
        }

        if (initialTemplate) {
          setTemplateId(initialTemplate.id);
          applyTemplate(initialTemplate);
        }
      } catch (err) {
        logger.error('Failed to load training form data', {}, err as Error);
        toast.error('Failed to load form data');
      } finally {
        setIsLoadingData(false);
      }
    }

    loadData();
  }, [initialTemplate, preselectedDatasetId]);

  // Handle loading a model
  const handleLoadModel = async () => {
    if (!selectedModelToLoad) return;

    setIsLoadingModel(true);
    try {
      await apiClient.loadBaseModel(selectedModelToLoad);
      toast.success('Model loaded successfully');
      // Refresh base model status to reflect the newly loaded model
      const modelStatusRes = await apiClient.getBaseModelStatus().catch(() => null);
      setBaseModelStatus(modelStatusRes);
      setSelectedModelToLoad('');
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load model';
      toast.error(message);
      logger.error('Failed to load model', { modelId: selectedModelToLoad }, err as Error);
    } finally {
      setIsLoadingModel(false);
    }
  };

  // Apply template settings
  const applyTemplate = (template: TrainingTemplate) => {
    if (template.config) {
      setConfig(prev => ({
        ...prev,
        ...template.config,
      }));
    } else {
      setConfig(prev => ({
        ...prev,
        rank: template.rank ?? prev.rank,
        alpha: template.alpha ?? prev.alpha,
        learning_rate: template.learning_rate ?? prev.learning_rate,
        epochs: template.epochs ?? prev.epochs,
        batch_size: template.batch_size ?? prev.batch_size,
        targets: template.targets ?? prev.targets,
      }));
    }
  };

  // Handle template selection
  const handleTemplateChange = (value: string) => {
    setTemplateId(value);
    const template = templates.find(t => t.id === value);
    if (template) {
      applyTemplate(template);
    }
  };

  // Validate adapter name format
  const validateAdapterName = (name: string): boolean => {
    // Semantic naming: organization/domain/purpose/revision
    const pattern = /^[a-z0-9-]+\/[a-z0-9-]+\/[a-z0-9-]+\/r\d{3}$/;
    if (!pattern.test(name)) {
      setNameError('Format: organization/domain/purpose/r001 (e.g., acme/engineering/code-review/r001)');
      return false;
    }
    setNameError(null);
    return true;
  };

  // Handle form submission
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    // Validate required fields
    if (!adapterName.trim()) {
      setError('Adapter name is required');
      return;
    }

    if (!validateAdapterName(adapterName)) {
      setError('Invalid adapter name format');
      return;
    }

    // Check dataset validation status
    if (datasetId && selectedDataset && selectedDataset.validation_status !== 'valid') {
      setError(`Dataset "${selectedDataset.name}" must be validated before training. Current status: ${selectedDataset.validation_status}`);
      return;
    }

    // Check if a model is loaded
    if (!isModelLoaded) {
      setError('A base model must be loaded before starting training. Please load a model first.');
      return;
    }

    setIsSubmitting(true);

    try {
      // Ensure targets is always set (required by backend)
      const configWithTargets = {
        ...config,
        targets: config.targets && config.targets.length > 0
          ? config.targets
          : ['q_proj', 'v_proj'],  // Default LoRA targets
      };

      const request: StartTrainingRequest = {
        adapter_name: adapterName,
        config: configWithTargets,
        template_id: templateId || undefined,
        dataset_id: datasetId || undefined,
        lora_tier: loraTier,
        scope: loraScope,
      };

      const response = await apiClient.startTraining(request);
      toast.success('Training job started successfully');
      onSuccess(response.id);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to start training';
      setError(message);
      logger.error('Failed to start training', { adapterName }, err as Error);
    } finally {
      setIsSubmitting(false);
    }
  };

  // Config update helper
  const updateConfig = <K extends keyof TrainingConfigRequest>(
    key: K,
    value: TrainingConfigRequest[K]
  ) => {
    setConfig(prev => ({ ...prev, [key]: value }));
  };

  if (isLoadingData) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Model Status Section */}
      <Card className={isModelLoaded ? 'border-green-200 bg-green-50/50 dark:border-green-800 dark:bg-green-950/50' : 'border-amber-200 bg-amber-50/50 dark:border-amber-800 dark:bg-amber-950/50'}>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm flex items-center gap-2">
            <Cpu className="h-4 w-4" />
            Base Model Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          {isModelLoaded && baseModelStatus ? (
            <div className="flex items-center gap-2">
              <CheckCircle className="h-4 w-4 text-green-600" />
              <span className="text-sm text-green-700 dark:text-green-300">
                Model loaded: <span className="font-medium">{baseModelStatus.model_name || baseModelStatus.model_id}</span>
              </span>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <AlertTriangle className="h-4 w-4 text-amber-600" />
                <span className="text-sm text-amber-700 dark:text-amber-300">
                  No model loaded. A base model must be loaded before training.
                </span>
              </div>
              {availableModels.length > 0 ? (
                <div className="flex gap-2">
                  <Select value={selectedModelToLoad} onValueChange={setSelectedModelToLoad}>
                    <SelectTrigger className="flex-1">
                      <SelectValue placeholder="Select a model to load..." />
                    </SelectTrigger>
                    <SelectContent>
                      {availableModels.map((model) => (
                        <SelectItem key={model.id} value={model.id}>
                          <span>{model.name || model.id}</span>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <Button
                    type="button"
                    onClick={handleLoadModel}
                    disabled={!selectedModelToLoad || isLoadingModel}
                  >
                    {isLoadingModel ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <>
                        <Power className="h-4 w-4 mr-2" />
                        Load
                      </>
                    )}
                  </Button>
                </div>
              ) : (
                <Alert>
                  <Info className="h-4 w-4" />
                  <AlertDescription>
                    No models available to load. Please import a model from the Owner Home page.
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="basic" className="gap-2">
            <Brain className="h-4 w-4" />
            Basic
          </TabsTrigger>
          <TabsTrigger value="data" className="gap-2">
            <Database className="h-4 w-4" />
            Data
          </TabsTrigger>
          <TabsTrigger value="advanced" className="gap-2">
            <Settings className="h-4 w-4" />
            Advanced
          </TabsTrigger>
        </TabsList>

        <TabsContent value="basic" className="space-y-4 mt-4">
          {/* Adapter Name */}
          <div className="space-y-2">
            <Label htmlFor="adapter-name">Adapter Name *</Label>
            <Input
              id="adapter-name"
              placeholder="organization/domain/purpose/r001"
              value={adapterName}
              onChange={(e) => {
                setAdapterName(e.target.value);
                if (e.target.value) {
                  validateAdapterName(e.target.value);
                } else {
                  setNameError(null);
                }
              }}
              className={nameError ? 'border-destructive' : ''}
            />
            {nameError && (
              <p className="text-sm text-destructive">{nameError}</p>
            )}
            <p className="text-xs text-muted-foreground">
              Semantic naming format: organization/domain/purpose/revision
            </p>
          </div>

          {/* Template Selection */}
          <div className="space-y-2">
            <Label htmlFor="template">Training Template</Label>
            <Select value={templateId} onValueChange={handleTemplateChange}>
              <SelectTrigger>
                <SelectValue placeholder="Select a template (optional)" />
              </SelectTrigger>
              <SelectContent>
                {templates.map((template) => (
                  <SelectItem key={template.id} value={template.id}>
                    <div className="flex flex-col">
                      <span>{template.name}</span>
                      {template.description && (
                        <span className="text-xs text-muted-foreground">
                          {template.description}
                        </span>
                      )}
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* LoRA Tier and Scope */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="lora-tier">LoRA Tier</Label>
              <Select value={loraTier} onValueChange={(value) => setLoraTier(value as 'micro' | 'standard' | 'max')}>
                <SelectTrigger id="lora-tier">
                  <SelectValue placeholder="Select tier" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="micro">Micro (smallest)</SelectItem>
                  <SelectItem value="standard">Standard (balanced)</SelectItem>
                  <SelectItem value="max">Max (largest)</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Marketing/operational tier for routing and UI badges.
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="lora-scope">LoRA Scope</Label>
              <Select value={loraScope} onValueChange={(value) => setLoraScope(value as 'project' | 'tenant')}>
                <SelectTrigger id="lora-scope">
                  <SelectValue placeholder="Select scope" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="project">Project</SelectItem>
                  <SelectItem value="tenant">Tenant</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Logical scope used for routing and visibility.
              </p>
            </div>
          </div>

          {/* Basic Config */}
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">LoRA Configuration</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label>Rank: {config.rank}</Label>
                  <Slider
                    value={[config.rank]}
                    onValueChange={([value]) => updateConfig('rank', value)}
                    min={4}
                    max={64}
                    step={4}
                  />
                </div>
                <div className="space-y-2">
                  <Label>Alpha: {config.alpha}</Label>
                  <Slider
                    value={[config.alpha]}
                    onValueChange={([value]) => updateConfig('alpha', value)}
                    min={8}
                    max={128}
                    step={8}
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="epochs">Epochs</Label>
                  <Input
                    id="epochs"
                    type="number"
                    min={1}
                    max={100}
                    value={config.epochs}
                    onChange={(e) => updateConfig('epochs', parseInt(e.target.value) || 1)}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="batch-size">Batch Size</Label>
                  <Input
                    id="batch-size"
                    type="number"
                    min={1}
                    max={64}
                    value={config.batch_size}
                    onChange={(e) => updateConfig('batch_size', parseInt(e.target.value) || 1)}
                  />
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="data" className="space-y-4 mt-4">
          {/* Dataset Selection */}
          <div className="space-y-2">
            <Label htmlFor="dataset">Training Dataset</Label>
            <Select value={datasetId} onValueChange={setDatasetId}>
              <SelectTrigger>
                <SelectValue placeholder="Select a dataset (optional)" />
              </SelectTrigger>
              <SelectContent>
                {datasets.map((dataset) => (
                  <SelectItem key={dataset.id} value={dataset.id}>
                    <div className="flex flex-col">
                      <span>{dataset.name}</span>
                      <span className="text-xs text-muted-foreground">
                        {dataset.file_count} files, {dataset.total_tokens.toLocaleString()} tokens
                      </span>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedDataset && selectedDataset.validation_status !== 'valid' && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  Dataset "{selectedDataset.name}" is not validated (status: {selectedDataset.validation_status}).
                  Please validate the dataset before starting training.
                </AlertDescription>
              </Alert>
            )}
            {datasets.length === 0 && (
              <Alert>
                <Info className="h-4 w-4" />
                <AlertDescription>
                  No datasets available. You can create one from the Datasets page.
                </AlertDescription>
              </Alert>
            )}
          </div>

          {/* Max Sequence Length */}
          <div className="space-y-2">
            <Label htmlFor="max-seq-length">Max Sequence Length</Label>
            <Input
              id="max-seq-length"
              type="number"
              min={128}
              max={8192}
              step={128}
              value={config.max_seq_length || 512}
              onChange={(e) => updateConfig('max_seq_length', parseInt(e.target.value) || 512)}
            />
            <p className="text-xs text-muted-foreground">
              Maximum number of tokens per training example
            </p>
          </div>
        </TabsContent>

        <TabsContent value="advanced" className="space-y-4 mt-4">
          {/* Learning Rate */}
          <div className="space-y-2">
            <Label htmlFor="learning-rate">Learning Rate</Label>
            <Input
              id="learning-rate"
              type="number"
              step="0.00001"
              min={0.000001}
              max={0.01}
              value={config.learning_rate}
              onChange={(e) => updateConfig('learning_rate', parseFloat(e.target.value) || 1e-4)}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="warmup-steps">Warmup Steps</Label>
              <Input
                id="warmup-steps"
                type="number"
                min={0}
                max={1000}
                value={config.warmup_steps || 100}
                onChange={(e) => updateConfig('warmup_steps', parseInt(e.target.value) || 0)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="weight-decay">Weight Decay</Label>
              <Input
                id="weight-decay"
                type="number"
                step="0.001"
                min={0}
                max={0.5}
                value={config.weight_decay || 0.01}
                onChange={(e) => updateConfig('weight_decay', parseFloat(e.target.value) || 0)}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="gradient-clip">Gradient Clip</Label>
              <Input
                id="gradient-clip"
                type="number"
                step="0.1"
                min={0.1}
                max={10}
                value={config.gradient_clip || 1.0}
                onChange={(e) => updateConfig('gradient_clip', parseFloat(e.target.value) || 1.0)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="gradient-accumulation">Gradient Accumulation Steps</Label>
              <Input
                id="gradient-accumulation"
                type="number"
                min={1}
                max={64}
                value={config.gradient_accumulation_steps || 4}
                onChange={(e) => updateConfig('gradient_accumulation_steps', parseInt(e.target.value) || 1)}
              />
            </div>
          </div>

          <div className="grid grid-cols-3 gap-4">
            <div className="space-y-2">
              <Label htmlFor="save-steps">Save Steps</Label>
              <Input
                id="save-steps"
                type="number"
                min={100}
                value={config.save_steps || 500}
                onChange={(e) => updateConfig('save_steps', parseInt(e.target.value) || 500)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="eval-steps">Eval Steps</Label>
              <Input
                id="eval-steps"
                type="number"
                min={100}
                value={config.eval_steps || 500}
                onChange={(e) => updateConfig('eval_steps', parseInt(e.target.value) || 500)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="logging-steps">Logging Steps</Label>
              <Input
                id="logging-steps"
                type="number"
                min={10}
                value={config.logging_steps || 100}
                onChange={(e) => updateConfig('logging_steps', parseInt(e.target.value) || 100)}
              />
            </div>
          </div>
        </TabsContent>
      </Tabs>

      {/* Form Actions */}
      <div className="flex justify-end gap-2 pt-4 border-t">
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          type="submit"
          disabled={isSubmitting || !isDatasetValid || !isModelLoaded}
          title={!isModelLoaded ? 'A base model must be loaded before training' : !isDatasetValid ? 'Dataset must be validated before training' : undefined}
        >
          {isSubmitting ? (
            <>
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              Starting...
            </>
          ) : (
            <>
              <Brain className="h-4 w-4 mr-2" />
              Start Training
            </>
          )}
        </Button>
      </div>
    </form>
  );
}
