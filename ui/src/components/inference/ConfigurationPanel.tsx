import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { ModelSelector } from '@/components/ModelSelector';
import { BackendSelector } from './BackendSelector';
import { AdapterSelector } from './AdapterSelector';
import { StackSelector, Stack } from './StackSelector';
import { CoreMLStatusPanel } from './CoreMLStatusPanel';
import { AdvancedOptions } from './AdvancedOptions';
import { TemplateManager } from './TemplateManager';
import { validatePrompt, ValidationResult, MAX_PROMPT_LENGTH } from './PromptInput';
import { sanitizeInput } from './helpers';
import {
  Play,
  Download,
  Plus,
  AlertTriangle,
  FileText,
  HelpCircle,
  Square,
} from 'lucide-react';
import { UseBackendSelectionReturn } from '@/hooks/inference/useBackendSelection';
import { UseCoreMLManagementReturn } from '@/hooks/inference/useCoreMLManagement';
import { UseAdapterSelectionReturn } from '@/hooks/inference/useAdapterSelection';
import { UseInferenceConfigReturn } from '@/hooks/inference/useInferenceConfig';
import { UseStreamingInferenceReturn } from '@/hooks/inference/useStreamingInference';
import { useFeatureDegradation } from '@/hooks/ui/useFeatureDegradation';
import { PromptTemplate as PromptTemplateType, usePromptTemplates } from '@/hooks/chat/usePromptTemplates';
import { InferenceConfig } from '@/api/types';
import { InferenceMode } from './types';

export interface ConfigurationPanelProps {
  /** Backend selection hook return */
  backend: UseBackendSelectionReturn;
  /** CoreML management hook return */
  coreml: UseCoreMLManagementReturn;
  /** Adapter selection hook return */
  adapter: UseAdapterSelectionReturn;
  /** Inference config hook return */
  config: UseInferenceConfigReturn;
  /** Streaming inference hook return */
  streaming: UseStreamingInferenceReturn;
  /** Current inference mode */
  inferenceMode: InferenceMode;
  /** Available stacks */
  stacks: Stack[];
  /** Selected stack ID */
  selectedStackId: string;
  /** Default stack ID */
  defaultStackId?: string;
  /** Selected model ID */
  selectedModelId: string;
  /** Selected tenant */
  selectedTenant: string;
  /** Callback when model changes */
  onModelChange: (modelId: string) => void;
  /** Callback when stack changes */
  onStackChange: (stackId: string) => void;
  /** Callback to set default stack */
  onSetDefaultStack: (stackId: string) => Promise<void>;
  /** Callback to clear stack selection */
  onClearStack: () => void;
  /** Callback to run inference */
  onInfer: () => void;
  /** Callback to run streaming inference */
  onStreamingInfer: () => void;
  /** Callback to cancel inference */
  onCancel: () => void;
  /** Callback to export results */
  onExport: () => void;
  /** Callback to save prompt as template */
  onSaveAsTemplate: () => void;
  /** Callback to set determinism mode */
  onSetDeterminismMode: (mode: 'deterministic' | 'adaptive') => void;
  /** Whether user can execute inference */
  canExecute: boolean;
  /** Whether inference is loading */
  isLoading: boolean;
  /** Whether streaming is active */
  isStreaming: boolean;
  /** Whether there's a response to export */
  hasResponse: boolean;
}

/**
 * Configuration panel for inference settings (left sidebar).
 */
export function ConfigurationPanel({
  backend,
  coreml,
  adapter,
  config,
  streaming,
  inferenceMode,
  stacks,
  selectedStackId,
  defaultStackId,
  selectedModelId,
  selectedTenant,
  onModelChange,
  onStackChange,
  onSetDefaultStack,
  onClearStack,
  onInfer,
  onStreamingInfer,
  onCancel,
  onExport,
  onSaveAsTemplate,
  onSetDeterminismMode,
  canExecute,
  isLoading,
  isStreaming,
  hasResponse,
}: ConfigurationPanelProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);
  const [promptValidation, setPromptValidation] = useState<ValidationResult | null>(null);

  // Template management
  const { recordTemplateUsage, substituteVariables, getRecentTemplates } = usePromptTemplates();
  const [selectedTemplate, setSelectedTemplate] = useState<PromptTemplateType | null>(null);
  const [templateVariables, setTemplateVariables] = useState<Record<string, string>>({});
  const [showVariableInputs, setShowVariableInputs] = useState(false);
  const [promptModifiedSinceTemplate, setPromptModifiedSinceTemplate] = useState(false);

  // Graceful degradation for adapter availability
  const adapterAvailability = useFeatureDegradation({
    featureId: 'adapters',
    healthCheck: () => adapter.adapters.length > 0,
    checkInterval: 30000,
  });

  const handlePromptChange = useCallback(
    (value: string) => {
      const sanitized = sanitizeInput(value);
      config.setConfigA({ ...config.configA, prompt: sanitized });
      setPromptValidation(validatePrompt(sanitized));

      if (selectedTemplate) {
        setPromptModifiedSinceTemplate(sanitized !== selectedTemplate.prompt);
      }
    },
    [config, selectedTemplate]
  );

  const handleApplyTemplate = useCallback(
    (template: PromptTemplateType) => {
      recordTemplateUsage(template.id);
      setSelectedTemplate(template);
      setTemplateVariables({});
      setPromptModifiedSinceTemplate(false);

      if (template.variables.length > 0) {
        setShowVariableInputs(true);
      } else {
        config.setConfigA({ ...config.configA, prompt: template.prompt });
        setShowTemplates(false);
      }
    },
    [config, recordTemplateUsage]
  );

  const handleApplyVariableSubstitution = useCallback(() => {
    if (!selectedTemplate) return;
    const substituted = substituteVariables(selectedTemplate.id, templateVariables);
    if (substituted) {
      config.setConfigA({ ...config.configA, prompt: substituted });
      setShowVariableInputs(false);
      setShowTemplates(false);
    }
  }, [config, selectedTemplate, substituteVariables, templateVariables]);

  const handleResetToTemplate = useCallback(() => {
    if (!selectedTemplate) return;
    if (confirm('Reset prompt to template? Any manual edits will be lost.')) {
      config.setConfigA({ ...config.configA, prompt: selectedTemplate.prompt });
      setTemplateVariables({});
      setShowVariableInputs(false);
      setPromptModifiedSinceTemplate(false);
    }
  }, [config, selectedTemplate]);

  const handleRunInference = useCallback(() => {
    if (inferenceMode === 'streaming') {
      onStreamingInfer();
    } else {
      onInfer();
    }
  }, [inferenceMode, onInfer, onStreamingInfer]);

  const coremlBackendAvailable = backend.backendOptions.find((o) => o.name === 'coreml')?.available ?? false;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Configuration</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Graceful degradation alert */}
          {adapterAvailability.isDegraded && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                {adapter.adapters.length === 0
                  ? 'No adapters available. Inference will use base model only.'
                  : 'Adapter loading issues detected. Some adapters may be unavailable.'}
                {adapterAvailability.failureCount === 0 && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => adapterAvailability.checkHealth()}
                    className="ml-2"
                  >
                    Retry
                  </Button>
                )}
              </AlertDescription>
            </Alert>
          )}

          {/* Model Selection */}
          <div className="space-y-2">
            <Label className="flex items-center gap-1">
              Base Model
              <GlossaryTooltip termId="inference-base-model">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
            </Label>
            <ModelSelector
              value={selectedModelId}
              onChange={onModelChange}
              disabled={backend.isLoading}
            />
            <p className="text-xs text-muted-foreground">
              Choose a loaded model; backend preferences are remembered per model.
            </p>
          </div>

          {/* Backend Selection */}
          <BackendSelector
            backendOptions={backend.backendOptions}
            selectedBackend={backend.selectedBackend}
            lastBackendUsed={backend.lastBackendUsed}
            hardwareCapabilities={backend.hardwareCapabilities}
            isLoading={backend.isLoading}
            error={backend.error}
            warning={backend.warning}
            onSelect={(b) => backend.selectBackend(b)}
            disabled={false}
          />

          {/* CoreML Status */}
          <CoreMLStatusPanel
            coreml={coreml}
            adapterId={adapter.selectedAdapterId}
            coremlBackendAvailable={coremlBackendAvailable}
          />

          {/* Stack Selection */}
          <StackSelector
            stacks={stacks}
            selectedStackId={selectedStackId}
            defaultStackId={defaultStackId}
            onSelect={onStackChange}
            onSetDefault={onSetDefaultStack}
            onClear={onClearStack}
          />

          {/* Adapter Selection */}
          <AdapterSelector
            adapters={adapter.adapters}
            selectedId={adapter.selectedAdapterId}
            onSelect={adapter.setSelectedAdapterId}
            strength={adapter.adapterStrength}
            onStrengthChange={adapter.setAdapterStrength}
            onStrengthCommit={adapter.commitStrength}
            isStrengthUpdating={adapter.isStrengthUpdating}
          />

          {/* Determinism Mode */}
          <div className="space-y-2">
            <Label className="flex items-center gap-1">
              Determinism
              <GlossaryTooltip termId="router-determinism">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
            </Label>
            <div className="flex gap-2" role="group" aria-label="Determinism mode">
              <Button
                variant={config.configA.routing_determinism_mode === 'deterministic' ? 'default' : 'outline'}
                size="sm"
                onClick={() => onSetDeterminismMode('deterministic')}
                aria-pressed={config.configA.routing_determinism_mode === 'deterministic'}
              >
                Deterministic
              </Button>
              <Button
                variant={config.configA.routing_determinism_mode === 'adaptive' ? 'default' : 'outline'}
                size="sm"
                onClick={() => onSetDeterminismMode('adaptive')}
                aria-pressed={config.configA.routing_determinism_mode === 'adaptive'}
              >
                Adaptive
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Per-request override; stack defaults stay unchanged.
            </p>
          </div>

          {/* Prompt Input */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="prompt" className="flex items-center gap-1">
                Prompt
                <GlossaryTooltip termId="inference-prompt">
                  <span className="cursor-help text-muted-foreground hover:text-foreground">
                    <HelpCircle className="h-3 w-3" />
                  </span>
                </GlossaryTooltip>
              </Label>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowTemplates(!showTemplates)}
                className="h-8 px-2"
              >
                <FileText className="h-3 w-3 mr-1" />
                Templates
              </Button>
            </div>

            <Textarea
              id="prompt"
              data-testid="inference-input"
              data-cy="prompt-input"
              placeholder="Enter your prompt here..."
              value={config.configA.prompt}
              onChange={(e) => handlePromptChange(e.target.value)}
              rows={6}
              className={promptValidation?.valid === false ? 'border-destructive' : ''}
            />

            {promptValidation?.error && (
              <Alert variant="destructive" className="text-sm">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  <strong>Validation Error:</strong> {promptValidation.error}
                  {promptValidation.suggestion && (
                    <div className="mt-1 text-sm opacity-90">
                      <strong>Suggestion:</strong> {promptValidation.suggestion}
                    </div>
                  )}
                </AlertDescription>
              </Alert>
            )}

            {promptValidation?.warning && (
              <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50">
                <AlertTriangle className="h-4 w-4 text-yellow-600" />
                <AlertDescription className="text-yellow-800">
                  <strong>Warning:</strong> {promptValidation.warning}
                </AlertDescription>
              </Alert>
            )}

            {/* Template Manager */}
            <SectionErrorBoundary sectionName="Template Manager">
              <TemplateManager
                templates={[]}
                recentTemplates={getRecentTemplates()}
                selectedTemplate={selectedTemplate}
                templateVariables={templateVariables}
                showTemplates={showTemplates}
                showVariableInputs={showVariableInputs}
                promptModifiedSinceTemplate={promptModifiedSinceTemplate}
                onSelect={handleApplyTemplate}
                onApplyVariables={handleApplyVariableSubstitution}
                onResetToTemplate={handleResetToTemplate}
                onSaveAsTemplate={onSaveAsTemplate}
                onManageTemplates={() => {}}
                onToggleTemplates={() => setShowTemplates(!showTemplates)}
                onCancelVariables={() => {
                  setShowVariableInputs(false);
                  setSelectedTemplate(null);
                  setTemplateVariables({});
                }}
                onVariableChange={(variable, value) =>
                  setTemplateVariables((prev) => ({ ...prev, [variable]: value }))
                }
                substituteVariables={substituteVariables}
              />
            </SectionErrorBoundary>
          </div>

          {/* Advanced Options */}
          <AdvancedOptions
            values={{
              max_tokens: config.configA.max_tokens || 100,
              temperature: config.configA.temperature || 0.7,
              top_k: config.configA.top_k || 50,
              top_p: config.configA.top_p || 0.9,
              backend: (config.configA.backend === 'cpu' ? 'auto' : config.configA.backend) || 'auto',
              seed: config.configA.seed ?? undefined,
              require_evidence: config.configA.require_evidence || false,
            }}
            onChange={(values) => config.setConfigA({ ...config.configA, ...values })}
            isOpen={showAdvanced}
            onOpenChange={setShowAdvanced}
            hideBackendSelect={true}
          />

          {/* Action Buttons */}
          <div className="flex gap-2">
            <Button
              className={`flex-1 ${!canExecute ? 'opacity-50 cursor-not-allowed' : ''}`}
              data-testid="inference-submit"
              data-cy="run-inference-btn"
              onClick={handleRunInference}
              disabled={isLoading || isStreaming || !canExecute}
              title={!canExecute ? 'Requires inference:execute permission' : undefined}
              aria-label={isLoading || isStreaming ? 'Generating response' : 'Generate response'}
            >
              <Play className="h-4 w-4 mr-2" aria-hidden="true" />
              {isLoading || isStreaming ? 'Generating...' : 'Generate'}
            </Button>
            {(isLoading || isStreaming) && (
              <Button variant="outline" onClick={onCancel} aria-label="Cancel generation">
                <Square className="h-4 w-4" aria-hidden="true" />
              </Button>
            )}
          </div>

          {/* Export Button */}
          {hasResponse && (
            <Button variant="outline" className="w-full" onClick={onExport} aria-label="Export inference results">
              <Download className="h-4 w-4 mr-2" aria-hidden="true" />
              Export
            </Button>
          )}

          {/* Save as Template Button */}
          {config.configA.prompt && (
            <Button variant="outline" className="w-full" onClick={onSaveAsTemplate} aria-label="Save current prompt as template">
              <Plus className="h-4 w-4 mr-2" aria-hidden="true" />
              Save Prompt as Template
            </Button>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
