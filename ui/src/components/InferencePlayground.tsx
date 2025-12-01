import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { validatePrompt, ValidationResult, MAX_PROMPT_LENGTH } from './inference/PromptInput';
import { AdvancedOptions } from './inference/AdvancedOptions';
import { InferenceOutput } from './inference/InferenceOutput';
import { TemplateManager } from './inference/TemplateManager';
import { BatchProcessor } from './inference/BatchProcessor';
import { ComparisonMode } from './inference/ComparisonMode';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import {
  useInferenceConfig,
  useStreamingInference,
  useBatchInference,
  useInferenceSessions
} from '@/hooks/inference';
import {
  Play,
  Download,
  History,
  Zap,
  Clock,
  Split,
  FileText,
  AlertTriangle,
  Code,
  Square,
  Wifi,
  Layers,
  TrendingUp,
  Target,
  Plus,
  HelpCircle
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { InferRequest, InferResponse, InferenceSession, Adapter, InferenceConfig } from '@/api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { logger, toError } from '@/utils/logger';
import { useSearchParams } from 'react-router-dom';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';
import { useProgressiveHints } from '@/hooks/useProgressiveHints';
import { getPageHints } from '@/data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { ToolPageHeader } from './ui/page-headers/ToolPageHeader';
import { useFeatureDegradation } from '@/hooks/useFeatureDegradation';
import { useCancellableOperation } from '@/hooks/useCancellableOperation';
import { PromptTemplateManager } from './PromptTemplateManager';
import { usePromptTemplates, PromptTemplate as PromptTemplateType } from '@/hooks/usePromptTemplates';
import { InferenceRequestSchema, BatchPromptSchema } from '@/schemas';
import { useAdapterStacks, useGetDefaultStack, useSetDefaultStack } from '@/hooks/useAdmin';
import { ZodError } from 'zod';

interface InferencePlaygroundProps {
  selectedTenant: string;
}

// Security: Input sanitization to prevent XSS and other injection attacks
const sanitizeInput = (input: string): string => {
  if (!input) return input;

  // Basic XSS prevention - remove potentially dangerous HTML/script tags
  const sanitized = input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '') // Remove script tags
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '') // Remove iframe tags
    .replace(/javascript:/gi, '') // Remove javascript: protocols
    .replace(/on\w+\s*=/gi, '') // Remove event handlers
    .replace(/<[^>]*>/g, '') // Remove all HTML tags as final fallback
    .trim();

  // Log if input was modified for security monitoring
  if (sanitized !== input) {
    logger.warn('Input sanitized for security', {
      component: 'InferencePlayground',
      operation: 'input_sanitization',
      originalLength: input.length,
      sanitizedLength: sanitized.length
    });
  }

  return sanitized;
};

// Privacy-aware monitoring (anonymized metrics only)
const recordPrivacySafeMetrics = (operation: string, data: Record<string, unknown>) => {
  // Remove any personally identifiable information
  const anonymized = { ...data };
  delete anonymized.userId;
  delete anonymized.email;
  delete anonymized.ip;
  delete anonymized.sessionId;

  logger.info(`Privacy-safe ${operation}`, {
    component: 'InferencePlayground',
    operation: `privacy_${operation}`,
    ...anonymized
  });
};


function InferencePlaygroundContent({ selectedTenant }: InferencePlaygroundProps) {
  const [searchParams] = useSearchParams();
  const { can, userRole } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const [mode, setMode] = useState<'single' | 'comparison'>('single');
  const [inferenceMode, setInferenceMode] = useState<'standard' | 'streaming' | 'batch'>('standard');
  const [prompt, setPrompt] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapterId, setSelectedAdapterId] = useState<string>('none');
  const [selectedStackId, setSelectedStackId] = useState<string>('');

  // Fetch stacks and default stack
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);
  const { mutateAsync: setDefaultStack } = useSetDefaultStack(selectedTenant);

  // Template management
  const { recordTemplateUsage, substituteVariables, getRecentTemplates } = usePromptTemplates();
  const [showTemplateManager, setShowTemplateManager] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<PromptTemplateType | null>(null);
  const [templateVariables, setTemplateVariables] = useState<Record<string, string>>({});
  const [showVariableInputs, setShowVariableInputs] = useState(false);
  const [promptModifiedSinceTemplate, setPromptModifiedSinceTemplate] = useState(false);

  // Additional state for metrics
  interface InferenceMetrics {
    latency: number;
    tokensPerSecond: number;
    totalTokens: number;
  }
  const [metrics, setMetrics] = useState<InferenceMetrics | null>(null);
  const [templates, setTemplates] = useState<PromptTemplateType[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);
  const [promptValidation, setPromptValidation] = useState<ValidationResult | null>(null);
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight });

  // Cancellation support for inference operations
  const { state: inferenceState, start: startInference, cancel: cancelInference } = useCancellableOperation();

  // Inference hooks
  const {
    configA, configB, setConfigA, setConfigB,
    responseA, responseB, setResponseA, setResponseB,
    isLoadingA, isLoadingB, setIsLoadingA, setIsLoadingB,
    resetConfig, resetAll
  } = useInferenceConfig();

  const {
    streamingState, isStreaming, streamedText, tokensPerSecond,
    startStreaming, cancelStreaming, resetStreaming
  } = useStreamingInference({
    config: configA,
    adapterId: selectedAdapterId,
    stackId: selectedStackId,
  });

  const {
    batchPrompts, setBatchPrompts, addPrompt, removePrompt,
    batchResults, isBatchRunning, metrics: batchMetrics, batchValidation,
    executeBatch, cancelBatch, clearResults,
    exportResultsCSV, exportResultsJSON
  } = useBatchInference({
    config: configA,
    adapterId: selectedAdapterId,
    stackId: selectedStackId,
  });

  const {
    recentSessions, addSession, saveCurrentSession, clearSessions
  } = useInferenceSessions();

  // Graceful degradation: Monitor adapter availability
  const adapterAvailability = useFeatureDegradation({
    featureId: 'adapters',
    healthCheck: () => {
      // Check current adapter state, don't reload (that's handled by useEffect)
      return adapters.length > 0;
    },
    checkInterval: 30000,
  });

  // Progressive hints
  const hints = getPageHints('inference').map(hint => ({
    ...hint,
    condition: hint.id === 'no-adapters-inference'
      ? () => adapters.length === 0
      : hint.condition
  }));
  const { getVisibleHint, dismissHint } = useProgressiveHints({
    pageKey: 'inference',
    hints
  });
  const visibleHint = getVisibleHint();

  const handleApplyTemplate = useCallback((template: PromptTemplateType) => {
    logger.info('Applying template', { templateId: template.id, templateName: template.name });

    // Record usage
    recordTemplateUsage(template.id);

    // Set template and show variable inputs if needed
    setSelectedTemplate(template);
    setTemplateVariables({});
    setPromptModifiedSinceTemplate(false);

    if (template.variables.length > 0) {
      // Show variable inputs for substitution
      setShowVariableInputs(true);
    } else {
      // No variables, apply directly
      setConfigA({ ...configA, prompt: template.prompt });
      setPrompt(template.prompt);
      setShowTemplates(false);
    }
  }, [recordTemplateUsage, configA, setConfigA]);

  const handleApplyVariableSubstitution = useCallback(() => {
    if (!selectedTemplate) return;

    const substituted = substituteVariables(selectedTemplate.id, templateVariables);
    if (substituted) {
      setConfigA({ ...configA, prompt: substituted });
      setPrompt(substituted);
      setShowVariableInputs(false);
      setShowTemplates(false);
      logger.info('Variables substituted', { templateId: selectedTemplate.id, variableCount: Object.keys(templateVariables).length });
    }
  }, [selectedTemplate, templateVariables, substituteVariables, configA, setConfigA]);

  const handleResetToTemplate = useCallback(() => {
    if (!selectedTemplate) return;

    if (confirm('Reset prompt to template? Any manual edits will be lost.')) {
      setConfigA({ ...configA, prompt: selectedTemplate.prompt });
      setPrompt(selectedTemplate.prompt);
      setTemplateVariables({});
      setShowVariableInputs(false);
      setPromptModifiedSinceTemplate(false);
      logger.info('Prompt reset to template', { templateId: selectedTemplate.id });
    }
  }, [selectedTemplate, configA, setConfigA]);

  const handleSavePromptAsTemplate = useCallback(() => {
    // Delegate to template manager
    setShowTemplateManager(true);
  }, []);


  useEffect(() => {
    // Load adapters
    const loadAdapters = async () => {
      try {
        const adapterList = await apiClient.listAdapters();
        setAdapters(adapterList);

        // Check for adapter query parameter
        const adapterParam = searchParams.get('adapter');
        if (adapterParam) {
          // Try to find the adapter by ID or adapter_id
          const targetAdapter = adapterList.find((a: Adapter) =>
            a.id === adapterParam || a.adapter_id === adapterParam
          );
          if (targetAdapter) {
            setSelectedAdapterId(targetAdapter.id);
            // Success - no need for toast, UI updates
            return;
          } else {
            logger.warn('Requested adapter not found', {
              component: 'InferencePlayground',
              operation: 'loadAdapters',
              requestedAdapter: adapterParam,
            });
          }
        }

        // Fallback: Select first active adapter if available
        const activeAdapter = adapterList.find((a: Adapter) => a.current_state && ['hot', 'warm', 'resident'].includes(a.current_state));
        if (activeAdapter && activeAdapter.id) {
          setSelectedAdapterId(activeAdapter.id);
        }
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to load adapters');
        logger.error('Failed to load adapters', {
          component: 'InferencePlayground',
          operation: 'loadAdapters',
        }, error);
        addError('adapters-load', error.message || 'Failed to load adapters. Inference will use base model only.', () => {
          clearError('adapters-load');
          apiClient.listAdapters().then(setAdapters).catch(err => {
            addError('adapters-load', err instanceof Error ? err.message : 'Failed to load adapters');
          });
        });
        // Don't block inference - allow graceful degradation with base model
      }
    };
    loadAdapters();
  }, [searchParams, addError, clearError]);

  // Load default stack on mount if none selected
  useEffect(() => {
    if (defaultStack && !selectedStackId) {
      setSelectedStackId(defaultStack.id);
      logger.info('Default stack loaded', {
        component: 'InferencePlayground',
        operation: 'loadDefaultStack',
        stackId: defaultStack.id,
        stackName: defaultStack.name,
      });
    }
  }, [defaultStack, selectedStackId]);

  const saveSession = useCallback((config: InferenceConfig, response: InferResponse) => {
    const selectedStack = stacks.find(s => s.id === selectedStackId);
    const session = saveCurrentSession(config, response);

    // Add stack information if available
    if (selectedStackId || selectedStack?.name) {
      session.stack_id = selectedStackId || undefined;
      session.stack_name = selectedStack?.name || undefined;
    }

    addSession(session);
  }, [stacks, selectedStackId, saveCurrentSession, addSession]);

  const handleInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    clearError('inference');
    setLoading(true);
    setResponse(null);

    try {
      // Resolve stack to adapter IDs for validation
      const validationAdapterIds = selectedStackId
        ? (() => {
            const selectedStack = stacks.find(s => s.id === selectedStackId);
            return selectedStack?.adapter_ids || undefined;
          })()
        : (selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined);

      // Validate prompt against schema
      const validationResult = await InferenceRequestSchema.parseAsync({
        prompt: config.prompt,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_k: config.top_k,
        top_p: config.top_p,
        backend: config.backend || 'auto',
        seed: config.seed,
        require_evidence: config.require_evidence,
        adapter_stack: validationAdapterIds,
      });

      await startInference(async (signal) => {
        // Include adapters array if selected
        // Resolve stack to adapter IDs if stack is selected
        const adapterIds = selectedStackId
          ? (() => {
              const selectedStack = stacks.find(s => s.id === selectedStackId);
              return selectedStack?.adapter_ids || undefined;
            })()
          : (selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined);

        const inferenceRequest: InferRequest = {
          ...config,
          adapter_stack: adapterIds,
        };
        const response = await apiClient.infer(inferenceRequest, {}, false, signal);
        setResponse(response);
        saveSession(config, response);
        return response;
      }, `inference-${config.id}`);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Inference failed');

      if (error instanceof ZodError) {
        logger.warn('Inference validation failed', {
          component: 'InferencePlayground',
          operation: 'validate',
          configId: config.id,
        });
        addError('inference', `Validation error: ${error.message}`, () => handleInfer(config, setResponse, setLoading));
      } else {
        logger.error('Inference request failed', {
          component: 'InferencePlayground',
          operation: 'infer',
          configId: config.id,
          tenantId: selectedTenant,
          adapterId: selectedAdapterId,
        }, toError(err));
        addError('inference', error.message || 'An unexpected error occurred during inference.', () => handleInfer(config, setResponse, setLoading));
      }
    } finally {
      setLoading(false);
    }
  };

  // Streaming inference handler
  const handleStreamingInfer = useCallback(async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    clearError('inference');
    setLoading(true);
    setResponse(null);

    try {
      await startStreaming(config.prompt);
      // startStreaming handles all the state updates internally
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Streaming inference failed');
      addError('inference', error.message, () => handleStreamingInfer(config, setResponse, setLoading));
    } finally {
      setLoading(false);
    }
  }, [startStreaming, clearError, addError]);


  const handleExport = (config: InferenceConfig, response: InferResponse | null) => {
    if (!response) return;

    const data = {
      prompt: config.prompt,
      config,
      response,
      timestamp: new Date().toISOString(),
    };

    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `inference-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    // Success - browser download feedback is sufficient
  };


  const handleBatchRetry = useCallback(async (itemId: string) => {
    logger.info('Retrying batch item', {
      component: 'InferencePlayground',
      operation: 'retryBatchItem',
      itemId,
    });
    toast.info('Batch retry not yet implemented with new hooks');
  }, []);

  const loadSession = (session: InferenceSession) => {
    setPrompt(session.prompt);
    setConfigA({ ...configA, ...session.request, prompt: session.prompt });
    if (session.response) {
      setResponseA(session.response);
    }
    // Success - UI updates are sufficient feedback
  };

  const handleReplay = async (bundleId: string) => {
    try {
      logger.info('Replay requested', {
        component: 'InferencePlayground',
        operation: 'handleReplay',
        bundleId
      });

      const session = await apiClient.getReplaySession(bundleId);

      if (session) {
        // Restore prompt from session
        setPrompt(session.prompt || '');
        setConfigA(prev => ({
          ...prev,
          prompt: session.prompt || '',
          max_tokens: session.config?.max_tokens || prev.max_tokens,
          temperature: session.config?.temperature || prev.temperature,
        }));

        toast.success('Session restored from replay');
        logger.info('Replay session loaded', {
          component: 'InferencePlayground',
          operation: 'handleReplay',
          sessionId: session.id
        });
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load replay session';
      logger.error('Replay failed', {
        component: 'InferencePlayground',
        operation: 'handleReplay',
        bundleId
      }, error instanceof Error ? error : new Error(errorMessage));
      toast.error(`Failed to load replay: ${errorMessage}`);
    }
  };

  const renderAdvancedOptions = (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => (
    <AdvancedOptions
      values={{
        max_tokens: config.max_tokens || 100,
        temperature: config.temperature || 0.7,
        top_k: config.top_k || 50,
        top_p: config.top_p || 0.9,
        backend: config.backend || 'auto',
        seed: config.seed,
        require_evidence: config.require_evidence || false
      }}
      onChange={(values) => setConfig({ ...config, ...values })}
      isOpen={showAdvanced}
      onOpenChange={setShowAdvanced}
    />
  );


  const renderResponse = (response: InferResponse | null, isLoading: boolean) => {
    // When streaming is active, show streaming output
    if (inferenceMode === 'streaming' && isStreaming) {
      return (
        <InferenceOutput
          response={{
            schema_version: '1.0',
            id: `stream-${Date.now()}`,
            text: streamedText,
            token_count: streamingState.tokenCount,
            tokens_generated: streamingState.tokenCount,
            latency_ms: streamingState.startTime ? Date.now() - streamingState.startTime : 0,
            finish_reason: 'stop',
            adapters_used: [],
          } as InferResponse}
          isLoading={false}
          metrics={{
            latency: streamingState.startTime ? Date.now() - streamingState.startTime : 0,
            tokensPerSecond: tokensPerSecond,
            totalTokens: streamingState.tokenCount,
          }}
          isStreaming={true}
        />
      );
    }

    return (
      <InferenceOutput
        response={response}
        isLoading={isLoading}
        metrics={metrics}
        isStreaming={inferenceMode === 'streaming'}
      />
    );
  };

  return (
    <div className="space-y-6">

      {/* Consolidated Error Display */}
      <PageErrors errors={errors} />

      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

      {/* Header */}
      <ToolPageHeader
        title="Inference Playground"
        description={
          selectedStackId
            ? `Using stack: ${stacks.find(s => s.id === selectedStackId)?.name || selectedStackId}`
            : "Test model inference with advanced configuration options"
        }
        secondaryActions={
          <div className="flex gap-2">
            <div className="flex gap-1 border rounded-md p-1">
              <Button
                variant={inferenceMode === 'standard' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('standard')}
              >
                <Zap className="h-3 w-3 mr-1" />
                Standard
              </Button>
              <GlossaryTooltip termId="inference-stream">
                <Button
                  variant={inferenceMode === 'streaming' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setInferenceMode('streaming')}
                >
                  <Wifi className="h-3 w-3 mr-1" />
                  Streaming
                </Button>
              </GlossaryTooltip>
              <Button
                variant={inferenceMode === 'batch' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('batch')}
              >
                <Layers className="h-3 w-3 mr-1" />
                Batch
              </Button>
            </div>

            {/* Single vs Comparison Mode */}
            <div className="flex gap-1 border rounded-md p-1">
              <Button
                variant={mode === 'single' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setMode('single')}
              >
                <FileText className="h-3 w-3 mr-1" />
                Single
              </Button>
              <GlossaryTooltip termId="inference-compare-mode">
                <Button
                  variant={mode === 'comparison' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setMode('comparison')}
                >
                  <Split className="h-3 w-3 mr-1" />
                  Compare
                </Button>
              </GlossaryTooltip>
            </div>
          </div>
        }
      />

      {/* Performance Metrics Display */}
      {metrics && (
        <Card className="mb-4">
          <CardContent className="pt-4">
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-1">
                <Clock className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.latency}ms</span>
              </div>
              <div className="flex items-center gap-1">
                <TrendingUp className="h-4 w-4 text-muted-foreground" />
                <span>{typeof metrics.tokensPerSecond === 'number' ? metrics.tokensPerSecond.toFixed(1) : '0.0'} tokens/sec</span>
              </div>
              <div className="flex items-center gap-1">
                <Target className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.totalTokens} tokens</span>
              </div>
              <span className="text-xs text-muted-foreground ml-auto">
                Metrics calculated from last inference run
              </span>
            </div>
          </CardContent>
        </Card>
      )}

      {inferenceMode === 'batch' ? (
        /* Batch Mode */
        <SectionErrorBoundary sectionName="Batch Processing">
          <BatchProcessor
            prompts={batchPrompts}
            results={batchResults}
            validation={batchValidation}
            isProcessing={isBatchRunning}
            config={{
              max_tokens: configA.max_tokens || 100,
              temperature: configA.temperature || 0.7,
              top_k: configA.top_k || 50,
              top_p: configA.top_p,
            }}
            canExecute={can('inference:execute')}
            onPromptsChange={setBatchPrompts}
            onProcess={executeBatch}
            onRetry={handleBatchRetry}
            onExportJSON={exportResultsJSON}
            onExportCSV={exportResultsCSV}
          />
        </SectionErrorBoundary>
      ) : mode === 'single' ? (
        /* Single Mode */
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Configuration Panel */}
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
                      {adapters.length === 0
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
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="stack" className="flex items-center gap-1">
                      Stack {selectedStackId && defaultStack?.id === selectedStackId && <Badge variant="outline" className="text-xs ml-1">Default</Badge>}
                      <GlossaryTooltip termId="inference-stack">
                        <span className="cursor-help text-muted-foreground hover:text-foreground">
                          <HelpCircle className="h-3 w-3" />
                        </span>
                      </GlossaryTooltip>
                    </Label>
                    <div className="flex items-center gap-2">
                      {selectedStackId && selectedStackId !== defaultStack?.id && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={async () => {
                            if (!selectedTenant) {
                              toast.error('No tenant selected');
                              return;
                            }
                            try {
                              await setDefaultStack(selectedStackId);
                            } catch (error) {
                              logger.error('Failed to set default stack', {
                                component: 'InferencePlayground',
                                operation: 'setDefaultStack',
                                stackId: selectedStackId,
                              }, toError(error));
                            }
                          }}
                          className="h-6 text-xs"
                          title="Set as default stack for this tenant"
                        >
                          Set Default
                        </Button>
                      )}
                      {selectedStackId && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => {
                            setSelectedStackId('');
                            setSelectedAdapterId('none');
                          }}
                          className="h-6 text-xs"
                        >
                          Clear
                        </Button>
                      )}
                    </div>
                  </div>
                  <Select value={selectedStackId || "_none"} onValueChange={(value) => {
                    setSelectedStackId(value === "_none" ? "" : value);
                    // Clear adapter selection when stack is selected
                    if (value && value !== "_none") {
                      setSelectedAdapterId('none');
                    }
                  }}>
                    <SelectTrigger id="stack">
                      <SelectValue placeholder={stacks.length === 0 ? "No stacks available" : "Select stack..."} />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="_none">None (Use individual adapters)</SelectItem>
                      {stacks
                        .filter((stack) => {
                          const state = stack.lifecycle_state?.toLowerCase() || 'active';
                          return state === 'active' || state === 'draft';
                        })
                        .map((stack) => {
                          const state = stack.lifecycle_state?.toLowerCase() || 'active';
                          const stateConfig: Record<string, { variant: 'default' | 'secondary' | 'outline'; className: string }> = {
                            active: { variant: 'default', className: 'bg-green-500 text-white' },
                            draft: { variant: 'secondary', className: 'bg-blue-500 text-white' },
                          };
                          const config = stateConfig[state] || stateConfig.active;

                          return (
                            <SelectItem key={stack.id} value={stack.id}>
                              <div className="flex items-center gap-2">
                                <Layers className="h-4 w-4" aria-hidden="true" />
                                <span>{stack.name}</span>
                                <Badge variant={config.variant} className={`text-xs ${config.className}`}>
                                  {state.charAt(0).toUpperCase() + state.slice(1)}
                                </Badge>
                                {defaultStack?.id === stack.id && (
                                  <Badge variant="secondary" className="text-xs">Default</Badge>
                                )}
                                <span className="text-xs text-muted-foreground ml-auto">
                                  ({stack.adapter_ids?.length || 0} adapters)
                                </span>
                              </div>
                            </SelectItem>
                          );
                        })}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    {selectedStackId
                      ? 'Using adapters from selected stack. Stack adapters will be shown below.'
                      : 'Stacks are reusable combinations of adapters. Select a stack to use its configured adapters for inference.'}
                  </p>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="adapter" className="flex items-center gap-1">
                    Adapter (Optional) {adapters.length === 0 && <span className="text-muted-foreground text-xs">(None available)</span>}
                    <GlossaryTooltip termId="inference-adapter-stack">
                      <span className="cursor-help text-muted-foreground hover:text-foreground">
                        <HelpCircle className="h-3 w-3" />
                      </span>
                    </GlossaryTooltip>
                  </Label>
                  <Select value={selectedAdapterId} onValueChange={setSelectedAdapterId} disabled={adapters.length === 0}>
                    <SelectTrigger id="adapter">
                      <SelectValue placeholder={adapters.length === 0 ? "No adapters available" : "Select adapter... (or use base model only)"} />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">Default (No adapter)</SelectItem>
                      {adapters.filter(adapter => adapter.id && adapter.id !== '').map((adapter) => {
                        // State indicator: color-coded dot based on lifecycle state
                        const stateIndicator = (adapter.current_state && {
                          'resident': { color: 'bg-green-500', label: 'Resident' },
                          'hot': { color: 'bg-emerald-400', label: 'Hot' },
                          'warm': { color: 'bg-yellow-400', label: 'Warm' },
                          'cold': { color: 'bg-blue-400', label: 'Cold' },
                          'unloaded': { color: 'bg-gray-400', label: 'Unloaded' },
                        }[adapter.current_state]) || { color: 'bg-gray-300', label: adapter.current_state || 'Unknown' };

                        return (
                          <SelectItem key={adapter.id} value={adapter.id}>
                            <div className="flex items-center gap-2">
                              <span
                                className={`h-2 w-2 rounded-full ${stateIndicator.color}`}
                                title={stateIndicator.label}
                                aria-label={`State: ${stateIndicator.label}`}
                              />
                              <Code className="h-4 w-4" aria-hidden="true" />
                              <span>{adapter.name}</span>
                              <span className="text-xs text-muted-foreground">
                                ({stateIndicator.label})
                              </span>
                            </div>
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    {adapters.length === 0
                      ? 'No adapters available. Inference will use base model only.'
                      : 'Adapters are trained LoRA modules that specialize the model for specific tasks. Select one to enhance inference quality. Base model runs without any adapter.'}
                  </p>
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="prompt" className="flex items-center gap-1">
                      Prompt
                      <GlossaryTooltip termId="inference-prompt">
                        <span className="cursor-help text-muted-foreground hover:text-foreground">
                          <HelpCircle className="h-3 w-3" />
                        </span>
                      </GlossaryTooltip>
                      <span className="sr-only">
                        Use Ctrl+G or Cmd+G to generate, Ctrl+S or Cmd+S to toggle streaming mode, Ctrl+B or Cmd+B to toggle batch mode, Escape to cancel
                      </span>
                    </Label>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setShowTemplates(!showTemplates)}
                        className="h-8 px-2"
                        aria-label={showTemplates ? "Hide prompt templates" : "Show prompt templates"}
                      >
                        <FileText className="h-3 w-3 mr-1" />
                        Templates
                      </Button>
                    </div>
                  </div>
                  <Textarea
                    id="prompt"
                    placeholder="Enter your prompt here..."
                    value={configA.prompt}

                    onChange={(e) => {
                      const sanitized = sanitizeInput(e.target.value);
                      setConfigA({ ...configA, prompt: sanitized });
                      setPromptValidation(validatePrompt(sanitized));

                      // Track if prompt has been modified since template was applied
                      if (selectedTemplate) {
                        setPromptModifiedSinceTemplate(sanitized !== selectedTemplate.prompt);
                      }
                    }}
                    rows={6}
                    className={promptValidation?.valid === false ? 'border-destructive' : ''}
                    aria-describedby={promptValidation?.error ? "prompt-error" : promptValidation?.warning ? "prompt-warning" : undefined}
                    aria-invalid={promptValidation?.valid === false}
                  />
                  {promptValidation?.error && (
                    <Alert variant="destructive" className="text-sm" id="prompt-error">
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
                    <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50" id="prompt-warning">
                      <AlertTriangle className="h-4 w-4 text-yellow-600" />
                      <AlertDescription className="text-yellow-800">
                        <strong>Warning:</strong> {promptValidation.warning}
                      </AlertDescription>
                    </Alert>
                  )}
                  {promptValidation?.valid === false && !promptValidation.error && (
                    <div className="text-xs text-muted-foreground">
                      Character count: {configA.prompt.length.toLocaleString()} / {MAX_PROMPT_LENGTH.toLocaleString()}
                    </div>
                  )}
                  {windowSize.width < 768 && (
                    <div className="text-xs text-muted-foreground mt-1">
                      💡 Swipe left/right to change modes, swipe up for templates
                    </div>
                  )}
                  {/* Template Management */}
                  <SectionErrorBoundary sectionName="Template Manager">
                    <TemplateManager
                      templates={templates}
                      recentTemplates={getRecentTemplates()}
                      selectedTemplate={selectedTemplate}
                      templateVariables={templateVariables}
                      showTemplates={showTemplates}
                      showVariableInputs={showVariableInputs}
                      promptModifiedSinceTemplate={promptModifiedSinceTemplate}
                      onSelect={handleApplyTemplate}
                      onApplyVariables={handleApplyVariableSubstitution}
                      onResetToTemplate={handleResetToTemplate}
                      onSaveAsTemplate={handleSavePromptAsTemplate}
                      onManageTemplates={() => setShowTemplateManager(true)}
                      onToggleTemplates={() => setShowTemplates(!showTemplates)}
                      onCancelVariables={() => {
                        setShowVariableInputs(false);
                        setSelectedTemplate(null);
                        setTemplateVariables({});
                      }}
                      onVariableChange={(variable, value) =>
                        setTemplateVariables({
                          ...templateVariables,
                          [variable]: value,
                        })
                      }
                      substituteVariables={substituteVariables}
                    />
                  </SectionErrorBoundary>
                </div>

                {renderAdvancedOptions(configA, setConfigA)}


                <div className="flex gap-2">
                  <Button
                    className={`flex-1 ${!can('inference:execute') ? 'opacity-50 cursor-not-allowed' : ''}`}
                    onClick={() => {
                      if (inferenceMode === 'streaming') {
                        handleStreamingInfer(configA, setResponseA, setIsLoadingA);
                      } else {
                        handleInfer(configA, setResponseA, setIsLoadingA);
                      }
                    }}
                    disabled={isLoadingA || isStreaming || !can('inference:execute')}
                    aria-label="Run inference with current configuration"
                    title={!can('inference:execute') ? 'Requires inference:execute permission' : undefined}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    {isLoadingA || isStreaming ? 'Generating...' : 'Generate'}
                  </Button>
                  {(inferenceState.isRunning || isStreaming) && (
                    <Button
                      variant="outline"
                      onClick={() => {
                        if (isStreaming) {
                          cancelStreaming();
                        } else {
                          cancelInference();
                        }
                      }}
                      aria-label="Cancel inference"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>

                {responseA && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => handleExport(configA, responseA)}
                  >
                    <Download className="h-4 w-4 mr-2" />
                    Export
                  </Button>
                )}

                {configA.prompt && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={handleSavePromptAsTemplate}
                  >
                    <Plus className="h-4 w-4 mr-2" />
                    Save Prompt as Template
                  </Button>
                )}
              </CardContent>
            </Card>

            {/* Recent Sessions */}
            {recentSessions.length > 0 && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <History className="h-4 w-4" aria-hidden="true" />
                    Recent Sessions
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-2">
                  {recentSessions.slice(0, 5).map((session) => (
                    <Button
                      key={session.id}
                      variant="ghost"
                      className="w-full justify-start text-left h-auto py-2"
                      onClick={() => loadSession(session)}
                    >
                      <div className="truncate">
                        <p className="text-sm truncate">{session.prompt}</p>
                        <p className="text-xs text-muted-foreground">
                          {new Date(session.created_at).toLocaleString()}
                        </p>
                      </div>
                    </Button>
                  ))}
                </CardContent>
              </Card>
            )}
          </div>

          {/* Response Panel */}
          <div className="lg:col-span-2">
            <Card className="min-h-[600px]">
              <CardHeader>
                <CardTitle className="text-base">Output</CardTitle>
              </CardHeader>
              <CardContent>
                <SectionErrorBoundary sectionName="Inference Output">
                  {renderResponse(responseA, isLoadingA)}
                </SectionErrorBoundary>
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        /* Comparison Mode */
        <SectionErrorBoundary sectionName="Comparison Mode">
          <ComparisonMode
            prompt={prompt}
            configA={configA}
            configB={configB}
            responseA={responseA}
            responseB={responseB}
            isLoadingA={isLoadingA}
            isLoadingB={isLoadingB}
            isRunning={inferenceState.isRunning}
            canExecute={can('inference:execute')}
            metrics={metrics}
            onPromptChange={(value) => {
              setPrompt(value);
              setConfigA({ ...configA, prompt: value });
              setConfigB({ ...configB, prompt: value });
            }}
            onConfigAChange={setConfigA}
            onConfigBChange={setConfigB}
            onRunA={() => handleInfer(configA, setResponseA, setIsLoadingA)}
            onRunB={() => handleInfer(configB, setResponseB, setIsLoadingB)}
            onCancel={cancelInference}
            onCopy={(text) => {
              navigator.clipboard.writeText(text);
              toast.success('Copied to clipboard');
            }}
            renderAdvancedOptions={renderAdvancedOptions}
          />
        </SectionErrorBoundary>
      )}

      {/* Prompt Template Manager Dialog */}
      <PromptTemplateManager
        open={showTemplateManager}
        onOpenChange={setShowTemplateManager}
        onSelectTemplate={handleApplyTemplate}
      />
    </div>
  );
}

// Wrap with PageErrorsProvider
export function InferencePlayground(props: InferencePlaygroundProps) {
  return (
    <PageErrorsProvider>
      <InferencePlaygroundContent {...props} />
    </PageErrorsProvider>
  );
}
